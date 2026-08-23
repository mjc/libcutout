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
    private static let rideMapPointBatchLimit: UInt32 = 512
    private static let rideMapPreviewPointLimit = 4_096

    private struct MusicCaptureKey: Equatable {
        let provider: MobileMusicProviderDto
        let itemIdentifier: String
        let state: MobileMusicPlaybackStateDto
    }

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
    private(set) var rideMapError: MobileRideMapError?
    private(set) var rideMapPoints = [MobileRideMapPointDto]()
    private(set) var rideMapHistory = [MobileRideMapHistorySummaryDto]()
    private(set) var rideMapHistoryPoints = [MobileRideMapPointDto]()
    private(set) var rideMapHistoryMusicEvents = [MobileMusicRideEventDto]()
    private(set) var rideMapHistoryPointsTruncated = false
    private(set) var selectedRideMapHistoryID: String?
    private(set) var rideMapLastDecision: MobileRideMapDecisionDto?
    private(set) var captureStatus: CaptureStatus?
    private(set) var captureProgress: CaptureProgress?
    private(set) var liveActivityError: LiveActivityRideLifecycleError?
    private(set) var musicNowPlaying: MusicNowPlaying?
    private(set) var musicTimelineEvents = [MobileMusicRideEventDto]()
    private(set) var selectedMusicProvider = MobileMusicProviderDto.appleMusic
    private(set) var isMusicPlayerHidden: Bool
    private(set) var musicHistoryPolicy = MobileMusicHistoryPolicyDto.disabled
    private(set) var isRecordOnlyCapture = false
    private(set) var isFinishingCapture = false
    private(set) var activeCaptureLabels = Set<CaptureQuickLabel>()
    private(set) var recordOnlyDeviceKind: String?
    private(set) var hasSavedDevice = false

    var selectedRideTitle: String? {
        connectionState.selection?.title
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
        switch rideMapSnapshot?.state {
        case .recording, .paused:
            true
        case .stopped, .saved, .discarded, nil:
            false
        }
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
    private let musicPlayerVisibilityStore: MusicPlayerVisibilityStore
    private let musicCoordinator: MusicIntegrationCoordinator
    private let spotifyMusicProvider = SpotifyProviderAdapter()
#if canImport(MediaPlayer) && os(iOS)
    private let appleMusicProvider = AppleMusicProviderAdapter()
#endif
    private var liveActivityIdentity: LiveActivityRideIdentity?
    private var liveActivityGlyph = LiveActivityRideGlyph.electricUnicycle
    private var lastLiveActivitySnapshot: LiveActivityRideSnapshot?
    private var lastLiveActivityUpdate: MonotonicMilliseconds?
    private var liveActivityRequestID: UInt64 = 0
    private var captureFileName: String?
    private var captureNotificationCount = 0
    private var lastMusicCaptureKey: MusicCaptureKey?
    private var captureLabel: String?
    private var hasStarted = false
    private var permitsStoredDeviceAutoPairing = true
    private var rideSessionRestorationState = RideSessionRestorationState.complete
    private var restorationMarkerAtLaunch: Data?
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
            liveActivityManager: LiveActivityRideActivityKitManager(),
            musicPlayerVisibilityStore: MusicPlayerVisibilityStore()
        )
    }

    convenience init(
        core: any CutoutSessionDriving,
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore(),
        rideSessionMarkerStore: RideSessionMarkerStore = RideSessionMarkerStore(),
        liveActivityManager: any LiveActivityRideLifecycleManaging = LiveActivityRideActivityKitManager(),
        musicPlayerVisibilityStore: MusicPlayerVisibilityStore = MusicPlayerVisibilityStore()
    ) {
        self.init(
            core: core,
            permitsStoredDeviceAutoPairing: true,
            selectedDeviceStore: selectedDeviceStore,
            rideSessionMarkerStore: rideSessionMarkerStore,
            liveActivityManager: liveActivityManager,
            musicPlayerVisibilityStore: musicPlayerVisibilityStore
        )
    }

    private init(
        core: any CutoutSessionDriving,
        permitsStoredDeviceAutoPairing: Bool,
        selectedDeviceStore: DevicePickerSelectionStore,
        rideSessionMarkerStore: RideSessionMarkerStore,
        liveActivityManager: any LiveActivityRideLifecycleManaging,
        musicPlayerVisibilityStore: MusicPlayerVisibilityStore
    ) {
        self.permitsStoredDeviceAutoPairing = permitsStoredDeviceAutoPairing
        self.core = core
        rideMapStorageError = core.rideMapStorageError
        liveActivityCoordinator = LiveActivityRideLifecycleCoordinator(
            manager: liveActivityManager,
            sessionState: core.rideSessionStateHandle,
            markerStore: rideSessionMarkerStore
        )
        self.selectedDeviceStore = selectedDeviceStore
        self.rideSessionMarkerStore = rideSessionMarkerStore
        self.musicPlayerVisibilityStore = musicPlayerVisibilityStore
        isMusicPlayerHidden = musicPlayerVisibilityStore.isHidden
        musicCoordinator = MusicIntegrationCoordinator(rideMapState: core.rideMapStateHandle)
        musicTimelineEvents = musicCoordinator.recordedEvents
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
            self?.rideMapError = error
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

    func handleMusicCommand(_ command: MobileMusicCommandDto) {
#if canImport(MediaPlayer) && os(iOS)
        guard let nowPlaying = musicNowPlaying, nowPlaying.supports(command) else { return }
        if nowPlaying.provider == .spotify {
            spotifyMusicProvider.perform(command)
        } else {
            appleMusicProvider.perform(command)
        }
        refreshMusicSnapshot()
#else
        _ = command
#endif
    }

    func dismissMusicPlayer() {
        musicPlayerVisibilityStore.setHidden(true)
        isMusicPlayerHidden = true
        musicNowPlaying = nil
    }

    func restoreMusicPlayer() {
        musicPlayerVisibilityStore.setHidden(false)
        isMusicPlayerHidden = false
        musicNowPlaying = musicCoordinator.nowPlaying
    }

    func selectMusicProvider(_ provider: MobileMusicProviderDto) {
        selectedMusicProvider = provider
        if isMusicPlayerHidden == false {
            refreshMusicSnapshot()
        }
    }

    func refreshMusicSnapshot() {
#if canImport(MediaPlayer) && os(iOS)
        let observedAtMs = core.now().rawValue
        let observation = if selectedMusicProvider == .spotify {
            // Keep the explicit Spotify-unavailable state until App Remote is
            // installed; do not silently replace it with Apple Music data.
            MusicProviderObservation(
                snapshot: spotifyMusicProvider.unavailableSnapshot(observedAtMs: observedAtMs)
            )
        } else {
            appleMusicProvider.observation(observedAtMs: observedAtMs)
        }
        _ = ingestMusicObservation(observation)
#endif
    }

    /// Accepts a provider callback without coupling the app model to a
    /// particular SDK. History failures remain best-effort while the current
    /// provider state still reaches the player and ride recorder.
    @discardableResult
    func ingestMusicObservation(
        _ observation: MusicProviderObservation,
        wallClockAtMs: UInt64? = nil,
        clockUncertaintyMs: UInt64 = 1_000
    ) -> Bool {
        let wallClockAtMs = wallClockAtMs ?? UInt64(Date().timeIntervalSince1970 * 1_000)
        do {
            let outcome = try musicCoordinator.ingest(
                observation: observation,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
            updateMusicCaptureObservationIfCurrent(
                observation,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs,
                outcome: outcome
            )
            refreshMusicTimeline()
            musicNowPlaying = isMusicPlayerHidden ? nil : musicCoordinator.nowPlaying
            return true
        } catch {
            updateMusicCaptureObservationIfCurrent(
                observation,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
            refreshMusicTimeline()
            musicNowPlaying = isMusicPlayerHidden ? nil : musicCoordinator.nowPlaying
            return false
        }
    }

    private func updateMusicCaptureObservationIfCurrent(
        _ observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64,
        outcome: MobileMusicTimelineOutcomeDto? = nil
    ) {
        guard musicCoordinator.nowPlaying == MusicNowPlaying(observation: observation) else { return }
        guard shouldUpdateMusicCaptureContext(for: outcome) else { return }
        guard allowsMusicCaptureContext else {
            core.updateMusicCaptureObservation(nil)
            return
        }
        updateMusicCaptureObservation(
            from: observation,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs
        )
    }

    private func shouldUpdateMusicCaptureContext(
        for outcome: MobileMusicTimelineOutcomeDto?
    ) -> Bool {
        switch outcome {
        case .outOfOrder?, .rideNotOpen?, .full?:
            return false
        default:
            return true
        }
    }

    private var allowsMusicCaptureContext: Bool {
        guard let state = core.rideMapStateHandle.currentSnapshot()?.state else { return false }
        return state == .recording || state == .paused
    }

    private func updateMusicCaptureObservation(
        from observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) {
        let captureObservation = musicCaptureObservation(
            from: observation,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs
        )
        guard captureObservation != nil
                || musicHistoryPolicy == .disabled
                || observation.snapshot.item == nil
                || observation.snapshot.positionMilliseconds == nil else { return }
        core.updateMusicCaptureObservation(captureObservation)
    }

    private func musicCaptureObservation(
        from observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) -> MobilePevcapMusicEventDto? {
        guard musicHistoryPolicy != .disabled else {
            lastMusicCaptureKey = nil
            return nil
        }
        guard
            let item = observation.snapshot.item,
            let positionMilliseconds = observation.snapshot.positionMilliseconds
        else {
            lastMusicCaptureKey = nil
            return nil
        }

        let key = MusicCaptureKey(
            provider: observation.snapshot.provider,
            itemIdentifier: item.identifier,
            state: observation.snapshot.state
        )
        guard key != lastMusicCaptureKey else { return nil }
        lastMusicCaptureKey = key

        return MobilePevcapMusicEventDto(
            provider: observation.snapshot.provider,
            trackId: item.identifier,
            trackPositionMs: positionMilliseconds,
            wallClockUnixMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs,
            rideSequence: nil
        )
    }

    func setMusicHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) -> Bool {
        let previousPolicy = musicHistoryPolicy
        musicHistoryPolicy = policy
        lastMusicCaptureKey = nil
        do {
            try musicCoordinator.setHistoryPolicy(policy)
            refreshMusicTimeline()
            if policy == .disabled {
                core.updateMusicCaptureObservation(nil)
            }
            return true
        } catch MobileRideMapError.NoActiveRide {
            // Keep the choice as the default for the next ride. Rust will
            // apply it once a recording exists.
            if policy == .disabled {
                core.updateMusicCaptureObservation(nil)
            }
            return true
        } catch {
            musicHistoryPolicy = previousPolicy
            return false
        }
    }

    func monitorMusic() async {
#if canImport(MediaPlayer) && os(iOS)
        guard await appleMusicProvider.requestAuthorization() else {
            _ = ingestMusicObservation(MusicProviderObservation(
                snapshot: appleMusicProvider.unauthorizedSnapshot(
                    observedAtMs: core.now().rawValue
                )
            ))
            return
        }
        appleMusicProvider.startMonitoring { [weak self] in
            self?.refreshMusicSnapshot()
        }
        defer { appleMusicProvider.stopMonitoring() }
        while !Task.isCancelled {
            refreshMusicSnapshot()
            do {
                try await Task.sleep(for: .seconds(1))
            } catch {
                return
            }
        }
#endif
    }

    private func restoreRideMapState() {
        rideMapSnapshot = core.rideMapStateHandle.currentSnapshot()
        refreshMusicTimeline()
        guard rideMapSnapshot != nil else { return }
        guard let (points, _) = collectRideMapPoints({ cursor, limit in
            core.rideMapStateHandle.pointsAfter(afterCursor: cursor, limit: limit)
        }) else { return }
        rideMapPoints = points
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
        let didStart = applyRideMapCommand(resetPoints: true) {
            try core.rideMapStateHandle.startGpsOnly(
                atMs: currentMonotonicTime.rawValue,
                lastConnectedVehicle: selectedDeviceStore.platformIdentifier
            )
        }
        guard didStart else { return false }
        resetMusicCaptureContext()
        refreshMusicTimeline()
        // Music history is deliberately best-effort: a provider or storage
        // failure must never prevent a ride from starting.
        try? musicCoordinator.setHistoryPolicy(musicHistoryPolicy)
        return true
    }

    @discardableResult
    func pauseRideMap() -> Bool {
        applyRideMapCommand { try core.rideMapStateHandle.pause() }
    }

    @discardableResult
    func resumeRideMap() -> Bool {
        applyRideMapCommand { try core.rideMapStateHandle.resume() }
    }

    @discardableResult
    func stopRideMap() -> Bool {
        applyRideMapCommand { try core.rideMapStateHandle.stop() }
    }

    @discardableResult
    func saveRideMap() -> Bool {
        guard applyRideMapCommand({ try core.rideMapStateHandle.save() }) else {
            return false
        }
        resetMusicCaptureContext()
        loadRideMapHistory()
        return true
    }

    @discardableResult
    func discardRideMap() -> Bool {
        guard applyRideMapCommand({ try core.rideMapStateHandle.discard() }) else {
            return false
        }
        resetMusicCaptureContext()
        rideMapHistoryPoints = []
        rideMapHistoryPointsTruncated = false
        rideMapHistoryMusicEvents = []
        loadRideMapHistory()
        return true
    }

    func loadRideMapHistory() {
        do {
            rideMapHistory = try core.rideMapStateHandle.storedSummaries(limit: 50)
            rideMapError = nil
            guard let first = rideMapHistory.first else {
                selectedRideMapHistoryID = nil
                rideMapHistoryPoints = []
                rideMapHistoryPointsTruncated = false
                rideMapHistoryMusicEvents = []
                return
            }
            selectRideMapHistory(first.rideId)
        } catch {
            rideMapError = error as? MobileRideMapError
            rideMapHistory = []
            selectedRideMapHistoryID = nil
            rideMapHistoryPoints = []
            rideMapHistoryPointsTruncated = false
            rideMapHistoryMusicEvents = []
        }
    }

    func selectRideMapHistory(_ rideID: String) {
        guard rideMapHistory.contains(where: { $0.rideId == rideID }) else { return }
        selectedRideMapHistoryID = rideID
        rideMapHistoryPointsTruncated = false
        do {
            let historyEvents = try core.rideMapStateHandle.storedMusicEvents(rideId: rideID)
            guard let (points, truncated) = try collectRideMapPoints({ cursor, limit in
                try core.rideMapStateHandle.storedPointsAfter(
                    rideId: rideID,
                    afterCursor: cursor,
                    limit: limit
                )
            }) else {
                rideMapHistoryPoints = []
                rideMapHistoryPointsTruncated = false
                return
            }
            rideMapError = nil
            rideMapHistoryPoints = points
            rideMapHistoryPointsTruncated = truncated
            rideMapHistoryMusicEvents = historyEvents
        } catch {
            rideMapError = error as? MobileRideMapError
            rideMapHistoryPoints = []
            rideMapHistoryPointsTruncated = false
            rideMapHistoryMusicEvents = []
        }
    }

    @discardableResult
    func deleteMusicHistory(rideID: String) -> Bool {
        do {
            try core.rideMapStateHandle.deleteMusicHistory(rideId: rideID)
            if selectedRideMapHistoryID == rideID {
                selectedRideMapHistoryID = nil
                rideMapHistoryPoints = []
                rideMapHistoryPointsTruncated = false
                rideMapHistoryMusicEvents = []
            }
            let isCurrentRide = core.rideMapStateHandle.currentSnapshot()?.rideId == rideID
            if isCurrentRide {
                resetActiveMusicHistoryState()
            }
            rideMapError = nil
            return true
        } catch {
            rideMapError = error as? MobileRideMapError
            return false
        }
    }

    private func resetMusicCaptureContext() {
        lastMusicCaptureKey = nil
        core.updateMusicCaptureObservation(nil)
    }

    private func resetActiveMusicHistoryState() {
        musicHistoryPolicy = .disabled
        resetMusicCaptureContext()
        try? musicCoordinator.setHistoryPolicy(.disabled)
        refreshMusicTimeline()
    }

    private func refreshMusicTimeline() {
        musicTimelineEvents = musicCoordinator.recordedEvents
    }

    private func collectRideMapPoints(
        _ fetch: (UInt64, UInt32) throws -> MobileRideMapPointBatchDto?
    ) rethrows -> ([MobileRideMapPointDto], Bool)? {
        var points = [MobileRideMapPointDto]()
        var cursor: UInt64 = 0
        repeat {
            guard let batch = try fetch(cursor, Self.rideMapPointBatchLimit) else {
                return nil
            }
            points.append(contentsOf: batch.points)
            cursor = batch.nextCursor
            if points.count >= Self.rideMapPreviewPointLimit {
                return (points, batch.hasMore)
            }
            if batch.hasMore == false {
                return (points, false)
            }
        } while true
    }

    private func applyRideMapDecision(
        snapshot: MobileRideMapSnapshotDto,
        decision: MobileRideMapDecisionDto
    ) {
        rideMapError = nil
        rideMapSnapshot = snapshot
        rideMapLastDecision = decision
        if case let .accepted(point, _) = decision {
            rideMapPoints.append(point)
        }
    }

    private func applyRideMapCommand(
        resetPoints: Bool = false,
        _ command: () throws -> MobileRideMapSnapshotDto
    ) -> Bool {
        do {
            rideMapSnapshot = try command()
            rideMapError = nil
            if resetPoints {
                rideMapPoints.removeAll(keepingCapacity: true)
                rideMapLastDecision = nil
            }
            return true
        } catch {
            rideMapError = error as? MobileRideMapError
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
            selectedDeviceStore.save(platformIdentifier: platformIdentifier)
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
        selectedDeviceStore.clear()
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
