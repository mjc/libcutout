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

struct MusicHistoryQueryResult: Equatable, Sendable {
    let events: [MobileMusicRideEventDto]
    let state: MobileMusicHistoryStateDto?
    let error: MobileRideMapError?
}

typealias RideHistoryQueryProvider = @MainActor () -> (any RideHistoryQuerying)?
typealias RideHistoryDateProvider = @MainActor () -> Date

@MainActor
@Observable
final class CutoutAppModel {

    nonisolated static func runCancellableDetached<Success: Sendable>(
        priority: TaskPriority,
        operation: @escaping @Sendable () throws -> Success
    ) async throws -> Success {
        let task = Task.detached(priority: priority) {
            try Task.checkCancellation()
            let result = try operation()
            try Task.checkCancellation()
            return result
        }
        return try await withTaskCancellationHandler(
            operation: {
                try await task.value
            },
            onCancel: {
                task.cancel()
            })
    }
    nonisolated private static var rideMapLimits: MobileRideMapLimits { .rustOwned }

    private(set) var displayState = RideDisplayState()
    private(set) var phase = SessionConnectionPhase.starting
    private(set) var devicePickerScanState: DevicePickerScanState?
    private(set) var connectionState = ConnectionState.picker
    private(set) var faultHistoryReadback: FaultHistoryReadback?
    private(set) var bmsSnapshot: BmsSnapshot?
    private(set) var phoneLocationReadback = PhoneLocationReadback(
        snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    )
    private(set) var rideMapSnapshot: MobileRideMapSnapshotDto?
    private(set) var rideMapStorageError: String?
    private(set) var rideMapAvailability = MobileRideMapAvailability.checking
    private(set) var rideMapLiveError: MobileRideMapError?
    private(set) var rideMapLiveDisplayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var rideMapLiveCameraRegion: MobileRideMapCameraRegion?
    private(set) var rideMapLiveEndpointMetadata = MobileRideMapRouteEndpointMetadata.empty
    private(set) var rideMapLiveSegments = [MobileRideMapSegmentDisplayMetadata]()
    private(set) var rideMapLiveTelemetryState: MobileRideMapTelemetryStateDto?
    private(set) var rideMapLiveBackgroundGapCount: UInt64 = 0
    private(set) var rideMapLiveProjectionVersion: UInt64 = 0
    private(set) var rideMapLivePointsTruncated = false
    private(set) var rideMapLiveSegmentsOmittedByBudget = false
    private(set) var rideMapLastDecision: MobileRideMapDecisionDto?
    let music: MusicFeatureModel

    /// Compatibility projection for callers that only display the live map.
    /// New route presentations should use the explicitly scoped error properties.
    var rideMapError: MobileRideMapError? { rideMapLiveError }
    private(set) var liveActivityError: LiveActivityRideLifecycleError?
    let capture: CaptureFeatureModel
    var isRecordOnlyCapture: Bool { capture.isManualCapture }
    private(set) var hasSavedDevice = false
    private(set) var settings: DeviceSettings?
    private(set) var phoneAlarmSettings: MobilePhoneAlarmPreferencesDto?
    private(set) var phoneAlarmAuthorization = PhoneRideAlarmAuthorization.unavailable
    private(set) var phoneAlarmDeliveryError: String?

    var selectedRideTitle: String? {
        connectionState.selection?.title
    }

    /// Stable identity/name pair used to relabel persisted ride-history vehicles.
    /// The persisted selection is the fallback when the connection state has not rebuilt yet.
    var rideMapVehicleIdentity: String? {
        connectionState.selection?.platformIdentifier ?? selectedDeviceStore.platformIdentifier
    }

    var rideMapVehicleName: String? {
        if let identity = rideMapVehicleIdentity {
            if let cached = rideMapVehicleNameCache[identity] {
                return cached
            }
            if let persisted = selectedDeviceStore.displayName(for: identity) {
                rideMapVehicleNameCache[identity] = persisted
                return persisted
            }
        }
        if let identity = rideMapVehicleIdentity,
            let candidate = core.protocolIdentityCandidate,
            candidate.platformIdentifier == identity,
            candidate.displayName != identity,
            !candidate.displayName.isEmpty
        {
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
        if let name = rideHistory.vehicleNames[identity] {
            return name
        }
        if let name = rideMapVehicleNameCache[identity] {
            return name
        }
        if let name = selectedDeviceStore.displayName(for: identity) {
            rideMapVehicleNameCache[identity] = name
            return name
        }
        return identity == rideMapVehicleIdentity ? rideMapVehicleName : nil
    }

    var phoneAlarmDeviceName: String? {
        phoneAlarmSettings.flatMap { rideMapVehicleName(for: $0.deviceIdentity) }
    }

    var phoneAlarmAuthorizationText: String {
        switch phoneAlarmAuthorization {
        case .unavailable:
            localizedAppText("phone_alarm.authorization.unavailable")
        case .notDetermined:
            localizedAppText("phone_alarm.authorization.not_determined")
        case .denied:
            localizedAppText("phone_alarm.authorization.denied")
        case let .permitted(alerts, sounds, quietly):
            if quietly {
                localizedAppText("phone_alarm.authorization.quiet")
            } else if alerts, sounds {
                localizedAppText("phone_alarm.authorization.alerts_and_sound")
            } else if alerts {
                localizedAppText("phone_alarm.authorization.alerts_no_sound")
            } else {
                localizedAppText("phone_alarm.authorization.no_alerts")
            }
        }
    }

    func setPhoneAlarmsEnabled(_ enabled: Bool, deviceIdentity: String) async {
        if enabled, !phoneAlarmAuthorization.capability.canSchedule {
            await requestPhoneAlarmAuthorization()
            guard phoneAlarmAuthorization.capability.canSchedule else { return }
        }
        do {
            applyPhoneAlarmActions(
                try core.rideSessionStateHandle.setPhoneAlarmEnabled(
                    deviceIdentity: deviceIdentity,
                    enabled: enabled
                ))
            syncPhoneAlarmPreferences()
            phoneAlarmDeliveryError = nil
        } catch {
            syncPhoneAlarmPreferences()
            phoneAlarmDeliveryError = Self.phoneAlarmErrorText(error)
        }
    }

    func setPhoneAlarmPwmDutyPercent(_ percent: Int, deviceIdentity: String) {
        guard let percent = UInt8(exactly: percent) else { return }
        do {
            applyPhoneAlarmActions(
                try core.rideSessionStateHandle.setPhoneAlarmPwmDutyPercent(
                    deviceIdentity: deviceIdentity,
                    dutyPercent: percent
                ))
            syncPhoneAlarmPreferences()
            phoneAlarmDeliveryError = nil
        } catch {
            syncPhoneAlarmPreferences()
            phoneAlarmDeliveryError = Self.phoneAlarmErrorText(error)
        }
    }

    func requestPhoneAlarmAuthorization() async {
        phoneAlarmAuthorizationTask?.cancel()
        phoneAlarmAuthorizationGeneration &+= 1
        let generation = phoneAlarmAuthorizationGeneration
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let authorization = await phoneAlarmDelivery.requestAuthorization()
            guard !Task.isCancelled, generation == phoneAlarmAuthorizationGeneration else { return }
            applyPhoneAlarmAuthorization(authorization)
        }
        phoneAlarmAuthorizationTask = task
        await task.value
    }

