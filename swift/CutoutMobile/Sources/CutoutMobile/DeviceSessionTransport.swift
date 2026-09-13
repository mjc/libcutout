import CutoutMobileFFI
import Foundation

/// Queue-confined CoreBluetooth effects for one Rust-owned, protocol-verified attempt.
final class DeviceSessionTransport: @unchecked Sendable {
    let token: ConnectionAttemptToken
    private let state: CutoutSessionStateHandle
    private let planner: CoreBluetoothTransportPlanner
    private let context: CoreBluetoothCaptureContext
    private weak var sink: CoreBluetoothOperationSink?
    private let executor: CoreBluetoothOperationExecutor
    private let queue: DispatchQueue
    private let clock: MonotonicClock
    private let chargeEstimator = MobileChargeEstimator()
    private let voltageSagStore = VoltageSagModelStore()
    private var persistedVoltageSagObservations: UInt16 = 0
    private var chargeEstimate = ChargeEstimateState.missingProfile
    private var timer: DispatchSourceTimer?
    private var waitingForSubscription: BluetoothUuid?
    private var pendingOperations: [CoreBluetoothPlannedOperation] = []
    private(set) var records: [CoreBluetoothLiveRecord] = []
    private var publishedControls: DeviceControlsSnapshot?
    private var lastControlsPublication: MonotonicMilliseconds?
    var onControlsChange: ((DeviceControlsSnapshot) -> Void)?

    init(
        state: CutoutSessionStateHandle,
        token: ConnectionAttemptToken,
        advertisement: CoreBluetoothAdvertisement,
        writeLimit: TransportWriteLimitBytes,
        operationSink: CoreBluetoothOperationSink,
        queue: DispatchQueue,
        clock: MonotonicClock
    ) {
        self.state = state
        self.token = token
        self.planner = CoreBluetoothTransportPlanner(writeLimit: writeLimit)
        self.context = CoreBluetoothCaptureContext(
            platformIdentifier: advertisement.peripheralIdentifier,
            advertisement: advertisement,
            writeLimit: writeLimit
        )
        self.sink = operationSink
        self.executor = CoreBluetoothOperationExecutor(sink: operationSink)
        self.queue = queue
        self.clock = clock
        let controls = state.deviceControlsSnapshot(validationMode: false)
        if controls.connection.token == token, let profile = controls.defaultChargeProfile {
            chargeEstimator.configureProfile(profile: profile)
        }
        if let model = voltageSagStore.load(for: token.platformIdentifier),
           chargeEstimator.restoreVoltageSagModel(model: model) {
            persistedVoltageSagObservations = model.observations
        }
    }

    deinit { timer?.cancel() }

    func configureChargeEstimate(profile: ChargeEstimateProfile) {
        let hadModel = chargeEstimator.voltageSagModel() != nil
        chargeEstimator.configureProfile(profile: profile.dto)
        if hadModel, chargeEstimator.voltageSagModel() == nil {
            persistedVoltageSagObservations = 0
            try? voltageSagStore.remove(for: token.platformIdentifier)
        }
    }


    func clearChargeEstimateProfile() {
        chargeEstimator.clearProfile()
        chargeEstimate = .missingProfile
    }

    func recordInventory(_ inventory: CoreBluetoothGattInventory) {
        record(.gattInventory(platformIdentifier: context.platformIdentifier, inventory: inventory))
    }

    func handleLinkUp(at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        let step = try ingest(.linkUp, at: at, writeLimit: planner.writeLimit)
        record(.linkUp(platformIdentifier: context.platformIdentifier, writeLimit: planner.writeLimit))
        return step
    }

    func handleNotification(bytes: Data, channel: BluetoothUuid, at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        record(.notification(channel: channel, byteCount: CoreBluetoothPayloadByteCount(bytes.count), at: at))
        return try ingest(.notification, at: at, channel: channel.bytes, bytes: bytes)
    }

    func handleCommand(_ command: DeviceCommand, at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        record(.command(command, at: at))
        return try ingest(.command, at: at, command: command)
    }

    func submitSetting(_ id: DeviceSettingID, value: DeviceSettingValue, at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        try process(state.submitSetting(token: token, id: id, value: value, validationMode: false, monotonicMs: at.rawValue), at: at)
    }

    func submitAction(_ id: DeviceActionID, at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        try process(state.submitAction(token: token, id: id, validationMode: false, monotonicMs: at.rawValue), at: at)
    }

    func handleTick(at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        try ingest(.tick, at: at)
    }

