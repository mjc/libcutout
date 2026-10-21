package lol.cutout.smoke

import java.util.UUID
import lol.cutout.*
import uniffi.cutout_mobile_ffi.*

private class ManualTimer : NativeTimer {
    val tasks = mutableListOf<() -> Unit>()
    override fun after(milliseconds: Long, task: () -> Unit): () -> Unit {
        check(milliseconds > 0)
        tasks += task
        return { tasks.remove(task); Unit }
    }
}

fun checkNativeGattQueue() {
    var current = true
    var failures = 0
    val started = mutableListOf<String>()
    val receipts = mutableListOf<Pair<String, NativeReceipt>>()
    val timer = ManualTimer()
    val queue = GattOperationQueue({ current }, timer, { failures++ }, capacity = 2)
    fun operation(name: String) = GattOperationQueue.Operation(name,
        { started += name; NativeStart.PENDING }, { receipts += name to it })
    check(queue.enqueue(operation("subscribe")))
    check(queue.enqueue(operation("write")))
    check(started == listOf("subscribe"))
    check(!queue.complete("wrong characteristic", true))
    check(started == listOf("subscribe"))
    check(queue.complete("subscribe", true))
    check(started == listOf("subscribe", "write"))
    check(queue.complete("write", true))
    check(queue.isIdle && timer.tasks.isEmpty())
    check(!queue.complete("write", true))

    queue.enqueue(operation("in flight"))
    queue.enqueue(operation("not submitted"))
    current = false
    check(!queue.complete("in flight", true))
    queue.close()
    check("in flight" to NativeReceipt.SUBMITTED in receipts)
    check("in flight" to NativeReceipt.CANCELLED !in receipts)
    check("not submitted" to NativeReceipt.CANCELLED in receipts)
    check("not submitted" !in started)

    current = true
    val overflow = GattOperationQueue({ current }, timer, { failures++ }, capacity = 2)
    overflow.enqueue(operation("active")); overflow.enqueue(operation("queued"))
    check(!overflow.enqueue(operation("overflow")))
    check("overflow" to NativeReceipt.REJECTED in receipts)
    check("queued" to NativeReceipt.CANCELLED in receipts)
    check("queued" !in started && failures == 1)

    val timed = GattOperationQueue({ current }, timer, { failures++ })
    timed.enqueue(operation("timeout"))
    timer.tasks.single().invoke()
    check(failures == 2 && timed.isIdle)
    check(!timed.complete("timeout", true))
    check(!timed.enqueue(operation("after timeout")))

    val rejected = GattOperationQueue({ true }, timer, { failures++ })
    check(!rejected.enqueue(GattOperationQueue.Operation("native reject", { NativeStart.REJECTED })))
    check(failures == 3)
    val permission = GattOperationQueue({ true }, timer, { failures++ })
    check(!permission.enqueue(GattOperationQueue.Operation("permission revoked", { throw SecurityException() })))
    check(failures == 4)
    val subscription = GattOperationQueue({ true }, timer, { failures++ })
    subscription.enqueue(operation("CCCD"))
    subscription.enqueue(operation("write after subscription"))
    check(subscription.complete("CCCD", false))
    check("write after subscription" !in started)
    check("write after subscription" to NativeReceipt.CANCELLED in receipts)
    check(failures == 5)
    val invalidNativeState = GattOperationQueue({ true }, timer, { failures++ })
    check(!invalidNativeState.enqueue(GattOperationQueue.Operation("native state invalid",
        { throw IllegalStateException() }, { receipts += "native state invalid" to it })))
    check("native state invalid" to NativeReceipt.REJECTED in receipts)
    check(invalidNativeState.isIdle && failures == 6)

    val batch = mutableListOf<NativeReceipt>()
    val receipt = NativeWriteReceipt(2) { batch += it }
    receipt.observe(0, NativeReceipt.SUBMITTED)
    check(batch == listOf(NativeReceipt.QUEUED))
    receipt.observe(1, NativeReceipt.SUBMITTED)
    check(batch.last() == NativeReceipt.SUBMITTED)
    receipt.observe(1, NativeReceipt.REJECTED)
    check(batch.last() == NativeReceipt.REJECTED)
    val partial = NativeWriteReceipt(2) { batch += it }
    partial.observe(0, NativeReceipt.SUBMITTED)
    partial.observe(1, NativeReceipt.CANCELLED)
    check(batch.last() == NativeReceipt.REJECTED) // Partial submission cannot be reported as never sent.

    val payload = ByteArray(513) { it.toByte() }
    val chunks = attChunks(payload, 20)
    check(chunks.size == 26 && chunks.last().size == 13)
    check(chunks.all { it.size <= 20 })
    check(chunks.flatMap { it.asList() }.toByteArray().contentEquals(payload))
    check(attChunks(payload, 512).map { it.size } == listOf(512, 1))
    val uuid = UUID.fromString("6e400003-b5a3-f393-e0a9-e50e24dcca9e")
    check(wireUuid(uuid.wireBytes()) == uuid)
}

