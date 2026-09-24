import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

private struct RideHistoryDetailLoadRequest: Sendable {
    let rideID: String
    let generation: UInt64
}

private struct RideHistoryDetailViewportRequest: Sendable {
    let rideID: String
    let projectionRideID: String
    let generation: UInt64
}

@MainActor
@Observable
final class RideHistoryModel {
    enum DateFilter: String {
        case last30Days
        case allTime
    }

    enum SelectionAction: Equatable {
        case none
        case load(String?)
        case select(String)
    }

    nonisolated private static var limits: MobileRideMapLimits { .rustOwned }

    private let stateProvider: @MainActor () -> MobileRideMapState?
    private var cursor: MobileRideCursorDto?
    private var queryDateAfterMilliseconds: UInt64?
    private var loadTask: Task<Void, Never>?
    private var pageTask: Task<Void, Never>?
    private var queryGeneration: UInt64 = 0
    private var selectionTask: Task<Void, Never>?
    private var detailLoadGeneration: UInt64 = 0
    private var selectionCancellation: MobileRideMapProjectionCancellation?
    private var viewportTask: Task<Void, Never>?
    private var viewportCancellation: MobileRideMapProjectionCancellation?
    private var contextTask: Task<Void, Never>?

    private(set) var error: MobileRideMapError?
    private(set) var rides = [MobileRideMapHistorySummaryDto]()
    private(set) var canLoadMore = false
    var searchText = ""
    private(set) var dateFilter = DateFilter.last30Days
    private(set) var vehicleFilter: String?
    private(set) var vehicleIdentities = [String]()
    private(set) var vehicleNames = [String: String]()
    private(set) var isLoading = false

    private(set) var routeError: MobileRideMapError?
    private(set) var detailRouteError: MobileRideMapError?
    private(set) var displayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var cameraRegion: MobileRideMapCameraRegion?
    private(set) var endpointMetadata = MobileRideMapRouteEndpointMetadata.empty
    private(set) var segments = [MobileRideMapSegmentDisplayMetadata]()
    private(set) var backgroundGapCount: UInt64 = 0
    private(set) var pointsTruncated = false
    private(set) var segmentsOmittedByBudget = false
    private(set) var detailDisplayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var detailRoutePresence = MobileRideMapRoutePresence.emptyRide
    private(set) var detailMusicTimeline = [MobileMusicRideEventDto]()
    private(set) var detailMusicTimelineUnavailable = false
    private(set) var detailMusicState: MobileMusicHistoryStateDto?
    private(set) var detailMusicError: MobileRideMapError?
    private(set) var detailProjectionRideID: String?
    private(set) var detailCameraRegion: MobileRideMapCameraRegion?
    private(set) var detailEndpointMetadata = MobileRideMapRouteEndpointMetadata.empty
    private(set) var detailSegments = [MobileRideMapSegmentDisplayMetadata]()
    private(set) var detailBackgroundGapCount: UInt64 = 0
    private(set) var detailCameraFitVersion: UInt64 = 0
    private(set) var cameraFitVersion: UInt64 = 0
    private(set) var detailPointsTruncated = false
    private(set) var detailSourcePointsOmittedByBudget = false
    private(set) var detailSourceSegmentsOmittedByBudget = false
    private(set) var detailSegmentsOmittedByBudget = false
    private(set) var contextRoutes = [MobileRideMapHistoryContextRoute]()
    private(set) var contextProjection: MobileRideMapHistoryContextProjection?
    private(set) var projectionVersion: UInt64 = 0
    private(set) var detailProjectionVersion: UInt64 = 0
    private(set) var routeLoading = false
    private(set) var detailRouteLoading = false
    private(set) var selectedRideID: String?

    var onPageUpdated: (() -> Void)?

    var filter: MobileRideHistoryFilterDto {
        historyFilter
    }

    init(stateProvider: @escaping @MainActor () -> MobileRideMapState?) {
        self.stateProvider = stateProvider
    }

    func prepareForReload() {
        invalidateProjectionWork()
        contextTask?.cancel()
        contextTask = nil
        contextProjection = nil
        contextRoutes.removeAll(keepingCapacity: true)
        error = nil
        routeError = nil
        detailRouteError = nil
        isLoading = true
        routeLoading = false
        detailRouteLoading = false
        clearMusicMetadata()
    }

