package lol.cutout.local

import android.app.Activity
import android.os.Looper
import android.view.View
import android.widget.ProgressBar
import android.widget.TextView
import lol.cutout.CaptureSendProgressView
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionProgressDto
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionProgressPhaseDto

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalCaptureSendProgressViewTest {
    @Test
    fun byteProgressAndReceiptWaitRemainDistinctUntilTheOperationCompletes() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val view = CaptureSendProgressView(activity)
        activity.setContentView(view)
        try {
            val bar = checkNotNull(view.findViewWithTag<ProgressBar>("captures.send.progress.bar"))
            val label = checkNotNull(view.findViewWithTag<TextView>("captures.send.progress.label"))
            val bytes = checkNotNull(view.findViewWithTag<TextView>("captures.send.progress.bytes"))
            val receiptSpinner = checkNotNull(view.findViewWithTag<ProgressBar>("captures.send.progress.receipt"))
            assertEquals(View.GONE, view.visibility)

            val listener = view.begin("operation")
            assertEquals(View.VISIBLE, view.visibility)
            assertTrue(bar.isIndeterminate)
            assertEquals("Starting Send", label.text.toString())

            listener.onProgress(progress(256uL, 1024uL))
            shadowOf(Looper.getMainLooper()).idle()
            assertFalse(bar.isIndeterminate)
            assertEquals(2500, bar.progress)
            assertEquals("Sending · 25%", label.text.toString())
            assertEquals("256 / 1024 bytes", bytes.text.toString())
            assertEquals("Sending · 25% · 256 / 1024 bytes", bar.contentDescription)
            assertEquals(View.ACCESSIBILITY_LIVE_REGION_POLITE, label.accessibilityLiveRegion)
            NativeSurfaceCapture.save(view, "capture-send-progress-dark")

            listener.onProgress(progress(1024uL, 1024uL))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(bar.max, bar.progress)
            assertEquals("Sending · 100%", label.text.toString())
            assertFalse(label.text.contains("Received"))

            listener.onProgress(progress(1024uL, 1024uL, MobileCaptureSubmissionProgressPhaseDto.VERIFYING_RECEIPT))
            shadowOf(Looper.getMainLooper()).idle()
            assertFalse(bar.isIndeterminate)
            assertEquals(bar.max, bar.progress)
            assertEquals(View.VISIBLE, receiptSpinner.visibility)
            assertEquals("Verifying receipt…", label.text.toString())
            assertEquals("1024 / 1024 bytes", bytes.text.toString())
            assertFalse(label.text.contains("Received"))
            NativeSurfaceCapture.save(view, "capture-send-awaiting-receipt-dark")

            view.clear()
            assertEquals(View.GONE, view.visibility)
            assertEquals("", label.text.toString())
            assertEquals(0, bar.progress)

            val retry = view.begin("operation")
            retry.onProgress(progress(128uL, 1024uL))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(1250, bar.progress)
            assertEquals("Sending · 12%", label.text.toString())
        } finally {
            host.pause().stop().destroy()
        }
    }

    @Test
    fun dismissCompletionAndRetryFenceQueuedCallbacksAndOlderByteCounts() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val view = CaptureSendProgressView(host.get())
        host.get().setContentView(view)
        try {
            val bar = checkNotNull(view.findViewWithTag<ProgressBar>("captures.send.progress.bar"))
            val first = view.begin("operation")
            first.onProgress(progress(800uL, 1024uL))
            view.clear()
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(View.GONE, view.visibility)

            val retry = view.begin("operation")
            first.onProgress(progress(1024uL, 1024uL))
            retry.onProgress(progress(256uL, 1024uL).copy(attemptId = 2uL))
            retry.onProgress(progress(128uL, 1024uL).copy(attemptId = 2uL))
            retry.onProgress(progress(900uL, 1024uL))
            retry.onProgress(progress(900uL, 1024uL).copy(operationId = "other", attemptId = 2uL))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(2500, bar.progress)

            view.clear()
            retry.onProgress(progress(1024uL, 1024uL).copy(attemptId = 2uL))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(View.GONE, view.visibility)
            assertEquals(0, bar.progress)
        } finally {
            host.pause().stop().destroy()
        }
    }

    @Test
    fun replacedOperationIdStillReportsThisAttempt() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val view = CaptureSendProgressView(host.get())
        host.get().setContentView(view)
        try {
            val label = checkNotNull(view.findViewWithTag<TextView>("captures.send.progress.label"))
            val listener = view.begin("cancelled-operation")
            listener.onProgress(progress(100uL, 1000uL).copy(operationId = "replacement"))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals("Sending · 10%", label.text.toString())
            listener.onProgress(progress(500uL, 1000uL).copy(operationId = "cancelled-operation"))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals("Sending · 10%", label.text.toString())
        } finally {
            host.pause().stop().destroy()
        }
    }

    private fun progress(
        sent: ULong,
        total: ULong,
        phase: MobileCaptureSubmissionProgressPhaseDto = MobileCaptureSubmissionProgressPhaseDto.UPLOADING,
    ) = MobileCaptureSubmissionProgressDto("operation", 1uL, sent, total, phase)
}
