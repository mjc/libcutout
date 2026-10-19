package lol.cutout.local

import android.app.Activity
import android.os.Looper
import android.view.View
import lol.cutout.CutoutApplication
import lol.cutout.DiagnosticCapturePanel
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalDiagnosticCapturePanelTest {
    @Test
    fun manualCaptureRequestsBluetoothThroughTheHostPermissionAction() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val bluetooth = (activity.application as CutoutApplication).bluetooth
        var requests = 0
        val panel = DiagnosticCapturePanel(activity, bluetooth, {}, { requests++ })
        activity.setContentView(panel)
        val ready = CountDownLatch(1)
        val observer = bluetooth.diagnostics.observe { if (it.ready && !it.loading) ready.countDown() }
        try {
            assertTrue(awaitWithMainLooper(ready, 15, TimeUnit.SECONDS))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            val start = checkNotNull(panel.findViewWithTag<View>("captures.manual.start"))
            assertTrue(start.isEnabled)
            assertTrue(start.performClick())
            assertEquals(1, requests)
            NativeSurfaceCapture.save(panel, "captures-dark")
        } finally {
            observer.close()
            panel.dispose()
            host.pause().stop().destroy()
        }
    }
}
