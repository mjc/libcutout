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

    let device: DevicePresentationModel
    var displayState: RideDisplayState { device.displayState }
    var phase: SessionConnectionPhase { device.phase }
    var devicePickerScanState: DevicePickerScanState? { device.scanState }
    var connectionState: ConnectionState { device.connectionState }
    var faultHistoryReadback: FaultHistoryReadback? { device.faultHistoryReadback }
    var bmsSnapshot: BmsSnapshot? { device.bmsSnapshot }
    var phoneLocationReadback: PhoneLocationReadback { device.phoneLocationReadback }
    let liveRide: LiveRideModel
    let music: MusicFeatureModel
    private(set) var liveActivityError: LiveActivityRideLifecycleError?
    private(set) var rideMapCheckpointError: MobileRideMapError?
    private(set) var rideAutostartEnabled: Bool?
    private(set) var rideAutostartSettingsBusy = false
    private(set) var rideAutostartSettingsError = false
    let capture: CaptureFeatureModel
    var isRecordOnlyCapture: Bool { capture.isManualCapture }
    var hasSavedDevice: Bool { device.hasSavedDevice }
    var settings: DeviceSettings? { device.settings }
    private(set) var phoneAlarmSettings: MobilePhoneAlarmPreferencesDto?
    private(set) var phoneAlarmAuthorization = PhoneRideAlarmAuthorization.unavailable
    private(set) var phoneAlarmDeliveryError: String?
    private var rideMapCheckpointTask: Task<Void, Never>?
    private var rideMapCheckpointLease: RideBackgroundTask?
    private var rideMapCheckpointGeneration: UInt64 = 0
    private(set) var cameraMediaReferences: [CameraMediaReference] = []

    var selectedRideTitle: String? { device.selectedRideTitle }

    /// Supplies the active capture identity to the camera route without
    /// giving the view ownership of capture state.
    var currentCameraCaptureIdentity: () -> (fileName: String, generation: CaptureGeneration)? {
        { [weak self] in
            guard
                let self,
                let fileName = self.capture.fileName,
                let generation = self.capture.activeGeneration
            else {
                return nil
            }
            return (fileName, generation)
        }
    }

    /// Exposes Rust-owned camera/session state to the camera route.
    var cameraSessionStateHandle: CutoutSessionStateHandle {
        core.rideSessionStateHandle
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

    var selectedRideIdentifier: String? { device.selectedRideIdentifier }
    var selectedConnectionRoute: DevicePickerConnectionRoute? { device.selectedConnectionRoute }
    var speed: SpeedReadout { device.speed }
    var rideMapSpeed: SpeedReadout {
        SpeedReadout(millimetersPerSecond: liveRide.snapshot?.liveSpeed?.millimetresPerSecond)
    }

    var currentMonotonicTime: MonotonicMilliseconds {
        core.now()
    }

    var rideState: EucRideScreenState { device.rideState }
    var eucRidePresentationState: EucRideScreenState? { device.eucRidePresentationState }
    var vescRideSnapshot: VescRideSnapshot? { device.vescRideSnapshot }
    var connectionStatusText: String { device.connectionStatusText }

    func annotateCapture(key: String, value: String) {
        _ = core.annotateCapture(key: key, value: value)
    }

    /// Records a completed camera download against the capture identity that
    /// was captured when the operation started.
    func recordCameraMediaReference(
        captureFileName: String,
        captureGeneration: CaptureGeneration,
        source: CameraSourceKind = .novatekR3Pro,
        media: CameraMediaEvidence,
        localURL: URL
    ) {
        guard !captureFileName.isEmpty else { return }
        guard capture.activeGeneration == captureGeneration else { return }
        guard captureFileName == capture.fileName else { return }
        let input = MobileCameraMediaProvenanceInput(
            captureGeneration: captureGeneration.dto,
            source: source.mobileDto,
            cameraPath: media.path,
            sizeBytes: media.sizeBytes,
            cameraTimecode: media.timecode,
            cameraTime: media.time,
            rideCaptureFileName: captureFileName,
            capturedAtMonotonicMs: currentMonotonicTime.rawValue,
            capturedAtWallClockMs: UInt64(max(0, Date().timeIntervalSince1970 * 1_000)),
            clockUncertainty: .unknown
        )
        let sessionState = core.rideSessionStateHandle
        guard (try? sessionState.recordCameraMediaProvenance(input: input)) != nil else {
            return
        }
        let provenanceRecords = sessionState.cameraMediaProvenance()
        guard
            let provenance = provenanceRecords.first(where: {
                $0.source == source.mobileDto
                    && $0.cameraPath == media.path
                    && $0.rideCaptureFileName == captureFileName
            })
        else {
            return
        }

        let reference = CameraMediaReference(provenance: provenance, localURL: localURL)
        guard capture.activeGeneration == captureGeneration,
            captureFileName == capture.fileName
        else { return }
        if let existingIndex = cameraMediaReferences.firstIndex(where: {
            $0.source.mobileDto == provenance.source
                && $0.rideCaptureFileName == provenance.rideCaptureFileName
                && $0.cameraPath == provenance.cameraPath
        }) {
            cameraMediaReferences[existingIndex] = reference
        } else {
            cameraMediaReferences.append(reference)
        }
        cameraMediaReferences.removeAll { reference in
            !provenanceRecords.contains { retained in
                reference.source.mobileDto == retained.source
                    && reference.rideCaptureFileName == retained.rideCaptureFileName
                    && reference.cameraPath == retained.cameraPath
            }
        }
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
    private var musicHistoryRestoreTask: Task<Void, Never>?
    private var musicHistoryRestoreGeneration: UInt64 = 0
    private var phoneAlarmAuthorizationTask: Task<Void, Never>?
    private var phoneAlarmAuthorizationGeneration: UInt64 = 0
    private static let liveActivityUpdateIntervalMilliseconds: UInt64 = 1_000

    isolated deinit {
        musicHistoryRestoreTask?.cancel()
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
        let now = UInt64(ProcessInfo.processInfo.systemUptime * 1_000)
        _ = try await state.restoreCommand(atMs: now)
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
        device = DevicePresentationModel(selectedDeviceStore: selectedDeviceStore)
        device.protocolIdentityCandidate = core.protocolIdentityCandidate
        self.rideHistory = rideHistory
        self.permitsStoredDeviceAutoPairing = permitsStoredDeviceAutoPairing
        self.core = core
        liveRide = LiveRideModel(
            state: core.rideMapStateHandle,
            storageError: core.rideMapStorageError,
            availability: core.rideMapAvailability,
            now: { core.now().rawValue },
            executeCommand: { command in
                switch command {
                case let .startGpsOnly(atMs, musicHistoryPolicy):
                    try await core.startRideMapGpsOnly(atMs: atMs, musicHistoryPolicy: musicHistoryPolicy)
                case let .pause(expected, atMs):
                    try await core.pauseRideMap(expected: expected, atMs: atMs)
                case let .resume(expected, atMs):
                    try await core.resumeRideMap(expected: expected, atMs: atMs)
                case let .stop(expected, atMs):
                    try await core.stopRideMap(expected: expected, atMs: atMs)
                case let .save(expected):
                    try await core.saveRideMap(expected: expected)
                case let .discard(expected):
                    try await core.discardRideMap(expected: expected)
                }
            }
        )
        capture = CaptureFeatureModel(
            sessionState: core.rideSessionStateHandle,
            flush: { await core.flushCapture() },
            finish: { await core.finishCapture() },
            changeCaptureLabel: { try core.changeCaptureLabel(generation: $0, action: $1) }
        )
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
        self.music.timelineEvents = music.coordinator.recordedEvents
        restoreRideMapState()
        CutoutSessionCallbackRegistrar(core: core).install(
            .init(
                displayState: { [weak self] displayState in
                    self?.device.displayState = displayState
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
                    self.device.settings = snapshot
                },
                faultHistory: { [weak self] faultHistoryReadback in
                    self?.device.faultHistoryReadback = faultHistoryReadback
                },
                bmsSnapshot: { [weak self] bmsSnapshot in
                    self?.device.bmsSnapshot = bmsSnapshot
                },
                phoneLocation: { [weak self] snapshot, receivedAt in
                    self?.device.phoneLocationReadback = PhoneLocationReadback(
                        snapshot: snapshot, receivedAt: receivedAt)
                },
                rideMapDecision: { [weak self] snapshot, decision in
                    self?.liveRide.applyDecision(snapshot: snapshot, decision: decision)
                },
                rideMapSnapshot: { [weak self] snapshot in
                    self?.liveRide.applySnapshot(snapshot)
                },
                rideMapError: { [weak self] event in
                    self?.liveRide.applyError(event)
                },
                rideMapAvailability: { [weak self] availability in
                    self?.liveRide.setAvailability(availability)
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
        liveRide.restore()
        musicHistoryRestoreTask?.cancel()
        musicHistoryRestoreGeneration &+= 1
        let historyGeneration = musicHistoryRestoreGeneration
        guard let restoredRideID = liveRide.snapshot?.rideID else {
            music.rideMapClosed()
            return
        }
        musicHistoryRestoreTask = Task { @MainActor [weak self] in
            do {
                let history = try await state.currentMusicHistoryAsync()
                guard
                    !Task.isCancelled,
                    let self,
                    self.musicHistoryRestoreGeneration == historyGeneration,
                    self.liveRide.snapshot?.rideID == restoredRideID
                else { return }
                self.music.synchronizeHistory(history)
            } catch {
                guard
                    !Task.isCancelled,
                    let self,
                    self.musicHistoryRestoreGeneration == historyGeneration,
                    self.liveRide.snapshot?.rideID == restoredRideID
                else { return }
                self.music.setHistoryPersistenceError(appRideMapError(error))
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

    func loadRideAutostartSetting() async {
        guard !rideAutostartSettingsBusy else { return }
        guard let state = core.rideMapStateHandle else {
            rideAutostartSettingsError = true
            return
        }
        rideAutostartSettingsBusy = true
        defer { rideAutostartSettingsBusy = false }
        do {
            rideAutostartEnabled = try await Task.detached(priority: .userInitiated) {
                try state.rideAutostartEnabled()
            }.value
            rideAutostartSettingsError = false
        } catch {
            rideAutostartSettingsError = true
        }
    }

    @discardableResult
    func setRideAutostartEnabled(_ enabled: Bool) async -> Bool {
        guard !rideAutostartSettingsBusy else { return false }
        guard rideAutostartEnabled != nil, let state = core.rideMapStateHandle else {
            rideAutostartSettingsError = true
            return false
        }
        rideAutostartSettingsBusy = true
        defer { rideAutostartSettingsBusy = false }
        do {
            // Await the durable result even if Setup closes while SQLite commits.
            try await Task.detached(priority: .userInitiated) {
                try state.setRideAutostartEnabled(enabled)
            }.value
            rideAutostartEnabled = enabled
            rideAutostartSettingsError = false
            return true
        } catch {
            rideAutostartSettingsError = true
            return false
        }
    }

    @discardableResult
    func startGpsOnlyRide() async -> Bool {
        let connectionToken = core.connectionSnapshot.token
        let started = await applyRideMapCommand(
            .startGpsOnly(
                atMs: currentMonotonicTime.rawValue,
                musicHistoryPolicy: music.historyPolicyStore.policy
            ))
        guard started else { return false }
        if let connectionToken {
            _ = core.resetTripMeterForNewRide(token: connectionToken)
        }
        core.resetRideMapLocationAdmission()
        if let error = await music.adoptHistoryForNewRideAsync() {
            liveRide.setError(error)
            guard core.rideMapStateHandle?.currentSnapshot() != nil else {
                return false
            }
        }
        return true
    }

    @discardableResult
    func pauseRideMap() async -> Bool {
        guard let expected = rideMapCommandToken() else { return false }
        return await applyRideMapCommand(.pause(expected: expected, atMs: currentMonotonicTime.rawValue))
    }

    @discardableResult
    func resumeRideMap() async -> Bool {
        guard let expected = rideMapCommandToken() else { return false }
        return await applyRideMapCommand(.resume(expected: expected, atMs: currentMonotonicTime.rawValue))
    }

    @discardableResult
    func stopRideMap() async -> Bool {
        guard let expected = rideMapCommandToken() else { return false }
        let stopped = await applyRideMapCommand(.stop(expected: expected, atMs: currentMonotonicTime.rawValue))
        if stopped {
            liveRide.invalidateProjection(clearPoints: false)
            music.clearMusicCaptureContext()
        }
        return stopped
    }

    func refreshRideMapDuration() {
        liveRide.refreshDuration()
    }

    @discardableResult
    func saveRideMap() async -> Bool {
        guard let expected = rideMapCommandToken() else { return false }
        guard await applyRideMapCommand(.save(expected: expected)) else {
            return false
        }
        liveRide.invalidateProjection(clearPoints: false)
        music.rideMapClosed()
        reloadRideHistory()
        return true
    }

    @discardableResult
    func discardRideMap() async -> Bool {
        guard let expected = rideMapCommandToken() else { return false }
        guard await applyRideMapCommand(.discard(expected: expected)) else {
            return false
        }
        liveRide.invalidateProjection(clearPoints: true)
        music.rideMapClosed()
        rideHistory.clearRouteProjection()
        reloadRideHistory()
        return true
    }

    private func reloadRideHistory() {
        rideHistory.reload()
    }

    private func applyRideMapCommand(_ command: LiveRideCommand) async -> Bool {
        guard await liveRide.perform(command) != nil else { return false }
        return true
    }

    private func rideMapCommandToken() -> MobileRideMapCommandTokenDto? {
        guard let token = liveRide.snapshot?.commandToken else {
            liveRide.setError(.noActiveRide)
            return nil
        }
        return token
    }

    func submitDeviceSetting(token: ConnectionAttemptToken, id: DeviceSettingID, value: DeviceSettingValue) throws {
        try core.submitDeviceSetting(token: token, id: id, value: value)
    }

    func submitDeviceAction(token: ConnectionAttemptToken, id: DeviceActionID) throws {
        try core.submitDeviceAction(token: token, id: id)
    }

    func pair(platformIdentifier: String) -> Bool {
        guard core.rideSessionStateHandle.captureLifecycleSnapshot().canPair else { return false }
        let outcome = device.pair(
            platformIdentifier: platformIdentifier,
            mayRetryCurrentSelection: liveActivityError != nil
        ) { selectedRow in
            liveActivityError = nil
            permitsStoredDeviceAutoPairing = true
            return capture.requestStart(device: CaptureDeviceIdentity(row: selectedRow), description: nil) {
                core.pair(platformIdentifier: platformIdentifier)
            }
        }
        switch outcome {
        case .ignored, .unavailable:
            return false
        case .refused:
            permitsStoredDeviceAutoPairing = false
            return false
        case let .accepted(selectedRow):
            syncPhoneAlarmPreferences()
            drainPhoneAlarmActions()
            liveActivityIdentity = liveActivityIdentity(for: selectedRow)
            liveActivityGlyph = liveActivityGlyph(for: selectedRow)
            syncLiveActivity()
            return true
        }
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
        let captureIdentity = devicePickerScanState?.rows.first { $0.id == platformIdentifier }.map(
            CaptureDeviceIdentity.init)
        let didStart = capture.requestStart(
            device: captureIdentity, description: annotationKind.isEmpty ? nil : annotationKind
        ) {
            core.recordOnly(
                platformIdentifier: platformIdentifier,
                note: "user-initiated Bluetooth capture",
                annotations: annotations
            )
        }
        guard didStart else { return false }
        capture.apply(.lifecycle(core.rideSessionStateHandle.captureLifecycleSnapshot()))

        device.connectionState = .picker
        permitsStoredDeviceAutoPairing = false
        liveActivityIdentity = nil
        liveActivityGlyph = .electricUnicycle
        syncLiveActivity()
        return true
    }

    func appDidEnterBackground() {
        music.sceneDidEnterBackground()
        checkpointRideMapForBackground()
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

    private func checkpointRideMapForBackground() {
        guard rideMapCheckpointTask == nil else { return }
        rideMapCheckpointGeneration &+= 1
        let generation = rideMapCheckpointGeneration
        rideMapCheckpointLease = RideBackgroundTask { [weak self] in
            self?.cancelRideMapCheckpoint(generation: generation)
        }
        let core = core
        rideMapCheckpointTask = Task { [weak self] in
            do {
                try await core.checkpointRideMap()
                guard self?.rideMapCheckpointGeneration == generation else { return }
                self?.rideMapCheckpointError = nil
            } catch let error as MobileRideMapError {
                guard self?.rideMapCheckpointGeneration == generation else { return }
                self?.rideMapCheckpointError = error
            } catch {
                guard self?.rideMapCheckpointGeneration == generation else { return }
                self?.rideMapCheckpointError = .storageError(error.localizedDescription)
            }
            guard let self, self.rideMapCheckpointGeneration == generation else { return }
            self.rideMapCheckpointTask = nil
            self.rideMapCheckpointLease?.end()
            self.rideMapCheckpointLease = nil
        }
    }

    private func cancelRideMapCheckpoint(generation: UInt64) {
        guard rideMapCheckpointGeneration == generation else { return }
        rideMapCheckpointGeneration &+= 1
        rideMapCheckpointTask?.cancel()
        rideMapCheckpointTask = nil
        rideMapCheckpointLease?.end()
        rideMapCheckpointLease = nil
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

    @discardableResult
    func disconnectTransport() async -> Bool {
        let expectedRecordingToken = core.rideMapStateHandle?.currentSnapshot(atMs: core.now().rawValue)?.recordingToken
        let connectionGeneration = core.connectionSnapshot.generation
        do {
            try await core.prepareRideMapForDisconnect(
                expectedRecordingToken: expectedRecordingToken,
                connectionGeneration: connectionGeneration
            )
            rideMapCheckpointError = nil
        } catch let error as MobileRideMapError {
            rideMapCheckpointError = error
            return false
        } catch {
            rideMapCheckpointError = .storageError(error.localizedDescription)
            return false
        }
        guard core.disconnectAndScan(expectedGeneration: connectionGeneration) else { return false }
        applyPhoneAlarmActions(core.rideSessionStateHandle.deactivatePhoneAlarmDevice())
        phoneAlarmSettings = nil
        endLiveActivity(reason: .disconnected)
        capture.clearLabels()
        device.connectionState = .picker
        device.phase = .scanning
        liveActivityIdentity = nil
        liveActivityGlyph = .electricUnicycle
        permitsStoredDeviceAutoPairing = false
        clearSettings()
        return true
    }

    private func clearSettings() {
        device.settings = nil
    }

    func forgetSavedDevice() async {
        guard await disconnectTransport() else { return }
        device.forgetSavedDevice()
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
        device.applyProtocolIdentityCandidate(
            candidate,
            allowsRidePresentation: !isRecordOnlyCapture
        )
        guard isRecordOnlyCapture != true else {
            liveActivityIdentity = nil
            liveActivityGlyph = .electricUnicycle
            syncLiveActivity()
            return
        }
        if let model = candidate?.support.electricUnicycleModel {
            liveActivityIdentity = .model(model)
            liveActivityGlyph = .electricUnicycle
        }
        guard let candidate,
            candidate.support.isSupported,
            candidate.support.connectionRoute != nil
        else {
            syncLiveActivity()
            return
        }
        if candidate.support.connectionRoute == .vescOnewheel {
            liveActivityIdentity = vescRideIdentity(using: candidate.displayName)
            liveActivityGlyph = .floatwheelAtom
        }
        syncLiveActivity()
    }

    private func handleScanStateChange(_ scanState: DevicePickerScanState) {
        device.scanState = scanState
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
        self.device.phase = phase
        if phase != .live {
            clearSettings()
        }
        switch phase {
        case .connecting, .discoveringServices, .subscribing:
            if let selection = connectionState.selection {
                device.connectionState = .connecting(selection, phase: phase)
            }
        case .live:
            if core.isRecordOnlyConnection {
                // Record-only is an explicit capture choice. A normal Use/auto-reconnect
                // attempt must never silently turn a known wheel into the capture screen when
                // protocol detection times out (for example while the wheel is powered off).
                if let selection = connectionState.selection {
                    device.connectionState = .failed(
                        selection,
                        .identificationFailed(.timedOut)
                    )
                    liveActivityIdentity = nil
                    core.disconnectAndScan()
                    syncLiveActivity()
                    break
                }
                device.connectionState = .picker
                liveActivityIdentity = nil
                syncLiveActivity()
                break
            }
            if let selection = DevicePresentationModel.connectionSelection(
                from: core.protocolIdentityCandidate
            ) {
                device.connectionState = .connected(
                    ConnectionSelection(
                        platformIdentifier: selection.platformIdentifier,
                        title: connectionState.selection?.title ?? selection.title,
                        route: selection.route
                    ))
            } else if let selection = connectionState.selection {
                device.connectionState = .connected(selection)
            }
        case let .failed(failure):
            guard let selection = connectionState.selection else { return }
            device.connectionState = .failed(selection, failure)
            let rows = devicePickerScanState?.rows ?? []
            device.scanState = .failed(phase.displayText, rows: rows)
        case .bluetoothPermissionDenied, .bluetoothUnavailable:
            device.connectionState = .picker
        case .starting, .scanning:
            break
        }
    }

    private func handleReconnectScheduled(_ retry: SessionConnectionRetry) {
        guard let selection = connectionState.selection,
            selection.platformIdentifier == retry.platformIdentifier
        else { return }
        if case .failed = phase {
            device.phase = .discoveringServices
        }
        device.connectionState = .retrying(selection, retry: retry)
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
            device.connectionState = .identified(
                ConnectionSelection(
                    platformIdentifier: platformIdentifier,
                    title: device.persistedVehicleName(for: platformIdentifier)
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
            case .reconnecting:
                break
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
        if case .started = event {
            core.rideSessionStateHandle.clearCameraMediaProvenance()
            cameraMediaReferences.removeAll(keepingCapacity: true)
        }
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
