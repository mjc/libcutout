import CutoutMobileFFI
import Foundation

/// Queue-confined CoreBluetooth effects for one Rust-owned, protocol-verified attempt.
final class DeviceSessionTransport: @unchecked Sendable {
    private typealias PendingOperation = (
        operation: CoreBluetoothPlannedOperation,
        applicationQuery: MobileApplicationQueryWriteDto?,
        receipt: (CoreBluetoothWriteDisposition) -> Void
    )

    private enum SettingsWriteReceiptTarget {
        case setting(id: DeviceSettingID, operationID: UInt64)
        case configRead(operationID: UInt64)
    }

    private final class SettingsWriteReceipt {
        let target: SettingsWriteReceiptTarget
        var chunks: [CoreBluetoothWriteDisposition]

        init(target: SettingsWriteReceiptTarget, chunkCount: Int) {
            self.target = target
            chunks = Array(repeating: .queued, count: chunkCount)
        }

        var disposition: CoreBluetoothWriteDisposition {
            if chunks.isEmpty { return .rejected }
            if chunks.contains(.cancelled) { return .cancelled }
            if chunks.contains(.rejected) { return .rejected }
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
    private var scheduledWakeupAt: UInt64?
    private var waitingForSubscription: BluetoothUuid?
    private var pendingOperations: [PendingOperation] = []
    private var invalidated = false
    private(set) var records: [CoreBluetoothLiveRecord] = []
    private var publishedSettings: DeviceSettings?
    private var publishedDeviceSession: MobileDeviceSessionSnapshotDto?
    private var lastSettingsReadAt: MonotonicMilliseconds?
    private var presentationActive = true
    var onSettingsChange: ((DeviceSettings) -> Void)?
    var onDeviceSessionSnapshotChange: ((MobileDeviceSessionSnapshotDto) -> Void)?
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
            chargeEstimator.restoreVoltageSagModel(model: model)
        {
            persistedVoltageSagObservations = model.observations
        }
    }

    deinit { timer?.cancel() }

    func setPresentationActive(_ active: Bool) {
        guard !invalidated, presentationActive != active else { return }
        presentationActive = active
        if active { publishSettings(at: clock.now(), immediately: true) }
    }

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

    func handleNotification(bytes: Data, channel: BluetoothUuid, at: MonotonicMilliseconds) throws
        -> CoreBluetoothSessionStep
    {
        record(.notification(channel: channel, byteCount: CoreBluetoothPayloadByteCount(bytes.count), at: at))
        return try ingest(.notification, at: at, channel: channel.bytes, bytes: bytes)
    }

    func submitSetting(_ id: DeviceSettingID, value: DeviceSettingValue, at: MonotonicMilliseconds) throws
        -> CoreBluetoothSessionStep
    {
        guard !invalidated, waitingForSubscription == nil else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        defer {
            publishSettings(at: clock.now(), immediately: true)
            rescheduleTimer()
        }
        let step = try state.submitSetting(token: token, id: id, value: value, monotonicMs: at.rawValue)
        return try process(step, at: at)
    }

    func submitAction(_ id: DeviceActionID, at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        guard !invalidated, waitingForSubscription == nil else {
            throw DeviceActionSubmissionError.ConnectionUnavailable
        }
        defer {
            publishSettings(at: clock.now(), immediately: true)
            rescheduleTimer()
        }
        return try process(state.submitAction(token: token, id: id, monotonicMs: at.rawValue), at: at)
    }

    func setValidationAuthorization(_ authorized: Bool, at: MonotonicMilliseconds) throws {
        let accepted =
            authorized
            ? state.authorizeDeviceControls(token: token)
            : state.revokeDeviceControls(token: token)
        guard accepted else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        publishSettings(at: at, immediately: true)
        rescheduleTimer()
    }

    func handleTick(at: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        try ingest(.tick, at: at)
    }

    func invalidate() {
        guard !invalidated else { return }
        timer?.cancel()
        timer = nil
        scheduledWakeupAt = nil
        rejectPendingOperations()
        waitingForSubscription = nil
        if state.connectionAttemptIsCurrent(token: token) {
            sink?.clearPendingWithoutResponseWrites()
            persistVoltageSag(force: true)
        }
        invalidated = true
        // Cancellation receipts may have advanced Rust deadlines while settling writes.
        cancelTimer()
        chargeEstimator.reset()
    }