    func refreshPhoneAlarmAuthorization() {
        phoneAlarmAuthorizationTask?.cancel()
        phoneAlarmAuthorizationGeneration &+= 1
        let generation = phoneAlarmAuthorizationGeneration
        let phoneAlarmDelivery = self.phoneAlarmDelivery
        phoneAlarmAuthorizationTask = Task { @MainActor [weak self, phoneAlarmDelivery] in
            let authorization = await phoneAlarmDelivery.authorizationStatus()
            guard let self, !Task.isCancelled, generation == self.phoneAlarmAuthorizationGeneration else { return }
            self.applyPhoneAlarmAuthorization(authorization)
        }
    }

    private func applyPhoneAlarmAuthorization(_ authorization: PhoneRideAlarmAuthorization) {
        phoneAlarmAuthorization = authorization
        applyPhoneAlarmActions(
            core.rideSessionStateHandle.setPhoneAlarmDeliveryCapability(
                capability: authorization.capability
            ))
    }

    private func syncPhoneAlarmPreferences() {
        if let error = core.rideSessionStateHandle.phoneAlarmActivationError() {
            phoneAlarmSettings = nil
            phoneAlarmDeliveryError = Self.phoneAlarmErrorText(error)
            return
        }
        phoneAlarmSettings = core.rideSessionStateHandle.phoneAlarmPreferences()
        if phoneAlarmSettings != nil {
            phoneAlarmDeliveryError = nil
        }
    }

    private func drainPhoneAlarmActions() {
        applyPhoneAlarmActions(core.rideSessionStateHandle.drainPhoneAlarmActions())
    }

    func applyPhoneAlarmActions(_ actions: MobilePhoneAlarmActionsDto) {
        phoneAlarmDelivery.cancel(requestIDs: actions.cancelRequestIds)
        actions.schedule.forEach(schedulePhoneAlarmDelivery)
        if core.rideSessionStateHandle.phoneAlarmPreferences() == nil {
            phoneAlarmSettings = nil
        }
    }

