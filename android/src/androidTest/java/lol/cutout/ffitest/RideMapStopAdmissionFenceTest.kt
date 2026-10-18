package lol.cutout.ffitest

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.RideMapRuntime
import lol.cutout.RideMapStopAdmissionFence
import org.junit.Test
import uniffi.cutout_mobile_ffi.MobileRideLifecycleStateDto
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

/** Confirms every RideMapRuntime Stop waits for its registered native required-effect barrier. */
open class RideMapStopAdmissionFenceTest {
    @Test
    fun stopIsNotSubmittedUntilNativeAdmissionFenceProceeds() {
        val context: Context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
        val runtime = RideMapRuntime(context)
        val fence = HoldingFence()
        runtime.setStopAdmissionFence(fence)
        try {
            val started = await { runtime.startGpsRide(completed = it) }
            check(started.state == MobileRideLifecycleStateDto.ACTIVE)

            val stopResult = CountDownLatch(1)
            val stopValue = AtomicReference<Result<uniffi.cutout_mobile_ffi.MobileRideMapCoreSnapshotDto>?>()
            runtime.stop { result -> stopValue.set(result); stopResult.countDown() }
            check(fence.entered.await(5, TimeUnit.SECONDS)) { "Stop did not consult its native admission fence" }
            check(!stopResult.await(100, TimeUnit.MILLISECONDS)) {
                "Ride Stop reached Rust before the native admission fence completed"
            }

            fence.proceed.get()?.invoke() ?: error("Fence did not retain the Stop continuation")
            check(awaitCallback(stopResult, 10, TimeUnit.SECONDS)) { "Stop callback did not complete after fence release" }
            check(checkNotNull(stopValue.get()).getOrThrow().state == MobileRideLifecycleStateDto.STOPPED)
            val saved = await { runtime.save(it) }
            check(saved.state == MobileRideLifecycleStateDto.SAVED) {
                "The test-owned stopped ride must be saved before another shared-database test runs"
            }
        } finally {
            runtime.close()
        }
    }

    private class HoldingFence : RideMapStopAdmissionFence {
        val entered = CountDownLatch(1)
        val proceed = AtomicReference<(() -> Unit)?>(null)

        override fun beforeRideStop(proceed: () -> Unit) {
            this.proceed.set(proceed)
            entered.countDown()
        }

        override fun onRideStopFailed() = Unit
        override fun onRideRecordingActive() = Unit
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