    func applyLoadFailure(_ error: MobileRideMapError) {
        invalidateProjectionWork()
        isLoading = false
        self.error = error
        routeLoading = false
        routeError = error
        detailRouteLoading = false
        detailRouteError = error
    }

    func applyQueryResult(
        requestedRideID: String?,
        selectionError: MobileRideMapError?
    ) {
        if let selectionError, requestedRideID == nil {
            applyLoadFailure(selectionError)
            return
        }
        guard let selectedRideID = Self.preferredHistorySelection(
            requestedID: requestedRideID,
            currentID: self.selectedRideID,
            summaries: rides
        ) else {
            self.selectedRideID = nil
            clearRouteProjection()
            routeLoading = false
            routeError = selectionError
            detailRouteError = selectionError
            detailRouteLoading = false
            return
        }
        select(
            rideID: selectedRideID,
            requestedPointLimit: Int(Self.limits.historyContextPerRouteBudget)
        )
    }

    func ensureSelection(requestedRideID: String?) -> SelectionAction {
        Self.selectionAction(
            requestedID: requestedRideID,
            currentID: selectedRideID,
            summaries: rides
        )
    }

    func projectDetailViewport(_ viewport: MobileGeoBoundsDto?) {
        guard let selectedRideID,
              rides.contains(where: { $0.rideID == selectedRideID }),
              let projectionRideID = detailProjectionRideID,
              projectionRideID == selectedRideID
        else {
            return
        }
        let request = RideHistoryDetailViewportRequest(
            rideID: selectedRideID,
            projectionRideID: projectionRideID,
            generation: detailLoadGeneration
        )
        viewportCancellation?.cancel()
        viewportTask?.cancel()
        detailRouteError = nil
        detailRouteLoading = true
        let cancellation = MobileRideMapProjectionCancellation()
        viewportCancellation = cancellation
        guard let viewport else {
            detailRouteLoading = false
            detailRouteError = .invalidRouteProjection
            detailRoutePresence = .emptyRide
            replaceDetailDisplayPoints([], truncated: false)
            return
        }
        guard let state = stateProvider() else {
            detailRouteLoading = false
            detailRouteError = .storageError("Rust ride database is unavailable")
            detailRoutePresence = .emptyRide
            replaceDetailDisplayPoints([], truncated: false)
            return
        }
        let budget = Self.limits.historyPreviewPointLimit
        viewportTask = Task { [weak self] in
            do {
                let result = try await withTaskCancellationHandler(operation: {
                    try await Self.runCancellableDetached(priority: .userInitiated) {
                        try state.projectStoredPoints(
                            rideID: request.rideID,
                            budget: budget,
                            viewport: viewport,
                            cancellation: cancellation
                        )
                    }
                }, onCancel: {
                    cancellation.cancel()
                })
                guard let self,
                      self.admits(request, isCancelled: Task.isCancelled)
                else { return }
                self.replaceDetailDisplayPoints(
                    result.points,
                    cameraRegion: result.canonicalCameraRegion ?? result.cameraRegion,
                    endpointMetadata: result.endpointMetadata,
                    segments: result.segments,
                    backgroundGapCount: result.backgroundGapCount,
                    truncated: Self.detailPointsAreTruncated(
                        sourcePointsOmittedByBudget: self.detailSourcePointsOmittedByBudget,
                        viewportPointsOmittedByBudget: result.pointsOmittedByBudget
                    ),
                    segmentsOmittedByBudget: Self.detailSegmentsAreOmitted(
                        sourceSegmentsOmittedByBudget: self.detailSourceSegmentsOmittedByBudget,
                        viewportSegmentsOmittedByBudget: result.segmentsOmittedByBudget
                    )
                )
                self.detailRoutePresence = result.presence
                self.detailRouteError = nil
                self.detailRouteLoading = false
            } catch {
                guard let self,
                      self.admits(request, isCancelled: Task.isCancelled)
                else { return }
                let mappedError = Self.mapError(error)
                if mappedError == .cancelled { return }
                self.detailRouteError = mappedError
                self.detailRouteLoading = false
                self.detailRoutePresence = .emptyRide
                self.replaceDetailDisplayPoints([], truncated: false)
            }
        }
    }

