package lol.cutout.local

import android.app.Activity
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import lol.cutout.CaptureSettingsReview
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalCaptureSettingsReviewTest {
    @Test
    fun readbackFailureRetainsIndependentValuesAndRetryShowsActualComparison() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val catalog = checkNotNull(captureResearchCatalog("nosfet_aero"))
        val observation = observation()
        var loads = 0
        var reads = 0
        var finish: ((Result<List<MobileCaptureResearchReadbackDto>>) -> Unit)? = null
        val review = CaptureSettingsReview(host.get(), { done -> loads++; done(Result.success(listOf(observation))) },
            { requests, done ->
                reads++
                assertEquals(listOf(MobileCaptureResearchReadbackRequestDto(catalog.profileId, 20_000UL)), requests)
                finish = done
            })
        val dialog = review.show()
        try {
            val root = dialog.window!!.decorView
            assertTrue(text(root).contains("124.7 V"))
            checkNotNull(finish)(Result.failure(MobileCaptureResearchException.WriterFailed()))
            assertTrue(text(root).contains("Could not read captured settings."))
            assertTrue(text(root).contains("124.7 V"))
            checkNotNull(root.findViewWithTag<View>("capture.settings.review.retry")).performClick()
            assertEquals(1, loads)
            assertEquals(2, reads)
            checkNotNull(finish)(Result.success(listOf(MobileCaptureResearchReadbackDto(catalog.profileId, 20_000UL,
                listOf(MobileCaptureResearchDecodedValueDto("displayed_pack_voltage", MobileSettingValueDto.Number(125000), 19_000UL))))))
            assertFalse(text(root).contains("Could not read captured settings."))
            assertTrue(text(root).contains("124.7 V"))
            assertTrue(text(root).contains("00:19"))
            assertTrue(text(root).contains("Values differ."))
            NativeSurfaceCapture.save(root, "capture-settings-review-dark")
        } finally { dialog.dismiss(); host.pause().stop().destroy() }
    }

    @Test
    fun dismissedReviewDoesNotStartReadbackWhenHistoryArrivesLate() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        var finish: ((Result<List<MobileCaptureSettingsObservationDto>>) -> Unit)? = null
        var reads = 0
        val dialog = CaptureSettingsReview(host.get(), { finish = it }, { _, _ -> reads++ }).show()
        dialog.dismiss()
        checkNotNull(finish)(Result.success(listOf(observation())))
        assertEquals(0, reads)
        host.pause().stop().destroy()
    }

    private fun observation(): MobileCaptureSettingsObservationDto {
        val catalog = checkNotNull(captureResearchCatalog("nosfet_aero"))
        return MobileCaptureSettingsObservationDto("capture", MobileCaptureResearchProfileDto(
            catalog.profileId, catalog.pevCategory, catalog.model, null, MobileCaptureResearchProfileSelectionDto.MANUAL),
            20_000UL, 21_000UL, MobileCaptureObservationSourceDto.WHEEL_DISPLAY,
            listOf(MobileCaptureSettingObservationValueDto("displayed_pack_voltage",
                MobileCaptureObservedValueDto.Entered("124.7"), "V", null)), "Read from the display")
    }

    private fun text(view: View): String = buildString {
        if (view is TextView) append(view.text).append('\n')
        if (view is ViewGroup) for (i in 0 until view.childCount) append(text(view.getChildAt(i)))
    }
}
