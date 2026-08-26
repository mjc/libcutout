import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

private enum RideSessionRestorationState {
    case complete
    case awaitingBluetooth
    case awaitingSnapshot(platformIdentifier: String)
    case recovering
}

@MainActor
@Observable
final class CutoutAppModel {
    enum RideMapMode: String {
        case live
        case history
    }

    enum RideMapHistoryDateFilter: String {
        case last30Days
        case allTime
    }

    private static let rideMapPreviewPointLimit = 4_096
    private static let rideMapHistoryDisplayPointLimit: UInt32 = 16_384

    private(set) var displayState = RideDisplayState()
    private(set) var phase = SessionConnectionPhase.starting
    private(set) var devicePickerScanState: DevicePickerScanState?
    private(set) var connectionState = ConnectionState.picker
    private(set) var settingsReadback: SettingsReadback?
    private(set) var faultHistoryReadback: FaultHistoryReadback?
    private(set) var bmsSnapshot: BmsSnapshot?
    private(set) var phoneLocationReadback = PhoneLocationReadback(
        snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    )
    private(set) var rideMapSnapshot: MobileRideMapSnapshotDto?
    private(set) var rideMapStorageError: String?
    private(set) var rideMapAvailability = MobileRideMapAvailability.checking
    private(set) var rideMapLiveError: MobileRideMapError?
    private(set) var rideMapHistoryError: MobileRideMapError?
    private(set) var rideMapHistoryRouteError: MobileRideMapError?
    private(set) var rideMapHistoryDetailRouteError: MobileRideMapError?
    private(set) var rideMapPoints = [MobileRideMapPointDto]()
    private(set) var rideMapLiveDisplayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var rideMapLivePointsTruncated = false
    private(set) var rideMapHistory = [MobileRideMapHistorySummaryDto]()
    private(set) var rideMapHistoryCanLoadMore = false
    var rideMapHistorySearchText = ""
    private(set) var rideMapHistoryDateFilter = RideMapHistoryDateFilter.last30Days
    private(set) var rideMapHistoryVehicleFilter: String?
    private(set) var rideMapHistoryDisplayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var rideMapHistoryPointsTruncated = false
    private(set) var rideMapHistoryDetailDisplayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var rideMapHistoryDetailPointsTruncated = false
    private(set) var rideMapHistoryRouteLoading = false
    private(set) var rideMapHistoryDetailRouteLoading = false
    private(set) var rideMapHistoryVehicleIdentities = [String]()
    private(set) var selectedRideMapHistoryID: String?
    private(set) var rideMapLastDecision: MobileRideMapDecisionDto?
    var rideMapMode = RideMapMode.live
    private(set) var rideMapHistoryLoading = false

    /// Compatibility projection for callers that only display the live map.
    /// New route presentations should use the explicitly scoped error properties.
    var rideMapError: MobileRideMapError? { rideMapLiveError }
    private(set) var captureStatus: CaptureStatus?
    private(set) var captureProgress: CaptureProgress?
    private(set) var liveActivityError: LiveActivityRideLifecycleError?
    private(set) var isRecordOnlyCapture = false
    private(set) var isFinishingCapture = false
    private(set) var activeCaptureLabels = Set<CaptureQuickLabel>()
    private(set) var recordOnlyDeviceKind: String?
    private(set) var hasSavedDevice = false

    var selectedRideTitle: String? {
        connectionState.selection?.title
    }

    /// Stable identity/name pair used to relabel persisted ride-history vehicles.
    /// The persisted selection is the fallback when the connection state has not rebuilt yet.
    var rideMapVehicleIdentity: String? {
        connectionState.selection?.platformIdentifier ?? selectedDeviceStore.platformIdentifier
    }

    var rideMapVehicleName: String? {
        if let identity = rideMapVehicleIdentity,
           let persisted = selectedDeviceStore.displayName(for: identity) {
            return persisted
        }
        if let identity = rideMapVehicleIdentity,
           let candidate = core.protocolIdentityCandidate,
           candidate.platformIdentifier == identity,
           candidate.displayName != identity,
           !candidate.displayName.isEmpty {
            return candidate.displayName
        }
        guard let identity = rideMapVehicleIdentity else {
            return connectionState.selection?.title
        }
        return Self.meaningfulDeviceName(connectionState.selection?.title, identity: identity)
            ?? Self.meaningfulDeviceName(
                devicePickerScanState?.rows.first(where: { $0.id == identity })?.title,
                identity: identity
            )
    }

    func rideMapVehicleName(for identity: String?) -> String? {
        guard let identity else { return nil }
        return selectedDeviceStore.displayName(for: identity)
            ?? (identity == rideMapVehicleIdentity ? rideMapVehicleName : nil)
    }

    static func meaningfulDeviceName(_ candidate: String?, identity: String) -> String? {
        guard let candidate,
              !candidate.isEmpty,
              candidate != identity
        else {
            return nil
        }
        return candidate
    }

    var selectedConnectionRoute: DevicePickerConnectionRoute? {
        connectionState.selection?.route
    }

    var speed: SpeedReadout {
        displayState.speed
    }

    var currentMonotonicTime: MonotonicMilliseconds {
        core.now()
    }

    var isRideMapRecording: Bool {
        rideMapSnapshot?.state == .recording
    }

    var isRideMapPaused: Bool {
        rideMapSnapshot?.state == .paused
    }

    var rideState: EucRideScreenState {
        EucRideScreenState(phase: phase, displayState: displayState)
    }

    var eucRidePresentationState: EucRideScreenState? {
        guard selectedRideTitle != nil || phase != .starting || displayState.notificationCount != 0 else {
            return nil
        }
        return rideState
    }

    var vescRideSnapshot: VescRideSnapshot? {
        VescRideSnapshot(displayState: displayState, title: selectedRideTitle)
    }

    var captureStatusText: String? {
        captureStatus?.displayText
    }

    var connectionStatusText: String {
        connectionState.statusText ?? phase.displayText
    }