    func loadRoutePreview() {
        guard let selectedRideID else { return }
        select(rideID: selectedRideID, requestedPointLimit: nil)
    }

    func select(rideID: String, requestedPointLimit: Int? = nil) {
        guard rides.contains(where: { $0.rideID == rideID }) else {
            routeLoading = false
            routeError = nil
            detailRouteLoading = false
            detailRouteError = nil
            clearMusicMetadata()
            return
        }
        invalidateProjectionWork()
        let request = RideHistoryDetailLoadRequest(
            rideID: rideID,
            generation: detailLoadGeneration
        )
        contextTask?.cancel()
        contextTask = nil
        contextProjection = nil
        contextRoutes.removeAll(keepingCapacity: true)
        let selectingDifferentRide = selectedRideID != rideID
        if selectingDifferentRide {
            clearRouteProjection()
        }
        selectedRideID = rideID
        routeError = nil
        detailRouteError = nil
        clearMusicMetadata()
        routeLoading = true
        detailRouteLoading = true
        guard let state = stateProvider() else {
            replaceDisplayPoints([], truncated: false)
            replaceDetailDisplayPoints([], truncated: false)
            detailRoutePresence = .emptyRide
            routeLoading = false
            detailRouteLoading = false
            routeError = .storageError("Rust ride database is unavailable")
            detailRouteError = routeError
            detailMusicError = routeError
            return
        }
        let cancellation = MobileRideMapProjectionCancellation()
        selectionCancellation = cancellation
        let budget = UInt32(
            min(
                requestedPointLimit ?? Int(Self.limits.historyPreviewPointLimit),
                Int(Self.limits.historyPreviewPointLimit)
            )
        )
        selectionTask = Task { [weak self] in
            do {
                let result = try await withTaskCancellationHandler(operation: {
                    try await Self.runCancellableDetached(priority: .userInitiated) {
                        let projection = try state.projectStoredPoints(
                            rideID: request.rideID,
                            budget: budget,
                            cancellation: cancellation
                        )
                        let musicHistory: MusicHistoryQueryResult
                        do {
                            let storedHistory = try state.storedMusicHistory(rideID: request.rideID)
                            musicHistory = MusicHistoryQueryResult(
                                events: storedHistory.events,
                                state: storedHistory.historyState,
                                error: nil
                            )
                        } catch {
                            musicHistory = MusicHistoryQueryResult(
                                events: [],
                                state: nil,
                                error: Self.mapError(error)
                            )
                        }
                        return (projection, musicHistory)
                    }
                }, onCancel: {
                    cancellation.cancel()
                })
                guard let self,
                      self.admits(request, isCancelled: Task.isCancelled)
                else { return }
                let (projection, musicHistory) = result
                self.cameraFitVersion &+= 1
                self.detailCameraFitVersion &+= 1
                self.routeError = nil
                self.detailRouteError = nil
                self.replaceDisplayPoints(
                    projection.points,
                    cameraRegion: projection.canonicalCameraRegion ?? projection.cameraRegion,
                    endpointMetadata: projection.endpointMetadata,
                    segments: projection.segments,
                    backgroundGapCount: projection.backgroundGapCount,
                    truncated: projection.pointsOmittedByBudget,
                    segmentsOmittedByBudget: projection.segmentsOmittedByBudget
                )
                self.detailSourcePointsOmittedByBudget = projection.pointsOmittedByBudget
                self.detailSourceSegmentsOmittedByBudget = projection.segmentsOmittedByBudget
                self.detailMusicTimeline = musicHistory.events
                self.detailMusicTimelineUnavailable = musicHistory.error != nil
                self.detailMusicState = musicHistory.state
                self.detailMusicError = musicHistory.error
                self.detailProjectionRideID = request.rideID
                self.detailRoutePresence = projection.presence
                self.replaceDetailDisplayPoints(
                    projection.points,
                    cameraRegion: projection.canonicalCameraRegion ?? projection.cameraRegion,
                    endpointMetadata: projection.endpointMetadata,
                    segments: projection.segments,
                    backgroundGapCount: projection.backgroundGapCount,
                    truncated: projection.pointsOmittedByBudget,
                    segmentsOmittedByBudget: Self.detailSegmentsAreOmitted(
                        sourceSegmentsOmittedByBudget: self.detailSourceSegmentsOmittedByBudget,
                        viewportSegmentsOmittedByBudget: projection.segmentsOmittedByBudget
                    )
                )
                self.routeLoading = false
                self.detailRouteLoading = false
            } catch {
                guard let self,
                      self.admits(request, isCancelled: Task.isCancelled)
                else { return }
                let mappedError = Self.mapError(error)
                self.routeError = mappedError
                self.routeLoading = false
                self.detailRouteError = mappedError
                self.detailRouteLoading = false
                self.detailMusicTimeline.removeAll(keepingCapacity: true)
                self.detailMusicTimelineUnavailable = true
                self.detailMusicState = nil
                self.detailMusicError = mappedError
                self.detailProjectionRideID = nil
                self.detailRoutePresence = .emptyRide
                self.replaceDisplayPoints([], truncated: false)
                self.replaceDetailDisplayPoints([], truncated: false)
            }
        }
    }

