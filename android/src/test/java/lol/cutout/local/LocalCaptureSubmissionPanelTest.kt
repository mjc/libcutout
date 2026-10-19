package lol.cutout.local

import android.app.Activity
import android.app.AlertDialog
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import lol.cutout.CaptureSubmissionController
import lol.cutout.CaptureSubmissionPanel
import lol.cutout.CaptureSubmissionPresentation
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowAlertDialog
import uniffi.cutout_mobile_ffi.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalCaptureSubmissionPanelTest {
    @Test
    fun prepareOnlyFreezesDisclosureAndSendRequiresConfirmation() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val controller = FakeController()
        val panel = CaptureSubmissionPanel(host.get(), controller)
        host.get().setContentView(panel)
        try {
            checkNotNull(panel.findViewWithTag<View>("capture.submission.prepare")).performClick()
            assertEquals(1, controller.prepares)
            assertEquals(null, controller.sentOperation)
            val rendered = text(panel)
            assertTrue(rendered.contains("Send privately"))
            assertTrue(rendered.contains("Snapshot size"))
            assertTrue(rendered.contains("raw Bluetooth evidence"))
            assertTrue(rendered.contains("Music and provider listening metadata"))

            checkNotNull(panel.findViewWithTag<View>("capture.submission.send")).performClick()
            assertEquals(null, controller.sentOperation)
            val confirmation = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            assertTrue(confirmation.findViewById<TextView>(android.R.id.message).text.toString().contains("event order and timing"))
            NativeSurfaceCapture.save(panel, "capture-submission-disclosure-dark")
            clickPositive(confirmation)
            assertEquals("operation-1", controller.sentOperation)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun retryReceiptAndDeleteFollowRustPhasesAndConfirmations() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val controller = FakeController()
        val panel = CaptureSubmissionPanel(host.get(), controller)
        host.get().setContentView(panel)
        try {
            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.SENDING)))
            assertTrue(text(panel).contains("not received a durable receipt yet"))
            checkNotNull(panel.findViewWithTag<View>("capture.submission.send")).performClick()
            assertEquals(null, controller.sentOperation)
            clickPositive(checkNotNull(ShadowAlertDialog.getLatestAlertDialog()))
            assertEquals("operation-1", controller.sentOperation)

            controller.update(CaptureSubmissionPresentation(
                status = status(MobileCaptureSubmissionPhaseDto.FAILED, MobileCaptureSubmissionFailureDto.NETWORK),
            ))
            assertTrue(text(panel).contains("The network request failed"))
            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.SENT)))
            assertTrue(text(panel).contains("Durable service receipt: operation-1"))
            assertTrue(text(panel).contains("Submitted snapshot SHA-256: abcdef0123456789"))

            checkNotNull(panel.findViewWithTag<View>("capture.submission.delete")).performClick()
            assertEquals(null, controller.deletedOperation)
            val confirmation = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            assertTrue(confirmation.findViewById<TextView>(android.R.id.message).text.toString().contains("original recording stays on this device"))
            clickPositive(confirmation)
            assertEquals("operation-1", controller.deletedOperation)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun staleOperationOrPhaseDismissesConfirmationAndCannotSubmitDifferentSnapshot() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val controller = FakeController()
        val panel = CaptureSubmissionPanel(host.get(), controller)
        host.get().setContentView(panel)
        try {
            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.PREPARED)))
            checkNotNull(panel.findViewWithTag<View>("capture.submission.send")).performClick()
            val staleSend = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.PREPARED).copy(
                operationId = "operation-2",
            )))
            assertTrue(!staleSend.isShowing)
            clickPositive(staleSend)
            assertEquals(null, controller.sentOperation)

            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.SENT)))
            checkNotNull(panel.findViewWithTag<View>("capture.submission.delete")).performClick()
            val staleDelete = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.SENT).copy(
                operationId = "operation-3",
            )))
            assertTrue(!staleDelete.isShowing)
            clickPositive(staleDelete)
            assertEquals(null, controller.deletedOperation)

            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.PREPARED)))
            checkNotNull(panel.findViewWithTag<View>("capture.submission.send")).performClick()
            val stalePhase = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.FAILED)))
            assertTrue(!stalePhase.isShowing)
            clickPositive(stalePhase)
            assertEquals(null, controller.sentOperation)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun rejectedAndPendingRemovalPhasesDoNotClaimCompletion() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val controller = FakeController()
        val panel = CaptureSubmissionPanel(host.get(), controller)
        host.get().setContentView(panel)
        try {
            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.REJECTED)))
            assertTrue(text(panel).contains("This Send cannot be retried"))
            assertEquals(null, panel.findViewWithTag<View>("capture.submission.send"))

            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.CANCELLING)))
            assertTrue(text(panel).contains("Cancellation is pending"))
            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.CANCELLED)))
            assertTrue(text(panel).contains("Send cancelled"))

            controller.update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.DELETING)))
            assertTrue(text(panel).contains("has not been confirmed removed"))
            checkNotNull(panel.findViewWithTag<View>("capture.submission.delete.retry")).performClick()
            assertEquals(null, controller.deletedOperation)
            clickPositive(checkNotNull(ShadowAlertDialog.getLatestAlertDialog()))
            assertEquals("operation-1", controller.deletedOperation)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    private inner class FakeController : CaptureSubmissionController {
        private var presentation = CaptureSubmissionPresentation()
        private val listeners = mutableSetOf<(CaptureSubmissionPresentation) -> Unit>()
        var prepares = 0
        var sentOperation: String? = null
        var deletedOperation: String? = null

        override fun current() = presentation
        override fun observe(listener: (CaptureSubmissionPresentation) -> Unit): AutoCloseable {
            listeners += listener
            listener(presentation)
            return AutoCloseable { listeners -= listener }
        }
        override fun refresh() = Unit
        override fun prepare() {
            prepares++
            update(CaptureSubmissionPresentation(status = status(MobileCaptureSubmissionPhaseDto.PREPARED)))
        }
        override fun send() { sentOperation = presentation.status?.operationId }
        override fun cancel() = Unit
        override fun delete() { deletedOperation = presentation.status?.operationId }
        fun update(value: CaptureSubmissionPresentation) {
            presentation = value
            listeners.toList().forEach { it(value) }
        }
    }

    private fun status(
        phase: MobileCaptureSubmissionPhaseDto,
        failure: MobileCaptureSubmissionFailureDto? = null,
    ) = MobileCaptureSubmissionStatusDto(
        "operation-1", phase,
        MobileCaptureSubmissionCategoriesDto(true, false, true, true, true),
        3UL, 2048UL,
        if (phase == MobileCaptureSubmissionPhaseDto.SENT) MobileCaptureSubmissionReceiptDto(
            "operation-1", "abcdef0123456789", 2048UL, 1_729_000_000_000UL,
        ) else null,
        failure,
    )

    private fun text(view: View): String = buildString {
        if (view is TextView) append(view.text).append('\n')
        if (view is ViewGroup) for (index in 0 until view.childCount) append(text(view.getChildAt(index)))
    }

    private fun clickPositive(dialog: AlertDialog) {
        dialog.getButton(AlertDialog.BUTTON_POSITIVE).performClick()
        shadowOf(Looper.getMainLooper()).idle()
    }
}
