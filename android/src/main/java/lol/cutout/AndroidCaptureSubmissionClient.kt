package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import kotlinx.coroutines.*
import uniffi.cutout_mobile_ffi.*
import java.io.Closeable
import java.util.concurrent.Executor

data class CaptureSubmissionPresentation(
    val status: MobileCaptureSubmissionStatusDto? = null,
    val working: Boolean = false,
    val sending: Boolean = false,
    val cancelling: Boolean = false,
    val error: String? = null,
)

interface CaptureSubmissionController {
    fun current(): CaptureSubmissionPresentation
    fun observe(listener: (CaptureSubmissionPresentation) -> Unit): AutoCloseable
    fun refresh()
    fun prepare()
    fun send()
    fun cancel()
    fun delete()
}

/** Process-owned native effects. Rust retains snapshots, operation identity and transfer state. */
class AndroidCaptureSubmissionClient internal constructor(
    private val backend: () -> CaptureSubmissionClientHandleInterface,
    private val credential: suspend (allowRegistration: Boolean) -> MobileChargingIdentityDto?,
    private val message: (Throwable) -> String,
) : Closeable {
    constructor(context: Context, runtime: RideMapRuntime, charging: AndroidChargingClient) : this(
        NativeBackend(runtime),
        { allowRegistration ->
            if (!allowRegistration) charging.existingCredentialForCaptureSubmission()
            else suspendCancellableCoroutine { continuation ->
                charging.credentialForCaptureSubmission { result ->
                    if (continuation.isActive) continuation.resumeWith(result)
                }
            }
        },
        { error -> CutoutUi(context.applicationContext).label(errorKey(error)) },
    )

    private val main = Handler(Looper.getMainLooper())
    private val scope = CoroutineScope(SupervisorJob() + Executor { task -> main.post(task) }.asCoroutineDispatcher())
    private val controllers = mutableMapOf<MobileCaptureSubmissionSourceDto, Controller>()
    private var closed = false

    fun controller(source: MobileCaptureSubmissionSourceDto): CaptureSubmissionController {
        checkMain()
        check(!closed) { "Capture submissions are closed" }
        return controllers.getOrPut(source) { Controller(source) }
    }

    private inner class Controller(private val source: MobileCaptureSubmissionSourceDto) : CaptureSubmissionController {
        private var presentation = CaptureSubmissionPresentation()
        private val observers = linkedSetOf<(CaptureSubmissionPresentation) -> Unit>()
        private var publication = 0L

        override fun current(): CaptureSubmissionPresentation { checkMain(); return presentation }
        override fun observe(listener: (CaptureSubmissionPresentation) -> Unit): AutoCloseable {
            checkMain()
            observers += listener
            listener(presentation)
            return AutoCloseable {
                checkMain()
                observers -= listener
                releaseIfIdle()
            }
        }

        override fun refresh() {
            checkMain()
            if (closed || presentation.working || presentation.sending) return
            perform { it.status(source) }
        }

        override fun prepare() {
            checkMain()
            if (closed || presentation.working || presentation.sending) return
            perform { it.prepare(source) }
        }

        /** Only the native Send confirmation calls this method. */
        override fun send() {
            checkMain()
            if (closed || presentation.working || presentation.sending) return
            val operation = presentation.status?.operationId ?: return
            publish(presentation.copy(sending = true))
            perform(readBackOnFailure = true, finished = {
                publish(presentation.copy(sending = false)); releaseIfIdle()
            }) { client ->
                val selected = credential(true) ?: throw MobileCaptureSubmissionException.Unauthorized()
                client.send(operation, selected.identityId, selected.token)
            }
        }

        override fun cancel() {
            checkMain()
            if (closed || presentation.cancelling || (presentation.working && !presentation.sending)) return
            val operation = presentation.status?.operationId ?: return
            publish(presentation.copy(cancelling = true))
            perform(readBackOnFailure = true, finished = {
                publish(presentation.copy(cancelling = false)); releaseIfIdle()
            }) { client ->
                val existing = credential(false)
                client.cancel(operation, existing?.identityId, existing?.token)
            }
        }

        override fun delete() {
            checkMain()
            if (closed || presentation.working || presentation.sending) return
            val operation = presentation.status?.operationId ?: return
            perform(readBackOnFailure = true) { client ->
                val existing = credential(false) ?: throw MobileCaptureSubmissionException.Unauthorized()
                client.delete(operation, existing.identityId, existing.token)
            }
        }

        private fun perform(
            readBackOnFailure: Boolean = false,
            finished: () -> Unit = {},
            operation: suspend (CaptureSubmissionClientHandleInterface) -> MobileCaptureSubmissionStatusDto?,
        ) {
            if (closed) return
            val generation = ++publication
            publish(presentation.copy(working = true, error = null))
            scope.launch {
                try {
                    val result = withContext(Dispatchers.IO) { operation(backend()) }
                    if (generation == publication) publish(presentation.copy(status = result))
                } catch (cancelled: CancellationException) { throw cancelled }
                catch (error: Exception) {
                    val latest = if (readBackOnFailure) try {
                        withContext(Dispatchers.IO) { backend().status(source) }
                    } catch (cancelled: CancellationException) { throw cancelled }
                    catch (_: Exception) { null } else null
                    if (generation == publication) publish(presentation.copy(
                        status = latest ?: presentation.status, error = message(error),
                    ))
                } finally {
                    if (generation == publication) publish(presentation.copy(working = false))
                    finished()
                    releaseIfIdle()
                }
            }
        }

        private fun publish(next: CaptureSubmissionPresentation) {
            presentation = next
            if (!closed) observers.toList().forEach { observer -> runCatching { observer(next) } }
        }

        private fun releaseIfIdle() {
            if (observers.isEmpty() && !presentation.working && !presentation.sending && !presentation.cancelling && controllers[source] === this)
                controllers.remove(source)
        }
    }

    override fun close() {
        checkMain()
        closed = true
        scope.cancel()
        controllers.clear()
        (backend as? Closeable)?.close()
    }

    private class NativeBackend(private val runtime: RideMapRuntime) : () -> CaptureSubmissionClientHandleInterface, Closeable {
        private val native = lazy { CaptureSubmissionClientHandle(runtime.databaseForNativeOwner()) }
        override fun invoke(): CaptureSubmissionClientHandleInterface = native.value
        override fun close() { if (native.isInitialized()) native.value.close() }
    }

    private fun checkMain() = check(Looper.myLooper() == Looper.getMainLooper()) { "Capture presentation requires the Android main thread" }

    companion object {
        internal fun errorKey(error: Throwable): String = "capture.submission.error." + when (error) {
            is MobileCaptureSubmissionException.InvalidSource -> "invalid_source"
            is MobileCaptureSubmissionException.NotReady -> "not_ready"
            is MobileCaptureSubmissionException.Unavailable -> "unavailable"
            is MobileCaptureSubmissionException.Unauthorized -> "unauthorized"
            is MobileCaptureSubmissionException.RateLimited -> "rate_limited"
            is MobileCaptureSubmissionException.QuotaExceeded -> "quota_exceeded"
            is MobileCaptureSubmissionException.InvalidCapture -> "invalid_capture"
            is MobileCaptureSubmissionException.StorageFailure -> "storage_failure"
            is MobileCaptureSubmissionException.Cancelled -> "cancelled"
            is MobileCaptureSubmissionException.ReceiptMismatch -> "receipt_mismatch"
            else -> "generic"
        }
    }
}