    func handleNotificationStateUpdate(channel: BluetoothUuid, isNotifying: Bool, error: Error?) {
        guard !invalidated, state.verifiedConnectionAttemptIsCurrent(token: token), waitingForSubscription == channel
        else { return }
        waitingForSubscription = nil
        guard error == nil, isNotifying else {
            rejectPendingOperations()
            onSubscriptionFailure?(channel, error)
            return
        }
        let operations = pendingOperations
        pendingOperations.removeAll()
        execute(operations)
        rescheduleTimer()
    }

    func handlePeripheralIsReadyToSendWithoutResponse() {
        guard !invalidated, state.verifiedConnectionAttemptIsCurrent(token: token) else { return }
        sink?.peripheralIsReadyToSendWithoutResponse()
        rescheduleTimer()
    }

    private func ingest(
        _ kind: MobileSessionInputKindDto,
        at: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes? = nil,
        channel: Data = Data(),
        bytes: Data = Data()
    ) throws -> CoreBluetoothSessionStep {
        guard !invalidated else { throw DeviceSettingSubmissionError.ConnectionUnavailable }
        guard
            let step = state.ingestDeviceSession(
                token: token,
                input: MobileSessionInputDto(
                    kind: kind, monotonicMs: at.dto, maxWriteLen: writeLimit?.dto,
                    channel: channel, bytes: bytes
                ))
        else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        defer { rescheduleTimer() }
        let immediate = kind != .tick && kind != .notification
        return try process(step, at: at, publishImmediately: immediate)
    }

    private func process(
        _ step: MobileDeviceSessionStepDto,
        at: MonotonicMilliseconds,
        publishImmediately: Bool = true
    ) throws -> CoreBluetoothSessionStep {
        guard step.session.connection.token == token,
            state.verifiedConnectionAttemptIsCurrent(token: token)
        else {
            throw DeviceSettingSubmissionError.ConnectionUnavailable
        }
        publishDeviceSessionSnapshot()
        chargeEstimate = ChargeEstimateState(
            chargeEstimator.update(
                input: MobileChargeEstimateInputDto(
                    at: at.dto, snapshot: step.telemetry, freshness: MobileDurationDto(milliseconds: 30_000)
                )))
        persistVoltageSag()
        publishSettings(at: at, immediately: publishImmediately)
        if let error = step.result.error { throw CutoutSessionError(error) }
        let actions = step.result.outputs.map(SessionAction.init)
        let operations = actions.flatMap(planner.plan(action:))
        let settings =
            actions.contains(where: { $0.kind == .write && $0.operationID != nil })
            ? state.settings() : nil
        // One receipt per protocol stage; Rust aggregates stages of a delayed sequence.
        // Resolve by operation identity on every step, including notification/tick outputs.
        var pending: [PendingOperation] = []
        for action in actions {
            let planned = planner.plan(action: action)
            let receipt: SettingsWriteReceipt?
            if action.kind == .write, let operationID = action.operationID,
                let setting = settings?.settings.first(where: { $0.requestId == operationID })
            {
                receipt = SettingsWriteReceipt(
                    target: .setting(id: setting.id, operationID: operationID),
                    chunkCount: planned.count)
            } else if action.kind == .write, let operationID = action.operationID,
                state.settingReadTransportIsCurrent(
                    token: token, operationId: operationID, monotonicMs: clock.now().rawValue)
            {
                receipt = SettingsWriteReceipt(
                    target: .configRead(operationID: operationID), chunkCount: planned.count)
            } else {
                receipt = nil
            }
            if let receipt, planned.isEmpty { recordSettingTransport(receipt) }
            if let query = action.applicationQuery, planned.isEmpty {
                recordApplicationQueryTransport(query, index: 0, count: 0, disposition: .rejected)
            }
            for (index, operation) in planned.enumerated() {
                pending.append(
                    (
                        operation,
                        action.applicationQuery,
                        { [weak self] disposition in
                            guard let self else { return }
                            // Keep the native effect receipt even if its authorization has retired.
                            self.record(
                                .writeReceipt(
                                    platformIdentifier: self.context.platformIdentifier,
                                    operation: operation, disposition: disposition))
                            guard !self.invalidated,
                                self.state.verifiedConnectionAttemptIsCurrent(token: self.token)
                            else { return }
                            if let query = action.applicationQuery {
                                self.recordApplicationQueryTransport(
                                    query, index: index, count: planned.count, disposition: disposition)
                            }
                            if let receipt, receipt.chunks[index] == .queued {
                                receipt.chunks[index] = disposition
                                self.recordSettingTransport(receipt)
                            }
                        }
                    ))
            }
        }
        execute(pending)
        return CoreBluetoothSessionStep(
            operations: operations,
            snapshot: TelemetrySnapshot(step.telemetry, chargeEstimate: chargeEstimate),
            semanticTelemetry: step.telemetry,
            captureNotificationEvidence: step.captureNotificationEvidence,
            speedObservation: step.telemetry.speed.flatMap { speed in
                step.telemetry.speedObservedAtMs.map { observedAt in
                    MobileRideMapSpeedObservationDto(
                        millimetresPerSecond: speed.value.value,
                        observedAtMs: observedAt.milliseconds
                    )
                }
            },
            actions: actions,
            captureContext: context,
            connectionAttempt: step.session.connection.token
        )
    }