    private let core: any CutoutSessionDriving
    private let liveActivityCoordinator: LiveActivityRideLifecycleCoordinator
    private let selectedDeviceStore: DevicePickerSelectionStore
    private let rideSessionMarkerStore: RideSessionMarkerStore
    private var liveActivityIdentity: LiveActivityRideIdentity?
    private var liveActivityGlyph = LiveActivityRideGlyph.electricUnicycle
    private var lastLiveActivitySnapshot: LiveActivityRideSnapshot?
    private var lastLiveActivityUpdate: MonotonicMilliseconds?
    private var liveActivityRequestID: UInt64 = 0
    private var captureFileName: String?
    private var captureNotificationCount = 0
    private var captureLabel: String?
    private var hasStarted = false
    private var permitsStoredDeviceAutoPairing = true
    private var rideSessionRestorationState = RideSessionRestorationState.complete
    private var restorationMarkerAtLaunch: Data?
    private var rideMapHistoryCursor: MobileRideCursorDto?
    private var rideMapHistoryQueryDateAfterMilliseconds: UInt64?
    private var rideMapHistoryLoadTask: Task<Void, Never>?
    private var rideMapHistoryPageTask: Task<Void, Never>?
    private var rideMapHistorySelectionTask: Task<Void, Never>?
    private var rideMapHistorySelectionCancellation: MobileRideMapProjectionCancellation?
    private var rideMapHistoryViewportTask: Task<Void, Never>?
    private var rideMapHistoryViewportCancellation: MobileRideMapProjectionCancellation?
    private var rideMapRestoreTask: Task<Void, Never>?
    private var rideMapLiveProjectionTask: Task<Void, Never>?
    private var rideMapLiveProjectionCancellation: MobileLiveRideMapProjectionCancellation?
    private var rideMapLiveProjectionGeneration: UInt64 = 0
    private var rideMapLiveProjectionEnabled = false
    private static let liveActivityUpdateIntervalMilliseconds: UInt64 = 1_000

    convenience init() {
        #if DEBUG
        let permitsStoredDeviceAutoPairing = Self.uiTestFixture == nil
        #else
        let permitsStoredDeviceAutoPairing = true
        #endif
        self.init(
            core: Self.makeSessionDriver(),
            permitsStoredDeviceAutoPairing: permitsStoredDeviceAutoPairing,
            selectedDeviceStore: DevicePickerSelectionStore(),
            rideSessionMarkerStore: RideSessionMarkerStore(),
            liveActivityManager: LiveActivityRideActivityKitManager()
        )
    }

    convenience init(
        core: any CutoutSessionDriving,
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore(),
        rideSessionMarkerStore: RideSessionMarkerStore = RideSessionMarkerStore(),
        liveActivityManager: any LiveActivityRideLifecycleManaging = LiveActivityRideActivityKitManager()
    ) {
        self.init(
            core: core,
            permitsStoredDeviceAutoPairing: true,
            selectedDeviceStore: selectedDeviceStore,
            rideSessionMarkerStore: rideSessionMarkerStore,
            liveActivityManager: liveActivityManager
        )
    }

    private init(
        core: any CutoutSessionDriving,
        permitsStoredDeviceAutoPairing: Bool,
        selectedDeviceStore: DevicePickerSelectionStore,
        rideSessionMarkerStore: RideSessionMarkerStore,
        liveActivityManager: any LiveActivityRideLifecycleManaging
    ) {
        self.permitsStoredDeviceAutoPairing = permitsStoredDeviceAutoPairing
        self.core = core
        rideMapStorageError = core.rideMapStorageError
        rideMapAvailability = core.rideMapAvailability
        liveActivityCoordinator = LiveActivityRideLifecycleCoordinator(
            manager: liveActivityManager,
            sessionState: core.rideSessionStateHandle,
            markerStore: rideSessionMarkerStore
        )
        self.selectedDeviceStore = selectedDeviceStore
        self.rideSessionMarkerStore = rideSessionMarkerStore
        hasSavedDevice = selectedDeviceStore.platformIdentifier != nil
        restoreRideMapState()
        self.core.onDisplayStateChange = { [weak self] displayState in
            self?.displayState = displayState
            self?.syncLiveActivity()
        }
        self.core.onPhaseChange = { [weak self] phase in
            self?.handlePhaseChange(phase)
            self?.syncLiveActivity()
        }
        self.core.onReconnectScheduled = { [weak self] retry in
            self?.handleReconnectScheduled(retry)
        }
        self.core.onScanStateChange = { [weak self] scanState in
            self?.handleScanStateChange(scanState)
        }
        self.core.onSettingsReadbackChange = { [weak self] settingsReadback in
            self?.settingsReadback = settingsReadback
        }
        self.core.onFaultHistoryReadbackChange = { [weak self] faultHistoryReadback in
            self?.faultHistoryReadback = faultHistoryReadback
        }
        self.core.onBmsSnapshotChange = { [weak self] bmsSnapshot in
            self?.bmsSnapshot = bmsSnapshot
        }
        self.core.onPhoneLocationSnapshotChange = { [weak self] snapshot, receivedAt in
            self?.phoneLocationReadback = PhoneLocationReadback(snapshot: snapshot, receivedAt: receivedAt)
        }
        self.core.onRideMapDecisionChange = { [weak self] snapshot, decision in
            self?.applyRideMapDecision(snapshot: snapshot, decision: decision)
        }
        self.core.onRideMapSnapshotChange = { [weak self] snapshot in
            self?.rideMapSnapshot = snapshot
        }
        self.core.onRideMapErrorChange = { [weak self] error in
            self?.rideMapLiveError = error
        }
        self.core.onRideMapAvailabilityChange = { [weak self] availability in
            self?.rideMapAvailability = availability
        }
        self.core.onProtocolIdentityCandidateChange = { [weak self] candidate in
            self?.applyProtocolIdentityCandidate(candidate)
        }
        self.core.onBluetoothRestorationResolved = { [weak self] platformIdentifier in
            self?.handleBluetoothRestorationResolved(platformIdentifier)
        }
        self.core.onCaptureEvent = { [weak self] event in
            self?.applyCaptureEvent(event)
        }
    }

    private func restoreRideMapState() {
        let state = core.rideMapStateHandle
        rideMapSnapshot = state.currentSnapshot()
        guard rideMapSnapshot != nil else { return }
        rideMapRestoreTask?.cancel()
        let previewLimit = Self.rideMapPreviewPointLimit
        rideMapRestoreTask = Task { [weak self] in
            do {
                let result = try await Task.detached(priority: .userInitiated) {
                    try Self.collectRideMapActiveTail(
                        state: state,
                        previewLimit: previewLimit
                    )
                }.value
                guard !Task.isCancelled, let self else { return }
                guard let result else { return }
                self.rideMapPoints = Array(result.0.suffix(previewLimit))
                self.applyLiveProjection(result.1)
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.rideMapLiveError = Self.mapRideMapError(error)
                self.rideMapPoints = []
                self.rideMapLiveDisplayPoints = []
            }
        }
    }

    func start() {
        guard hasStarted == false else { return }
        hasStarted = true
        permitsStoredDeviceAutoPairing = false
        restorationMarkerAtLaunch = rideSessionMarkerStore.marker
        rideSessionRestorationState = .awaitingBluetooth
        core.start()
    }

    @discardableResult
    func startGpsOnlyRide() -> Bool {
        applyRideMapCommand(resetPoints: true) {
            try core.rideMapStateHandle.startGpsOnly(
                atMs: currentMonotonicTime.rawValue,
                lastConnectedVehicle: selectedDeviceStore.platformIdentifier
            )
        }
    }