    private func admits(
        _ request: RideHistoryDetailLoadRequest,
        isCancelled: Bool
    ) -> Bool {
        Self.shouldApplyHistoryDetailLoad(
            rideID: request.rideID,
            selectedRideID: selectedRideID,
            loadGeneration: request.generation,
            currentGeneration: detailLoadGeneration,
            isCancelled: isCancelled
        )
    }

    private func admits(
        _ request: RideHistoryDetailViewportRequest,
        isCancelled: Bool
    ) -> Bool {
        Self.shouldApplyHistoryDetailViewport(
            rideID: request.rideID,
            selectedRideID: selectedRideID,
            expectedProjectionRideID: request.projectionRideID,
            currentProjectionRideID: detailProjectionRideID,
            loadGeneration: request.generation,
            currentGeneration: detailLoadGeneration,
            isCancelled: isCancelled
        )
    }

    private func replaceDisplayPoints(
        _ points: [MobileRideMapRouteDisplayPoint],
        cameraRegion: MobileRideMapCameraRegion? = nil,
        endpointMetadata: MobileRideMapRouteEndpointMetadata = .empty,
        segments: [MobileRideMapSegmentDisplayMetadata] = [],
        backgroundGapCount: UInt64 = 0,
        truncated: Bool,
        segmentsOmittedByBudget: Bool = false
    ) {
        displayPoints = points
        self.cameraRegion = cameraRegion
        self.endpointMetadata = endpointMetadata
        self.segments = segments
        self.backgroundGapCount = backgroundGapCount
        pointsTruncated = truncated
        self.segmentsOmittedByBudget = segmentsOmittedByBudget
        projectionVersion &+= 1
    }

    private func replaceDetailDisplayPoints(
        _ points: [MobileRideMapRouteDisplayPoint],
        cameraRegion: MobileRideMapCameraRegion? = nil,
        endpointMetadata: MobileRideMapRouteEndpointMetadata = .empty,
        segments: [MobileRideMapSegmentDisplayMetadata] = [],
        backgroundGapCount: UInt64 = 0,
        truncated: Bool,
        segmentsOmittedByBudget: Bool = false
    ) {
        detailDisplayPoints = points
        detailCameraRegion = cameraRegion
        detailEndpointMetadata = endpointMetadata
        detailSegments = segments
        detailBackgroundGapCount = backgroundGapCount
        detailPointsTruncated = truncated
        detailSegmentsOmittedByBudget = segmentsOmittedByBudget
        detailProjectionVersion &+= 1
    }

    func clearRouteProjection() {
        contextTask?.cancel()
        contextTask = nil
        contextProjection = nil
        contextRoutes.removeAll(keepingCapacity: true)
        replaceDisplayPoints([], truncated: false)
        detailRoutePresence = .emptyRide
        clearMusicMetadata()
        detailProjectionRideID = nil
        detailSourcePointsOmittedByBudget = false
        detailSourceSegmentsOmittedByBudget = false
        routeLoading = false
        detailRouteLoading = false
        routeError = nil
        detailRouteError = nil
        replaceDetailDisplayPoints([], truncated: false)
    }