    private func execute(_ operations: [PendingOperation]) {
        for (operation, applicationQuery, onReceipt) in operations {
            guard !invalidated, state.verifiedConnectionAttemptIsCurrent(token: token) else { return }
            if waitingForSubscription != nil {
                pendingOperations.append((operation, applicationQuery, onReceipt))
                if case .writeWithoutResponse = operation { onReceipt(.queued) }
                continue
            }
            // An already-enabled characteristic can acknowledge synchronously.
            // Install the wait before asking the native sink to subscribe.
            if case .subscribe(let channel) = operation { waitingForSubscription = channel }
            executor.execute(
                operation,
                isCurrent: { [weak self] in
                    guard let self, !self.invalidated,
                        self.state.verifiedConnectionAttemptIsCurrent(token: self.token)
                    else { return false }
                    if let applicationQuery {
                        let now = self.clock.now().rawValue
                        return now < applicationQuery.submitBeforeMs
                            && self.state.applicationQueryTransportIsCurrent(
                                token: self.token, queryId: applicationQuery.queryId, monotonicMs: now)
                    }
                    guard let operationID = self.operationID(of: operation) else { return true }
                    return self.state.settingTransportIsCurrent(
                        token: self.token, operationId: operationID, monotonicMs: self.clock.now().rawValue
                    )
                }, onWriteReceipt: onReceipt)
            record(.operation(platformIdentifier: context.platformIdentifier, operation: operation))
        }
    }

    private func rejectPendingOperations() {
        let cancelled = pendingOperations
        pendingOperations.removeAll()
        for (operation, _, onReceipt) in cancelled {
            if isWrite(operation) { onReceipt(.cancelled) }
        }
    }

    private func isWrite(_ operation: CoreBluetoothPlannedOperation) -> Bool {
        switch operation {
        case .writeWithoutResponse, .writeWithoutResponseWithOperationID:
            return true
        case .subscribe, .disconnect:
            return false
        }
    }

    private func operationID(of operation: CoreBluetoothPlannedOperation) -> UInt64? {
        guard case .writeWithoutResponseWithOperationID(_, _, let operationID) = operation else {
            return nil
        }
        return operationID
    }

    private func publishDeviceSessionSnapshot() {
        let snapshot = state.deviceSessionSnapshot()
        guard snapshot.connection.token == token, snapshot != publishedDeviceSession else { return }
        publishedDeviceSession = snapshot
        onDeviceSessionSnapshotChange?(snapshot)
    }

