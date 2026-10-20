package lol.cutout

import uniffi.cutout_mobile_ffi.*

/** Native event/effect adapter. Detection, admission, settings and wakeups stay in Rust. */
class PevSession(
    private val state: CutoutSessionStateHandle,
    val token: MobileConnectionAttemptTokenDto,
    private val now: () -> ULong,
    private val send: (MobileSessionOutputDto) -> Unit,
    private val onRideTelemetry: (
        MobileConnectionAttemptTokenDto,
        ULong,
        MobileRideMapSpeedObservationDto?,
    ) -> Unit = { _, _, _ -> },
    private val onVerified: (MobileConnectionAttemptTokenDto) -> Unit = {},
    private val onCaptureNotification: (
        ByteArray, ByteArray, ByteArray, MobileTelemetrySnapshotDto?,
        MobileCaptureNotificationEvidenceDto, ULong,
    ) -> Unit = { _, _, _, _, _, _ -> },
    private val updated: (MobileDeviceSessionSnapshotDto, MobileTelemetrySnapshotDto?, MobileSettingsDto) -> Unit,
) {
    private var protocolLinked = false
    private var telemetry: MobileTelemetrySnapshotDto? = null
    val isCurrent: Boolean get() = state.connectionAttemptIsCurrent(token)

    fun identify(complete: Boolean = false) {
        if (!isCurrent) return
        val session = state.resolveDeviceSession(token, complete, now())
        if (session.connection.readiness == MobileConnectionReadinessDto.VERIFIED && !protocolLinked) {
            onVerified(token)
            protocolLinked = true
            input(MobileSessionInputKindDto.LINK_UP, writeLimit = writeLimit)
        }
        publish()
    }

    var writeLimit: UShort = 20U
        private set

    fun negotiatedMtu(mtu: Int) {
        require(!protocolLinked)
        require(mtu in 23..517)
        writeLimit = minOf(mtu - 3, 512).toUShort()
    }

    fun notification(channel: ByteArray, service: ByteArray, bytes: ByteArray, capture: Boolean = true) {
        if (!isCurrent) return
        val receivedAtMs = now()
        state.observeConnectionNotification(token, bytes)
        identify()
        val step = if (protocolLinked) input(MobileSessionInputKindDto.NOTIFICATION, channel, bytes, receivedAtMs = receivedAtMs) else null
        if (capture) onCaptureNotification(
            channel.copyOf(), service.copyOf(), bytes.copyOf(), step?.telemetry,
            step?.captureNotificationEvidence ?: MobileCaptureNotificationEvidenceDto.UNCLASSIFIED,
            receivedAtMs,
        )
        publish()
    }

    /** Compatibility for session-only fixtures without native service metadata. */
    fun notification(channel: ByteArray, bytes: ByteArray) = notification(channel, byteArrayOf(), bytes)

    fun tick() {
        if (!isCurrent) return
        if (protocolLinked) input(MobileSessionInputKindDto.TICK) else identify()
    }

    fun submit(id: MobileSettingIdDto, value: MobileSettingValueDto) {
        check(isCurrent)
        val receivedAtMs = now()
        process(state.submitSetting(token, id, value, receivedAtMs), receivedAtMs)
    }

    /** Submit the next protocol-selected action step through the existing Rust session owner. */
    fun submitAction(id: MobileDeviceActionIdDto) {
        check(isCurrent)
        val receivedAtMs = now()
        process(state.submitAction(token, id, receivedAtMs), receivedAtMs)
    }

    fun writeIsCurrent(operationId: ULong?): Boolean = isCurrent && (operationId == null ||
        state.settingTransportIsCurrent(token, operationId, now()) ||
        state.settingReadTransportIsCurrent(token, operationId, now()))

    fun writeReceipt(operationId: ULong?, receipt: NativeReceipt) {
        if (operationId == null || !isCurrent) return
        val status = when (receipt) {
            NativeReceipt.QUEUED -> MobileSettingTransportStatusDto.QUEUED
            NativeReceipt.SUBMITTED -> MobileSettingTransportStatusDto.SUBMITTED
            NativeReceipt.REJECTED -> MobileSettingTransportStatusDto.REJECTED
            NativeReceipt.CANCELLED -> MobileSettingTransportStatusDto.CANCELLED
        }
        val setting = state.settings().settings.firstOrNull { it.requestId == operationId }
        if (setting != null) state.markSettingTransport(token, setting.id, operationId, status, now())
        else state.markSettingReadTransport(token, operationId, status, now())
        publish()
    }

    fun writeReceipt(output: MobileSessionOutputDto, receipt: NativeReceipt) {
        val query = output.applicationQuery
        if (query == null) writeReceipt(output.operationId, receipt)
        else queryReceipt(query, 0, 1, receipt)
    }

    private fun queryReceipt(
        query: MobileApplicationQueryWriteDto,
        index: Int,
        count: Int,
        receipt: NativeReceipt,
    ) {
        if (!isCurrent) return
        val status = when (receipt) {
            NativeReceipt.QUEUED -> MobileApplicationQueryTransportStatusDto.QUEUED
            NativeReceipt.SUBMITTED -> MobileApplicationQueryTransportStatusDto.SUBMITTED
            NativeReceipt.REJECTED -> MobileApplicationQueryTransportStatusDto.REJECTED
            NativeReceipt.CANCELLED -> MobileApplicationQueryTransportStatusDto.CANCELLED
        }
        state.markApplicationQueryTransport(token, query.queryId, index.toUShort(), count.toUShort(), status, now())
        publish()
    }

    /** Shared production submission path; Android supplies only the native write call. */
    fun enqueueWrite(
        output: MobileSessionOutputDto,
        queue: GattOperationQueue,
        key: String,
        onReceipt: ((NativeReceipt) -> Unit)? = null,
        submit: (ByteArray) -> NativeStart,
    ) = enqueueWrite(output.bytes, output.operationId, queue, key, output.applicationQuery, onReceipt, submit)

    fun enqueueWrite(
        bytes: ByteArray,
        operationId: ULong?,
        queue: GattOperationQueue,
        key: String,
        applicationQuery: MobileApplicationQueryWriteDto? = null,
        onReceipt: ((NativeReceipt) -> Unit)? = null,
        submit: (ByteArray) -> NativeStart,
    ) {
        val chunks = attChunks(bytes, writeLimit.toInt())
        val receipt = NativeWriteReceipt(chunks.size) {
            if (applicationQuery == null) writeReceipt(operationId, it)
            onReceipt?.invoke(it)
        }
        chunks.forEachIndexed { index, chunk ->
            queue.enqueue(GattOperationQueue.Operation(key, {
                val admitted = if (applicationQuery == null) writeIsCurrent(operationId)
                    else state.applicationQueryTransportIsCurrent(token, applicationQuery.queryId, now())
                when {
                    admitted -> submit(chunk)
                    applicationQuery != null -> NativeStart.CANCELLED
                    else -> NativeStart.REJECTED
                }
            }, {
                if (applicationQuery != null) queryReceipt(applicationQuery, index, chunks.size, it)
                receipt.observe(index, it)
            }))
        }
    }

    fun nextWakeup(canPoll: Boolean): ULong? =
        if (isCurrent) state.deviceSessionNextWakeupAt(token, canPoll) else null

    fun publish() {
        if (isCurrent) updated(state.deviceSessionSnapshot(), telemetry, state.settings())
    }

    private fun input(
        kind: MobileSessionInputKindDto,
        channel: ByteArray = byteArrayOf(),
        bytes: ByteArray = byteArrayOf(),
        writeLimit: UShort? = null,
        receivedAtMs: ULong = now(),
    ): MobileDeviceSessionStepDto? {
        val step = state.ingestDeviceSession(token, MobileSessionInputDto(kind,
            MobileMonotonicMillisDto(receivedAtMs), writeLimit?.let(::MobileTransportWriteLimitDto), channel, bytes))
        step?.let { process(it, receivedAtMs) }
        return step
    }

    private fun process(step: MobileDeviceSessionStepDto, receivedAtMs: ULong) {
        if (!isCurrent || step.session.connection.token != token) return
        check(step.result.error == null) { "Device session refused input: ${step.result.error?.kind}" }
        telemetry = step.telemetry
        step.result.outputs.forEach(send)
        val speedObservation = step.telemetry.speed?.let { speed ->
            step.telemetry.speedObservedAtMs?.let { observedAt ->
                MobileRideMapSpeedObservationDto(speed.value.value, observedAt.milliseconds)
            }
        }
        onRideTelemetry(token, receivedAtMs, speedObservation)
        publish()
    }
}
