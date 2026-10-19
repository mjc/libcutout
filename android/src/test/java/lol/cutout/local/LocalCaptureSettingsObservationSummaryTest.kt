package lol.cutout.local

import android.app.Activity
import android.os.Looper
import android.view.View
import lol.cutout.CaptureSettingsObservationSummary
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalCaptureSettingsObservationSummaryTest {
    @Test
    fun failedReadOffersRetryAndOnlyLoadedObservationsOpenReview() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        var loads = 0
        var reviews = 0
        var complete: ((Result<List<MobileCaptureSettingsObservationDto>>) -> Unit)? = null
        val panel = CaptureSettingsObservationSummary(host.get(), { loads++; complete = it }, { reviews++ })
        host.get().setContentView(panel)
        try {
            val review = checkNotNull(panel.findViewWithTag<View>("capture.settings.observations"))
            val retry = checkNotNull(panel.findViewWithTag<View>("capture.settings.retry-load"))
            assertEquals(View.GONE, review.visibility)
            review.performClick()
            assertEquals(0, reviews)
            panel.refresh()
            panel.refresh()
            assertEquals("Only one read is submitted while loading", 1, loads)
            checkNotNull(complete)(Result.failure(IllegalStateException("Read unavailable")))
            assertEquals(View.VISIBLE, retry.visibility)
            assertEquals(View.VISIBLE, panel.findViewWithTag<View>("capture.settings.load-error").visibility)
            retry.performClick()
            assertEquals(2, loads)
            checkNotNull(complete)(Result.success(listOf(observation())))
            assertEquals(View.GONE, retry.visibility)
            assertSame(review, panel.findViewWithTag("capture.settings.observations"))
            assertEquals(View.VISIBLE, review.visibility)
            assertTrue(review.contentDescription.toString().endsWith(", 1"))
            review.performClick()
            assertEquals(1, reviews)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun newerOwnerReceiptSupersedesAnOlderReadWithoutReplacingTheReviewControl() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        var complete: ((Result<List<MobileCaptureSettingsObservationDto>>) -> Unit)? = null
        val panel = CaptureSettingsObservationSummary(host.get(), { complete = it }, {})
        host.get().setContentView(panel)
        try {
            val review = checkNotNull(panel.findViewWithTag<View>("capture.settings.observations"))
            panel.refresh()
            panel.render(listOf(observation(), observation().copy(observedAtMs = 30_000UL)))
            checkNotNull(complete)(Result.success(emptyList()))
            assertEquals(View.VISIBLE, review.visibility)
            assertTrue(review.contentDescription.toString().endsWith(", 2"))
            assertSame(review, panel.findViewWithTag("capture.settings.observations"))
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun emptyRetryClearsFailureAndAnOlderDuplicateCannotChangeIt() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val callbacks = mutableListOf<(Result<List<MobileCaptureSettingsObservationDto>>) -> Unit>()
        val panel = CaptureSettingsObservationSummary(host.get(), { callbacks.add(it); Unit }, {})
        host.get().setContentView(panel)
        try {
            panel.refresh()
            callbacks[0](Result.failure(IllegalStateException("Read unavailable")))
            panel.refresh()
            callbacks[0](Result.success(listOf(observation())))
            assertEquals(View.VISIBLE, panel.findViewWithTag<View>("capture.settings.summary.loading").visibility)
            callbacks[1](Result.success(emptyList()))
            assertEquals(View.GONE, panel.findViewWithTag<View>("capture.settings.observations").visibility)
            assertEquals(View.GONE, panel.findViewWithTag<View>("capture.settings.load-error").visibility)
            callbacks[0](Result.failure(IllegalStateException("Older duplicate failure")))
            assertEquals(View.GONE, panel.findViewWithTag<View>("capture.settings.load-error").visibility)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun newRouteRejectsPreviousReadEvenWhenBothSnapshotsAreEmpty() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val callbacks = mutableListOf<(Result<List<MobileCaptureSettingsObservationDto>>) -> Unit>()
        val panel = CaptureSettingsObservationSummary(host.get(), { callbacks.add(it); Unit }, {})
        host.get().setContentView(panel)
        try {
            panel.refresh()
            panel.reset()
            panel.refresh()
            assertEquals(2, callbacks.size)
            callbacks[0](Result.failure(IllegalStateException("Old recording ended")))
            assertEquals(View.GONE, panel.findViewWithTag<View>("capture.settings.load-error").visibility)
            callbacks[1](Result.success(listOf(observation())))
            assertEquals(View.VISIBLE, panel.findViewWithTag<View>("capture.settings.observations").visibility)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun disposedSummaryIgnoresBackgroundCompletionAndCannotRetryOrReview() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        var complete: ((Result<List<MobileCaptureSettingsObservationDto>>) -> Unit)? = null
        var loads = 0
        var reviews = 0
        val panel = CaptureSettingsObservationSummary(host.get(), { loads++; complete = it }, { reviews++ })
        host.get().setContentView(panel)
        try {
            panel.render(listOf(observation()))
            panel.refresh()
            panel.dispose()
            val callback = checkNotNull(complete)
            Thread { callback(Result.failure(IllegalStateException("Late read"))) }.apply { start(); join() }
            shadowOf(Looper.getMainLooper()).idle()
            panel.refresh()
            checkNotNull(panel.findViewWithTag<View>("capture.settings.observations")).performClick()
            assertEquals(1, loads)
            assertEquals(0, reviews)
            assertEquals(View.GONE, panel.findViewWithTag<View>("capture.settings.load-error").visibility)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }

    private fun observation(): MobileCaptureSettingsObservationDto {
        val catalog = checkNotNull(captureResearchCatalog("nosfet_aero"))
        return MobileCaptureSettingsObservationDto("capture", MobileCaptureResearchProfileDto(
            catalog.profileId, catalog.pevCategory, catalog.model, null, MobileCaptureResearchProfileSelectionDto.MANUAL),
            20_000UL, 21_000UL, MobileCaptureObservationSourceDto.WHEEL_DISPLAY,
            listOf(MobileCaptureSettingObservationValueDto("displayed_pack_voltage",
                MobileCaptureObservedValueDto.Entered("124.7"), "V", null)), null)
    }
}
