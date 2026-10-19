package lol.cutout.local

import android.app.Activity
import android.os.Looper
import android.view.View
import android.widget.Button
import lol.cutout.CaptureLabelControls
import lol.cutout.DiagnosticCapturePresentation
import uniffi.cutout_mobile_ffi.*
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
    fun labelControlsWaitForRustAdmissionAndKeepPriorStateOnFailure() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val generation = MobileCaptureGenerationDto(42UL)
        var requests = 0
        var failures = 0
        var completion: ((Result<List<MobileCaptureLabelDto>>) -> Unit)? = null
        val controls = CaptureLabelControls(activity, generation, { expected, action, done ->
            assertEquals(generation, expected)
            assertTrue(action is MobileCaptureLabelActionDto.Start)
            requests++
            completion = done
        }, { failures++ })
        activity.setContentView(controls)
        val snapshot = DiagnosticCapturePresentation(ready = true,
            lifecycle = MobileCaptureLifecycleSnapshotDto(
                MobileCaptureAttemptDto(generation, MobileCaptureOriginDto.MANUAL, MobileCaptureStageDto.RECORDING), false, true,
            ))
        try {
            controls.render(snapshot)
            val stationary = checkNotNull(controls.findViewWithTag<Button>("capture.label.powered_on_stationary.action"))
            assertTrue(stationary.performClick())
            assertEquals(1, requests)
            assertFalse(stationary.isEnabled)
            assertFalse(stationary.isSelected)
            stationary.performClick()
            assertEquals("An in-flight marker must not submit twice", 1, requests)
            checkNotNull(completion)(Result.success(listOf(MobileCaptureLabelDto.POWERED_ON_STATIONARY)))
            assertTrue(stationary.isSelected)
            assertTrue(stationary.isEnabled)
            assertTrue(stationary.contentDescription.toString().startsWith("Stop "))
            val charge = checkNotNull(controls.findViewWithTag<Button>("capture.label.charging.action"))
            charge.performClick()
            checkNotNull(completion)(Result.failure(MobileCaptureAnnotationException.AdmissionLost(1UL)))
            assertEquals(1, failures)
            assertTrue(stationary.isSelected)
            assertFalse(charge.isSelected)
            assertSame(stationary, controls.findViewWithTag("capture.label.powered_on_stationary.action"))
            controls.render(snapshot.copy(lifecycle = snapshot.lifecycle.copy(
                attempt = snapshot.lifecycle.attempt!!.copy(stage = MobileCaptureStageDto.SAVING))))
            assertFalse(stationary.isEnabled)
        } finally { host.pause().stop().destroy() }
    }

    @Test
    fun captureProgressKeepsTheFocusedLabelControlAndAdmittedSelection() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val bluetooth = (activity.application as CutoutApplication).bluetooth
        val panel = DiagnosticCapturePanel(activity, bluetooth, {})
        activity.setContentView(panel)
        val loaded = CountDownLatch(1)
        val observer = bluetooth.diagnostics.observe { if (it.ready && !it.loading) loaded.countDown() }
        try {
            assertTrue(awaitWithMainLooper(loaded, 15, TimeUnit.SECONDS))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            val snapshot = DiagnosticCapturePresentation(ready = true,
                lifecycle = MobileCaptureLifecycleSnapshotDto(
                    MobileCaptureAttemptDto(MobileCaptureGenerationDto(42UL), MobileCaptureOriginDto.MANUAL,
                        MobileCaptureStageDto.RECORDING), false, true),
                writerStatus = MobileCaptureWriterStatusDto(0UL, 0UL, 0UL, 20UL, 20UL, false, null),
                activeLabels = listOf(MobileCaptureLabelDto.POWERED_ON_STATIONARY))
            panel.renderCapture(snapshot)
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            val button = checkNotNull(panel.findViewWithTag<Button>("capture.label.powered_on_stationary.action"))
            button.isFocusableInTouchMode = true
            assertTrue(button.requestFocus())
            panel.renderCapture(snapshot.copy(notificationCount = 25UL,
                writerStatus = snapshot.writerStatus!!.copy(bytesWritten = 400UL)))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            assertSame(button, panel.findViewWithTag("capture.label.powered_on_stationary.action"))
            assertTrue(button.hasFocus())
            assertTrue(button.isSelected)
            NativeSurfaceCapture.save(panel, "capture-active-labels-dark")
        } finally {
            observer.close()
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

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