    @discardableResult
    func pauseRideMap() -> Bool {
        applyRideMapCommand {
            try core.rideMapStateHandle.pause(atMs: currentMonotonicTime.rawValue)
        }
    }

    @discardableResult
    func resumeRideMap() -> Bool {
        applyRideMapCommand {
            try core.rideMapStateHandle.resume(atMs: currentMonotonicTime.rawValue)
        }
    }

    @discardableResult
    func stopRideMap() -> Bool {
        applyRideMapCommand {
            try core.rideMapStateHandle.stop(atMs: currentMonotonicTime.rawValue)
        }
    }

    func refreshRideMapDuration() {
        guard let snapshot = core.rideMapStateHandle.currentSnapshot(atMs: currentMonotonicTime.rawValue),
              snapshot.state == .recording
        else {
            return
        }
        rideMapSnapshot = snapshot
    }

    @discardableResult
    func saveRideMap() -> Bool {
        guard applyRideMapCommand({ try core.rideMapStateHandle.save() }) else {
            return false
        }
        loadRideMapHistory()
        return true
    }

    @discardableResult
    func discardRideMap() -> Bool {
        guard applyRideMapCommand({ try core.rideMapStateHandle.discard() }) else {
            return false
        }
        rideMapHistoryDisplayPoints = []
        rideMapHistoryPointsTruncated = false
        rideMapHistoryDetailDisplayPoints = []
        rideMapHistoryDetailPointsTruncated = false
        rideMapHistoryRouteLoading = false
        rideMapHistoryDetailRouteError = nil
        rideMapHistoryDetailRouteLoading = false
        loadRideMapHistory()
        return true
    }

    func loadRideMapHistory(selecting requestedRideID: String? = nil) {
        rideMapHistoryLoadTask?.cancel()
        rideMapHistoryPageTask?.cancel()
        // A reload changes the query that owns the selected route. Do not let an
        // older route request repopulate points after the new page arrives.
        rideMapHistorySelectionTask?.cancel()
        rideMapHistorySelectionCancellation?.cancel()
        rideMapHistoryViewportCancellation?.cancel()
        rideMapHistoryViewportTask?.cancel()
        rideMapHistoryError = nil
        rideMapHistoryRouteError = nil
        rideMapHistoryDetailRouteError = nil
        rideMapHistoryLoading = true
        rideMapHistoryRouteLoading = false
        rideMapHistoryDetailRouteLoading = false
        rideMapHistoryQueryDateAfterMilliseconds = historyDateAfterMilliseconds
        if let rideMapStorageError {
            rideMapHistoryLoading = false
            rideMapHistoryError = .Storage(rideMapStorageError)
            return
        }
        let state = core.rideMapStateHandle
        let filter = rideMapHistoryFilter
        let existingSelectedID = selectedRideMapHistoryID
        rideMapHistoryLoadTask = Task { [weak self] in
            do {
                let result = try await Task.detached(priority: .userInitiated) {
                    let page = try state.storedHistoryPage(cursor: nil, limit: 50, filter: filter)
                    var summaries = page.summaries
                    if let requestedRideID,
                       summaries.contains(where: { $0.rideId == requestedRideID }) == false,
                       let requestedRide = try state.storedHistoryRide(rideID: requestedRideID)
                    {
                        summaries.append(requestedRide)
                    }
                    return (summaries, page.nextCursor)
                }.value
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryLoading = false
                self.rideMapHistory = result.0
                self.rideMapHistoryVehicleIdentities = Self.mergeRideMapHistoryVehicleIdentities(
                    existing: self.rideMapHistoryVehicleIdentities,
                    incoming: result.0.flatMap { [$0.associatedVehicle, $0.candidateVehicle].compactMap { $0 } }
                )
                self.rideMapHistoryCursor = result.1
                self.rideMapHistoryCanLoadMore = result.1 != nil
                self.rideMapHistoryError = nil
                let selectedID = Self.preferredHistorySelection(
                    requestedID: requestedRideID,
                    currentID: existingSelectedID,
                    summaries: result.0
                )
                guard let selectedID else {
                    self.selectedRideMapHistoryID = nil
                    self.rideMapHistoryDisplayPoints = []
                    self.rideMapHistoryPointsTruncated = false
                    self.rideMapHistoryDetailDisplayPoints = []
                    self.rideMapHistoryDetailPointsTruncated = false
                    self.rideMapHistoryRouteLoading = false
                    self.rideMapHistoryDetailRouteError = nil
                    self.rideMapHistoryDetailRouteLoading = false
                    return
                }
                self.selectRideMapHistory(selectedID)
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryLoading = false
                // Preserve the last good page so a transient storage failure does not
                // turn an otherwise usable history screen into an empty state.
                self.rideMapHistoryError = Self.mapRideMapError(error)
                self.rideMapHistoryRouteLoading = false
                self.rideMapHistoryDetailRouteLoading = false
            }
        }
    }

    @MainActor
    static func preferredHistorySelection(
        requestedID: String?,
        currentID: String?,
        summaries: [MobileRideMapHistorySummaryDto]
    ) -> String? {
        preferredHistorySelection(
            requestedID: requestedID,
            currentID: currentID,
            summaryIDs: summaries.map(\.rideId)
        )
    }

    @MainActor
    static func preferredHistorySelection(
        requestedID: String?,
        currentID: String?,
        summaryIDs: [String]
    ) -> String? {
        if let requestedID {
            return summaryIDs.first(where: { $0 == requestedID })
        }
        return summaryIDs.first(where: { $0 == currentID }) ?? summaryIDs.first
    }

    @MainActor
    static func appendingUniqueHistory<T>(
        existing: [T],
        incoming: [T],
        id: (T) -> String
    ) -> [T] {
        var seen = Set(existing.map(id))
        return existing + incoming.filter { seen.insert(id($0)).inserted }
    }

    @MainActor
    static func mergeRideMapHistoryVehicleIdentities(
        existing: [String],
        incoming: [String]
    ) -> [String] {
        Array(Set(existing + incoming)).sorted()
    }

    func loadMoreRideMapHistory() {
        guard rideMapHistoryCanLoadMore else { return }
        rideMapHistoryPageTask?.cancel()
        let state = core.rideMapStateHandle
        let cursor = rideMapHistoryCursor
        let filter = rideMapHistoryFilter
        rideMapHistoryPageTask = Task { [weak self] in
            do {
                let page = try await Task.detached(priority: .userInitiated) {
                    try state.storedHistoryPage(cursor: cursor, limit: 50, filter: filter)
                }.value
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistory = Self.appendingUniqueHistory(
                    existing: self.rideMapHistory,
                    incoming: page.summaries,
                    id: \.rideId
                )
                self.rideMapHistoryVehicleIdentities = Self.mergeRideMapHistoryVehicleIdentities(
                    existing: self.rideMapHistoryVehicleIdentities,
                    incoming: page.summaries.flatMap { [$0.associatedVehicle, $0.candidateVehicle].compactMap { $0 } }
                )
                self.rideMapHistoryCursor = page.nextCursor
                self.rideMapHistoryCanLoadMore = page.nextCursor != nil
                self.rideMapHistoryError = nil
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryError = Self.mapRideMapError(error)
            }
        }
    }