fun checkProductionSessionReplay() {
    checkApplicationQueryReceipts()
    checkSettingDeadlinesUnderBackpressure()
    CutoutSessionStateHandle().use { state ->
        var clock = 0UL
        val token = checkNotNull(state.beginConnectionAttempt("production-adapter", clock).token)
        state.connectionLinkEstablished(token)
        state.observeConnectionGatt(token, eucReplayGatt())
        val outputs = mutableListOf<MobileSessionOutputDto>()
        var latest: MobileTelemetrySnapshotDto? = null
        var publications = 0
        val session = PevSession(state, token, { clock }, { outputs += it }) { snapshot, telemetry, settings ->
            check(snapshot.connection.token == token && settings.connection.token == token)
            latest = telemetry
            publications++
        }
        session.negotiatedMtu(185)
        check(session.writeLimit == 182U.toUShort())
        for (input in eucReplayInputs().filter { it.kind == MobileSessionInputKindDto.NOTIFICATION }) {
            clock = input.monotonicMs.milliseconds
            session.notification(input.channel, input.bytes)
        }
        check(state.deviceSessionSnapshot().identity?.vehicleKind == MobileVehicleKindDto.ELECTRIC_UNICYCLE)
        check(outputs.any { it.kind == MobileSessionOutputKindDto.SUBSCRIBE })
        check(checkNotNull(latest).voltage != null && checkNotNull(latest).speed != null)
        val count = publications
        val outputCount = outputs.size
        state.beginConnectionAttempt("production-adapter", 2_000UL)
        session.notification(eucReplayInputs()[1].channel, eucReplayInputs()[1].bytes)
        session.tick()
        check(publications == count && outputs.size == outputCount)
        check(!session.writeIsCurrent(null))
        check(state.deviceSessionSnapshot().identity == null)
        state.disconnectConnectionAttempt()
    }
}

private class ApplicationQueryFixture : AutoCloseable {
    val state = CutoutSessionStateHandle()
    var clock = 1UL
    val token = checkNotNull(state.beginConnectionAttempt("vesc-query", 0UL).token)
    val outputs = mutableListOf<MobileSessionOutputDto>()
    val publications = mutableListOf<MobileDeviceSessionSnapshotDto>()
    val session = PevSession(state, token, { clock }, { outputs += it }) { snapshot, _, _ ->
        publications += snapshot
    }
    val query: MobileSessionOutputDto

    init {
        state.connectionLinkEstablished(token)
        state.observeConnectionNotification(token, byteArrayOf(
            2, 20, 157.toByte(), 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117,
            115, 101, 114, 104, 97, 115, 104, 0, 38, 208.toByte(), 3,
        ))
        session.identify()
        query = outputs.single { it.applicationQuery != null }
    }

    fun queryState() = state.deviceSessionSnapshot().identity?.vescApplicationQuery
    override fun close() = state.close()
}