    private func recordApplicationQueryTransport(
        _ query: MobileApplicationQueryWriteDto, index: Int, count: Int,
        disposition: CoreBluetoothWriteDisposition
    ) {
        guard let fragmentIndex = UInt16(exactly: index), let fragmentCount = UInt16(exactly: count) else { return }
        let status: MobileApplicationQueryTransportStatusDto
        switch disposition {
        case .queued: status = .queued
        case .submitted: status = .submitted
        case .rejected: status = .rejected
        case .cancelled: status = .cancelled
        }
        _ = state.markApplicationQueryTransport(
            token: token, queryId: query.queryId, fragmentIndex: fragmentIndex, fragmentCount: fragmentCount,
            status: status, monotonicMs: clock.now().rawValue
        )
        publishDeviceSessionSnapshot()
        rescheduleTimer()
    }

    private func recordSettingTransport(
        _ receipt: SettingsWriteReceipt
    ) {
        let status: MobileSettingTransportStatusDto
        switch receipt.disposition {
        case .submitted: status = .submitted
        case .queued: status = .queued
        case .rejected: status = .rejected
        case .cancelled: status = .cancelled
        }
        let accepted: Bool
        switch receipt.target {
        case .setting(let id, let operationID):
            accepted = state.markSettingTransport(
                token: token, id: id, requestId: operationID,
                status: status, monotonicMs: clock.now().rawValue)
        case .configRead(let operationID):
            accepted = state.markSettingReadTransport(
                token: token, operationId: operationID,
                status: status, monotonicMs: clock.now().rawValue)
        }
        guard accepted else { return }
        publishSettings(at: clock.now(), immediately: true)
        rescheduleTimer()
    }

    private func cancelTimer() {
        timer?.cancel()
        timer = nil
        scheduledWakeupAt = nil
    }

    private func rescheduleTimer() {
        guard !invalidated else { return }
        let canPoll = waitingForSubscription == nil && sink?.canSubmitWithoutResponse() == true
        guard let wakeupAt = state.deviceSessionNextWakeupAt(token: token, canPoll: canPoll) else {
            cancelTimer()
            return
        }
        guard scheduledWakeupAt != wakeupAt else { return }
        cancelTimer()
        let now = clock.now().rawValue
        let delay = wakeupAt > now ? wakeupAt - now : 0
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + .milliseconds(Int(clamping: delay)))
        timer.setEventHandler { [weak self] in
            guard let self, self.scheduledWakeupAt == wakeupAt else { return }
            self.handleTimer()
        }
        self.timer = timer
        scheduledWakeupAt = wakeupAt
        timer.resume()
    }

    func handleTimer() {
        cancelTimer()
        guard !invalidated else { return }
        guard state.verifiedConnectionAttemptIsCurrent(token: token) else {
            invalidate()
            return
        }
        defer { rescheduleTimer() }
        // Confirmation and queue authorization deadlines advance even under backpressure.
        let now = clock.now()
        _ = state.tickSettingTransport(token: token, monotonicMs: now.rawValue)
        _ = state.tickApplicationQueryTransport(token: token, monotonicMs: now.rawValue)
        publishDeviceSessionSnapshot()
        publishSettings(at: now, immediately: false)
        guard waitingForSubscription == nil, sink?.canSubmitWithoutResponse() == true else { return }
        _ = try? handleTick(at: now)
    }

    private func publishSettings(at: MonotonicMilliseconds, immediately: Bool) {
        guard presentationActive else { return }
        guard immediately || lastSettingsReadAt.map({ at.elapsed(since: $0).rawValue >= 1_000 }) != false else {
            return
        }
        lastSettingsReadAt = at
        let value = state.settings()
        guard value.connection.token == token, value != publishedSettings else { return }
        publishedSettings = value
        onSettingsChange?(value)
    }

    private func persistVoltageSag(force: Bool = false) {
        guard let model = chargeEstimator.voltageSagModel(),
            model.observations != persistedVoltageSagObservations,
            force || persistedVoltageSagObservations == 0
                || UInt32(model.observations) >= UInt32(persistedVoltageSagObservations) + 8
        else { return }
        voltageSagStore.save(model, for: token.platformIdentifier)
        persistedVoltageSagObservations = model.observations
    }

    private func record(_ value: CoreBluetoothLiveRecord) {
        guard records.count < 2_048 else { return }
        records.append(value)
    }
}