    var filteredRideMapHistory: [MobileRideMapHistorySummaryDto] {
        rideMapHistory
    }

    func setRideMapHistoryDateFilter(_ filter: RideMapHistoryDateFilter) {
        guard rideMapHistoryDateFilter != filter else { return }
        rideMapHistoryDateFilter = filter
        loadRideMapHistory()
    }

    func setRideMapHistoryVehicleFilter(_ identity: String?) {
        guard rideMapHistoryVehicleFilter != identity else { return }
        rideMapHistoryVehicleFilter = identity
        loadRideMapHistory()
    }

    func clearRideMapHistoryFilters() {
        rideMapHistorySearchText = ""
        rideMapHistoryDateFilter = .last30Days
        rideMapHistoryVehicleFilter = nil
        loadRideMapHistory()
    }

    private var historyDateAfterMilliseconds: UInt64? {
        guard rideMapHistoryDateFilter == .last30Days else { return nil }
        let milliseconds = Date().addingTimeInterval(-30 * 24 * 60 * 60).timeIntervalSince1970 * 1_000
        return milliseconds.isFinite && milliseconds > 0 ? UInt64(milliseconds) : 0
    }

    private var rideMapHistoryFilter: MobileRideHistoryFilterDto {
        MobileRideHistoryFilterDto(
            createdAfterMilliseconds: rideMapHistoryQueryDateAfterMilliseconds ?? historyDateAfterMilliseconds,
            vehicleIdentity: rideMapHistoryVehicleFilter,
            searchText: RideMapHistoryContentView.normalizedSearchText(rideMapHistorySearchText)
        )
    }

    func selectRideMapHistory(_ rideID: String) {
        selectRideMapHistory(rideID, previewLimit: Self.rideMapPreviewPointLimit)
    }

    func projectRideMapHistoryDetailViewport(_ viewport: MobileGeoBoundsDto?) {
        guard let viewport,
              let selectedRideMapHistoryID,
              rideMapHistory.contains(where: { $0.rideId == selectedRideMapHistoryID })
        else {
            return
        }
        rideMapHistoryViewportCancellation?.cancel()
        rideMapHistoryViewportTask?.cancel()
        rideMapHistoryDetailRouteError = nil
        rideMapHistoryDetailRouteLoading = true
        let cancellation = MobileRideMapProjectionCancellation()
        rideMapHistoryViewportCancellation = cancellation
        let state = core.rideMapStateHandle
        let budget = Self.rideMapHistoryDisplayPointLimit
        rideMapHistoryViewportTask = Task { [weak self] in
            do {
                let result = try await withTaskCancellationHandler(operation: {
                    try await Task.detached(priority: .userInitiated) {
                        try state.projectStoredPoints(
                            rideID: selectedRideMapHistoryID,
                            budget: budget,
                            viewport: viewport,
                            cancellation: cancellation
                        )
                    }.value
                }, onCancel: {
                    cancellation.cancel()
                })
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryDetailDisplayPoints = result.points
                self.rideMapHistoryDetailPointsTruncated = result.sourcePointCount
                    > UInt64(result.points.count)
                self.rideMapHistoryDetailRouteError = nil
                self.rideMapHistoryDetailRouteLoading = false
            } catch {
                guard !Task.isCancelled, let self else { return }
                let mappedError = Self.mapRideMapError(error)
                if mappedError == .cancelled {
                    return
                }
                self.rideMapHistoryDetailRouteError = mappedError
                self.rideMapHistoryDetailRouteLoading = false
            }
        }
    }

    func loadFullRideMapHistory() {
        guard let selectedRideMapHistoryID else { return }
        selectRideMapHistory(selectedRideMapHistoryID, previewLimit: nil)
    }

    private func selectRideMapHistory(_ rideID: String, previewLimit: Int?) {
        guard rideMapHistory.contains(where: { $0.rideId == rideID }) else {
            rideMapHistoryRouteLoading = false
            rideMapHistoryRouteError = nil
            rideMapHistoryDetailRouteLoading = false
            rideMapHistoryDetailRouteError = nil
            return
        }
        rideMapHistorySelectionTask?.cancel()
        rideMapHistorySelectionCancellation?.cancel()
        rideMapHistoryViewportCancellation?.cancel()
        rideMapHistoryViewportTask?.cancel()
        let selectingDifferentRide = selectedRideMapHistoryID != rideID
        if selectingDifferentRide {
            rideMapHistoryDisplayPoints = []
            rideMapHistoryPointsTruncated = false
            rideMapHistoryDetailDisplayPoints = []
            rideMapHistoryDetailPointsTruncated = false
        }
        selectedRideMapHistoryID = rideID
        rideMapHistoryRouteError = nil
        rideMapHistoryDetailRouteError = nil
        rideMapHistoryRouteLoading = true
        rideMapHistoryDetailRouteLoading = true
        let state = core.rideMapStateHandle
        let cancellation = MobileRideMapProjectionCancellation()
        rideMapHistorySelectionCancellation = cancellation
        let budget = UInt32(
            min(
                previewLimit ?? Int(Self.rideMapHistoryDisplayPointLimit),
                Int(Self.rideMapHistoryDisplayPointLimit)
            )
        )
        rideMapHistorySelectionTask = Task { [weak self] in
            do {
                let result = try await withTaskCancellationHandler(operation: {
                    try await Task.detached(priority: .userInitiated) {
                        try state.projectStoredPoints(
                            rideID: rideID,
                            budget: budget,
                            cancellation: cancellation
                        )
                    }.value
                }, onCancel: {
                    cancellation.cancel()
                })
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryRouteError = nil
                self.rideMapHistoryDetailRouteError = nil
                self.rideMapHistoryDisplayPoints = result.points
                self.rideMapHistoryPointsTruncated = result.sourcePointCount
                    > UInt64(result.points.count)
                self.rideMapHistoryDetailDisplayPoints = result.points
                self.rideMapHistoryDetailPointsTruncated = result.sourcePointCount
                    > UInt64(result.points.count)
                self.rideMapHistoryRouteLoading = false
                self.rideMapHistoryDetailRouteLoading = false
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryRouteError = Self.mapRideMapError(error)
                self.rideMapHistoryRouteLoading = false
                self.rideMapHistoryDetailRouteError = self.rideMapHistoryRouteError
                self.rideMapHistoryDetailRouteLoading = false
            }
        }
    }

    private static func mapRideMapError(_ error: Error) -> MobileRideMapError {
        if let error = error as? MobileRideMapError {
            return error
        }
        return .Storage(String(describing: error))
    }