    private func schedulePhoneAlarmDelivery(_ request: MobilePhoneAlarmDeliveryRequestDto) {
        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await phoneAlarmDelivery.deliver(request)
                let accepted = core.rideSessionStateHandle.completePhoneAlarmDelivery(
                    requestId: request.id,
                    delivered: true,
                    monotonicMilliseconds: core.now().rawValue
                )
                if !accepted {
                    phoneAlarmDelivery.cancel(requestIDs: [request.id])
                } else {
                    phoneAlarmDeliveryError = nil
                }
            } catch {
                let accepted = core.rideSessionStateHandle.completePhoneAlarmDelivery(
                    requestId: request.id,
                    delivered: false,
                    monotonicMilliseconds: core.now().rawValue
                )
                if accepted {
                    phoneAlarmDeliveryError = Self.phoneAlarmErrorText(error)
                }
            }
        }
    }

    static func phoneAlarmErrorText(_ error: any Error) -> String {
        guard let error = error as? MobilePhoneAlarmError else {
            return error.localizedDescription
        }
        let key =
            switch error {
            case .NoActiveDevice: "phone_alarm.error.no_active_device"
            case .InvalidDeviceIdentity: "phone_alarm.error.invalid_device_identity"
            case .InvalidPwmDutyThreshold: "phone_alarm.error.invalid_pwm_duty"
            case .InvalidPwmHeadroomThreshold: "phone_alarm.error.invalid_pwm_headroom"
            case .TooManyDevices: "phone_alarm.error.too_many_devices"
            case .DeviceIdentityChanged: "phone_alarm.error.device_changed"
            case .StorageFailure: "phone_alarm.error.storage_failure"
            }
        return localizedAppText(key)
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

    var selectedRideIdentifier: String? {
        connectionState.selection?.platformIdentifier
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
        rideMapSnapshot?.state == .active
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

    var connectionStatusText: String {
        connectionState.statusText ?? phase.displayText
    }

    private let core: any CutoutSessionDriving
    let rideHistory: RideHistoryModel
    private let liveActivityCoordinator: LiveActivityRideLifecycleCoordinator
    private let selectedDeviceStore: DevicePickerSelectionStore
    private let rideSessionMarkerStore: RideSessionMarkerStore
    private let phoneAlarmDelivery: any PhoneRideAlarmDelivering
    private var liveActivityIdentity: LiveActivityRideIdentity?
    private var liveActivityGlyph = LiveActivityRideGlyph.electricUnicycle
    private var lastLiveActivitySnapshot: LiveActivityRideSnapshot?
    private var lastLiveActivityUpdate: MonotonicMilliseconds?
    private var liveActivityRequestID: UInt64 = 0
    private var hasStarted = false
    private var permitsStoredDeviceAutoPairing = true
    private var rideSessionRestorationState = RideSessionRestorationState.complete
    private var restorationMarkerAtLaunch: Data?
    private var rideMapVehicleNameCache = [String: String]()
    private var rideMapRestoreTask: Task<Void, Never>?
    private var phoneAlarmAuthorizationTask: Task<Void, Never>?
    private var rideMapLiveProjectionTask: Task<Void, Never>?
    private var rideMapDurationTask: Task<Void, Never>?
    private var rideMapLiveProjectionCancellation: MobileLiveRideMapProjectionCancellation?
    private var rideMapDurableProjectionCancellation: MobileRideMapProjectionCancellation?
    private var rideMapLiveProjectionGeneration: UInt64 = 0
    private var rideMapLiveProjectionEnabled = false
    private var phoneAlarmAuthorizationGeneration: UInt64 = 0
    private static let liveActivityUpdateIntervalMilliseconds: UInt64 = 1_000

    isolated deinit {
        stopMusicMonitoring()
    }

    /// Opens and restores durable state before constructing the main-actor presentation model.
    ///
    /// Database creation, migration, and ride recovery happen inside Rust and are deliberately
    /// kept off the main actor. The synchronous convenience initializer remains for deterministic
    /// tests and injected drivers; the application uses this path.
    static func open() async throws -> CutoutAppModel {
        let database = try await RustPersistenceStore.open()
        try Task.checkCancellation()
        let state = MobileRideMapState(database: database)
        try await runCancellableDetached(priority: .userInitiated) {
            let now = UInt64(ProcessInfo.processInfo.systemUptime * 1_000)
            _ = try state.restore(atMs: now)
        }
        try Task.checkCancellation()
        #if DEBUG
            let permitsStoredDeviceAutoPairing = uiTestFixture == nil
        #else
            let permitsStoredDeviceAutoPairing = true
        #endif
        return CutoutAppModel(
            core: makeSessionDriver(rideMapState: state),
            permitsStoredDeviceAutoPairing: permitsStoredDeviceAutoPairing,
            selectedDeviceStore: DevicePickerSelectionStore(),
            rideSessionMarkerStore: RideSessionMarkerStore(),
            liveActivityManager: LiveActivityRideActivityKitManager(),
            musicHistoryPolicyStore: MusicHistoryPolicyStore(),
            musicProviderSelectionStore: MusicProviderSelectionStore(),
            musicMonitoringPreferenceStore: MusicMonitoringPreferenceStore(),
            phoneAlarmDelivery: makePhoneRideAlarmDelivery(),
            rideHistoryQueryProvider: nil
        )
    }

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
            liveActivityManager: LiveActivityRideActivityKitManager(),
            musicHistoryPolicyStore: MusicHistoryPolicyStore(),
            musicProviderSelectionStore: MusicProviderSelectionStore(),
            musicMonitoringPreferenceStore: MusicMonitoringPreferenceStore(),
            phoneAlarmDelivery: makePhoneRideAlarmDelivery(),
            rideHistoryQueryProvider: nil
        )
    }

    convenience init(
        core: any CutoutSessionDriving,
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore(),
        rideSessionMarkerStore: RideSessionMarkerStore = RideSessionMarkerStore(),
        liveActivityManager: any LiveActivityRideLifecycleManaging = LiveActivityRideActivityKitManager(),
        musicHistoryPolicyStore: MusicHistoryPolicyStore = MusicHistoryPolicyStore(),
        musicProviderSelectionStore: MusicProviderSelectionStore = MusicProviderSelectionStore(),
        musicMonitoringPreferenceStore: MusicMonitoringPreferenceStore = MusicMonitoringPreferenceStore(),
        appleMusicMonitor: (any AppleMusicMonitorDriving)? = nil,
        phoneAlarmDelivery: any PhoneRideAlarmDelivering = makePhoneRideAlarmDelivery(),
        rideHistoryQueryProvider: RideHistoryQueryProvider? = nil,
        rideHistoryDateProvider: @escaping RideHistoryDateProvider = { Date() }
    ) {
        self.init(
            core: core,
            permitsStoredDeviceAutoPairing: true,
            selectedDeviceStore: selectedDeviceStore,
            rideSessionMarkerStore: rideSessionMarkerStore,
            liveActivityManager: liveActivityManager,
            musicHistoryPolicyStore: musicHistoryPolicyStore,
            musicProviderSelectionStore: musicProviderSelectionStore,
            musicMonitoringPreferenceStore: musicMonitoringPreferenceStore,
            appleMusicMonitor: appleMusicMonitor,
            phoneAlarmDelivery: phoneAlarmDelivery,
            rideHistoryQueryProvider: rideHistoryQueryProvider,
            rideHistoryDateProvider: rideHistoryDateProvider
        )
    }

    private init(
        core: any CutoutSessionDriving,
        permitsStoredDeviceAutoPairing: Bool,
        selectedDeviceStore: DevicePickerSelectionStore,
        rideSessionMarkerStore: RideSessionMarkerStore,
        liveActivityManager: any LiveActivityRideLifecycleManaging,
        musicHistoryPolicyStore: MusicHistoryPolicyStore,
        musicProviderSelectionStore: MusicProviderSelectionStore,
        musicMonitoringPreferenceStore: MusicMonitoringPreferenceStore,
        appleMusicMonitor: (any AppleMusicMonitorDriving)? = nil,
        phoneAlarmDelivery: any PhoneRideAlarmDelivering,
        rideHistoryQueryProvider: RideHistoryQueryProvider?,
        rideHistoryDateProvider: @escaping RideHistoryDateProvider = { Date() }
    ) {
        let rideHistory = RideHistoryModel(
            stateProvider: rideHistoryQueryProvider ?? { core.rideMapStateHandle },
            dateProvider: rideHistoryDateProvider,
            storageErrorProvider: { core.rideMapStorageError }
        )
        self.rideHistory = rideHistory
        self.permitsStoredDeviceAutoPairing = permitsStoredDeviceAutoPairing
        self.core = core
        capture = CaptureFeatureModel(
            sessionState: core.rideSessionStateHandle,
            flush: { await core.flushCapture() },
            finish: { await core.finishCapture() },
            changeCaptureLabel: { try core.changeCaptureLabel(generation: $0, action: $1) }
        )
        rideMapStorageError = core.rideMapStorageError
        rideMapAvailability = core.rideMapAvailability
        liveActivityCoordinator = LiveActivityRideLifecycleCoordinator(
            manager: liveActivityManager,
            sessionState: core.rideSessionStateHandle,
            markerStore: rideSessionMarkerStore
        )
        self.selectedDeviceStore = selectedDeviceStore
        self.rideSessionMarkerStore = rideSessionMarkerStore
        self.phoneAlarmDelivery = phoneAlarmDelivery
        self.music = MusicFeatureModel(
            providerSelectionStore: musicProviderSelectionStore,
            historyPolicyStore: musicHistoryPolicyStore,
            monitoringPreferenceStore: musicMonitoringPreferenceStore,
            rideMapState: core.rideMapStateHandle,
            monotonicNow: { core.now().rawValue },
            updateCapturePolicy: { core.updateMusicCapturePolicy($0) },
            updateCaptureObservation: { core.updateMusicCaptureObservation($0) },
            invalidateHistoryForDeletion: { rideHistory.invalidateForMusicDeletion() },
            selectedHistoryRideID: { rideHistory.selectedRideID },
            clearSelectedHistoryMusic: { rideHistory.clearMusicMetadata() },
            setRideHistoryError: { rideHistory.setError($0) },
            appleMonitor: appleMusicMonitor
        )
        self.rideHistory.onPageUpdated = { [weak self] in
            self?.applyRideHistoryPageResult()
        }
        self.music.timelineEvents = music.coordinator.recordedEvents
        hasSavedDevice = selectedDeviceStore.platformIdentifier != nil
        if let identity = selectedDeviceStore.platformIdentifier,
            let name = selectedDeviceStore.displayName(for: identity)
        {
            rideMapVehicleNameCache[identity] = name
        }
        restoreRideMapState()
        CutoutSessionCallbackRegistrar(core: core).install(
            .init(
                displayState: { [weak self] displayState in
                    self?.displayState = displayState
                    self?.syncLiveActivity()
                },
                phase: { [weak self] phase in
                    guard let self else { return }
                    self.handlePhaseChange(phase)
                    self.syncLiveActivity()
                },
                reconnectScheduled: { [weak self] retry in
                    self?.handleReconnectScheduled(retry)
                },
                captureEvent: { [weak self] event in
                    self?.applyCaptureEvent(event)
                },
                scanState: { [weak self] scanState in
                    self?.handleScanStateChange(scanState)
                },
                settings: { [weak self] snapshot in
                    guard let self, self.phase == .live,
                        self.core.rideSessionStateHandle.connectionAttemptSnapshot().revision
                            == snapshot.connection.revision
                    else { return }
                    self.settings = snapshot
                },
                faultHistory: { [weak self] faultHistoryReadback in
                    self?.faultHistoryReadback = faultHistoryReadback
                },
                bmsSnapshot: { [weak self] bmsSnapshot in
                    self?.bmsSnapshot = bmsSnapshot
                },
                phoneLocation: { [weak self] snapshot, receivedAt in
                    self?.phoneLocationReadback = PhoneLocationReadback(snapshot: snapshot, receivedAt: receivedAt)
                },
                rideMapDecision: { [weak self] snapshot, decision in
                    self?.applyRideMapDecision(snapshot: snapshot, decision: decision)
                },
                rideMapSnapshot: { [weak self] snapshot in
                    guard let self else { return }
                    guard self.acceptsRideMapSnapshot(snapshot) else { return }
                    if let previousRideID = self.rideMapSnapshot?.rideID,
                        previousRideID != snapshot.rideID
                    {
                        // A newer Rust snapshot can be an automatic replacement, not merely a new
                        // revision of the same ride. Invalidate both successful and failed projections
                        // from the previous ride before publishing the replacement.
                        self.invalidateLiveProjection(clearPoints: true)
                        self.rideMapLiveError = nil
                    }
                    self.rideMapSnapshot = snapshot
                    self.rideMapLiveTelemetryState = snapshot.telemetryState
                    self.updateRideMapDurationTicker()
                    if self.rideMapRestoreTask == nil {
                        self.restoreRideMapState()
                    }
                },
                rideMapError: { [weak self] event in
                    guard let self,
                        Self.shouldApplyRideMapError(
                            context: event.context,
                            currentSnapshot: self.rideMapSnapshot
                        )
                    else { return }
                    self.rideMapLiveError = event.error
                },
                rideMapAvailability: { [weak self] availability in
                    self?.rideMapAvailability = availability
                },
                protocolIdentity: { [weak self] candidate in
                    self?.applyProtocolIdentityCandidate(candidate)
                },
                bluetoothRestoration: { [weak self] platformIdentifier in
                    self?.handleBluetoothRestorationResolved(platformIdentifier)
                },
                phoneAlarmActions: { [weak self] actions in
                    self?.applyPhoneAlarmActions(actions)
                }
            )
        )
        syncPhoneAlarmPreferences()
        drainPhoneAlarmActions()
        refreshPhoneAlarmAuthorization()
    }

    private func stopMusicMonitoring() {
        music.stopMonitoring()
    }

    private func beginMusicMonitoring() {
        music.beginMonitoring()
    }

    private func restoreRideMapState() {
        guard let state = core.rideMapStateHandle else { return }
        rideMapSnapshot = state.currentSnapshot()
        if rideMapSnapshot != nil {
            music.synchronizeHistory(state.currentMusicHistory())
        } else {
            music.rideMapClosed()
        }
        rideMapLiveTelemetryState = rideMapSnapshot?.telemetryState
        updateRideMapDurationTicker()
        guard let restoredRideID = rideMapSnapshot?.rideID else { return }
        rideMapRestoreTask?.cancel()
        let previewLimit = Self.rideMapLimits.liveTailPointLimit
        let restorationGeneration = rideMapLiveProjectionGeneration
        rideMapRestoreTask = Task { [weak self] in
            do {
                let result = try await Self.runCancellableDetached(priority: .userInitiated) {
                    // Restoration loads only the recorder tail synchronously. Project the
                    // durable route here so relaunch preserves the whole ride and its canonical
                    // camera bounds; subsequent live updates may use the bounded recorder path.
                    try state.projectStoredPoints(
                        rideID: restoredRideID,
                        budget: previewLimit
                    )
                }
                guard !Task.isCancelled, let self else { return }
                guard
                    Self.shouldApplyRestoredLiveProjection(
                        restorationGeneration: restorationGeneration,
                        currentGeneration: self.rideMapLiveProjectionGeneration,
                        liveProjectionEnabled: self.rideMapLiveProjectionEnabled
                    )
                else {
                    return
                }
                self.applyLiveProjection(result)
            } catch {
                guard !Task.isCancelled, let self else { return }
                guard
                    Self.shouldApplyRestoredLiveProjection(
                        restorationGeneration: restorationGeneration,
                        currentGeneration: self.rideMapLiveProjectionGeneration,
                        liveProjectionEnabled: self.rideMapLiveProjectionEnabled
                    )
                else {
                    return
                }
                self.rideMapLiveError = Self.mapRideMapError(error)
                self.clearLiveProjectionState()
            }
        }
    }

    func start(sceneIsActive: Bool = true) {
        guard hasStarted == false else { return }
        hasStarted = true
        permitsStoredDeviceAutoPairing = false
        restorationMarkerAtLaunch = rideSessionMarkerStore.marker
        rideSessionRestorationState = .awaitingBluetooth
        core.start()
        music.start(sceneIsActive: sceneIsActive)
    }

    @discardableResult
    func startGpsOnlyRide() async -> Bool {
        let connectionToken = core.connectionSnapshot.token
        let started = await applyRideMapCommand(resetPoints: true) {
            try await core.startRideMapGpsOnly(atMs: currentMonotonicTime.rawValue)
        }
        guard started else { return false }
        if let connectionToken {
            _ = core.resetTripMeterForNewRide(token: connectionToken)
        }
        core.resetRideMapLocationAdmission()
        if let error = music.applyHistoryForNewRide() {
            rideMapLiveError = error
            guard core.rideMapStateHandle?.currentSnapshot() != nil else {
                return false
            }
        }
        return true
    }

    @discardableResult
    func pauseRideMap() async -> Bool {
        await applyRideMapCommand {
            try await core.pauseRideMap(atMs: currentMonotonicTime.rawValue)
        }
    }

    @discardableResult
    func resumeRideMap() async -> Bool {
        await applyRideMapCommand {
            try await core.resumeRideMap(atMs: currentMonotonicTime.rawValue)
        }
    }

    @discardableResult
    func stopRideMap() async -> Bool {
        let stopped = await applyRideMapCommand {
            try await core.stopRideMap(atMs: currentMonotonicTime.rawValue)
        }
        if stopped {
            invalidateLiveProjection(clearPoints: false)
            music.clearMusicCaptureContext()
        }
        return stopped
    }

    func refreshRideMapDuration() {
        guard let snapshot = core.rideMapStateHandle?.currentSnapshot(atMs: currentMonotonicTime.rawValue),
            snapshot.state == .active
        else {
            return
        }
        rideMapSnapshot = snapshot
    }

    private func updateRideMapDurationTicker() {
        rideMapDurationTask?.cancel()
        guard rideMapSnapshot?.state == .active else {
            rideMapDurationTask = nil
            return
        }
        rideMapDurationTask = Task { [weak self] in
            while Task.isCancelled == false {
                self?.refreshRideMapDuration()
                do {
                    try await Task.sleep(for: .seconds(1))
                } catch {
                    return
                }
            }
        }
    }

    @discardableResult
    func saveRideMap() async -> Bool {
        guard await applyRideMapCommand({ try await core.saveRideMap() }) else {
            return false
        }
        invalidateLiveProjection(clearPoints: false)
        music.rideMapClosed()
        reloadRideHistory()
        return true
    }

    @discardableResult
    func discardRideMap() async -> Bool {
        guard await applyRideMapCommand({ try await core.discardRideMap() }) else {
            return false
        }
        invalidateLiveProjection(clearPoints: true)
        music.rideMapClosed()
        rideHistory.clearRouteProjection()
        reloadRideHistory()
        return true
    }

    private func reloadRideHistory() {
        rideHistory.reload()
    }

    private func applyRideHistoryPageResult() {
        rideMapVehicleNameCache.merge(
            rideHistory.vehicleNames,
            uniquingKeysWith: { _, incoming in incoming }
        )
    }

    nonisolated static func mapRideMapError(_ error: Error) -> MobileRideMapError {
        if let error = error as? MobileRideMapError {
            return error
        }
        return .storageError(String(describing: error))
    }

    static func shouldApplyLiveProjection(
        generation: UInt64,
        currentGeneration: UInt64,
        enabled: Bool,
        rideID: String,
        currentRideID: String?
    ) -> Bool {
        enabled && generation == currentGeneration && currentRideID == rideID
    }

    static func shouldApplyRestoredLiveProjection(
        restorationGeneration: UInt64,
        currentGeneration: UInt64,
        liveProjectionEnabled: Bool
    ) -> Bool {
        !liveProjectionEnabled && restorationGeneration == currentGeneration
    }

    static func shouldApplyRideMapError(
        context: MobileRideMapErrorContext,
        currentSnapshot: MobileRideMapSnapshotDto?
    ) -> Bool {
        guard let currentSnapshot else { return context.rideID == nil }
        guard context.rideID == currentSnapshot.rideID else { return false }
        guard let generation = context.generation else { return true }
        return currentSnapshot.recordingToken?.generation == generation
    }

    private func applyRideMapDecision(
        snapshot: MobileRideMapSnapshotDto,
        decision: MobileRideMapDecisionDto
    ) {
        guard acceptsRideMapSnapshot(snapshot) else { return }
        rideMapLiveError = nil
        rideMapSnapshot = snapshot
        rideMapLastDecision = decision
        switch decision {
        case let .pending(point):
            rideMapLiveTelemetryState = point.telemetryState
        case let .accepted(point):
            rideMapLiveTelemetryState = point.telemetryState
            requestLiveProjection()
        case .rejected, .ignored, .storageError:
            break
        }
    }

    private func acceptsRideMapSnapshot(_ snapshot: MobileRideMapSnapshotDto) -> Bool {
        guard let current = rideMapSnapshot else { return true }
        guard snapshot.revision >= current.revision else { return false }
        return snapshot.revision != current.revision || snapshot.rideID == current.rideID
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
        rideMapDurableProjectionCancellation?.cancel()
        guard rideMapLiveProjectionTask == nil else { return }

        guard let state = core.rideMapStateHandle else { return }
        let budget = Self.rideMapLimits.liveTailPointLimit
        rideMapLiveProjectionTask = Task { [weak self] in
            defer {
                self?.rideMapLiveProjectionTask = nil
                self?.rideMapLiveProjectionCancellation = nil
                self?.rideMapDurableProjectionCancellation = nil
            }
            while let self {
                guard self.rideMapLiveProjectionEnabled else { break }
                // Capture the identity for this individual query. A replacement can arrive
                // while the previous query is unwinding; the surviving task must then retry
                // the replacement ride instead of comparing every result with the old ride.
                guard let rideID = self.rideMapSnapshot?.rideID, rideID.isEmpty == false else {
                    break
                }
                let generation = self.rideMapLiveProjectionGeneration
                let liveCancellation = MobileLiveRideMapProjectionCancellation()
                let durableCancellation = MobileRideMapProjectionCancellation()
                self.rideMapLiveProjectionCancellation = liveCancellation
                self.rideMapDurableProjectionCancellation = durableCancellation
                do {
                    let projection = try await Self.runCancellableDetached(priority: .userInitiated) {
                        try state.projectCurrentRoutePoints(
                            budget: budget,
                            rideID: rideID,
                            durableCancellation: durableCancellation,
                            liveCancellation: liveCancellation
                        )
                    }
                    guard self.rideMapLiveProjectionEnabled else {
                        break
                    }
                    guard
                        Self.shouldApplyLiveProjection(
                            generation: generation,
                            currentGeneration: self.rideMapLiveProjectionGeneration,
                            enabled: self.rideMapLiveProjectionEnabled,
                            rideID: rideID,
                            currentRideID: self.rideMapSnapshot?.rideID
                        )
                    else {
                        continue
                    }
                    self.applyLiveProjection(projection)
                } catch {
                    guard self.rideMapLiveProjectionEnabled else {
                        break
                    }
                    guard
                        Self.shouldApplyLiveProjection(
                            generation: generation,
                            currentGeneration: self.rideMapLiveProjectionGeneration,
                            enabled: self.rideMapLiveProjectionEnabled,
                            rideID: rideID,
                            currentRideID: self.rideMapSnapshot?.rideID
                        )
                    else {
                        continue
                    }
                    self.rideMapLiveError = Self.mapRideMapError(error)
                    self.clearLiveProjectionState()
                }
                return
            }
        }
    }

    private func applyLiveProjection(_ projection: MobileRideMapRouteProjection) {
        rideMapLiveProjectionVersion &+= 1
        rideMapLiveDisplayPoints = projection.points
        rideMapLiveCameraRegion = projection.canonicalCameraRegion ?? projection.cameraRegion
        rideMapLiveEndpointMetadata = projection.endpointMetadata
        rideMapLiveSegments = projection.segments
        rideMapLiveBackgroundGapCount = projection.backgroundGapCount
        rideMapLivePointsTruncated = projection.pointsOmittedByBudget
        rideMapLiveSegmentsOmittedByBudget = projection.segmentsOmittedByBudget
    }

    private func clearLiveProjectionState() {
        rideMapLiveProjectionVersion &+= 1
        rideMapLiveDisplayPoints.removeAll(keepingCapacity: true)
        rideMapLiveCameraRegion = nil
        rideMapLiveEndpointMetadata = .empty
        rideMapLiveSegments.removeAll(keepingCapacity: true)
        rideMapLiveTelemetryState = nil
        rideMapLiveBackgroundGapCount = 0
        rideMapLivePointsTruncated = false
        rideMapLiveSegmentsOmittedByBudget = false
    }

    private func invalidateLiveProjection(clearPoints: Bool) {
        rideMapLiveProjectionGeneration &+= 1
        rideMapLiveProjectionEnabled = false
        rideMapLiveProjectionCancellation?.cancel()
        rideMapDurableProjectionCancellation?.cancel()
        if clearPoints {
            clearLiveProjectionState()
            rideMapLastDecision = nil
        }
    }

    private func applyRideMapCommand(
        resetPoints: Bool = false,
        _ command: () async throws -> MobileRideMapSnapshotDto
    ) async -> Bool {
        do {
            rideMapSnapshot = try await command()
            if let rideMapSnapshot {
                core.updateRideLocationDemand(for: rideMapSnapshot.state)
            }
            rideMapLiveError = nil
            updateRideMapDurationTicker()
            if resetPoints {
                invalidateLiveProjection(clearPoints: true)
            }
            rideMapLiveTelemetryState = rideMapSnapshot?.telemetryState
            return true
        } catch {
            rideMapLiveError = Self.mapRideMapError(error)
            return false
        }
    }

    func submitDeviceSetting(token: ConnectionAttemptToken, id: DeviceSettingID, value: DeviceSettingValue) throws {
        try core.submitDeviceSetting(token: token, id: id, value: value)
    }

    func submitDeviceAction(token: ConnectionAttemptToken, id: DeviceActionID) throws {
        try core.submitDeviceAction(token: token, id: id)
    }

    func pair(platformIdentifier: String) -> Bool {
        guard core.rideSessionStateHandle.captureLifecycleSnapshot().canPair else { return false }
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
        guard selectedRow.isSupported || selectedRow.isProbeRecommended else { return false }

        let selection = ConnectionSelection(
            platformIdentifier: selectedRow.id,
            title: selectedRow.title,
            route: selectedRow.connectionRoute ?? .electricUnicycle
        )
        clearSettings()
        liveActivityError = nil
        connectionState = .connecting(selection, phase: .discoveringServices)
        permitsStoredDeviceAutoPairing = true
        phase = .discoveringServices
        let didPair = capture.requestStart(device: CaptureDeviceIdentity(row: selectedRow), description: nil) {
            core.pair(platformIdentifier: platformIdentifier)
        }
        if didPair {
            syncPhoneAlarmPreferences()
            drainPhoneAlarmActions()
            let displayName = Self.meaningfulDeviceName(
                selectedRow.title,
                identity: platformIdentifier
            )
            let persistedDisplayName = selectedDeviceStore.displayName(for: platformIdentifier)
            let selectionChanged = selectedDeviceStore.platformIdentifier != platformIdentifier
            if selectionChanged {
                selectedDeviceStore.save(
                    platformIdentifier: platformIdentifier,
                    displayName: displayName
                )
            } else if let displayName, displayName != persistedDisplayName {
                selectedDeviceStore.save(
                    platformIdentifier: platformIdentifier,
                    displayName: displayName
                )
            }
            if let displayName, selectionChanged || displayName != persistedDisplayName {
                rideMapVehicleNameCache[platformIdentifier] = displayName
            }
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
        guard core.rideSessionStateHandle.captureLifecycleSnapshot().canStart else { return false }
        let trimmedKind = deviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
        let annotationKind =
            trimmedKind
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: "=", with: " ")
        let annotations = annotationKind.isEmpty ? [] : ["capture_description=\(annotationKind)"]
        let device = devicePickerScanState?.rows.first { $0.id == platformIdentifier }.map(CaptureDeviceIdentity.init)
        let didStart = capture.requestStart(device: device, description: annotationKind.isEmpty ? nil : annotationKind)
        {
            core.recordOnly(
                platformIdentifier: platformIdentifier,
                note: "user-initiated Bluetooth capture",
                annotations: annotations
            )
        }
        guard didStart else { return false }
        capture.apply(.lifecycle(core.rideSessionStateHandle.captureLifecycleSnapshot()))

        connectionState = .picker
        permitsStoredDeviceAutoPairing = false
        liveActivityIdentity = nil
        liveActivityGlyph = .electricUnicycle
        syncLiveActivity()
        return true
    }

    func startProbe(platformIdentifier: String) -> Bool {
        pair(platformIdentifier: platformIdentifier)
    }

    func appDidEnterBackground() {
        music.sceneDidEnterBackground()
        guard let snapshot = currentLiveActivitySnapshot() else {
            guard isRecordOnlyCapture else { return }
            let capture = self.capture
            Task { _ = await capture.flush() }
            return
        }
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        let atMs = core.now().rawValue
        let capture = self.capture
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.appDidEnterBackground(
                requestID: requestID,
                atMs: atMs,
                snapshot: snapshot,
                captureFlush: { await capture.flush() }
            )
            self?.liveActivityError = await liveActivityCoordinator.lastError
        }
    }

    func appDidBecomeActive() {
        refreshPhoneAlarmAuthorization()
        if music.sceneDidBecomeActive() {
            beginMusicMonitoring()
        }
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

    func disconnectTransport() {
        applyPhoneAlarmActions(core.rideSessionStateHandle.deactivatePhoneAlarmDevice())
        phoneAlarmSettings = nil
        endLiveActivity(reason: .disconnected)
        capture.clearLabels()
        connectionState = .picker
        phase = .scanning
        liveActivityIdentity = nil
        liveActivityGlyph = .electricUnicycle
        permitsStoredDeviceAutoPairing = false
        clearSettings()
        core.disconnectAndScan()
    }

    private func clearSettings() {
        settings = nil
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

    func retryLiveActivity() {
        guard liveActivityError != nil else { return }
        liveActivityError = nil
        lastLiveActivitySnapshot = nil
        lastLiveActivityUpdate = nil
        syncLiveActivity()
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
            )
        {
            // Persist every resolved identity, not only the currently selected one. History can
            // contain rides from an older CoreBluetooth identifier and must still be relabelable.
            let persistedDisplayName = selectedDeviceStore.displayName(
                for: candidate.platformIdentifier
            )
            if persistedDisplayName != displayName {
                selectedDeviceStore.save(
                    platformIdentifier: candidate.platformIdentifier,
                    displayName: displayName
                )
                rideMapVehicleNameCache[candidate.platformIdentifier] = displayName
            }
        }
        if let model = candidate?.support.electricUnicycleModel {
            liveActivityIdentity = .model(model)
            liveActivityGlyph = .electricUnicycle
        }
        guard let selection = selection(from: candidate) else {
            syncLiveActivity()
            return
        }
        // Advertisement-derived routes only decide which device the user selected. The protocol
        // detector owns the route once bytes have resolved it.
        if connectionState.selection?.platformIdentifier == nil
            || connectionState.selection?.platformIdentifier == selection.platformIdentifier
        {
            let resolvedSelection = ConnectionSelection(
                platformIdentifier: selection.platformIdentifier,
                title: connectionState.selection?.title ?? selection.title,
                route: selection.route
            )
            connectionState = connectionState.replacingSelection(with: resolvedSelection)
        }
        if selection.route == .vescOnewheel {
            liveActivityIdentity = vescRideIdentity(using: selection.title)
            liveActivityGlyph = .floatwheelAtom
        }
        syncLiveActivity()
    }

    private func handleScanStateChange(_ scanState: DevicePickerScanState) {
        devicePickerScanState = scanState
        if let row = scanState.rows.first(where: { $0.id == capture.device?.platformIdentifier }) {
            capture.updateDevice(row)
        }
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
        guard let row = scanState.rows.first(where: { $0.id == platformIdentifier }) else { return }
        guard row.isSupported || row.isProbeRecommended else { return }
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
        guard !phase.supportsLiveActivity || connectionState.selection != nil || permitsStoredDeviceAutoPairing else {
            return
        }
        if case .live = phase {
            switch connectionState {
            case .connecting(_, phase: .subscribing), .identified:
                break
            case .connecting(_, phase: .discoveringServices) where core.isRecordOnlyConnection:
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
        if phase != .live {
            clearSettings()
        }
        switch phase {
        case .connecting, .discoveringServices, .subscribing:
            if let selection = connectionState.selection {
                connectionState = .connecting(selection, phase: phase)
            }
        case .live:
            if core.isRecordOnlyConnection {
                // Record-only is an explicit capture choice. A normal Use/auto-reconnect
                // attempt must never silently turn a known wheel into the capture screen when
                // protocol detection times out (for example while the wheel is powered off).
                if let selection = connectionState.selection {
                    connectionState = .failed(
                        selection,
                        .identificationFailed(.timedOut)
                    )
                    liveActivityIdentity = nil
                    core.disconnectAndScan()
                    syncLiveActivity()
                    break
                }
                connectionState = .picker
                liveActivityIdentity = nil
                syncLiveActivity()
                break
            }
            if let selection = selection(from: core.protocolIdentityCandidate) {
                connectionState = .connected(
                    ConnectionSelection(
                        platformIdentifier: selection.platformIdentifier,
                        title: connectionState.selection?.title ?? selection.title,
                        route: selection.route
                    ))
            } else if let selection = connectionState.selection {
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
        if case .failed = phase {
            phase = .discoveringServices
        }
        connectionState = .retrying(selection, retry: retry)
        syncLiveActivity()
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
        if let platformIdentifier {
            connectionState = .identified(
                ConnectionSelection(
                    platformIdentifier: platformIdentifier,
                    title: selectedDeviceStore.displayName(for: platformIdentifier)
                        ?? localizedAppText("setup.device"),
                    route: .electricUnicycle
                ))
        }
        if platformIdentifier != nil, marker == nil {
            permitsStoredDeviceAutoPairing = false
            rideSessionRestorationState = .complete
            return
        }
        if let platformIdentifier, let marker {
            let markerMatches =
                (try? core.rideSessionStateHandle
                    .rideSessionMarkerMatchesPlatformIdentifier(
                        marker: marker,
                        platformIdentifier: platformIdentifier
                    )) == true
            permitsStoredDeviceAutoPairing = false
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
            switch recoveryResult {
            case .adopted:
                if error == nil,
                    core.rideSessionStateHandle.rideSessionSnapshot().phase == .active
                {
                    lastLiveActivitySnapshot = snapshot
                    lastLiveActivityUpdate = snapshot == nil ? nil : core.now()
                }
            case .ended, .noPersistedRide:
                if restoredPlatformIdentifier == nil,
                    let scanState = devicePickerScanState
                {
                    permitsStoredDeviceAutoPairing = true
                    handleScanStateChange(scanState)
                }
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

    private static func makeSessionDriver(
        rideMapState: MobileRideMapState? = nil
    ) -> any CutoutSessionDriving {
        #if DEBUG
            if let fixture = uiTestFixture {
                return CutoutSessionCore(testScript: fixture.testScript, rideMapState: rideMapState)
            }
        #endif
        if let rideMapState {
            return CutoutSessionCore(rideMapState: rideMapState)
        }
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
            let previousSnapshot = lastLiveActivitySnapshot
        {
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
        let endReason: LiveActivityRideLifecycleEndReason =
            switch phase {
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
            guard let self, self.liveActivityRequestID == requestID else { return }
            self.liveActivityError = error
            if error != nil {
                self.lastLiveActivitySnapshot = nil
                self.lastLiveActivityUpdate = nil
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
            ?? phase.connectingModel
        {
            return .model(model)
        }
        return nil
    }

    private func liveActivityGlyph(for selectedRow: DevicePickerRow?) -> LiveActivityRideGlyph {
        if selectedRow?.connectionRoute == .vescOnewheel
            || core.protocolIdentityCandidate?.support.connectionRoute == .vescOnewheel
        {
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
                || lastLiveActivityUpdate.map({
                    now.elapsed(since: $0).rawValue >= Self.liveActivityUpdateIntervalMilliseconds
                }) != false
        else { return false }

        lastLiveActivitySnapshot = snapshot
        lastLiveActivityUpdate = now
        return true
    }

    func applyCaptureEvent(_ event: CaptureEvent) {
        capture.apply(event)
    }

}

extension SessionConnectionPhase {
    fileprivate var supportsLiveActivity: Bool {
        switch self {
        case .connecting, .discoveringServices, .subscribing, .live:
            true
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .failed:
            false
        }
    }

    fileprivate var connectingModel: ElectricUnicycleModel? {
        guard case .connecting(let model) = self else { return nil }
        return model
    }

    fileprivate var isReconnectingTransport: Bool {
        switch self {
        case .connecting, .discoveringServices, .subscribing:
            true
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .live, .failed:
            false
        }
    }
}
