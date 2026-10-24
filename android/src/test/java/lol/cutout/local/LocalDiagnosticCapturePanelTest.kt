package lol.cutout.local

import android.app.Activity
import android.graphics.Color
import android.os.Looper
import android.view.View
import android.widget.ImageView
import android.widget.TextView
import lol.cutout.CutoutApplication
import lol.cutout.CutoutIcon
import lol.cutout.DiagnosticCapturePanel
import lol.cutout.R
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
import uniffi.cutout_mobile_ffi.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalDiagnosticCapturePanelTest {
    @Test
    fun interruptedHistoryKeepsPrivateReviewActionAndSentBadgeHasGreenIconAndText() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val panel = DiagnosticCapturePanel(
            activity,
            (activity.application as CutoutApplication).bluetooth,
            {},
            charging = (activity.application as CutoutApplication).charging,
        )
        val receipt = MobileCaptureSubmissionReceiptDto(
            operationId = "fixed-operation", sha256 = "a".repeat(64), bytes = 100uL,
            receivedAtMilliseconds = 1_700_000_000_000uL,
        )
        val status = MobileCaptureSubmissionStatusDto(
            operationId = "fixed-operation", phase = MobileCaptureSubmissionPhaseDto.SENT,
            origin = MobileCaptureSubmissionOriginDto.UNKNOWN, integrity = MobileCaptureIntegrityDto.Unknown,
            localPayloadState = MobileCaptureSubmissionLocalPayloadStateDto.RETAINED,
            categories = MobileCaptureSubmissionCategoriesDto(
                deviceIdentifiers = true, location = false, notes = false,
                annotations = false, settingsObservations = false,
            ),
            droppedMessages = 0uL, byteCount = 100uL, receipt = receipt, failure = null,
        )
        val entry = MobileLiveCaptureHistoryEntryDto(
            liveCaptureId = "interrupted-id", interrupted = true, integrity = MobileCaptureIntegrityDto.Unknown,
            recording = null, platformIdentifier = "wheel", startedAtMilliseconds = 100,
            finishedAtMilliseconds = 200, eventCount = 1uL, storedBytes = 100uL,
            submission = MobileCaptureHistorySubmissionDto(
                source = MobileCaptureSubmissionSourceDto.LiveCapture("interrupted-id"),
                sourceArtifactId = "interrupted-id", status = status,
            ),
        )

        try {
            val row = panel.historyRow(entry)
            val review = checkNotNull(row.findViewWithTag<View>("captures.history.interrupted-id.notes"))
            assertTrue(review.isEnabled)
            assertEquals(activity.getString(R.string.capture_review_private_send), (review as android.widget.Button).text)
            val interrupted = findText(row, activity.getString(R.string.capture_interrupted))
            assertEquals(Color.rgb(255, 159, 10), interrupted.currentTextColor)
            val sent = checkNotNull(row.findViewWithTag<TextView>("captures.history.interrupted-id.sent"))
            assertEquals(activity.getString(R.string.capture_sent_privately), sent.text)
            assertEquals(Color.rgb(48, 209, 88), sent.currentTextColor)
            val icon = checkNotNull(row.findViewWithTag<ImageView>("captures.history.interrupted-id.sent.icon"))
            assertTrue(icon.drawable is CutoutIcon)
            assertEquals(activity.getString(R.string.capture_sent_privately), icon.contentDescription)
        } finally {
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

    private fun findText(root: View, value: String): TextView {
        if (root is TextView && root.text.toString() == value) return root
        if (root is android.view.ViewGroup) {
            for (index in 0 until root.childCount) {
                runCatching { findText(root.getChildAt(index), value) }.getOrNull()?.let { return it }
            }
        }
        error("Text '$value' not found")
    }
}
