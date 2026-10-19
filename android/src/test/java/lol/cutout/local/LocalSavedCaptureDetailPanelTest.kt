package lol.cutout.local

import android.app.Activity
import android.content.res.Configuration
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import lol.cutout.CaptureSubmissionController
import lol.cutout.CaptureSubmissionPresentation
import lol.cutout.SavedCaptureDetailPanel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.*
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalSavedCaptureDetailPanelTest {
    @Test
    fun rendersDatabaseEvidenceAndOnlySharesAfterSuccessfulExport() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val controller = FakeSubmissionController()
        val exported = File(host.get().cacheDir, "saved-capture-export.jsonl").apply { writeText("{}\n") }
        var requestedId: String? = null
        var exportCallback: ((Result<File>) -> Unit)? = null
        val shares = mutableListOf<File>()
        var settingsOpened = 0
        val panel = SavedCaptureDetailPanel(
            host.get(), fixture(),
            onExport = { id, callback -> requestedId = id; exportCallback = callback },
            onShare = { file -> shares.add(file); Unit },
            submissionController = controller,
            onSettings = { settingsOpened++ },
        )
        host.get().setContentView(panel)
        try {
            assertTrue(text(panel).contains("Demo wheel"))
            assertTrue(text(panel).contains("Incomplete capture"))
            assertTrue(text(panel).contains("Events"))
            assertTrue(text(panel).contains("Stored bytes"))
            assertTrue(text(panel).contains("Model"))
            assertTrue(text(panel).contains("Verified Demo Model"))
            assertTrue(text(panel).contains("Review private Send"))
            assertNull(panel.findViewWithTag<View>("captures.detail.share"))
            val submissionPanel = checkNotNull(panel.findViewWithTag<View>("capture.submission.panel"))

            checkNotNull(panel.findViewWithTag<View>("captures.detail.export")).performClick()
            assertEquals("db-capture-1", requestedId)
            assertTrue(text(panel).contains("Preparing file"))
            assertTrue(shares.isEmpty())

            exportCallback?.invoke(Result.failure(IllegalStateException("synthetic export failure")))
            org.robolectric.Shadows.shadowOf(host.get().mainLooper).idle()
            assertTrue(text(panel).contains("Capture operation failed"))
            assertTrue(panel.findViewWithTag<View>("captures.detail.export") != null)
            assertSame(submissionPanel, panel.findViewWithTag<View>("capture.submission.panel"))
            assertTrue(shares.isEmpty())

            checkNotNull(panel.findViewWithTag<View>("captures.detail.export")).performClick()
            exportCallback?.invoke(Result.success(exported))
            org.robolectric.Shadows.shadowOf(host.get().mainLooper).idle()
            assertNull(panel.findViewWithTag<View>("captures.detail.export"))
            assertSame(submissionPanel, panel.findViewWithTag<View>("capture.submission.panel"))
            checkNotNull(panel.findViewWithTag<View>("captures.detail.share")).performClick()
            assertEquals(listOf(exported), shares)

            checkNotNull(panel.findViewWithTag<View>("captures.detail.settings")).performClick()
            assertEquals(1, settingsOpened)
            NativeSurfaceCapture.save(panel, "capture-saved-detail-dark")
        } finally {
            panel.dispose()
            exported.delete()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun disposedDetailIgnoresLateExportAndCannotShare() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        var callback: ((Result<File>) -> Unit)? = null
        val shares = mutableListOf<File>()
        val panel = SavedCaptureDetailPanel(
            host.get(), fixture(),
            onExport = { _, done -> callback = done },
            onShare = { file -> shares.add(file); Unit },
            submissionController = FakeSubmissionController(),
            onSettings = {},
        )
        host.get().setContentView(panel)
        try {
            checkNotNull(panel.findViewWithTag<View>("captures.detail.export")).performClick()
            panel.dispose()
            callback?.invoke(Result.success(File(host.get().cacheDir, "late-export.jsonl")))
            org.robolectric.Shadows.shadowOf(host.get().mainLooper).idle()
            assertTrue(shares.isEmpty())
            assertNull(panel.findViewWithTag<View>("captures.detail.share"))
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun longVerifiedModelWrapsWithoutCollapsingMetricLabelAtLargeFontScale() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val base = host.get()
        val largeConfiguration = Configuration(base.resources.configuration).apply { fontScale = 1.5f }
        val largeContext = base.createConfigurationContext(largeConfiguration)
        val modelName = "Verified Demo Model, high capacity Falcon long-range battery platform edition"
        val panel = SavedCaptureDetailPanel(
            largeContext, fixture(modelName),
            onExport = { _, done -> done(Result.failure(IllegalStateException("unused"))) },
            onShare = {},
            submissionController = FakeSubmissionController(),
            onSettings = {},
        )
        base.setContentView(panel)
        try {
            NativeSurfaceCapture.save(panel, "capture-saved-detail-large-text")
            assertEquals("Opening a recording must start at its identity", 0, panel.scrollY)
            val row = checkNotNull(panel.findViewWithTag<LinearLayout>("captures.detail.model"))
            val label = checkNotNull(panel.findViewWithTag<TextView>("captures.detail.model.label"))
            val value = checkNotNull(panel.findViewWithTag<TextView>("captures.detail.model.value"))
            assertTrue(label.measuredWidth > 0)
            assertTrue(value.measuredWidth > 0)
            assertTrue(value.lineCount > 1)
            assertTrue(label.left >= 0 && label.right <= row.width)
            assertTrue(value.left >= 0 && value.right <= row.width)
            assertEquals(modelName, value.text.toString())
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    private fun fixture(modelName: String = "Verified Demo Model") = MobileLiveCaptureHistoryEntryDto(
        liveCaptureId = "db-capture-1",
        interrupted = false,
        integrity = MobileCaptureIntegrityDto.Incomplete(2UL),
        recording = MobileRecordedCaptureDto(
            artifactId = MobileCaptureArtifactIdDto("artifact-1"),
            origin = MobileCaptureOriginDto.MANUAL,
            advertisedName = "Demo wheel",
            platformIdentifier = "opaque-platform-id",
            startedAt = MobileWallClockUnixMillisDto(1_729_000_000_000UL),
            model = MobileVerifiedStringDto(modelName, MobileVerificationStatusDto.HARDWARE_VERIFIED),
        ),
        platformIdentifier = "opaque-platform-id",
        startedAtMilliseconds = 1_729_000_000_000UL,
        finishedAtMilliseconds = 1_729_000_001_000UL,
        eventCount = 42UL,
        storedBytes = 4096UL,
    )

    private class FakeSubmissionController : CaptureSubmissionController {
        private val listeners = mutableSetOf<(CaptureSubmissionPresentation) -> Unit>()
        override fun current() = CaptureSubmissionPresentation()
        override fun observe(listener: (CaptureSubmissionPresentation) -> Unit): AutoCloseable {
            listeners += listener
            listener(current())
            return AutoCloseable { listeners -= listener }
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
