import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

@MainActor
@Observable
final class RideHistoryModel {
    typealias DateFilter = CutoutAppModel.RideMapHistoryDateFilter

    nonisolated private static var limits: MobileRideMapLimits { .rustOwned }

    private let stateProvider: @MainActor () -> MobileRideMapState?
    private var cursor: MobileRideCursorDto?
    private var queryDateAfterMilliseconds: UInt64?
    private var loadTask: Task<Void, Never>?
    private var pageTask: Task<Void, Never>?
    private var queryGeneration: UInt64 = 0

    private(set) var error: MobileRideMapError?
    private(set) var rides = [MobileRideMapHistorySummaryDto]()
    private(set) var canLoadMore = false
    var searchText = ""
    private(set) var dateFilter = DateFilter.last30Days
    private(set) var vehicleFilter: String?
    private(set) var vehicleIdentities = [String]()
    private(set) var vehicleNames = [String: String]()
    private(set) var isLoading = false

    var onSelectionRequired: ((String?, MobileRideMapError?) -> Void)?
    var onPageUpdated: (() -> Void)?

    var filter: MobileRideHistoryFilterDto {
        historyFilter
    }

    init(stateProvider: @escaping @MainActor () -> MobileRideMapState?) {
        self.stateProvider = stateProvider
    }

    isolated deinit {
        loadTask?.cancel()
        pageTask?.cancel()
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
                requestedRideID: requestedRideID,
                error: .storageError("Rust ride database is unavailable")
            )
            return
        }

        let filter = historyFilter
        loadTask = Task { [weak self] in
            do {
                let result = try await CutoutAppModel.runCancellableDetached(priority: .userInitiated) {
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
                self.onSelectionRequired?(
                    requestedRideID,
                    Self.selectionError(requestedID: requestedRideID, summaries: result.0)
                )
            } catch {
                guard let self,
                      self.accepts(generation: generation, isCancelled: Task.isCancelled)
                else { return }
                self.finishLoad(
                    generation: generation,
                    requestedRideID: requestedRideID,
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
                let page = try await CutoutAppModel.runCancellableDetached(priority: .userInitiated) {
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
        requestedRideID: String?,
        error: MobileRideMapError
    ) {
        guard accepts(generation: generation, isCancelled: false) else { return }
        loadTask = nil
        isLoading = false
        self.error = error
        onSelectionRequired?(nil, error)
    }

    private func accepts(generation: UInt64, isCancelled: Bool) -> Bool {
        !isCancelled && generation == queryGeneration
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
            searchText: normalizedRideMapHistorySearchText(searchText)
        )
    }

    private static func selectionError(
        requestedID: String?,
        summaries: [MobileRideMapHistorySummaryDto]
    ) -> MobileRideMapError? {
        guard let requestedID, summaries.contains(where: { $0.rideID == requestedID }) == false else {
            return nil
        }
        return .rideNotFound
    }

    private static func appendingUniqueHistory(
        existing: [MobileRideMapHistorySummaryDto],
        incoming: [MobileRideMapHistorySummaryDto]
    ) -> [MobileRideMapHistorySummaryDto] {
        var seen = Set(existing.map(\.rideID))
        return existing + incoming.filter { seen.insert($0.rideID).inserted }
    }

    private static func mergeVehicleIdentities(existing: [String], incoming: [String]) -> [String] {
        Array(Set(existing + incoming)).sorted()
    }

    private static func vehicleNames(
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

    private static func mapError(_ error: Error) -> MobileRideMapError {
        if let error = error as? MobileRideMapError { return error }
        return .storageError(String(describing: error))
    }
}