    func invalidateProjectionWork() {
        selectionTask?.cancel()
        selectionCancellation?.cancel()
        viewportCancellation?.cancel()
        viewportTask?.cancel()
        detailLoadGeneration &+= 1
    }

    func clearMusicMetadata() {
        detailMusicTimeline.removeAll(keepingCapacity: true)
        detailMusicTimelineUnavailable = false
        detailMusicState = nil
        detailMusicError = nil
    }

    func invalidateForMusicDeletion() {
        invalidateProjectionWork()
        routeLoading = false
        detailRouteLoading = false
    }

    private func projectContext(for rideID: String) {
        contextTask?.cancel()
        contextProjection = nil
        contextRoutes.removeAll(keepingCapacity: true)
        guard let state = stateProvider() else { return }
        let filter = self.filter
        let budget = MobileRideMapHistoryContextBudget.overview
        contextTask = Task { [weak self] in
            do {
                let projection = try await Self.runCancellableDetached(priority: .userInitiated) {
                    try state.projectStoredHistoryContext(
                        filter: filter,
                        selectedRideID: rideID,
                        budget: budget
                    )
                }
                guard !Task.isCancelled,
                      let self,
                      self.selectedRideID == rideID
                else { return }
                self.contextProjection = projection
                self.contextRoutes = projection.routes
            } catch {
                guard !Task.isCancelled,
                      let self,
                      self.selectedRideID == rideID
                else { return }
                self.contextProjection = nil
                self.contextRoutes.removeAll(keepingCapacity: true)
            }
        }
    }

    isolated deinit {
        loadTask?.cancel()
        pageTask?.cancel()
        selectionTask?.cancel()
        selectionCancellation?.cancel()
        viewportTask?.cancel()
        viewportCancellation?.cancel()
        contextTask?.cancel()
    }

    func load(selecting requestedRideID: String? = nil) {
        loadTask?.cancel()
        pageTask?.cancel()
        queryGeneration &+= 1
        let generation = queryGeneration
        queryDateAfterMilliseconds = historyDateAfterMilliseconds
        error = nil
        isLoading = true

        guard let state = stateProvider() else {
            finishLoad(
                generation: generation,
                error: .storageError("Rust ride database is unavailable")
            )
            return
        }

        let filter = historyFilter
        loadTask = Task { [weak self] in
            do {
                let result = try await Self.runCancellableDetached(priority: .userInitiated) {
                    let page = try state.storedHistoryPage(
                        cursor: nil,
                        limit: Self.limits.historyPageLimit,
                        filter: filter
                    )
                    let vehicleOptions = try state.storedHistoryVehicleOptions()
                    var summaries = page.summaries
                    if let requestedRideID,
                       summaries.contains(where: { $0.rideID == requestedRideID }) == false,
                       let requestedRide = try state.storedHistoryRide(rideID: requestedRideID)
                    {
                        let insertionIndex = summaries.firstIndex {
                            $0.createdAtMilliseconds < requestedRide.createdAtMilliseconds
                        } ?? summaries.endIndex
                        summaries.insert(requestedRide, at: insertionIndex)
                    }
                    return (summaries, page.nextCursor, vehicleOptions)
                }
                guard let self,
                      self.accepts(generation: generation, isCancelled: Task.isCancelled)
                else { return }
                self.loadTask = nil
                self.isLoading = false
                self.rides = result.0
                self.vehicleIdentities = Self.mergeVehicleIdentities(
                    existing: result.2.map(\.platformIdentifier),
                    incoming: result.0.flatMap { [$0.associatedVehicle, $0.candidateVehicle].compactMap { $0 } }
                )
                self.vehicleNames = Self.vehicleNames(result.2, summaries: result.0)
                self.cursor = result.1
                self.canLoadMore = result.1 != nil
                self.error = nil
                self.onPageUpdated?()
                self.applyQueryResult(
                    requestedRideID: requestedRideID,
                    selectionError: Self.selectionError(
                        requestedID: requestedRideID,
                        summaries: result.0
                    )
                )
            } catch {
                guard let self,
                      self.accepts(generation: generation, isCancelled: Task.isCancelled)
                else { return }
                self.finishLoad(
                    generation: generation,
                    error: Self.mapError(error)
                )
            }
        }
    }

