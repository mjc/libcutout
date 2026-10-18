package lol.cutout.ffitest

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import lol.cutout.RideHistoryController
import lol.cutout.RideMapRuntime
import org.junit.Test

open class RideHistoryControllerTest {
    @Test
    fun rustHistoryQueryAndFilterReloadUseTheProcessOwnedDatabase() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val runtime = RideMapRuntime(context)
        val controller = RideHistoryController(runtime)
        val loaded = CountDownLatch(1)
        controller.onChanged = {
            if (!controller.state.loading) loaded.countDown()
        }

        try {
            InstrumentationRegistry.getInstrumentation().runOnMainSync { controller.refresh() }
            check(awaitCallback(loaded, 10, TimeUnit.SECONDS)) { "Rust ride history query did not complete" }
            check(controller.state.error == null) { "Rust ride history query failed: ${controller.state.error}" }
            check(controller.state.rides.map { it.id }.toSet().size == controller.state.rides.size) {
                "A Rust history page must not repeat ride identities"
            }

            val reloaded = CountDownLatch(1)
            controller.onChanged = {
                if (!controller.state.loading) reloaded.countDown()
            }
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                controller.setIncludeShortRides(true)
            }
            check(awaitCallback(reloaded, 10, TimeUnit.SECONDS)) { "Filtered Rust history query did not complete" }
            check(controller.state.error == null) { "Filtered Rust history query failed: ${controller.state.error}" }
            check(controller.state.rides.map { it.id }.toSet().size == controller.state.rides.size) {
                "A filtered Rust history page must not repeat ride identities"
            }
            check(controller.includeShortRides) { "The selected short-ride filter was lost" }
        } finally {
            runtime.close()
        }
    }

    /** Local Robolectric wrappers override this to drain their paused main looper. */
    protected open fun awaitCallback(latch: CountDownLatch, timeout: Long, unit: TimeUnit): Boolean =
        latch.await(timeout, unit)
}
