package lol.cutout.local

import android.app.Activity
import android.content.res.Configuration
import android.os.Looper
import android.view.View
import android.widget.Button
import android.widget.ScrollView
import lol.cutout.CaptureLabelControls
import lol.cutout.DiagnosticCapturePresentation
import uniffi.cutout_mobile_ffi.*
import lol.cutout.CutoutApplication
import lol.cutout.DiagnosticCapturePanel
import lol.cutout.DiagnosticCaptureArtifact
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
import java.io.File

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
            assertEquals(lol.cutout.CutoutUi(activity).muted, stationary.currentTextColor)
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
            val startedElapsed = android.os.SystemClock.elapsedRealtime().toULong()
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofSeconds(65))
            val snapshot = DiagnosticCapturePresentation(ready = true,
                lifecycle = MobileCaptureLifecycleSnapshotDto(
                    MobileCaptureAttemptDto(MobileCaptureGenerationDto(42UL), MobileCaptureOriginDto.MANUAL,
                        MobileCaptureStageDto.RECORDING), false, true),
                writerStatus = MobileCaptureWriterStatusDto(0UL, 0UL, 0UL, 20UL, 20UL, false, null),
                activeLabels = listOf(MobileCaptureLabelDto.POWERED_ON_STATIONARY),
                deviceKind = "Bench capture", deviceName = "Falcon", note = "Bench capture",
                startedElapsedMillis = startedElapsed)
            panel.renderCapture(snapshot)
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            assertNull(panel.findViewWithTag<Button>("capture.label.powered_on_stationary.action"))
            checkNotNull(panel.findViewWithTag<View>("captures.active.open")).performClick()
            assertEquals("Bench capture", checkNotNull(panel.findViewWithTag<android.widget.TextView>("captures.active.kind")).text)
            assertEquals("Falcon", checkNotNull(panel.findViewWithTag<android.widget.TextView>("captures.active.device")).text)
            val elapsedText = checkNotNull(panel.findViewWithTag<android.widget.TextView>("captures.active.elapsed")).text.toString()
            val elapsedParts = elapsedText.split(":").map { it.toLong() }
            assertTrue("Elapsed time should include the Rust-provided monotonic start", elapsedParts[0] * 60 + elapsedParts[1] in 65L..67L)
            val elapsedBeforeTick = elapsedParts[0] * 60 + elapsedParts[1]
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(1_200))
            val elapsedAfterTick = checkNotNull(panel.findViewWithTag<android.widget.TextView>("captures.active.elapsed")).text
                .toString().split(":").map { it.toLong() }.let { it[0] * 60 + it[1] }
            assertTrue("Elapsed display should advance from monotonic time", elapsedAfterTick >= elapsedBeforeTick + 1)
            assertEquals(lol.cutout.CutoutUi(activity).label("captures.waiting"), checkNotNull(panel.findViewWithTag<android.widget.TextView>("captures.active.status")).text)
            val footer = checkNotNull(panel.findViewWithTag<View>("captures.active.footer"))
            assertSame("Stop controls must remain pinned outside the scroll content", panel, footer.parent)
            assertEquals(View.VISIBLE, footer.visibility)
            val settingsSummary = checkNotNull(panel.findViewWithTag<View>("capture.settings.summary"))
            val button = checkNotNull(panel.findViewWithTag<Button>("capture.label.powered_on_stationary.action"))
            button.isFocusableInTouchMode = true
            assertTrue(button.requestFocus())
            panel.renderCapture(snapshot.copy(notificationCount = 25UL,
                writerStatus = snapshot.writerStatus!!.copy(bytesWritten = 400UL)))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            assertSame(button, panel.findViewWithTag("capture.label.powered_on_stationary.action"))
            assertSame("The settings summary should remain stable across telemetry refreshes", settingsSummary,
                panel.findViewWithTag("capture.settings.summary"))
            assertTrue(button.hasFocus())
            assertTrue(button.isSelected)
            assertEquals(lol.cutout.CutoutUi(activity).label("captures.receiving"), checkNotNull(panel.findViewWithTag<android.widget.TextView>("captures.active.status")).text)
            checkNotNull(panel.findViewWithTag<Button>("captures.technical")).performClick()
            assertEquals("25", checkNotNull(panel.findViewWithTag<android.widget.TextView>("captures.active.packets.value")).text.toString())
            NativeSurfaceCapture.save(panel, "capture-active-labels-dark")
            assertTrue(panel.handleBack())
            assertNotNull(panel.findViewWithTag<View>("captures.active.open"))
            assertEquals(View.GONE, footer.visibility)
            assertFalse(panel.handleBack())
            checkNotNull(panel.findViewWithTag<View>("captures.active.open")).performClick()
            assertTrue(checkNotNull(panel.findViewWithTag<Button>("capture.label.powered_on_stationary.action")).isSelected)
            panel.renderCapture(snapshot.copy(lifecycle = MobileCaptureLifecycleSnapshotDto(null, true, true)))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            assertEquals(View.GONE, checkNotNull(panel.findViewWithTag<View>("captures.active.footer")).visibility)
            assertNotNull(panel.findViewWithTag<View>("captures.manual.start"))
            assertFalse(panel.handleBack())
        } finally {
            observer.close()
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun activeFinishCoalescesUntilRustPublishesCompletionAndAllowsRetryAfterFailure() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val generation = MobileCaptureGenerationDto(51UL)
        var submissions = 0
        var receipt: ((Boolean) -> Unit)? = null
        val panel = lol.cutout.ActiveCapturePanel(activity, { expected, completed ->
            assertEquals(generation, expected)
            submissions++
            receipt = completed
        }, {})
        activity.setContentView(panel.footer)
        val recording = DiagnosticCapturePresentation(lifecycle = MobileCaptureLifecycleSnapshotDto(
            MobileCaptureAttemptDto(generation, MobileCaptureOriginDto.MANUAL, MobileCaptureStageDto.RECORDING), false, true),
        )
        try {
            panel.render(recording, showFooter = true)
            val stop = checkNotNull(panel.footer.findViewWithTag<Button>("capture.stop"))
            assertTrue(stop.isEnabled)
            stop.performClick()
            stop.performClick()
            assertEquals("Repeated taps must coalesce before the Rust receipt", 1, submissions)
            assertFalse(stop.isEnabled)
            assertEquals(lol.cutout.CutoutUi(activity).label("captures.saving"), stop.text)
            checkNotNull(receipt)(true)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(1, submissions)
            panel.render(recording.copy(lifecycle = recording.lifecycle.copy(
                attempt = recording.lifecycle.attempt!!.copy(stage = MobileCaptureStageDto.SAVING))), true)
            assertFalse(stop.isEnabled)
            panel.render(recording.copy(lifecycle = recording.lifecycle.copy(
                attempt = recording.lifecycle.attempt!!.copy(stage = MobileCaptureStageDto.SAVE_FAILED))), true)
            assertTrue(stop.isEnabled)
            stop.performClick()
            assertEquals("A failed Rust save can be retried", 2, submissions)
            checkNotNull(receipt)(false)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(2, submissions)
            assertTrue(stop.isEnabled)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun savedRowOpensDetailAndBackPreservesTheLibrary() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val bluetooth = (activity.application as CutoutApplication).bluetooth
        val titles = mutableListOf<String>()
        val panel = DiagnosticCapturePanel(activity, bluetooth, {}, onTitleChanged = { titles += it })
        activity.setContentView(panel)
        val loaded = CountDownLatch(1)
        val observer = bluetooth.diagnostics.observe { if (it.ready && !it.loading) loaded.countDown() }
        try {
            assertTrue(awaitWithMainLooper(loaded, 15, TimeUnit.SECONDS))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            val entry = MobileLiveCaptureHistoryEntryDto("fixture-row", false,
                MobileCaptureIntegrityDto.Complete, null, "Falcon", 1792357200000UL, 1792357260000UL, 240UL, 8192UL)
            panel.renderCapture(DiagnosticCapturePresentation(ready = true, captures = listOf(entry)))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            val row = checkNotNull(panel.findViewWithTag<View>("captures.database.fixture-row"))
            assertTrue(row.performClick())
            assertNull(panel.findViewWithTag<View>("captures.manual.start"))
            assertNotNull(panel.findViewWithTag<View>("capture.submission.panel"))
            assertEquals("Recording", titles.last())
            assertTrue(panel.handleBack())
            assertSame(row, panel.findViewWithTag("captures.database.fixture-row"))
            assertEquals("Bluetooth Captures", titles.last())
            assertFalse(panel.handleBack())
            val recording = DiagnosticCapturePresentation(ready = true, captures = listOf(entry),
                lifecycle = MobileCaptureLifecycleSnapshotDto(
                    MobileCaptureAttemptDto(MobileCaptureGenerationDto(80UL), MobileCaptureOriginDto.MANUAL,
                        MobileCaptureStageDto.RECORDING), false, true))
            panel.renderCapture(recording)
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            checkNotNull(panel.findViewWithTag<View>("captures.database.fixture-row")).performClick()
            assertTrue(panel.handleBack())
            checkNotNull(panel.findViewWithTag<View>("captures.active.open")).performClick()
            val restoredFooter = checkNotNull(panel.findViewWithTag<View>("captures.active.footer"))
            assertSame(panel, restoredFooter.parent)
            assertEquals(View.VISIBLE, restoredFooter.visibility)
            assertTrue(panel.handleBack())
            panel.renderCapture(DiagnosticCapturePresentation(ready = true, historyError = "Unavailable"))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            assertNotNull(panel.findViewWithTag<View>("captures.history.retry"))
        } finally {
            observer.close()
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun activeFooterRemainsPinnedAndLastSectionClearsItAtNormalAndLargeText() {
        for (fontScale in listOf(1f, 1.5f)) {
            val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
            val base = host.get()
            val context = base.createConfigurationContext(Configuration(base.resources.configuration).apply { this.fontScale = fontScale })
            val bluetooth = (base.application as CutoutApplication).bluetooth
            val panel = DiagnosticCapturePanel(context, bluetooth, {})
            base.setContentView(panel)
            val loaded = CountDownLatch(1)
            val observer = bluetooth.diagnostics.observe { if (it.ready && !it.loading) loaded.countDown() }
            try {
                assertTrue(awaitWithMainLooper(loaded, 15, TimeUnit.SECONDS))
                shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
                val recording = DiagnosticCapturePresentation(ready = true,
                    lifecycle = MobileCaptureLifecycleSnapshotDto(
                        MobileCaptureAttemptDto(MobileCaptureGenerationDto(91UL), MobileCaptureOriginDto.MANUAL,
                            MobileCaptureStageDto.RECORDING), false, true),
                    deviceKind = "Bench capture", deviceName = "Falcon",
                    startedElapsedMillis = android.os.SystemClock.elapsedRealtime().toULong())
                panel.renderCapture(recording)
                shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
                checkNotNull(panel.findViewWithTag<View>("captures.active.open")).performClick()
                val name = if (fontScale > 1f) "capture-active-large-text" else "capture-active-header-dark"
                NativeSurfaceCapture.save(panel, name)
                val footer = checkNotNull(panel.findViewWithTag<View>("captures.active.footer"))
                val scroll = panel.getChildAt(0) as ScrollView
                assertEquals("Active recording opens at its header", 0, scroll.scrollY)
                assertTrue(footer.top > 0 && footer.bottom <= panel.height)
                val oldFooterHeight = footer.height
                val saving = recording.copy(lifecycle = recording.lifecycle.copy(
                    attempt = recording.lifecycle.attempt!!.copy(stage = MobileCaptureStageDto.SAVING)))
                panel.renderCapture(saving)
                shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
                NativeSurfaceCapture.save(panel, "$name-saving")
                assertEquals("Saving must not reflow the pinned control", oldFooterHeight, footer.height)
                val content = scroll.getChildAt(0)
                scroll.scrollTo(0, content.height)
                assertTrue("The final section must scroll above the footer",
                    content.height - content.paddingBottom - scroll.scrollY <= footer.top)
                assertEquals(panel.height, footer.bottom)
            } finally {
                observer.close()
                panel.dispose()
                host.pause().stop().destroy()
            }
        }
    }

    @Test
    fun sessionArtifactOpensFileDetailAndBackReturnsToSameLibraryRow() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val bluetooth = (activity.application as CutoutApplication).bluetooth
        val titles = mutableListOf<String>()
        val missingFile = File(activity.cacheDir, "missing-session-capture.jsonl")
        val artifact = DiagnosticCaptureArtifact(
            id = "artifact-session-1",
            file = missingFile,
            origin = MobileCaptureOriginDto.AUTOMATIC,
            platformIdentifier = "opaque-native-id",
            deviceName = "Falcon",
            description = "Connection attempt capture",
            startedAtUnixMillis = 1_792_357_200_000UL,
            elapsedMillis = 12_000UL,
            status = MobileCaptureWriterStatusDto(0UL, 0UL, 0UL, 256UL, 256UL, false, null),
            notificationCount = 1UL,
            error = null,
        )
        val panel = DiagnosticCapturePanel(activity, bluetooth, {}, onTitleChanged = { titles += it })
        activity.setContentView(panel)
        val loaded = CountDownLatch(1)
        val observer = bluetooth.diagnostics.observe { if (it.ready && !it.loading) loaded.countDown() }
        try {
            assertTrue(awaitWithMainLooper(loaded, 15, TimeUnit.SECONDS))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            panel.renderCapture(DiagnosticCapturePresentation(ready = true, sessionArtifacts = listOf(artifact)))
            shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(200))
            val row = checkNotNull(panel.findViewWithTag<View>("captures.session.artifact-session-1"))
            val initialTitle = titles.last()
            assertTrue(row.performClick())
            assertNotNull(panel.findViewWithTag<View>("captures.artifact.detail"))
            assertTrue(panelText(panel).contains("Capture file is unavailable"))
            assertTrue(titles.last().isNotBlank())
            assertTrue(panel.handleBack())
            assertSame(row, panel.findViewWithTag("captures.session.artifact-session-1"))
            assertEquals(initialTitle, titles.last())
        } finally {
            observer.close()
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    private fun panelText(view: View): String = buildString {
        if (view is android.widget.TextView) append(view.text).append('\n')
        if (view is android.view.ViewGroup) for (index in 0 until view.childCount) append(panelText(view.getChildAt(index)))
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