    func loadMore() {
        guard canLoadMore,
              loadTask == nil,
              isLoading == false,
              let cursor,
              let state = stateProvider()
        else { return }
        pageTask?.cancel()
        queryGeneration &+= 1
        let generation = queryGeneration
        let filter = historyFilter
        pageTask = Task { [weak self] in
            do {
                let page = try await Self.runCancellableDetached(priority: .userInitiated) {
                    try state.storedHistoryPage(
                        cursor: cursor,
                        limit: Self.limits.historyPageLimit,
                        filter: filter
                    )
                }
                guard let self,
                      self.accepts(generation: generation, isCancelled: Task.isCancelled)
                else { return }
                self.pageTask = nil
                self.rides = Self.appendingUniqueHistory(
                    existing: self.rides,
                    incoming: page.summaries
                )
                self.vehicleIdentities = Self.mergeVehicleIdentities(
                    existing: self.vehicleIdentities,
                    incoming: page.summaries.flatMap { [$0.associatedVehicle, $0.candidateVehicle].compactMap { $0 } }
                )
                self.vehicleNames = Self.mergeVehicleNames(
                    existing: self.vehicleNames,
                    incoming: Self.vehicleNames([], summaries: page.summaries)
                )
                self.cursor = page.nextCursor
                self.canLoadMore = page.nextCursor != nil
                self.error = nil
                self.onPageUpdated?()
            } catch {
                guard let self,
                      self.accepts(generation: generation, isCancelled: Task.isCancelled)
                else { return }
                self.pageTask = nil
                self.error = Self.mapError(error)
                self.onPageUpdated?()
            }
        }
    }

    func setDateFilter(_ filter: DateFilter) {
        dateFilter = filter
    }

    func setVehicleFilter(_ identity: String?) {
        vehicleFilter = identity
    }

    func clearFilters() {
        searchText = ""
        dateFilter = .last30Days
        vehicleFilter = nil
    }

    func setError(_ error: MobileRideMapError?) {
        self.error = error
    }

    private func finishLoad(
        generation: UInt64,
        error: MobileRideMapError
    ) {
        guard accepts(generation: generation, isCancelled: false) else { return }
        loadTask = nil
        isLoading = false
        self.error = error
        applyLoadFailure(error)
    }

    private func accepts(generation: UInt64, isCancelled: Bool) -> Bool {
        Self.shouldApplyHistoryQuery(
            generation: generation,
            currentGeneration: queryGeneration,
            isCancelled: isCancelled
        )
    }

    private var historyDateAfterMilliseconds: UInt64? {
        guard dateFilter == .last30Days else { return nil }
        let now = Date().timeIntervalSince1970 * 1_000
        guard now.isFinite, now > 0 else { return 0 }
        let window = Double(Self.limits.historyRecentWindowMilliseconds)
        return UInt64(max(0, now - window))
    }

    private var historyFilter: MobileRideHistoryFilterDto {
        MobileRideHistoryFilterDto(
            createdAfterMilliseconds: queryDateAfterMilliseconds ?? historyDateAfterMilliseconds,
            vehicleIdentity: vehicleFilter,
            searchText: Self.normalizedSearchText(searchText)
        )
    }

    static func selectionError(
        requestedID: String?,
        summaries: [MobileRideMapHistorySummaryDto]
    ) -> MobileRideMapError? {
        guard let requestedID, summaries.contains(where: { $0.rideID == requestedID }) == false else {
            return nil
        }
        return .rideNotFound
    }

    static func appendingUniqueHistory(
        existing: [MobileRideMapHistorySummaryDto],
        incoming: [MobileRideMapHistorySummaryDto]
    ) -> [MobileRideMapHistorySummaryDto] {
        var seen = Set(existing.map(\.rideID))
        return existing + incoming.filter { seen.insert($0.rideID).inserted }
    }

