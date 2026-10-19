package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import uniffi.cutout_mobile_ffi.MobileSoundCloudState
import java.util.concurrent.CopyOnWriteArrayList

/** Receipts for Android foreground-service startup before a native audio-focus request. */
class MediaPlaybackForegroundAdmission(
    private val context: Context,
    private val startService: (Context) -> Boolean = RideForegroundService::startForCurrentWork,
) : AutoCloseable {
    private sealed interface State {
        data object Idle : State
        data class Starting(val generation: Long, val completed: (Result<Unit>) -> Unit) : State
        data class Admitted(val generation: Long) : State
    }
    private val main = Handler(Looper.getMainLooper())
    private val observers = CopyOnWriteArrayList<() -> Unit>()
    @Volatile private var state: State = State.Idle
    private var generation = 0L
    private var deadline: Runnable? = null
    val hasPendingNativePlayback: Boolean get() = state != State.Idle

    fun observe(observer: () -> Unit): AutoCloseable {
        observers += observer
        return AutoCloseable { observers -= observer }
    }

    fun request(completed: (Result<Unit>) -> Unit) {
        main.post {
            val previous = state
            deadline?.let(main::removeCallbacks)
            val request = ++generation
            state = State.Starting(request, completed)
            if (previous is State.Starting) previous.completed(Result.failure(IllegalStateException("Playback request was replaced")))
            // Android requires a foreground service before background audio focus. This
            // deadline bounds the native startup receipt; it is not a playback-time policy.
            deadline = Runnable {
                if (currentGeneration() == request) clear(IllegalStateException("Audio service did not start"))
            }.also { main.postDelayed(it, 4_000) }
            changed()
            if (!startService(context)) {
                clear(IllegalStateException("Audio service could not start"))
            }
        }
    }

    /** Called only after Android accepted startForeground with the media-playback type. */
    fun foregroundReady() {
        val pending = state as? State.Starting ?: return
        state = State.Admitted(pending.generation)
        deadline?.let(main::removeCallbacks)
        deadline = null
        pending.completed(Result.success(Unit))
    }

    fun foregroundFailed() = clear(IllegalStateException("Audio service could not start"))

    /** Release the startup claim once a real player outcome replaces it. */
    fun playbackChanged(value: AndroidSoundCloudPresentation, active: Boolean) {
        if (active) clear(null)
        else if (value.snapshot.state in setOf(MobileSoundCloudState.PAUSED, MobileSoundCloudState.FAILED, MobileSoundCloudState.IDLE)) {
            clear(IllegalStateException("Playback request ended"))
        }
    }

    private fun currentGeneration(): Long? = when (val current = state) {
        State.Idle -> null
        is State.Starting -> current.generation
        is State.Admitted -> current.generation
    }

    private fun clear(error: Exception?) {
        val previous = state
        if (previous == State.Idle) return
        state = State.Idle
        deadline?.let(main::removeCallbacks)
        deadline = null
        if (previous is State.Starting) previous.completed(Result.failure(error ?: IllegalStateException("Playback request ended")))
        changed()
    }

    private fun changed() { observers.forEach { it() } }
    override fun close() { main.post { clear(IllegalStateException("Audio service closed")); observers.clear() } }
}
