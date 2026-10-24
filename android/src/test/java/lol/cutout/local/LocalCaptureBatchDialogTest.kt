package lol.cutout.local

import android.app.Activity
import android.app.AlertDialog
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import lol.cutout.CaptureBatchCandidate
import lol.cutout.CaptureBatchProgress
import lol.cutout.CaptureBatchSummary
import lol.cutout.CaptureSubmissionBatchDialog
import lol.cutout.CaptureSubmissionBatchOperations
import lol.cutout.CutoutApplication
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

@RunWith(org.robolectric.RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalCaptureBatchDialogTest {
    @Test
    fun failedLocalReviewOffersRetryWithoutSending() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        var reviews = 0
        var sends = 0
        val operations = object : CaptureSubmissionBatchOperations {
            override fun review(completed: (Result<List<CaptureBatchCandidate>>) -> Unit) {
                reviews += 1
                if (reviews == 1) completed(Result.failure(IllegalStateException("local review failed")))
                else completed(Result.success(emptyList()))
            }

            override fun sendSelected(
                sourceArtifactIds: List<String>,
                progress: (CaptureBatchProgress) -> Unit,
                completed: (Result<CaptureBatchSummary>) -> Unit,
            ) {
                sends += 1
                completed(Result.success(CaptureBatchSummary(
                    sent = 0uL, cleaned = 0uL, failed = 0uL, skipped = 0uL, cancelled = 0uL,
                    failedSourceArtifactIds = emptyList(), pendingCleanupSourceArtifactIds = emptyList(),
                )))
            }

            override fun requestCancel() = Unit
        }
        val dialog = CaptureSubmissionBatchDialog(
            context = activity,
            charging = (activity.application as CutoutApplication).charging,
            directories = emptyList(),
            onFinished = {},
            testOperations = operations,
        )
        try {
            dialog.show()
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(containsText(dialog.reviewDialog, "local review failed"))
            assertEquals(0, sends)
            find(dialog.reviewDialog, "captures.batch.retryReview").performClick()
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(2, reviews)
            assertEquals(0, sends)
            assertFalse(dialog.reviewDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.isEnabled ?: true)
        } finally {
            dialog.reviewDialog?.dismiss()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun stopAfterCurrentRequestsOnlyBoundedRustAdmissionCancellation() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val row = candidate("source-current", eligible = true)
        var stopRequests = 0
        var reviews = 0
        var sends = 0
        var completeSend: ((Result<CaptureBatchSummary>) -> Unit)? = null
        val operations = object : CaptureSubmissionBatchOperations {
            override fun review(completed: (Result<List<CaptureBatchCandidate>>) -> Unit) {
                reviews += 1
                completed(Result.success(listOf(row)))
            }

            override fun sendSelected(
                sourceArtifactIds: List<String>,
                progress: (CaptureBatchProgress) -> Unit,
                completed: (Result<CaptureBatchSummary>) -> Unit,
            ) {
                sends += 1
                completeSend = completed
                progress(CaptureBatchProgress(
                    sourceArtifactId = row.sourceArtifactId, phase = "Current", message = "Sending",
                    total = 1uL, sent = 0uL, cleaned = 0uL, failed = 0uL, skipped = 0uL,
                    cancelled = 0uL, current = 1uL,
                ))
            }

            override fun requestCancel() { stopRequests += 1 }
        }
        val dialog = CaptureSubmissionBatchDialog(
            context = activity,
            charging = (activity.application as CutoutApplication).charging,
            directories = emptyList(),
            onFinished = {},
            testOperations = operations,
        )
        try {
            dialog.show()
            shadowOf(Looper.getMainLooper()).idle()
            dialog.reviewDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            shadowOf(Looper.getMainLooper()).idle()
            dialog.confirmationDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            shadowOf(Looper.getMainLooper()).idle()
            val stop = find(dialog.reviewDialog, "captures.batch.stop")
            assertTrue(stop.isShown)
            stop.performClick()
            assertEquals(1, stopRequests)
            assertFalse(find(dialog.reviewDialog, "captures.batch.stop").isEnabled)
            completeSend?.invoke(Result.success(CaptureBatchSummary(
                sent = 0uL, cleaned = 0uL, failed = 0uL, skipped = 0uL, cancelled = 1uL,
                failedSourceArtifactIds = emptyList(),
                pendingCleanupSourceArtifactIds = emptyList(),
            )))
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(null, dialog.reviewDialog?.window?.decorView?.findViewWithTag<View>("captures.batch.retry"))
            find(dialog.reviewDialog, "captures.batch.reviewAgain").performClick()
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(2, reviews)
            assertTrue(dialog.reviewDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.isEnabled == true)
            assertEquals(1, stopRequests)
            assertEquals("A new review creates a fresh plan but does not send it", 1, sends)
        } finally {
            dialog.confirmationDialog?.dismiss()
            dialog.reviewDialog?.dismiss()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun reviewRequiresConfirmationAndPartialFailureKeepsCaptureForRetry() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val eligible = candidate("source-eligible", eligible = true)
        val skipped = candidate("source-skipped", eligible = false)
        val calls = mutableListOf<List<String>>()
        var finishes = 0
        var receiptConfirmed = false
        val operations = object : CaptureSubmissionBatchOperations {
            override fun review(completed: (Result<List<CaptureBatchCandidate>>) -> Unit) {
                completed(Result.success(listOf(eligible, skipped)))
            }

            override fun sendSelected(
                sourceArtifactIds: List<String>,
                progress: (CaptureBatchProgress) -> Unit,
                completed: (Result<CaptureBatchSummary>) -> Unit,
            ) {
                calls += sourceArtifactIds
                assertEquals(listOf(eligible.sourceArtifactId), sourceArtifactIds)
                progress(CaptureBatchProgress(
                    sourceArtifactId = eligible.sourceArtifactId, phase = "Failed",
                    message = "Network outcome is not confirmed; local capture retained.",
                    total = 2uL, sent = 0uL, cleaned = 0uL, failed = 1uL, skipped = 1uL, cancelled = 0uL, current = 0uL,
                ))
                completed(Result.success(CaptureBatchSummary(
                    sent = 0uL, cleaned = 0uL, failed = 1uL, skipped = 1uL, cancelled = 0uL,
                    failedSourceArtifactIds = listOf(eligible.sourceArtifactId),
                    pendingCleanupSourceArtifactIds = emptyList(),
                )))
            }

            override fun requestCancel() = Unit
        }
        val dialog = CaptureSubmissionBatchDialog(
            context = activity,
            charging = (activity.application as CutoutApplication).charging,
            directories = listOf(File(activity.filesDir, "captures")),
            onFinished = { receipt -> finishes += 1; receiptConfirmed = receipt },
            testOperations = operations,
        )

        try {
            dialog.show()
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(find(dialog.reviewDialog, "captures.batch.candidate.${eligible.sourceArtifactId}").isShown)
            assertTrue(containsText(dialog.reviewDialog, "Opening this review does not contact the service"))
            assertEquals(0, calls.size)

            dialog.reviewDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            assertTrue(dialog.confirmationDialog?.isShowing == true)
            assertEquals("Showing the explicit confirmation still cannot send", 0, calls.size)
            val disclosure = dialog.confirmationDialog?.findViewById<TextView>(android.R.id.message)?.text?.toString().orEmpty()
            assertTrue(disclosure.contains("does not delete server copies"))
            shadowOf(Looper.getMainLooper()).idle()
            dialog.confirmationDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            shadowOf(Looper.getMainLooper()).idle()

            assertEquals(listOf(listOf(eligible.sourceArtifactId)), calls)
            assertEquals(1, finishes)
            assertFalse(receiptConfirmed)
            assertTrue(containsText(dialog.reviewDialog, "Network outcome is not confirmed; local capture retained."))
            assertFalse(containsText(dialog.reviewDialog, "Local copies removed"))
            assertTrue(find(dialog.reviewDialog, "captures.batch.candidate.${eligible.sourceArtifactId}").isShown)

            find(dialog.reviewDialog, "captures.batch.reviewFailedAgain").performClick()
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals("Reviewing failed captures cannot send", 1, calls.size)
            assertTrue(dialog.confirmationDialog?.isShowing != true)
            dialog.reviewDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            assertTrue(dialog.confirmationDialog?.isShowing == true)
            assertEquals(1, calls.size)
            shadowOf(Looper.getMainLooper()).idle()
            dialog.confirmationDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(listOf(eligible.sourceArtifactId), calls[1])
            assertEquals(2, finishes)
        } finally {
            dialog.confirmationDialog?.dismiss()
            dialog.reviewDialog?.dismiss()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun lateProgressFromPreviousAttemptCannotReplaceRetryResult() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val row = candidate("source-retry", eligible = true)
        var attempts = 0
        var reviews = 0
        var oldProgress: ((CaptureBatchProgress) -> Unit)? = null
        val operations = object : CaptureSubmissionBatchOperations {
            override fun review(completed: (Result<List<CaptureBatchCandidate>>) -> Unit) {
                reviews += 1
                completed(Result.success(listOf(row)))
            }

            override fun sendSelected(
                sourceArtifactIds: List<String>,
                progress: (CaptureBatchProgress) -> Unit,
                completed: (Result<CaptureBatchSummary>) -> Unit,
            ) {
                attempts += 1
                if (attempts == 1) {
                    oldProgress = progress
                    completed(Result.success(CaptureBatchSummary(
                        sent = 0uL, cleaned = 0uL, failed = 1uL, skipped = 0uL, cancelled = 0uL,
                        failedSourceArtifactIds = listOf(row.sourceArtifactId),
                        pendingCleanupSourceArtifactIds = emptyList(),
                    )))
                } else {
                    progress(CaptureBatchProgress(
                        sourceArtifactId = row.sourceArtifactId, phase = "Sent", message = "current receipt",
                        total = 1uL, sent = 1uL, cleaned = 0uL, failed = 0uL, skipped = 0uL,
                        cancelled = 0uL, current = 0uL,
                    ))
                    oldProgress?.invoke(CaptureBatchProgress(
                        sourceArtifactId = row.sourceArtifactId, phase = "Failed", message = "stale failure",
                        total = 1uL, sent = 0uL, cleaned = 0uL, failed = 1uL, skipped = 0uL,
                        cancelled = 0uL, current = 0uL,
                    ))
                    completed(Result.success(CaptureBatchSummary(
                        sent = 3uL, cleaned = 2uL, failed = 0uL, skipped = 0uL, cancelled = 0uL,
                        failedSourceArtifactIds = emptyList(),
                        pendingCleanupSourceArtifactIds = emptyList(),
                    )))
                }
            }

            override fun requestCancel() = Unit
        }
        val dialog = CaptureSubmissionBatchDialog(
            context = activity,
            charging = (activity.application as CutoutApplication).charging,
            directories = emptyList(),
            onFinished = {},
            testOperations = operations,
        )

        try {
            dialog.show()
            shadowOf(Looper.getMainLooper()).idle()
            dialog.reviewDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            shadowOf(Looper.getMainLooper()).idle()
            dialog.confirmationDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(find(dialog.reviewDialog, "captures.batch.reviewFailedAgain").isShown)
            assertTrue(containsText(dialog.reviewDialog, "Finished: 0 sent"))

            find(dialog.reviewDialog, "captures.batch.reviewFailedAgain").performClick()
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals("The fresh local review does not send", 1, attempts)
            assertEquals(2, reviews)
            dialog.reviewDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            assertTrue(dialog.confirmationDialog?.isShowing == true)
            shadowOf(Looper.getMainLooper()).idle()
            dialog.confirmationDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.performClick()
            shadowOf(Looper.getMainLooper()).idle()

            assertEquals(2, attempts)
            assertFalse(containsText(dialog.reviewDialog, "stale failure"))
            assertEquals(null, dialog.reviewDialog?.window?.decorView?.findViewWithTag<View>("captures.batch.reviewFailedAgain"))
            assertTrue(containsText(dialog.reviewDialog, "Finished: 3 sent"))
            assertFalse(containsText(dialog.reviewDialog, "Finished: 1 sent"))
        } finally {
            dialog.confirmationDialog?.dismiss()
            dialog.reviewDialog?.dismiss()
            host.pause().stop().destroy()
        }
    }

    private fun candidate(id: String, eligible: Boolean) = CaptureBatchCandidate(
        sourceArtifactId = id,
        sourceKind = "Live capture",
        displayName = null,
        eligible = eligible,
        skipReason = if (eligible) null else "Already sent",
        origin = "Unknown",
        integrity = "Unknown",
        sourceBytes = 100u,
        sourceSha256 = "a".repeat(64),
        transferBytes = 200u,
        transferSha256 = "b".repeat(64),
        categories = listOf("device identifiers"),
        noteBytes = 3u,
        screenshotCount = 0u,
        receipt = null,
    )

    private fun find(dialog: AlertDialog?, tag: String): View {
        val root = checkNotNull(dialog?.window?.decorView)
        return checkNotNull(root.findViewWithTag(tag))
    }

    private fun containsText(dialog: AlertDialog?, expected: String): Boolean {
        fun visit(view: View): Boolean {
            if (view is TextView && view.text.contains(expected)) return true
            if (view is ViewGroup) for (index in 0 until view.childCount) if (visit(view.getChildAt(index))) return true
            return false
        }
        return dialog?.window?.decorView?.let(::visit) == true
    }
}