    static func mergeVehicleIdentities(existing: [String], incoming: [String]) -> [String] {
        Array(Set(existing + incoming)).sorted()
    }

    static func vehicleNames(
        _ options: [MobileRideMapHistoryVehicleOptionDto],
        summaries: [MobileRideMapHistorySummaryDto]
    ) -> [String: String] {
        var names = Dictionary(
            options.compactMap { option in
                option.displayName.map { (option.platformIdentifier, $0) }
            },
            uniquingKeysWith: { first, _ in first }
        )
        for summary in summaries {
            if let identity = summary.associatedVehicle,
               let name = summary.associatedVehicleName
            {
                names[identity] = name
            }
            if let identity = summary.candidateVehicle,
               let name = summary.candidateVehicleName
            {
                names[identity] = name
            }
        }
        return names
    }

    private static func mergeVehicleNames(
        existing: [String: String],
        incoming: [String: String]
    ) -> [String: String] {
        existing.merging(incoming) { _, incoming in incoming }
    }

    static func preferredHistorySelection(
        requestedID: String?,
        currentID: String?,
        summaries: [MobileRideMapHistorySummaryDto]
    ) -> String? {
        if let requestedID {
            return summaries.first(where: { $0.rideID == requestedID })?.rideID
        }
        return summaries.first(where: { $0.rideID == currentID })?.rideID
            ?? summaries.first?.rideID
    }

    static func selectionAction(
        requestedID: String?,
        currentID: String?,
        summaries: [MobileRideMapHistorySummaryDto]
    ) -> SelectionAction {
        guard let requestedID else {
            return summaries.isEmpty ? .load(nil) : .none
        }
        guard summaries.contains(where: { $0.rideID == requestedID }) else {
            return .load(requestedID)
        }
        return currentID == requestedID ? .none : .select(requestedID)
    }

    static func detailPointsAreTruncated(
        sourcePointsOmittedByBudget: Bool,
        viewportPointsOmittedByBudget: Bool
    ) -> Bool {
        sourcePointsOmittedByBudget || viewportPointsOmittedByBudget
    }

    static func detailSegmentsAreOmitted(
        sourceSegmentsOmittedByBudget: Bool,
        viewportSegmentsOmittedByBudget: Bool
    ) -> Bool {
        sourceSegmentsOmittedByBudget || viewportSegmentsOmittedByBudget
    }

    static func shouldApplyHistoryDetailLoad(
        rideID: String,
        selectedRideID: String?,
        loadGeneration: UInt64,
        currentGeneration: UInt64,
        isCancelled: Bool
    ) -> Bool {
        !isCancelled && loadGeneration == currentGeneration && selectedRideID == rideID
    }

    static func shouldApplyHistoryDetailViewport(
        rideID: String,
        selectedRideID: String?,
        expectedProjectionRideID: String?,
        currentProjectionRideID: String?,
        loadGeneration: UInt64,
        currentGeneration: UInt64,
        isCancelled: Bool
    ) -> Bool {
        !isCancelled
            && loadGeneration == currentGeneration
            && selectedRideID == rideID
            && expectedProjectionRideID == rideID
            && currentProjectionRideID == expectedProjectionRideID
    }

    static func shouldApplyHistoryQuery(
        generation: UInt64,
        currentGeneration: UInt64,
        isCancelled: Bool
    ) -> Bool {
        !isCancelled && generation == currentGeneration
    }

    private static func normalizedSearchText(_ text: String) -> String? {
        let normalized = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return normalized.isEmpty ? nil : normalized
    }

    private nonisolated static func runCancellableDetached<Success: Sendable>(
        priority: TaskPriority,
        operation: @escaping @Sendable () throws -> Success
    ) async throws -> Success {
        let task = Task.detached(priority: priority) {
            try Task.checkCancellation()
            let result = try operation()
            try Task.checkCancellation()
            return result
        }
        return try await withTaskCancellationHandler(operation: {
            try await task.value
        }, onCancel: {
            task.cancel()
        })
    }

    nonisolated private static func mapError(_ error: Error) -> MobileRideMapError {
        if let error = error as? MobileRideMapError { return error }
        return .storageError(String(describing: error))
    }
}
