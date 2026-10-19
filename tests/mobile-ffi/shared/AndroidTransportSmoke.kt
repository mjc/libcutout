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
    checkProductionApplicationQueryTransport()
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

/** Generated Kotlin -> Rust policy, through the same submission path as Android WRITE effects. */
private fun checkProductionApplicationQueryTransport() {
    for (scenario in listOf("submitted", "fragmented", "deadline", "partial", "retired")) {
        CutoutSessionStateHandle().use { state ->
            var clock = 1UL
            val token = checkNotNull(state.beginConnectionAttempt("query-$scenario", 0UL).token)
            state.connectionLinkEstablished(token)
            val outputs = mutableListOf<MobileSessionOutputDto>()
            val session = PevSession(state, token, { clock }, { outputs += it }) { _, _, _ -> }
            val reply = byteArrayOf(2, 20, 157.toByte(), 7, 1, 2, 97, 98, 99, 49, 50, 51, 0,
                117, 115, 101, 114, 104, 97, 115, 104, 0, 38, 208.toByte(), 3)
            session.notification(byteArrayOf(), reply)
            val query = outputs.first { it.applicationQuery != null }
            val admission = checkNotNull(query.applicationQuery)
            var submissions = 0
            val queue = GattOperationQueue({ session.isCurrent }, ManualTimer(), {})
            if (scenario == "deadline" || scenario == "retired") {
                queue.enqueue(GattOperationQueue.Operation("blocker", { NativeStart.PENDING }))
            }
            // Force three actual ATT submissions for one authorized protocol output.
            val write = if (scenario == "partial" || scenario == "fragmented") query.copy(bytes = ByteArray(45)) else query
            session.enqueueWrite(write, queue, "query") {
                submissions++
                if (scenario == "partial" || scenario == "fragmented") NativeStart.PENDING else NativeStart.COMPLETE
            }
            when (scenario) {
                "submitted" -> {
                    check(submissions > 0)
                    val awaiting = state.deviceSessionSnapshot().identity?.vescApplicationQuery
                    check(awaiting is MobileVescApplicationQueryStateDto.AwaitingResponse) {
                        "Actual Android query submission must reach Rust: $awaiting"
                    }
                    clock = awaiting.responseDeadlineMs
                    session.tick()
                    check(state.deviceSessionSnapshot().identity?.vescApplication == MobileVescApplicationDiscoveryDto.ABSENT)
                }
                "fragmented" -> {
                    check(submissions == 1)
                    check(state.deviceSessionSnapshot().identity?.vescApplicationQuery is
                        MobileVescApplicationQueryStateDto.Queued)
                    clock = 100UL
                    check(queue.complete("query", true))
                    check(submissions == 2)
                    check(state.deviceSessionSnapshot().identity?.vescApplicationQuery is
                        MobileVescApplicationQueryStateDto.Queued)
                    clock = 200UL
                    check(queue.complete("query", true))
                    check(submissions == 3)
                    val awaiting = state.deviceSessionSnapshot().identity?.vescApplicationQuery
                    check(awaiting is MobileVescApplicationQueryStateDto.AwaitingResponse)
                    check(awaiting.submittedAtMs == clock)
                    clock = awaiting.responseDeadlineMs
                    session.tick()
                    check(state.deviceSessionSnapshot().identity?.vescApplication == MobileVescApplicationDiscoveryDto.ABSENT)
                }
                "deadline" -> {
                    clock = admission.submitBeforeMs
                    check(queue.complete("blocker", true))
                    check(submissions == 0) { "Expired query reached native GATT" }
                    check(state.deviceSessionSnapshot().identity?.vescApplication != MobileVescApplicationDiscoveryDto.ABSENT)
                }
                "partial" -> {
                    check(submissions == 1)
                    queue.close()
                    check(state.deviceSessionSnapshot().identity?.vescApplicationQuery is
                        MobileVescApplicationQueryStateDto.PartialUncertain)
                    clock = admission.submitBeforeMs + 10_000UL
                    session.tick()
                    check(state.deviceSessionSnapshot().identity?.vescApplication != MobileVescApplicationDiscoveryDto.ABSENT)
                }
                "retired" -> {
                    state.beginConnectionAttempt("replacement", clock)
                    queue.close()
                    check(submissions == 0)
                    check(state.deviceSessionSnapshot().identity == null)
                }
            }
            queue.close()
            state.disconnectConnectionAttempt()
        }
    }
}