private fun checkApplicationQueryReceipts() {
    ApplicationQueryFixture().use { fixture ->
        val queue = GattOperationQueue({ fixture.session.isCurrent }, ManualTimer(), { error(it) })
        var submitted = 0
        val receipts = mutableListOf<NativeReceipt>()
        val query = fixture.query
        fixture.session.enqueueWrite(queue, "query", attChunks(query.bytes, 20), query.operationId,
            query.applicationQuery, { submitted++; NativeStart.PENDING }, { receipts += it })
        check(submitted == 1 && receipts.last() == NativeReceipt.SUBMITTED)
        val awaiting = fixture.queryState()
        check(awaiting is MobileVescApplicationQueryStateDto.AwaitingResponse) {
            "A submitted Android application query must start its response interval"
        }
        val outputs = fixture.outputs.size
        fixture.clock = awaiting.responseDeadlineMs
        fixture.session.tick(canPoll = false)
        check(fixture.state.deviceSessionSnapshot().identity?.vescApplication == MobileVescApplicationDiscoveryDto.ABSENT)
        check(fixture.publications.last().identity?.vescApplication == MobileVescApplicationDiscoveryDto.ABSENT)
        check(fixture.outputs.size == outputs && submitted == 1)
        check(fixture.queryState() is MobileVescApplicationQueryStateDto.ResponseExpired)
        queue.close()
    }

    ApplicationQueryFixture().use { fixture ->
        var failures = 0
        val queue = GattOperationQueue({ fixture.session.isCurrent }, ManualTimer(), { failures++ })
        queue.enqueue(GattOperationQueue.Operation("blocked", { NativeStart.PENDING }))
        val query = fixture.query
        var submitted = 0
        val receipts = mutableListOf<NativeReceipt>()
        fixture.session.enqueueWrite(queue, "query", attChunks(query.bytes, 20), query.operationId,
            query.applicationQuery, { submitted++; NativeStart.PENDING }, { receipts += it })
        fixture.clock = checkNotNull(query.applicationQuery).submitBeforeMs
        fixture.session.tick(canPoll = false)
        check(fixture.queryState() is MobileVescApplicationQueryStateDto.KnownUnsent)
        check(fixture.publications.last().identity?.vescApplicationQuery is
            MobileVescApplicationQueryStateDto.KnownUnsent)
        check(queue.complete("blocked", true))
        check(submitted == 0 && receipts.last() == NativeReceipt.CANCELLED)
        check(queue.isIdle && failures == 0)
        var healthyStarted = false
        check(queue.enqueue(GattOperationQueue.Operation("healthy", {
            healthyStarted = true; NativeStart.COMPLETE
        })))
        check(healthyStarted && queue.isIdle && failures == 0)
        queue.close()
    }

    for (cancel in listOf(false, true)) ApplicationQueryFixture().use { fixture ->
        val queue = GattOperationQueue({ fixture.session.isCurrent }, ManualTimer(), { error(it) })
        val query = fixture.query
        val fragments = listOf(query.bytes.copyOfRange(0, 6), query.bytes.copyOfRange(6, query.bytes.size))
        var submitted = 0
        val receipts = mutableListOf<NativeReceipt>()
        fixture.session.enqueueWrite(queue, "query", fragments, query.operationId, query.applicationQuery,
            { submitted++; NativeStart.PENDING }, { receipts += it })
        check(submitted == 1 && receipts.last() == NativeReceipt.QUEUED)
        if (cancel) queue.close() else {
            fixture.clock = checkNotNull(query.applicationQuery).submitBeforeMs
            fixture.session.tick(canPoll = false)
            check(queue.complete("query", true))
        }
        check(submitted == 1 && receipts.last() == NativeReceipt.REJECTED)
        check(fixture.queryState() is MobileVescApplicationQueryStateDto.PartialUncertain)
        check(fixture.state.deviceSessionSnapshot().identity?.vescApplication == MobileVescApplicationDiscoveryDto.PENDING)
        queue.close()
    }

    ApplicationQueryFixture().use { fixture ->
        val queue = GattOperationQueue({ fixture.session.isCurrent }, ManualTimer(), { error(it) })
        queue.enqueue(GattOperationQueue.Operation("blocked", { NativeStart.PENDING }))
        val query = fixture.query
        var submitted = 0
        fixture.session.enqueueWrite(queue, "query", attChunks(query.bytes, 20), query.operationId,
            query.applicationQuery, { submitted++; NativeStart.PENDING })
        fixture.state.beginConnectionAttempt("replacement", 2UL)
        val publications = fixture.publications.size
        queue.close()
        fixture.session.applicationQueryReceipt(checkNotNull(query.applicationQuery), 0U, 1U, NativeReceipt.SUBMITTED)
        check(submitted == 0 && fixture.state.deviceSessionSnapshot().identity == null)
        check(fixture.publications.size == publications)
        check(!fixture.state.applicationQueryTransportIsCurrent(fixture.token,
            checkNotNull(query.applicationQuery).queryId, 2UL))
    }
}