    private nonisolated static func collectRideMapActiveTail(
        state: MobileRideMapState,
        previewLimit: Int
    ) throws -> ([MobileRideMapPointDto], MobileRideMapRouteProjection)? {
        guard let tail = try state.latestRoutePoints() else {
            return nil
        }
        let projection = try state.projectPoints(budget: UInt32(previewLimit))
        return (tail.points, projection)
    }

    /// Keeps the Swift route-truth input bounded to the Rust live-route preview tail.
    static func appendBoundedRideMapPoint(
        _ point: MobileRideMapPointDto,
        to points: inout [MobileRideMapPointDto],
        limit: Int
    ) {
        points.append(point)
        if points.count > limit {
            points.removeFirst(points.count - limit)
        }
    }

    static func shouldApplyLiveProjection(
        generation: UInt64,
        currentGeneration: UInt64,
        enabled: Bool
    ) -> Bool {
        enabled && generation == currentGeneration
    }

    private func applyRideMapDecision(
        snapshot: MobileRideMapSnapshotDto,
        decision: MobileRideMapDecisionDto
    ) {
        rideMapLiveError = nil
        rideMapSnapshot = snapshot
        rideMapLastDecision = decision
        if case let .accepted(point, _) = decision {
            Self.appendBoundedRideMapPoint(
                point,
                to: &rideMapPoints,
                limit: Self.rideMapPreviewPointLimit
            )
            requestLiveProjection()
        }
    }

    /// Serializes live projections while allowing a burst of accepted points to coalesce.
    ///
    /// The Rust projection snapshots the recorder before doing its work, and the detached
    /// operation receives a live-only cancellation token. A generation change cancels the
    /// in-flight operation; the task remains alive until that operation returns so projections
    /// never overlap on the same map core.
    private func requestLiveProjection() {
        rideMapLiveProjectionGeneration &+= 1
        rideMapLiveProjectionEnabled = true
        rideMapLiveProjectionCancellation?.cancel()
        guard rideMapLiveProjectionTask == nil else { return }

        let state = core.rideMapStateHandle
        let budget = UInt32(Self.rideMapPreviewPointLimit)
        rideMapLiveProjectionTask = Task { [weak self] in
            while let self {
                let generation = self.rideMapLiveProjectionGeneration
                let cancellation = MobileLiveRideMapProjectionCancellation()
                self.rideMapLiveProjectionCancellation = cancellation
                do {
                    let projection = try await Task.detached(priority: .userInitiated) {
                        try state.projectPoints(budget: budget, cancellation: cancellation)
                    }.value
                    guard self.rideMapLiveProjectionEnabled else {
                        self.rideMapLiveProjectionTask = nil
                        self.rideMapLiveProjectionCancellation = nil
                        return
                    }
                    guard Self.shouldApplyLiveProjection(
                        generation: generation,
                        currentGeneration: self.rideMapLiveProjectionGeneration,
                        enabled: self.rideMapLiveProjectionEnabled
                    ) else {
                        continue
                    }
                    self.applyLiveProjection(projection)
                } catch {
                    guard self.rideMapLiveProjectionEnabled else {
                        self.rideMapLiveProjectionTask = nil
                        self.rideMapLiveProjectionCancellation = nil
                        return
                    }
                    guard Self.shouldApplyLiveProjection(
                        generation: generation,
                        currentGeneration: self.rideMapLiveProjectionGeneration,
                        enabled: self.rideMapLiveProjectionEnabled
                    ) else {
                        continue
                    }
                    self.rideMapLiveError = Self.mapRideMapError(error)
                    self.rideMapLiveDisplayPoints = []
                }
                self.rideMapLiveProjectionTask = nil
                self.rideMapLiveProjectionCancellation = nil
                return
            }
        }
    }

    private func applyLiveProjection(_ projection: MobileRideMapRouteProjection) {
        rideMapLiveDisplayPoints = projection.points
        rideMapLivePointsTruncated = projection.sourcePointCount > UInt64(projection.points.count)
    }

    private func applyRideMapCommand(
        resetPoints: Bool = false,
        _ command: () throws -> MobileRideMapSnapshotDto
    ) -> Bool {
        do {
            rideMapSnapshot = try command()
            rideMapLiveError = nil
            if resetPoints {
                rideMapLiveProjectionGeneration &+= 1
                rideMapLiveProjectionEnabled = false
                rideMapLiveProjectionCancellation?.cancel()
                rideMapPoints.removeAll(keepingCapacity: true)
                rideMapLiveDisplayPoints.removeAll(keepingCapacity: true)
                rideMapLivePointsTruncated = false
                rideMapLastDecision = nil
            }
            return true
        } catch {
            rideMapLiveError = Self.mapRideMapError(error)
            return false
        }
    }

    func pair(platformIdentifier: String) -> Bool {
        switch connectionState {
        case .connecting, .retrying, .connected:
            let isSameSelection = connectionState.selection?.platformIdentifier == platformIdentifier
            guard !isSameSelection || liveActivityError != nil else { return false }
        case .picker, .identified, .failed:
            break
        }
        let rows = devicePickerScanState?.rows ?? []
        guard let selectedRow = rows.first(where: { $0.id == platformIdentifier }) else {
            phase = .scanning
            devicePickerScanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
            return false
        }
        guard selectedRow.isSupported, let route = selectedRow.connectionRoute else { return false }

        let selection = ConnectionSelection(
            platformIdentifier: selectedRow.id,
            title: selectedRow.title,
            route: route
        )
        liveActivityError = nil
        connectionState = .connecting(selection, phase: .discoveringServices)
        permitsStoredDeviceAutoPairing = true
        phase = .discoveringServices
        let didPair = core.pair(platformIdentifier: platformIdentifier)
        if didPair {
            isRecordOnlyCapture = false
            captureLabel = nil
            recordOnlyDeviceKind = nil
            selectedDeviceStore.save(
                platformIdentifier: platformIdentifier,
                displayName: selectedRow.title
            )
            hasSavedDevice = true
            liveActivityIdentity = liveActivityIdentity(for: selectedRow)
            liveActivityGlyph = liveActivityGlyph(for: selectedRow)
            syncLiveActivity()
        } else {
            connectionState = .picker
            permitsStoredDeviceAutoPairing = false
            phase = .scanning
            devicePickerScanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
        }
        return didPair
    }

