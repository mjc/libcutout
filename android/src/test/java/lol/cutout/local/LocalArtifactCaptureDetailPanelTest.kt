package lol.cutout.local

import android.app.Activity
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import lol.cutout.ArtifactCaptureDetailPanel
import lol.cutout.CaptureSubmissionController
import lol.cutout.CaptureSubmissionPresentation
import lol.cutout.DiagnosticCaptureArtifact
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.MobileCaptureOriginDto
import uniffi.cutout_mobile_ffi.MobileCaptureWriterStatusDto
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalArtifactCaptureDetailPanelTest {
    @Test
    fun showsArtifactMetadataTechnicalDetailsAndExplicitShareAfterRetry() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val retained = captureFile(host.get().applicationContext.filesDir)
        val shared = File(host.get().cacheDir, "shared-capture.jsonl").apply { writeText("shareable\n") }
        val artifact = artifact(retained, error = "Saved file could not be indexed")
        var exportCalls = 0
        var callback: ((Result<File>) -> Unit)? = null
        val shares = mutableListOf<File>()
        val panel = ArtifactCaptureDetailPanel(
            host.get(), artifact,
            onExport = { supplied, done ->
                assertEquals(artifact.id, supplied.id)
                exportCalls++
                callback = done
            },
            onShare = { file -> shares.add(file); Unit },
            submissionController = FakeSubmissionController(),
            onSettings = {},
        )
        host.get().setContentView(panel)
        try {
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals("Detail must open at the artifact identity", 0, panel.scrollY)
            assertEquals(1, exportCalls)
            assertTrue(text(panel).contains("Demo wheel"))
            assertTrue(text(panel).contains("Manual capture"))
            assertTrue(text(panel).contains("Saved file could not be indexed"))
            assertTrue(text(panel).contains("Preparing file"))
            assertNull(panel.findViewWithTag<View>("captures.artifact.share"))

            val technicalToggle = checkNotNull(panel.findViewWithTag<View>("captures.artifact.technical.toggle"))
            technicalToggle.performClick()
            assertTrue(technicalToggle.isSelected)
            assertTrue(text(panel).contains("opaque-android-platform-identity"))
            assertTrue(text(panel).contains("capture-raw.jsonl"))
            assertTrue(text(panel).contains("Healthy"))

            callback?.invoke(Result.failure(IllegalStateException("synthetic share-copy error")))
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(text(panel).contains("Capture operation failed"))
            assertTrue(shares.isEmpty())
            assertTrue(panel.findViewWithTag<View>("captures.artifact.export.retry") != null)

            checkNotNull(panel.findViewWithTag<View>("captures.artifact.export.retry")).performClick()
            assertEquals(2, exportCalls)
            callback?.invoke(Result.success(shared))
            shadowOf(Looper.getMainLooper()).idle()
            assertNull(panel.findViewWithTag<View>("captures.artifact.export.retry"))
            checkNotNull(panel.findViewWithTag<View>("captures.artifact.share")).performClick()
            assertEquals(listOf(shared), shares)
            NativeSurfaceCapture.save(panel, "capture-file-artifact-detail-dark")
        } finally {
            panel.dispose()
            retained.delete()
            shared.delete()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun unavailableArtifactDoesNotStartExport() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val missing = File(host.get().cacheDir, "capture-absent.jsonl")
        var exportCalls = 0
        var callback: ((Result<File>) -> Unit)? = null
        val shares = mutableListOf<File>()
        val panel = ArtifactCaptureDetailPanel(
            host.get(), artifact(missing),
            onExport = { _, done -> exportCalls++; callback = done },
            onShare = { file -> shares.add(file); Unit },
            submissionController = FakeSubmissionController(),
            onSettings = {},
        )
        host.get().setContentView(panel)
        try {
            assertTrue(text(panel).contains("Capture file is unavailable"))
            assertEquals(0, exportCalls)
            panel.dispose()
            callback?.invoke(Result.success(File(host.get().cacheDir, "late-share.jsonl")))
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(shares.isEmpty())
            assertNull(panel.findViewWithTag<View>("captures.artifact.share"))
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun lateExportCompletionAfterDetailDisposalCannotRenderOrShare() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val retained = captureFile(host.get().applicationContext.filesDir)
        val callbackFile = File(host.get().cacheDir, "late-callback.jsonl").apply { writeText("late\n") }
        var callback: ((Result<File>) -> Unit)? = null
        val shares = mutableListOf<File>()
        val panel = ArtifactCaptureDetailPanel(
            host.get(), artifact(retained),
            onExport = { _, done -> callback = done },
            onShare = { file -> shares.add(file); Unit },
            submissionController = FakeSubmissionController(),
            onSettings = {},
        )
        host.get().setContentView(panel)
        try {
            assertNotNull(callback)
            panel.dispose()
            checkNotNull(callback)(Result.success(callbackFile))
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(shares.isEmpty())
            assertNull(panel.findViewWithTag<View>("captures.artifact.share"))
        } finally {
            panel.dispose()
            retained.delete()
            callbackFile.delete()
            host.pause().stop().destroy()
        }
    }

    private fun artifact(file: File, error: String? = null) = DiagnosticCaptureArtifact(
        id = "artifact-123",
        file = file,
        origin = MobileCaptureOriginDto.MANUAL,
        platformIdentifier = "opaque-android-platform-identity",
        deviceName = "Demo wheel",
        description = "Bench recording notes",
        startedAtUnixMillis = 1_729_000_000_000UL,
        elapsedMillis = 65_000UL,
        status = MobileCaptureWriterStatusDto(
            queuedMessages = 2UL,
            peakQueuedMessages = 8UL,
            droppedMessages = 0UL,
            bytesWritten = 8192UL,
            physicalBytesWritten = 8200UL,
            failed = false,
            lastError = null,
        ),
        notificationCount = 42UL,
        error = error,
    )

    private fun captureFile(filesDir: File): File = File(filesDir, "DiagnosticCaptures/capture-raw.jsonl").apply {
        parentFile?.mkdirs()
        writeText("raw capture\n")
    }

    private class FakeSubmissionController : CaptureSubmissionController {
        override fun current() = CaptureSubmissionPresentation()
        override fun observe(listener: (CaptureSubmissionPresentation) -> Unit): AutoCloseable {
            listener(current())
            return AutoCloseable { }
        }
        override fun refresh() = Unit
        override fun prepare() = Unit
        override fun send() = Unit
        override fun cancel() = Unit
        override fun delete() = Unit
    }

    private fun text(view: View): String = buildString {
        if (view is TextView) append(view.text).append('\n')
        if (view is ViewGroup) for (index in 0 until view.childCount) append(text(view.getChildAt(index)))
    }
}
