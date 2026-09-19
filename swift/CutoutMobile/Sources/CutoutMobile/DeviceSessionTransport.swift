import CutoutMobileFFI
import Foundation

/// Queue-confined CoreBluetooth effects for one Rust-owned, protocol-verified attempt.
final class DeviceSessionTransport: @unchecked Sendable {
    private typealias PendingOperation = (CoreBluetoothPlannedOperation, (CoreBluetoothWriteDisposition) -> Void)

    private final class SettingWriteReceipt {
        let id: DeviceSettingID
        let requestID: UInt64
        var chunks: [CoreBluetoothWriteDisposition]

        init(id: DeviceSettingID, requestID: UInt64, chunkCount: Int) {
            self.id = id
            self.requestID = requestID
            chunks = Array(repeating: .queued, count: chunkCount)
        }

        var disposition: CoreBluetoothWriteDisposition {
            if chunks.isEmpty || chunks.contains(.rejected) { return .rejected }
            return chunks.contains(.queued) ? .queued : .submitted
        }
    }

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
    private var pendingOperations: [PendingOperation] = []
    private var invalidated = false
    private(set) var records: [CoreBluetoothLiveRecord] = []
    private var publishedSettings: DeviceSettings?
    private var lastSettingsPublication: MonotonicMilliseconds?
    var onSettingsChange: ((DeviceSettings) -> Void)?
    var onSubscriptionFailure: ((BluetoothUuid, Error?) -> Void)?

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
        let settings = state.settings()
        if settings.connection.token == token, let profile = settings.defaultChargeProfile {
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

    func submitSetting(_ id: DeviceSettingID, value: DeviceSettingValue, at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        guard !invalidated, waitingForSubscription == nil else { throw DeviceSettingSubmissionError.ConnectionUnavailable }
        defer { publishSettings(at: clock.now(), immediately: true) }
        let step = try state.submitSetting(token: token, id: id, value: value, monotonicMs: at.rawValue)
        guard let requestID = state.settings().setting(for: id)?.requestId else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        return try process(
            step,
            at: at,
            settingRequest: (id, requestID)
        )
    }

    func submitAction(_ id: DeviceActionID, at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        guard !invalidated, waitingForSubscription == nil else { throw DeviceActionSubmissionError.ConnectionUnavailable }
        defer { publishSettings(at: clock.now(), immediately: true) }
        return try process(state.submitAction(token: token, id: id, monotonicMs: at.rawValue), at: at)
    }

    func setValidationAuthorization(_ authorized: Bool, at: MonotonicMilliseconds) throws {
        let accepted = authorized
            ? state.authorizeDeviceControls(token: token)
            : state.revokeDeviceControls(token: token)
        guard accepted else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        publishSettings(at: at, immediately: true)
    }

    func handleTick(at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        try ingest(.tick, at: at)
    }

    func invalidate() {
        guard !invalidated else { return }
        timer?.cancel()
        timer = nil
        rejectPendingOperations()
        waitingForSubscription = nil
        if state.connectionAttemptIsCurrent(token: token) {
            sink?.clearPendingWithoutResponseWrites()
            persistVoltageSag(force: true)
        }
        invalidated = true
        chargeEstimator.reset()
    }

    func handleNotificationStateUpdate(channel: BluetoothUuid, isNotifying: Bool, error: Error?) {
        guard !invalidated, state.verifiedConnectionAttemptIsCurrent(token: token), waitingForSubscription == channel else { return }
        waitingForSubscription = nil
        guard error == nil, isNotifying else {
            rejectPendingOperations()
            onSubscriptionFailure?(channel, error)
            return
        }
        let operations = pendingOperations
        pendingOperations.removeAll()
        execute(operations)
        startTimer()
    }

    func handlePeripheralIsReadyToSendWithoutResponse() {
        guard !invalidated, state.verifiedConnectionAttemptIsCurrent(token: token) else { return }
        sink?.peripheralIsReadyToSendWithoutResponse()
    }

    private func ingest(
        _ kind: MobileSessionInputKindDto,
        at: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes? = nil,
        channel: Data = Data(),
        bytes: Data = Data()
    ) throws -> CoreBluetoothSessionStep {
        guard !invalidated else { throw DeviceSettingSubmissionError.ConnectionUnavailable }
        guard let step = state.ingestDeviceSession(token: token, input: MobileSessionInputDto(
            kind: kind, monotonicMs: at.dto, maxWriteLen: writeLimit?.dto,
            channel: channel, bytes: bytes
        )) else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        let immediate = kind != .tick && kind != .notification
        return try process(step, at: at, publishImmediately: immediate)
    }

    private func process(
        _ step: MobileDeviceSessionStepDto,
        at: MonotonicMilliseconds,
        publishImmediately: Bool = true,
        settingRequest: (id: DeviceSettingID, requestID: UInt64)? = nil
    ) throws -> CoreBluetoothSessionStep {
        guard step.session.connection.token == token,
              state.verifiedConnectionAttemptIsCurrent(token: token) else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        chargeEstimate = ChargeEstimateState(chargeEstimator.update(input: MobileChargeEstimateInputDto(
            at: at.dto, snapshot: step.telemetry, freshness: MobileDurationDto(milliseconds: 30_000)
        )))
        persistVoltageSag()
        publishSettings(at: at, immediately: publishImmediately)
        if let error = step.result.error { throw CutoutSessionError(error) }
        let actions = step.result.outputs.map(SessionAction.init)
        let operations = actions.flatMap(planner.plan(action:))
        let chunkCount = operations.reduce(0) { count, operation in
            if case .writeWithoutResponse = operation { return count + 1 }
            return count
        }
        let receipt = settingRequest.map {
            SettingWriteReceipt(id: $0.id, requestID: $0.requestID, chunkCount: chunkCount)
        }
        // Later tick outputs have no request identity; only attribute this submission's writes.
        if let receipt, chunkCount == 0, actions.contains(where: { $0.kind == .write }) {
            recordSettingTransport(receipt)
        }
        var chunkIndex = 0
        execute(operations.map { operation in
            let index = chunkIndex
            if case .writeWithoutResponse = operation { chunkIndex += 1 }
            return (operation, { [weak self] disposition in
                guard let self, !self.invalidated,
                      self.state.verifiedConnectionAttemptIsCurrent(token: self.token) else { return }
                self.record(.writeReceipt(
                    platformIdentifier: self.context.platformIdentifier,
                    operation: operation,
                    disposition: disposition
                ))
                if let receipt, receipt.chunks[index] == .queued {
                    receipt.chunks[index] = disposition
                    self.recordSettingTransport(receipt)
                }
            })
        })
        return CoreBluetoothSessionStep(
            operations: operations,
            snapshot: TelemetrySnapshot(step.telemetry, chargeEstimate: chargeEstimate),
            actions: actions,
            captureContext: context,
            connectionAttempt: step.session.connection.token
        )
    }

    private func execute(_ operations: [PendingOperation]) {
        for (operation, onReceipt) in operations {
            guard !invalidated, state.verifiedConnectionAttemptIsCurrent(token: token) else { return }
            if waitingForSubscription != nil {
                pendingOperations.append((operation, onReceipt))
                if case .writeWithoutResponse = operation { onReceipt(.queued) }
                continue
            }
            // An already-enabled characteristic can acknowledge synchronously.
            // Install the wait before asking the native sink to subscribe.
            if case .subscribe(let channel) = operation { waitingForSubscription = channel }
            executor.execute(operation, onWriteReceipt: onReceipt)
            record(.operation(platformIdentifier: context.platformIdentifier, operation: operation))
        }
    }

    private func rejectPendingOperations() {
        let cancelled = pendingOperations
        pendingOperations.removeAll()
        for (operation, onReceipt) in cancelled {
            if case .writeWithoutResponse = operation { onReceipt(.rejected) }
        }
    }

    private func recordSettingTransport(
        _ receipt: SettingWriteReceipt
    ) {
        let status: MobileSettingTransportStatusDto
        switch receipt.disposition {
        case .submitted: status = .submitted
        case .queued: status = .queued
        case .rejected: status = .rejected
        }
        guard state.markSettingTransport(
            token: token, id: receipt.id, requestId: receipt.requestID,
            status: status, monotonicMs: clock.now().rawValue
        ) else { return }
        publishSettings(at: clock.now(), immediately: true)
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

    private func publishSettings(at: MonotonicMilliseconds, immediately: Bool) {
        guard immediately || lastSettingsPublication.map({ at.elapsed(since: $0).rawValue >= 1_000 }) != false else { return }
        let value = state.settings()
        guard value.connection.token == token, value != publishedSettings else { return }
        publishedSettings = value
        lastSettingsPublication = at
        onSettingsChange?(value)
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
