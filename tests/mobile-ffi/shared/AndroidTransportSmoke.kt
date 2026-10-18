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
