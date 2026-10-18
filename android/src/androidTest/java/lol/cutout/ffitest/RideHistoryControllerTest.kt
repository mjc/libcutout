package lol.cutout.ffitest

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import lol.cutout.RideHistoryController
import lol.cutout.restoreRideSelection
import lol.cutout.RideMapRuntime
import org.junit.Test

open class RideHistoryControllerTest {
    @Test
    fun selectionRestorationUsesStableRideIdentityAcrossFreshRows() {
        data class Row(val identity: ByteArray, val version: Int)
        val original = Row(byteArrayOf(4, 2), 1)
        val refreshed = Row(byteArrayOf(4, 2), 2)
        val other = Row(byteArrayOf(9), 2)

        check(restoreRideSelection(original.identity, listOf(other, refreshed)) { it.identity } === refreshed) {
            "The selected ride should resolve to its refreshed Rust row by identity"
        }
        check(restoreRideSelection(original.identity, listOf(other)) { it.identity } == null) {
            "A ride removed by the active query must no longer remain selected"
        }
    }

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

            val allTime = CountDownLatch(1)
            controller.onChanged = { if (!controller.state.loading) allTime.countDown() }
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                controller.setLast30Days(false)
                controller.setIncludeShortRides(true)
                controller.setSearch("")
            }
            check(awaitCallback(allTime, 10, TimeUnit.SECONDS)) { "All-time history query did not complete" }
            val selectedRide = controller.state.rides.firstOrNull()
            if (selectedRide != null) {
                val rideKey = selectedRide.id.bytes.toList()
                val opened = CountDownLatch(1)
                controller.onChanged = { if (controller.state.selected != null && !controller.state.loading) opened.countDown() }
                InstrumentationRegistry.getInstrumentation().runOnMainSync { controller.select(selectedRide) }
                check(awaitCallback(opened, 10, TimeUnit.SECONDS)) { "Selected ride details did not complete" }

                val refreshed = CountDownLatch(1)
                controller.onChanged = {
                    if (controller.state.selected?.id?.bytes?.toList() == rideKey && !controller.state.loading) {
                        refreshed.countDown()
                    }
                }
                InstrumentationRegistry.getInstrumentation().runOnMainSync {
                    controller.refresh(preserveSelection = true)
                }
                check(awaitCallback(refreshed, 10, TimeUnit.SECONDS)) { "History refresh did not restore selected ride details" }
                check(controller.searchText.isEmpty() && !controller.last30Days && controller.includeShortRides) {
                    "Refreshing history changed its active filters"
                }
            } else {
                check(controller.searchText.isEmpty() && !controller.last30Days && controller.includeShortRides) {
                    "Refreshing empty history changed its active filters"
                }
            }
        } finally {
            runtime.close()
        }
    }

    /** Local Robolectric wrappers override this to drain their paused main looper. */
    protected open fun awaitCallback(latch: CountDownLatch, timeout: Long, unit: TimeUnit): Boolean =
        latch.await(timeout, unit)
}