    func recordOnly(platformIdentifier: String, deviceKind: String) -> Bool {
        connectionState = .picker
        let trimmedKind = deviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedKind.isEmpty else { return false }
        let annotationKind = trimmedKind
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: "=", with: " ")
        let annotations = ["device_kind=\(annotationKind)"]
        let modelHint = CutoutModelHint(deviceKind: annotationKind)
        let previousCapture = (
            status: captureStatus,
            progress: captureProgress,
            isFinishing: isFinishingCapture,
            activeLabels: activeCaptureLabels,
            fileName: captureFileName,
            notificationCount: captureNotificationCount,
            label: captureLabel,
            deviceKind: recordOnlyDeviceKind
        )
        resetCaptureSession()
        let didStart = switch modelHint {
        case .falcon:
            core.pair(platformIdentifier: platformIdentifier, model: .falcon)
        case .aero:
            core.pair(platformIdentifier: platformIdentifier, model: .aero)
        case .unknown:
            core.recordOnly(
                platformIdentifier: platformIdentifier,
                note: "unsupported picker row",
                annotations: annotations
            )
        }
        guard didStart else {
            captureStatus = previousCapture.status
            captureProgress = previousCapture.progress
            isFinishingCapture = previousCapture.isFinishing
            activeCaptureLabels = previousCapture.activeLabels
            captureFileName = previousCapture.fileName
            captureNotificationCount = previousCapture.notificationCount
            captureLabel = previousCapture.label
            recordOnlyDeviceKind = previousCapture.deviceKind
            return false
        }

