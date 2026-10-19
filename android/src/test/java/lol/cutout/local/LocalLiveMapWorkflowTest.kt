package lol.cutout.local

import android.app.Activity
import android.content.DialogInterface
import android.view.View
import android.widget.Button
import lol.cutout.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowAlertDialog
import uniffi.cutout_mobile_ffi.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

/** Exercises production controls against Rust; only the map's GPU renderer is injected. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalLiveMapWorkflowTest {
    @Test
    fun idleRideCanStartAndDiscardRequiresConfirmation() {
        val host = Robolectric.buildActivity(Activity::class.java).setup()
        val activity = host.get()
        val runtime = RideMapRuntime(activity)
        val charging = AndroidChargingClient(activity, runtime)
        var permissionRequests = 0
        val panel = RideMapPanel(activity, runtime, charging, { permissionRequests++ }, {},
            mapSurfaceFactory = { TestRideMapSurface(it) })
        activity.setContentView(panel)
        try {
            assertTrue(await<Boolean> { done -> runtime.withCore({ it.isReady() }, done) })
            shadowOf(activity.mainLooper).idle()
            val start = checkNotNull(panel.findViewWithTag<Button>("ride_map_action_start"))
            assertTrue(start.performClick())
            val active = await<MobileRideMapCoreSnapshotDto?> { done -> runtime.withCore({ it.currentSnapshot(now()) }, done) }
            assertEquals(MobileRideLifecycleStateDto.ACTIVE, active?.state)
            assertEquals(1, permissionRequests)
            shadowOf(activity.mainLooper).idle()
            NativeSurfaceCapture.save(panel, "map-live-recording-dark")

            checkNotNull(panel.findViewWithTag<Button>("ride_map_action_stop")).performClick()
            val stopped = await<MobileRideMapCoreSnapshotDto?> { done -> runtime.withCore({ it.currentSnapshot(now()) }, done) }
            assertEquals(MobileRideLifecycleStateDto.STOPPED, stopped?.state)
            shadowOf(activity.mainLooper).idle()
            checkNotNull(panel.findViewWithTag<Button>("ride_map_action_discard")).performClick()
            val confirmation = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            assertTrue(confirmation.isShowing)
            val beforeConfirmation = await<MobileRideMapCoreSnapshotDto?> { done -> runtime.withCore({ it.currentSnapshot(now()) }, done) }
            assertEquals(MobileRideLifecycleStateDto.STOPPED, beforeConfirmation?.state)
            confirmation.getButton(DialogInterface.BUTTON_POSITIVE).performClick()
            shadowOf(activity.mainLooper).idle()
            assertNull(await<MobileRideMapCoreSnapshotDto?> { done -> runtime.withCore({ it.currentSnapshot(now()) }, done) })
        } finally {
            panel.onHostDestroy()
            charging.close()
            runtime.close()
            host.pause().stop().destroy()
        }
    }

    private fun now(): ULong = android.os.SystemClock.elapsedRealtime().toULong()

    private fun <T> await(start: ((Result<T>) -> Unit) -> Unit): T {
        val latch = CountDownLatch(1)
        val result = AtomicReference<Result<T>?>()
        start { result.set(it); latch.countDown() }
        check(awaitWithMainLooper(latch, 15, TimeUnit.SECONDS)) { "Live map callback did not settle" }
        return checkNotNull(result.get()).getOrThrow()
    }

}
