package lol.cutout.ffitest

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.RideMapRuntime
import org.junit.Test
import uniffi.cutout_mobile_ffi.MobileRideLifecycleStateDto
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

/** Exercises the runtime against the real Rust ride core and its durable SQLite receipt path. */
open class RideMapRuntimeTest {
    @Test
    fun gpsRideLifecyclePublishesTheSettledRustSnapshots() {
        val context: Context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
        // Every test shares the app's one Rust-owned service/path; Rust intentionally rejects
        // concurrent process-wide services opened from arbitrary per-test paths.
        val runtime = RideMapRuntime(context)
        try {
            val nativeDatabaseId = AtomicReference<String>()
            val nativeDatabaseError = AtomicReference<Throwable?>()
            val nativeOwnerDone = CountDownLatch(1)
            val nativeOwner = Thread {
                try {
                    nativeDatabaseId.set(runtime.databaseForNativeOwner().serviceId())
                } catch (error: Throwable) {
                    nativeDatabaseError.set(error)
                } finally {
                    nativeOwnerDone.countDown()
                }
            }
            nativeOwner.start()
            check(awaitCallback(nativeOwnerDone, 10, TimeUnit.SECONDS)) { "Shared database accessor did not settle" }
            nativeDatabaseError.get()?.let { throw it }
            val mapOwnerDatabaseId = await { done ->
                runtime.withDatabase({ it.serviceId() }, done)
            }
            check(nativeDatabaseId.get() == mapOwnerDatabaseId) {
                "Ride map and native owner received different Rust database services"
            }

            val initial = await { done ->
                runtime.withCore({ core ->
                    check(core.isReady()) { "Ride core must restore before publishing readiness" }
                    core.currentSnapshot(1UL)
                }, done)
            }
            check(initial == null) { "Fresh ride runtime unexpectedly restored an open ride" }

            val started = await { done -> runtime.startGpsRide(completed = done) }
            check(started.state == MobileRideLifecycleStateDto.ACTIVE)
            check(started.recordingToken != null)

            val paused = await { done -> runtime.pause(done) }
            check(paused.state == MobileRideLifecycleStateDto.PAUSED)

            val resumed = await { done -> runtime.resume(done) }
            check(resumed.state == MobileRideLifecycleStateDto.ACTIVE)

            val stopped = await { done -> runtime.stop(done) }
            check(stopped.state == MobileRideLifecycleStateDto.STOPPED)

            val saved = await { done -> runtime.save(done) }
            check(saved.state == MobileRideLifecycleStateDto.SAVED)

            val current = await { done ->
                runtime.withCore({ it.currentSnapshot(2UL) }, done)
            }
            check(current == null) { "Saving should retire the active ride projection" }
        } finally {
            runtime.close()
        }
    }

    private fun <T> await(start: ((Result<T>) -> Unit) -> Unit): T {
        val latch = CountDownLatch(1)
        val result = AtomicReference<Result<T>?>()
        start { value -> result.set(value); latch.countDown() }
        check(awaitCallback(latch, 15, TimeUnit.SECONDS)) { "Ride runtime callback did not complete" }
        return checkNotNull(result.get()).getOrThrow()
    }

    /** Local Robolectric wrappers override this to drain their paused main looper. */
    protected open fun awaitCallback(latch: CountDownLatch, timeout: Long, unit: TimeUnit): Boolean =
        latch.await(timeout, unit)
}