        permitsStoredDeviceAutoPairing = false
        if modelHint != .unknown {
            core.annotateCapture(key: "device_kind", value: annotationKind)
        }
        isRecordOnlyCapture = modelHint == .unknown
        recordOnlyDeviceKind = annotationKind
        if modelHint == .unknown {
            liveActivityIdentity = nil
            liveActivityGlyph = .electricUnicycle
        }
        syncLiveActivity()
        return true
    }

    func startProbe(platformIdentifier: String) -> Bool {
        let rows = devicePickerScanState?.rows ?? []
        guard let selectedRow = rows.first(where: { $0.id == platformIdentifier }),
              selectedRow.isProbeRecommended
        else {
            return false
        }
        let selection = ConnectionSelection(
            platformIdentifier: selectedRow.id,
            title: selectedRow.title,
            route: .electricUnicycle
        )
        connectionState = .connecting(selection, phase: .discoveringServices)
        phase = .discoveringServices
        let didStart = core.probe(platformIdentifier: platformIdentifier)
        if !didStart {
            connectionState = .picker
            phase = .scanning
            devicePickerScanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
        }
        return didStart
    }

    private func resetCaptureSession() {
        captureStatus = nil
        captureProgress = nil
        isFinishingCapture = false
        activeCaptureLabels.removeAll()
        captureFileName = nil
        captureNotificationCount = 0
        captureLabel = nil
        recordOnlyDeviceKind = nil
    }

    func startCaptureLabel(_ label: CaptureQuickLabel) {
        guard !activeCaptureLabels.contains(label) else { return }

        if let activeLabel = activeCaptureLabels.first(where: label.isMutuallyExclusive)
        {
            activeCaptureLabels.remove(activeLabel)
            core.annotateCapture(label: "\(activeLabel.annotationValue)_stop")
        }

        activeCaptureLabels.insert(label)
        captureLabel = label.title
        core.annotateCapture(label: "\(label.annotationValue)_start")
        captureStatus = .labelStarted(
            label: label.title,
            notificationCount: captureNotificationCount,
            fileName: captureFileName
        )
    }

    @discardableResult
    func flushCapture() async -> Bool {
        let didFlush = await core.flushCapture()
        if !didFlush, captureStatus?.isRecording == true {
            captureStatus = .failed
        }
        return didFlush
    }

    func appDidEnterBackground() {
        guard let snapshot = currentLiveActivitySnapshot() else {
            guard isRecordOnlyCapture else { return }
            Task { [weak self] in _ = await self?.flushCapture() }
            return
        }
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.appDidEnterBackground(
                requestID: requestID,
                snapshot: snapshot,
                captureFlush: { [weak self] in
                    await self?.flushCapture() ?? false
                }
            )
            self?.liveActivityError = await liveActivityCoordinator.lastError
        }
    }

    func appDidBecomeActive() {
        guard let snapshot = currentLiveActivitySnapshot() else { return }
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.appDidBecomeActive(
                requestID: requestID,
                snapshot: snapshot
            )
            self?.liveActivityError = await liveActivityCoordinator.lastError
        }
    }

    @discardableResult
    func finishCapture() async -> Bool {
        guard !isFinishingCapture else { return false }
        isFinishingCapture = true

        guard await flushCapture() else {
            captureStatus = .failed
            isFinishingCapture = false
            return false
        }

        disconnectTransport()
        return true
    }

    func stopCaptureLabel(_ label: CaptureQuickLabel) {
        guard activeCaptureLabels.remove(label) != nil else { return }
        captureLabel = label.title
        core.annotateCapture(label: "\(label.annotationValue)_stop")
        captureStatus = .labelStopped(
            label: label.title,
            notificationCount: captureNotificationCount,
            fileName: captureFileName
        )
    }

    func disconnectTransport() {
        endLiveActivity(reason: .disconnected)
        isRecordOnlyCapture = false
        activeCaptureLabels.removeAll()
        captureLabel = nil
        recordOnlyDeviceKind = nil
        connectionState = .picker
        phase = .scanning
        liveActivityIdentity = nil
        liveActivityGlyph = .electricUnicycle
        permitsStoredDeviceAutoPairing = false
        core.disconnectAndScan()
    }

    func forgetSavedDevice() {
        disconnectTransport()
        try? selectedDeviceStore.clear()
        hasSavedDevice = false
    }

    func endLiveActivity(reason: LiveActivityRideLifecycleEndReason = .sessionEnded) {
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.end(requestID: requestID, reason: reason)
            self?.liveActivityError = await liveActivityCoordinator.lastError
        }
        lastLiveActivitySnapshot = nil
        lastLiveActivityUpdate = nil
    }

    func applyProtocolIdentityCandidate(_ candidate: DevicePickerDiscoveryCandidate?) {
        guard isRecordOnlyCapture != true else {
            liveActivityIdentity = nil
            liveActivityGlyph = .electricUnicycle
            syncLiveActivity()
            return
        }
        if let candidate,
           let displayName = Self.meaningfulDeviceName(
               candidate.displayName,
               identity: candidate.platformIdentifier
           ) {
            // Persist every resolved identity, not only the currently selected one. History can
            // contain rides from an older CoreBluetooth identifier and must still be relabelable.
            selectedDeviceStore.save(
                platformIdentifier: candidate.platformIdentifier,
                displayName: displayName
            )
        }
        if let model = candidate?.support.electricUnicycleModel {
            liveActivityIdentity = .model(model)
            liveActivityGlyph = .electricUnicycle
        }
        guard let selection = selection(from: candidate) else {
            syncLiveActivity()
            return
        }
        if connectionState.selection == nil {
            connectionState = .identified(selection)
        }
        if selection.route == .vescOnewheel {
            liveActivityIdentity = vescRideIdentity(using: selection.title)
            liveActivityGlyph = .floatwheelAtom
        }
        syncLiveActivity()
    }

    private func handleScanStateChange(_ scanState: DevicePickerScanState) {
        devicePickerScanState = scanState
        guard phase == .starting || phase == .scanning else { return }
        switch rideSessionRestorationState {
        case .complete:
            break
        case let .awaitingSnapshot(platformIdentifier):
            guard selectedDeviceStore.platformIdentifier == platformIdentifier else { return }
        case .awaitingBluetooth, .recovering:
            return
        }
        guard permitsStoredDeviceAutoPairing else { return }
        guard let platformIdentifier = selectedDeviceStore.platformIdentifier else { return }
        guard scanState.storedSupportedRow(platformIdentifier: platformIdentifier) != nil else { return }
        _ = pair(platformIdentifier: platformIdentifier)
    }

    private func handlePhaseChange(_ phase: SessionConnectionPhase) {
        switch (phase, connectionState) {
        case (.starting, .identified), (.starting, .connecting), (.starting, .retrying), (.starting, .connected),
             (.scanning, .identified), (.scanning, .connecting), (.scanning, .retrying), (.scanning, .connected):
            return
        default:
            break
        }
        guard !phase.supportsLiveActivity || connectionState.selection != nil || permitsStoredDeviceAutoPairing else { return }
        if case .live = phase {
            switch connectionState {
            case .connecting(_, phase: .subscribing), .identified:
                break
            default:
                return
            }
        }
        if case .failed = phase {
            guard connectionState.selection != nil else { return }
        }
        if case .failed = connectionState {
            switch phase {
            case .connecting, .discoveringServices, .subscribing, .live:
                return
            case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .failed:
                break
            }
        }
        self.phase = phase
        switch phase {
        case .connecting, .discoveringServices, .subscribing:
            if let selection = connectionState.selection {
                connectionState = .connecting(selection, phase: phase)
            }
        case .live:
            if let selection = connectionState.selection {
                connectionState = .connected(selection)
            } else if let selection = selection(from: core.protocolIdentityCandidate) {
                connectionState = .connected(selection)
            }
        case let .failed(failure):
            guard let selection = connectionState.selection else { return }
            connectionState = .failed(selection, failure)
            let rows = devicePickerScanState?.rows ?? []
            devicePickerScanState = .failed(phase.displayText, rows: rows)
        case .bluetoothPermissionDenied, .bluetoothUnavailable:
            connectionState = .picker
        case .starting, .scanning:
            break
        }
    }

    private func handleReconnectScheduled(_ retry: SessionConnectionRetry) {
        guard let selection = connectionState.selection,
              selection.platformIdentifier == retry.platformIdentifier
        else { return }
        if case .failed = connectionState { return }
        connectionState = .retrying(selection, retry: retry)
    }

    private func handleBluetoothRestorationResolved(_ platformIdentifier: String?) {
        guard case .awaitingBluetooth = rideSessionRestorationState else { return }
        let marker = restorationMarkerAtLaunch
        guard platformIdentifier != nil || marker != nil else {
            permitsStoredDeviceAutoPairing = true
            rideSessionRestorationState = .complete
            if let scanState = devicePickerScanState {
                handleScanStateChange(scanState)
            }
            return
        }
        if platformIdentifier != nil, marker == nil {
            permitsStoredDeviceAutoPairing = false
            rideSessionRestorationState = .complete
            return
        }
        if let platformIdentifier, let marker {
            let markerMatches = (try? core.rideSessionStateHandle
                .rideSessionMarkerMatchesPlatformIdentifier(
                    marker: marker,
                    platformIdentifier: platformIdentifier
                )) == true
            permitsStoredDeviceAutoPairing = markerMatches
            if !markerMatches {
                beginRideSessionRecovery(
                    restoredPlatformIdentifier: platformIdentifier,
                    snapshot: nil
                )
                return
            }
        }
        guard let platformIdentifier else {
            beginRideSessionRecovery(restoredPlatformIdentifier: nil, snapshot: nil)
            return
        }
        rideSessionRestorationState = .awaitingSnapshot(platformIdentifier: platformIdentifier)
        syncLiveActivity()
    }

    private func beginRideSessionRecovery(
        restoredPlatformIdentifier: String?,
        snapshot: LiveActivityRideSnapshot?
    ) {
        rideSessionRestorationState = .recovering
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        Task { [weak self, liveActivityCoordinator] in
            let recoveryResult = await liveActivityCoordinator.recoverPersistedRide(
                requestID: requestID,
                restoredPlatformIdentifier: restoredPlatformIdentifier,
                snapshot: snapshot
            )
            let error = await liveActivityCoordinator.lastError
            guard let self else { return }
            rideSessionRestorationState = .complete
            liveActivityError = error
            if case .adopted = recoveryResult,
               error == nil,
               core.rideSessionStateHandle.rideSessionSnapshot().phase == .active
            {
                lastLiveActivitySnapshot = snapshot
                lastLiveActivityUpdate = snapshot == nil ? nil : core.now()
            }
            syncLiveActivity()
        }
    }

    private func waitForRideSessionRecoveryIfNeeded(
        snapshot: LiveActivityRideSnapshot?
    ) -> Bool {
        switch rideSessionRestorationState {
        case .complete:
            return false
        case .awaitingBluetooth, .recovering:
            return true
        case let .awaitingSnapshot(platformIdentifier):
            if let snapshot {
                beginRideSessionRecovery(
                    restoredPlatformIdentifier: platformIdentifier,
                    snapshot: snapshot
                )
            }
            return true
        }
    }

    private static func makeSessionDriver() -> any CutoutSessionDriving {
        #if DEBUG
        if let fixture = uiTestFixture {
            return CutoutSessionCore(testScript: fixture.testScript)
        }
        #endif
        return CutoutSessionCore()
    }

    #if DEBUG
    private static var uiTestFixture: CutoutUITestSessionFixture? {
        CutoutUITestSessionFixture.resolve(
            environmentValue: ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_FIXTURE"],
            persistedValue: UserDefaults.standard.string(forKey: "CUTOUT_UI_TEST_FIXTURE"),
            arguments: ProcessInfo.processInfo.arguments
        )
    }
    #endif

    private func syncLiveActivity() {
        let snapshot = currentLiveActivitySnapshot()
        guard waitForRideSessionRecoveryIfNeeded(snapshot: snapshot) == false else { return }
        if case .failed = phase, let snapshot {
            liveActivityRequestID += 1
            let requestID = liveActivityRequestID
            lastLiveActivitySnapshot = nil
            lastLiveActivityUpdate = nil
            Task { [weak self, liveActivityCoordinator] in
                await liveActivityCoordinator.reconnectExhausted(
                    requestID: requestID,
                    snapshot: snapshot
                )
                self?.liveActivityError = await liveActivityCoordinator.lastError
            }
            return
        }

        switch phase {
        case .bluetoothPermissionDenied, .bluetoothUnavailable:
            guard let snapshot else { break }
            liveActivityRequestID += 1
            let requestID = liveActivityRequestID
            lastLiveActivitySnapshot = nil
            lastLiveActivityUpdate = nil
            Task { [weak self, liveActivityCoordinator] in
                await liveActivityCoordinator.unrecoverableSessionFailure(
                    requestID: requestID,
                    snapshot: snapshot
                )
                self?.liveActivityError = await liveActivityCoordinator.lastError
            }
            return
        default:
            break
        }

        let rideLifecyclePhase = core.rideSessionStateHandle.rideSessionSnapshot().phase
        if phase.isReconnectingTransport,
           rideLifecyclePhase == .active || rideLifecyclePhase == .reconnecting,
           let previousSnapshot = lastLiveActivitySnapshot {
            guard rideLifecyclePhase == .active else { return }
            let staleSnapshot = previousSnapshot.presented(isStale: true)
            liveActivityRequestID += 1
            let requestID = liveActivityRequestID
            let atMs = core.now().rawValue
            lastLiveActivitySnapshot = staleSnapshot
            lastLiveActivityUpdate = core.now()
            Task { [weak self, liveActivityCoordinator] in
                await liveActivityCoordinator.transportDisconnected(
                    requestID: requestID,
                    atMs: atMs,
                    snapshot: staleSnapshot
                )
                self?.liveActivityError = await liveActivityCoordinator.lastError
            }
            return
        }

        let shouldBeActive = phase.supportsLiveActivity && liveActivityIdentity != nil && isRecordOnlyCapture == false
        let endReason: LiveActivityRideLifecycleEndReason = switch phase {
        case .scanning:
            .disconnected
        case .bluetoothPermissionDenied, .bluetoothUnavailable, .failed:
            .unavailable
        default:
            .sessionEnded
        }
        guard shouldReconcileLiveActivity(snapshot: snapshot, shouldBeActive: shouldBeActive) else { return }
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        let platformIdentifier = connectionState.selection?.platformIdentifier
        let monotonicTimeMs = core.now().rawValue
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.reconcile(
                requestID: requestID,
                platformIdentifier: platformIdentifier,
                monotonicTimeMs: monotonicTimeMs,
                snapshot: snapshot,
                shouldBeActive: shouldBeActive,
                endReason: endReason
            )
            let error = await liveActivityCoordinator.lastError
            self?.liveActivityError = error
            if error != nil {
                self?.lastLiveActivitySnapshot = nil
                self?.lastLiveActivityUpdate = nil
            }
        }
    }

    private func currentLiveActivitySnapshot() -> LiveActivityRideSnapshot? {
        liveActivityIdentity.map {
            LiveActivityRideSnapshot(identity: $0, glyph: liveActivityGlyph, rideState: rideState, now: core.now())
        }
    }

    private func selection(from candidate: DevicePickerDiscoveryCandidate?) -> ConnectionSelection? {
        guard let candidate, candidate.support.isSupported, let route = candidate.support.connectionRoute else {
            return nil
        }
        return ConnectionSelection(
            platformIdentifier: candidate.platformIdentifier,
            title: candidate.displayName,
            route: route
        )
    }

    private func vescRideIdentity(using title: String?) -> LiveActivityRideIdentity {
        .device(title ?? VescRideSnapshot.defaultTitle)
    }

    private func liveActivityIdentity(for selectedRow: DevicePickerRow?) -> LiveActivityRideIdentity? {
        if selectedRow?.connectionRoute == .vescOnewheel {
            return vescRideIdentity(using: selectedRow?.title)
        }
        if core.protocolIdentityCandidate?.support.connectionRoute == .vescOnewheel {
            return vescRideIdentity(using: core.protocolIdentityCandidate?.displayName)
        }
        if let model = selectedRow?.electricUnicycleModel
            ?? core.protocolIdentityCandidate?.support.electricUnicycleModel
            ?? phase.connectingModel {
            return .model(model)
        }
        return nil
    }

    private func liveActivityGlyph(for selectedRow: DevicePickerRow?) -> LiveActivityRideGlyph {
        if selectedRow?.connectionRoute == .vescOnewheel
            || core.protocolIdentityCandidate?.support.connectionRoute == .vescOnewheel {
            return .floatwheelAtom
        }
        return .electricUnicycle
    }

    private func shouldReconcileLiveActivity(
        snapshot: LiveActivityRideSnapshot?,
        shouldBeActive: Bool
    ) -> Bool {
        let now = core.now()
        guard shouldBeActive else {
            lastLiveActivitySnapshot = nil
            lastLiveActivityUpdate = nil
            return true
        }
        guard let snapshot else { return false }
        if core.rideSessionStateHandle.rideSessionSnapshot().phase == .reconnecting {
            lastLiveActivitySnapshot = snapshot
            lastLiveActivityUpdate = now
            return true
        }
        guard snapshot != lastLiveActivitySnapshot else { return false }
        let previousSnapshot = lastLiveActivitySnapshot
        guard
            previousSnapshot == nil
                || snapshot.connectionState != previousSnapshot?.connectionState
                || lastLiveActivityUpdate.map({ now.elapsed(since: $0).rawValue >= Self.liveActivityUpdateIntervalMilliseconds }) != false
        else { return false }

        lastLiveActivitySnapshot = snapshot
        lastLiveActivityUpdate = now
        return true
    }

    func applyCaptureEvent(_ event: CaptureEvent) {
        switch event {
        case let .started(fileURL):
            captureFileName = fileURL.lastPathComponent
            captureNotificationCount = 0
            captureStatus = captureFileName.map(CaptureStatus.recordingLocally)
        case .notificationRecorded:
            captureNotificationCount += 1
            captureStatus = .recording(
                label: captureLabel,
                notificationCount: captureNotificationCount,
                fileName: captureFileName
            )
        case let .progress(progress):
            captureProgress = progress
            captureNotificationCount = Int(clamping: progress.notificationCount)
            captureStatus = .recording(
                label: captureLabel,
                notificationCount: captureNotificationCount,
                fileName: captureFileName
            )
        case let .finished(fileURL):
            captureFileName = fileURL.lastPathComponent
            captureStatus = .saved(fileName: fileURL.lastPathComponent)
        case .failed:
            captureStatus = .failed
        }
    }
}

private extension SessionConnectionPhase {
    var supportsLiveActivity: Bool {
        switch self {
        case .connecting, .discoveringServices, .subscribing, .live:
            true
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .failed:
            false
        }
    }

    var connectingModel: ElectricUnicycleModel? {
        guard case .connecting(let model) = self else { return nil }
        return model
    }

    var isReconnectingTransport: Bool {
        switch self {
        case .connecting, .discoveringServices, .subscribing:
            true
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .live, .failed:
            false
        }
    }
}
