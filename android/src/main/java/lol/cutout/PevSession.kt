package lol.cutout

import uniffi.cutout_mobile_ffi.*

/** Native event/effect adapter. Detection, admission, settings and wakeups stay in Rust. */
class PevSession(
    private val state: CutoutSessionStateHandle,
    val token: MobileConnectionAttemptTokenDto,
    private val now: () -> ULong,
    private val send: (MobileSessionOutputDto) -> Unit,
    private val updated: (MobileDeviceSessionSnapshotDto, MobileTelemetrySnapshotDto?, MobileSettingsDto) -> Unit,
) {
    private var protocolLinked = false
    private var telemetry: MobileTelemetrySnapshotDto? = null
    val isCurrent: Boolean get() = state.connectionAttemptIsCurrent(token)

    fun identify(complete: Boolean = false) {
        if (!isCurrent) return
        val session = state.resolveDeviceSession(token, complete, now())
        if (session.connection.readiness == MobileConnectionReadinessDto.VERIFIED && !protocolLinked) {
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

    fun notification(channel: ByteArray, bytes: ByteArray) {
        if (!isCurrent) return
        state.observeConnectionNotification(token, bytes)
        identify()
        if (protocolLinked) input(MobileSessionInputKindDto.NOTIFICATION, channel, bytes)
        publish()
    }

    fun tick() {
        if (!isCurrent) return
        if (protocolLinked) input(MobileSessionInputKindDto.TICK) else identify()
    }

    fun submit(id: MobileSettingIdDto, value: MobileSettingValueDto) {
        check(isCurrent)
        process(state.submitSetting(token, id, value, now()))
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
        submit: (ByteArray) -> NativeStart,
    ) = enqueueWrite(output.bytes, output.operationId, queue, key, output.applicationQuery, submit)

    fun enqueueWrite(
        bytes: ByteArray,
        operationId: ULong?,
        queue: GattOperationQueue,
        key: String,
        applicationQuery: MobileApplicationQueryWriteDto? = null,
        submit: (ByteArray) -> NativeStart,
    ) {
        val chunks = attChunks(bytes, writeLimit.toInt())
        val receipt = if (applicationQuery == null)
            NativeWriteReceipt(chunks.size) { writeReceipt(operationId, it) } else null
        chunks.forEachIndexed { index, chunk ->
            queue.enqueue(GattOperationQueue.Operation(key, {
                val admitted = if (applicationQuery == null) writeIsCurrent(operationId)
                    else state.applicationQueryTransportIsCurrent(token, applicationQuery.queryId, now())
                if (!admitted) NativeStart.REJECTED else submit(chunk)
            }, {
                if (applicationQuery == null) checkNotNull(receipt).observe(index, it)
                else queryReceipt(applicationQuery, index, chunks.size, it)
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
    ) {
        state.ingestDeviceSession(token, MobileSessionInputDto(kind,
            MobileMonotonicMillisDto(now()), writeLimit?.let(::MobileTransportWriteLimitDto), channel, bytes))
            ?.let(::process)
    }

    private fun process(step: MobileDeviceSessionStepDto) {
        if (!isCurrent || step.session.connection.token != token) return
        check(step.result.error == null) { "Device session refused input: ${step.result.error?.kind}" }
        telemetry = step.telemetry
        step.result.outputs.forEach(send)
        publish()
    }
}