    func invalidate() {
        timer?.cancel()
        timer = nil
        pendingOperations.removeAll()
        waitingForSubscription = nil
        if state.connectionAttemptIsCurrent(token: token) {
            sink?.clearPendingWithoutResponseWrites()
            persistVoltageSag(force: true)
        }
        chargeEstimator.reset()
    }

    func handleNotificationStateUpdate(channel: BluetoothUuid, isNotifying: Bool, error: Error?) {
        guard state.verifiedConnectionAttemptIsCurrent(token: token), waitingForSubscription == channel else { return }
        waitingForSubscription = nil
        guard error == nil, isNotifying else {
            pendingOperations.removeAll()
            return
        }
        let operations = pendingOperations
        pendingOperations.removeAll()
        execute(operations)
        startTimer()
    }

    func handlePeripheralIsReadyToSendWithoutResponse() {
        guard state.verifiedConnectionAttemptIsCurrent(token: token) else { return }
        sink?.peripheralIsReadyToSendWithoutResponse()
    }

    private func ingest(
        _ kind: MobileSessionInputKindDto,
        at: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes? = nil,
        channel: Data = Data(),
        bytes: Data = Data(),
        command: DeviceCommand? = nil
    ) throws -> CoreBluetoothSessionStep {
        guard let step = state.ingestDeviceSession(token: token, input: MobileSessionInputDto(
            kind: kind, monotonicMs: at.dto, maxWriteLen: writeLimit?.dto,
            channel: channel, bytes: bytes, command: command?.dto
        )) else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        let immediate = kind != .tick && (kind != .notification
            || step.result.outputs.contains { $0.kind == .settingsReadback })
        return try process(step, at: at, publishImmediately: immediate)
    }

    private func process(_ step: MobileDeviceSessionStepDto, at: MonotonicMilliseconds, publishImmediately: Bool = true) throws -> CoreBluetoothSessionStep {
        guard step.session.connection.token == token,
              state.verifiedConnectionAttemptIsCurrent(token: token) else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        chargeEstimate = ChargeEstimateState(chargeEstimator.update(input: MobileChargeEstimateInputDto(
            at: at.dto, snapshot: step.telemetry, freshness: MobileDurationDto(milliseconds: 30_000)
        )))
        persistVoltageSag()
        publishControls(at: at, immediately: publishImmediately)
        if let error = step.result.error { throw CutoutSessionError(error) }
        let actions = step.result.outputs.map(SessionAction.init)
        let operations = actions.flatMap(planner.plan(action:))
        execute(operations)
        return CoreBluetoothSessionStep(
            operations: operations,
            snapshot: TelemetrySnapshot(step.telemetry, chargeEstimate: chargeEstimate),
            actions: actions,
            captureContext: context,
            connectionAttempt: step.session.connection.token
        )
    }

    private func execute(_ operations: [CoreBluetoothPlannedOperation]) {
        for operation in operations {
            guard state.verifiedConnectionAttemptIsCurrent(token: token) else { return }
            if waitingForSubscription != nil {
                pendingOperations.append(operation)
                continue
            }
            executor.execute(operation)
            record(.operation(platformIdentifier: context.platformIdentifier, operation: operation))
            if case .subscribe(let channel) = operation { waitingForSubscription = channel }
        }
    }

    private func startTimer() {
        guard timer == nil else { return }
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + .milliseconds(100), repeating: .milliseconds(100))
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            guard self.state.verifiedConnectionAttemptIsCurrent(token: self.token) else {
                self.invalidate()
                return
            }
            guard self.sink?.canSubmitWithoutResponse() == true else { return }
            _ = try? self.handleTick(at: self.clock.now())
        }
        self.timer = timer
        timer.resume()
    }

    private func publishControls(at: MonotonicMilliseconds, immediately: Bool) {
        guard immediately || lastControlsPublication.map({ at.elapsed(since: $0).rawValue >= 1_000 }) != false else { return }
        let value = state.deviceControlsSnapshot(validationMode: false)
        guard value.connection.token == token, value != publishedControls else { return }
        publishedControls = value
        lastControlsPublication = at
        onControlsChange?(value)
    }

    private func persistVoltageSag(force: Bool = false) {
        guard let model = chargeEstimator.voltageSagModel(),
              model.observations != persistedVoltageSagObservations,
              force || persistedVoltageSagObservations == 0
                || UInt32(model.observations) >= UInt32(persistedVoltageSagObservations) + 8 else { return }
        voltageSagStore.save(model, for: token.platformIdentifier)
        persistedVoltageSagObservations = model.observations
    }

    private func record(_ value: CoreBluetoothLiveRecord) {
        guard records.count < 2_048 else { return }
        records.append(value)
    }
}