private fun checkSettingDeadlinesUnderBackpressure() {
    for (submitted in listOf(true, false)) CutoutSessionStateHandle().use { state ->
        var clock = 1UL
        val token = checkNotNull(state.beginConnectionAttempt("NF2557", 0UL).token)
        state.connectionLinkEstablished(token)
        val outputs = mutableListOf<MobileSessionOutputDto>()
        var publication: MobileSettingsDto? = null
        val session = PevSession(state, token, { clock }, { outputs += it }) { _, _, settings ->
            publication = settings
        }
        // Same validated stationary NF2557 frame as the Rust settings-owner fixture.
        val frame = ByteArray(42)
        byteArrayOf(0xdc.toByte(), 0x5a, 0x5c, 38).copyInto(frame)
        frame[28] = (43_000 ushr 8).toByte()
        frame[29] = 43_000.toByte()
        state.observeConnectionNotification(token, frame)
        session.identify()
        check(state.deviceSessionSnapshot().identity?.model == "NOSFET Aero")
        val channel = eucReplayInputs().first {
            it.kind == MobileSessionInputKindDto.NOTIFICATION
        }.channel
        clock = 2UL
        session.notification(channel, frame)
        clock = 3UL
        session.submit(MobileSettingIdDto.VOLTAGE_CORRECTION, MobileSettingValueDto.Number(10))
        val write = outputs.single { it.kind == MobileSessionOutputKindDto.WRITE && it.operationId != null }
        val queue = GattOperationQueue({ session.isCurrent }, ManualTimer(), { error(it) })
        if (!submitted) queue.enqueue(GattOperationQueue.Operation("blocked", { NativeStart.PENDING }))
        var nativeWrites = 0
        val chunks = attChunks(write.bytes, 20)
        session.enqueueWrite(queue, "setting", chunks, write.operationId, null,
            { nativeWrites++; NativeStart.PENDING })
        if (submitted) repeat(chunks.size - 1) { check(queue.complete("setting", true)) }
        val expectedWrites = if (submitted) chunks.size else 0
        check(nativeWrites == expectedWrites)
        check(!queue.isIdle)
        val deadline = checkNotNull(session.nextWakeup(canPoll = false))
        check(deadline == 2_003UL)
        val outputCount = outputs.size
        clock = deadline
        session.tick(canPoll = false)
        val expected = if (submitted) MobileSettingStatusDto.TIMED_OUT else MobileSettingStatusDto.FAILED
        val setting = state.settings().settings.single { it.id == MobileSettingIdDto.VOLTAGE_CORRECTION }
        check(setting.status == expected) {
            "Blocked Android transport must advance ${if (submitted) "submitted confirmation" else "queued authorization"} deadline: ${setting.status}"
        }
        check(checkNotNull(publication).settings.single {
            it.id == MobileSettingIdDto.VOLTAGE_CORRECTION
        }.status == expected)
        check(session.nextWakeup(canPoll = false) == null) {
            "An expired setting deadline must not keep rearming the blocked transport timer"
        }
        check(outputs.size == outputCount && nativeWrites == expectedWrites)
        queue.close()
    }
}
