package lol.cutout

import java.nio.ByteBuffer
import java.util.ArrayDeque
import java.util.UUID
import uniffi.cutout_mobile_ffi.MobilePevcapWriteDispositionDto
import uniffi.cutout_mobile_ffi.aggregateWriteDispositions

fun UUID.wireBytes(): ByteArray = ByteBuffer.allocate(16)
    .putLong(mostSignificantBits).putLong(leastSignificantBits).array()

fun wireUuid(bytes: ByteArray): UUID {
    require(bytes.size == 16)
    val buffer = ByteBuffer.wrap(bytes)
    return UUID(buffer.long, buffer.long)
}

fun attChunks(bytes: ByteArray, limit: Int): List<ByteArray> {
    require(limit in 20..512)
    require(bytes.isNotEmpty())
    return bytes.asList().chunked(limit).map { it.toByteArray() }
}

/** The adapter supplies Android timers; deterministic tests supply a manual clock. */
fun interface NativeTimer {
    fun after(milliseconds: Long, task: () -> Unit): () -> Unit
}

enum class NativeReceipt { QUEUED, SUBMITTED, REJECTED, CANCELLED }
enum class NativeStart { PENDING, COMPLETE, REJECTED, CANCELLED }

/** Serial Android GATT operations, confined to the same owner as its connection. */
class GattOperationQueue(
    private val isCurrent: () -> Boolean,
    private val timer: NativeTimer,
    private val onFailure: (String) -> Unit,
    private val capacity: Int = 64,
) {
    class Operation(
        val key: String,
        val start: () -> NativeStart,
        val receipt: (NativeReceipt) -> Unit = {},
    )
    private val pending = ArrayDeque<Operation>()
    private var active: Operation? = null
    private var cancelTimeout: (() -> Unit)? = null
    private var closed = false
    val isIdle: Boolean get() = active == null && pending.isEmpty()

    fun enqueue(operation: Operation): Boolean {
        if (closed || !isCurrent()) {
            operation.receipt(NativeReceipt.CANCELLED)
            return false
        }
        if (pending.size + (if (active == null) 0 else 1) >= capacity) {
            operation.receipt(NativeReceipt.REJECTED)
            fail("Bluetooth operation queue is full")
            return false
        }
        operation.receipt(NativeReceipt.QUEUED)
        pending.addLast(operation)
        pump()
        return !closed
    }

    fun complete(key: String, succeeded: Boolean): Boolean {
        val operation = active ?: return false
        if (closed || !isCurrent() || operation.key != key) return false
        cancelTimeout?.invoke()
        cancelTimeout = null
        active = null
        if (!succeeded) {
            operation.receipt(NativeReceipt.REJECTED)
            fail("Bluetooth operation failed")
        } else pump()
        return true
    }

    fun close() {
        if (closed) return
        closed = true
        cancelTimeout?.invoke()
        cancelTimeout = null
        // An active operation was already submitted to Android. Cancelling the
        // connection cannot truthfully report that it was never submitted.
        active = null
        while (pending.isNotEmpty()) pending.removeFirst().receipt(NativeReceipt.CANCELLED)
    }

    private fun pump() {
        while (!closed && active == null && pending.isNotEmpty()) {
            if (!isCurrent()) { close(); return }
            val operation = pending.removeFirst()
            active = operation
            val result = try { operation.start() } catch (_: RuntimeException) { NativeStart.REJECTED }
            when (result) {
                NativeStart.CANCELLED -> {
                    active = null
                    operation.receipt(NativeReceipt.CANCELLED)
                }
                NativeStart.REJECTED -> {
                    active = null
                    operation.receipt(NativeReceipt.REJECTED)
                    fail("Bluetooth request was rejected")
                }
                NativeStart.COMPLETE -> {
                    active = null
                    operation.receipt(NativeReceipt.SUBMITTED)
                }
                NativeStart.PENDING -> {
                    operation.receipt(NativeReceipt.SUBMITTED)
                    cancelTimeout = timer.after(10_000) {
                        if (active === operation) {
                            // Android callbacks carry no request ID. Retire this GATT
                            // rather than advancing and attributing a late callback to
                            // another request against the same characteristic.
                            fail("Bluetooth request timed out")
                        }
                    }
                }
            }
        }
    }

    private fun fail(message: String) {
        close()
        onFailure(message)
    }
}

/** One Rust protocol write may require several native ATT writes. */
class NativeWriteReceipt(
    count: Int,
    private val report: (NativeReceipt) -> Unit,
) {
    private val parts = Array(count) { NativeReceipt.QUEUED }
    private var last: NativeReceipt? = null
    fun observe(index: Int, receipt: NativeReceipt) {
        parts[index] = receipt
        val aggregate = aggregateWriteDispositions(parts.map { it.toDisposition() }).toReceipt()
        if (last != aggregate) { last = aggregate; report(aggregate) }
    }

    private fun NativeReceipt.toDisposition(): MobilePevcapWriteDispositionDto = when (this) {
        NativeReceipt.QUEUED -> MobilePevcapWriteDispositionDto.QUEUED
        NativeReceipt.SUBMITTED -> MobilePevcapWriteDispositionDto.SUBMITTED
        NativeReceipt.REJECTED -> MobilePevcapWriteDispositionDto.REJECTED
        NativeReceipt.CANCELLED -> MobilePevcapWriteDispositionDto.CANCELLED
    }

    private fun MobilePevcapWriteDispositionDto.toReceipt(): NativeReceipt = when (this) {
        MobilePevcapWriteDispositionDto.QUEUED -> NativeReceipt.QUEUED
        MobilePevcapWriteDispositionDto.SUBMITTED -> NativeReceipt.SUBMITTED
        MobilePevcapWriteDispositionDto.REJECTED -> NativeReceipt.REJECTED
        MobilePevcapWriteDispositionDto.CANCELLED -> NativeReceipt.CANCELLED
    }
}
