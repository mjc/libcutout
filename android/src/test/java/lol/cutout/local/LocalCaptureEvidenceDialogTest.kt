package lol.cutout.local

import android.app.Activity
import android.app.AlertDialog
import android.os.Looper
import android.view.View
import lol.cutout.CaptureEvidenceDialog
import lol.cutout.CutoutApplication
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.Shadows.shadowOf
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalCaptureEvidenceDialogTest {
    @Test
    fun sentCaptureDoesNotOfferServerDeletion() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val receipt = MobileCaptureSubmissionReceiptDto(
            operationId = "fixed-operation", sha256 = "a".repeat(64), bytes = 100uL,
            receivedAtMilliseconds = 1_700_000_000_000uL,
        )
        fun status(phase: MobileCaptureSubmissionPhaseDto) = MobileCaptureSubmissionStatusDto(
            operationId = "fixed-operation", phase = phase,
            origin = MobileCaptureSubmissionOriginDto.UNKNOWN,
            integrity = MobileCaptureIntegrityDto.Unknown,
            localPayloadState = MobileCaptureSubmissionLocalPayloadStateDto.RETAINED,
            categories = MobileCaptureSubmissionCategoriesDto(
                deviceIdentifiers = true, location = false, notes = false,
                annotations = false, settingsObservations = false,
            ),
            droppedMessages = 0uL, byteCount = 100uL, receipt = receipt, failure = null,
        )
        val evidence = MobileCaptureEvidenceDto(
            note = "", screenshots = emptyList(), noteLimitBytes = 1_024u,
            screenshotLimit = 4u, screenshotLimitBytes = 5uL * 1_024uL * 1_024uL,
        )
        val dialog = CaptureEvidenceDialog(
            context = activity,
            charging = (activity.application as CutoutApplication).charging,
            source = MobileCaptureSubmissionSourceDto.LiveCapture("live-id"),
            pickImages = null,
            onReceiptConfirmed = {},
            onStatusChanged = {},
            testOperations = CaptureEvidenceDialog.TestOperations(
                load = { status(MobileCaptureSubmissionPhaseDto.SENT) to evidence },
                prepare = { error("A sent capture cannot be prepared again") },
                send = { _, _ -> error("No send expected") },
                remove = { _, _ -> error("A sent capture is not deleted from the server") },
            ),
        )

        try {
            dialog.show()
            val remove = find(dialog.currentDialog, "captures.send.remove")
            assertEquals(View.GONE, remove.visibility)
        } finally {
            dialog.removalConfirmationDialog?.dismiss()
            dialog.currentDialog?.dismiss()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun evidenceFailureShowsRetryAndOnlyExplicitConfirmationInvokesSend() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        var loads = 0
        var prepares = 0
        var sends = 0
        val prepared = MobileCaptureSubmissionStatusDto(
            operationId = "fixed-operation",
            phase = MobileCaptureSubmissionPhaseDto.PREPARED,
            origin = MobileCaptureSubmissionOriginDto.UNKNOWN,
            integrity = MobileCaptureIntegrityDto.Unknown,
            localPayloadState = MobileCaptureSubmissionLocalPayloadStateDto.RETAINED,
            categories = MobileCaptureSubmissionCategoriesDto(
                deviceIdentifiers = true, location = false, notes = true,
                annotations = false, settingsObservations = false,
            ),
            droppedMessages = 0uL,
            byteCount = 100uL,
            receipt = null,
            failure = null,
        )
        val evidence = MobileCaptureEvidenceDto(
            note = "private note must not be copied into the confirmation",
            screenshots = emptyList(),
            noteLimitBytes = 1_024u,
            screenshotLimit = 4u,
            screenshotLimitBytes = 5uL * 1_024uL * 1_024uL,
        )
        val dialog = CaptureEvidenceDialog(
            context = activity,
            charging = (activity.application as CutoutApplication).charging,
            source = MobileCaptureSubmissionSourceDto.LiveCapture("live-id"),
            pickImages = null,
            testOperations = CaptureEvidenceDialog.TestOperations(
                load = {
                    loads += 1
                    if (loads == 1) error("evidence unavailable")
                    null to evidence
                },
                prepare = { prepares += 1; prepared },
                send = { operationID, _ ->
                    assertEquals("fixed-operation", operationID)
                    sends += 1
                    prepared
                },
            ),
        )

        try {
            dialog.show()
            val review = find(dialog.currentDialog, "captures.send.review")
            val retry = find(dialog.currentDialog, "captures.evidence.retry")
            assertTrue(retry.isShown)
            assertFalse(review.isEnabled)
            assertEquals(0, prepares)
            assertEquals(0, sends)

            assertTrue(retry.performClick())
            assertEquals(2, loads)
            assertFalse(retry.isShown)
            assertTrue(review.isEnabled)
            assertEquals(0, sends)

            assertTrue(review.performClick())
            val confirmation = checkNotNull(dialog.confirmationDialog)
            assertTrue(confirmation.isShowing)
            val disclosure = confirmation.findViewById<android.widget.TextView>(android.R.id.message).text.toString()
            assertTrue(disclosure.contains("Origin unknown"))
            assertTrue(disclosure.contains("Integrity unknown"))
            assertFalse(disclosure.contains(evidence.note))
            assertEquals(1, prepares)
            assertEquals(0, sends)

            shadowOf(Looper.getMainLooper()).idle()
            confirmation.getButton(AlertDialog.BUTTON_POSITIVE).performClick()
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(1, sends)
        } finally {
            dialog.confirmationDialog?.dismiss()
            dialog.currentDialog?.dismiss()
            host.pause().stop().destroy()
        }
    }

    private fun find(dialog: AlertDialog?, tag: String): View {
        val root = checkNotNull(dialog?.window?.decorView)
        return checkNotNull(root.findViewWithTag(tag))
    }
}
