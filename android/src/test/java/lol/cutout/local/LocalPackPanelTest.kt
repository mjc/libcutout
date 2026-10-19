package lol.cutout.local

import android.app.Activity
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.GridLayout
import android.widget.TextView
import lol.cutout.PackPanel
import lol.cutout.R
import lol.cutout.CutoutUi
import lol.cutout.ffitest.PackPanelTest
import lol.cutout.ffitest.productionReplayPresentation
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.robolectric.Robolectric
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalPackPanelTest : PackPanelTest() {
    @Test
    fun controllerOnlyEstimateStacksAtAccessibilityTextSize() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val base = host.get()
        val context = base.createConfigurationContext(Configuration(base.resources.configuration).apply { fontScale = 1.5f })
        val panel = PackPanel(context)
        base.setContentView(panel)
        try {
            panel.render(productionReplayPresentation().copy(bms = null))
            NativeSurfaceCapture.save(panel, "pack-controller-only-large-text")
            val estimateCard = checkNotNull(panel.findViewWithTag<ViewGroup>("bms.no-data.estimate"))
            val hero = estimateCard.getChildAt(0) as android.widget.LinearLayout
            assertEquals(android.widget.LinearLayout.VERTICAL, hero.orientation)
            assertTrue(hero.getChildAt(1).top >= hero.getChildAt(0).bottom)
            assertEquals(0, panel.scrollY)
        } finally { host.pause().stop().destroy() }
    }

    @Test
    fun noBmsRendersSharedEvidenceAndDoesNotPromoteRetainedReadingsToLiveConfidence() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val panel = PackPanel(activity)
        activity.setContentView(panel)
        try {
            val replay = productionReplayPresentation()
            val fallback = partialSnapshot().copy(availability = MobileReadbackAvailabilityDto.UNSUPPORTED,
                energyPercent = BatteryLevelReading(BatteryLevel(45u), MobileValueSourceDto.REPORTED,
                    MobileValueQualityDto.KNOWN, MobileVerificationStatusDto.SOURCE_VERIFIED))
            val telemetry = checkNotNull(replay.telemetry).copy(
                batteryLevelReported = BatteryLevelReading(BatteryLevel(0u), MobileValueSourceDto.REPORTED,
                    MobileValueQualityDto.KNOWN, MobileVerificationStatusDto.SOURCE_VERIFIED),
                voltageSag = VoltageDeltaReading(VoltageDelta(0), MobileValueSourceDto.CALCULATED,
                    MobileValueQualityDto.KNOWN, MobileVerificationStatusDto.SOURCE_VERIFIED),
            )
            panel.render(replay.copy(bms = fallback, telemetry = telemetry, chargeEstimate = null))
            val confidence = checkNotNull(panel.findViewWithTag<TextView>("bms.no-data.confidence"))
            assertEquals("Medium", confidence.text.toString())
            assertTrue(texts(panel).contains("0 %"))
            assertNotNull(panel.findViewWithTag<View>("bms.no-data.warning"))
            assertNull(panel.findViewWithTag<View>("pack.group.1"))
            NativeSurfaceCapture.save(panel, "pack-controller-only")

            panel.render(replay.copy(bms = fallback, telemetry = null, chargeEstimate = null))
            assertSame(confidence, panel.findViewWithTag("bms.no-data.confidence"))
            assertEquals("Unknown", confidence.text.toString())
            assertTrue(texts(panel).contains("45 %"))
            panel.render(lol.cutout.BlePresentation())
            assertNull(panel.findViewWithTag<View>("bms.no-data.warning"))
        } finally { host.pause().stop().destroy() }
    }

    @Test
    fun balancingCardAppearsForNonblankSummaryAndUpdatesInPlace() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val panel = PackPanel(activity)
        activity.setContentView(panel)
        try {
            val replay = productionReplayPresentation()
            panel.render(replay.copy(bms = partialSnapshot().copy(balancingSummary = "")))
            assertNull(panel.findViewWithTag<TextView>("pack.balancing.detail"))

            panel.render(replay.copy(bms = partialSnapshot().copy(balancingSummary = "Balancing active")))
            val detail = panel.findViewWithTag<TextView>("pack.balancing.detail")
            assertNotNull(detail)
            assertEquals("Balancing active", detail.text.toString())

            panel.render(replay.copy(bms = partialSnapshot().copy(balancingSummary = "Balancing stopped")))
            assertSame(detail, panel.findViewWithTag<TextView>("pack.balancing.detail"))
            assertEquals("Balancing stopped", detail.text.toString())
        } finally {
            host.pause().stop().destroy()
        }
    }

    @Test
    fun partialSnapshotShowsOnlyPresentPackMetricsAndKeepsDistinctBmsSections() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val panel = PackPanel(activity)
        activity.setContentView(panel)
        try {
            val snapshot = partialSnapshot()
            panel.render(productionReplayPresentation().copy(bms = snapshot))

            assertNotNull(panel.findViewWithTag<TextView>("pack.metric.${activity.getString(R.string.voltage)}"))
            assertNull(panel.findViewWithTag<TextView>("pack.metric.${activity.getString(R.string.battery)}"))
            assertFalse(texts(panel).contains(activity.getString(R.string.pack_current_1)))
            assertFalse(texts(panel).contains(activity.getString(R.string.pack_current_2)))
            assertFalse(texts(panel).contains("Fault summary must not be repeated"))

            assertTrue(texts(panel).contains(activity.getString(R.string.pack_difference)))
            assertEquals(1, texts(panel).count { it == "12 mV" })
            assertTrue(texts(panel).contains(activity.getString(R.string.pack_balancing)))
            assertTrue(texts(panel).contains("Balancing is active"))
            assertTrue(texts(panel).any { it.contains("Cell temperature high") })
            assertNotNull(panel.findViewWithTag<View>("pack.group.1"))
            assertNotNull(panel.findViewWithTag<View>("pack.group.2"))
        } finally {
            host.pause().stop().destroy()
        }
    }

    @Test
    fun groupTilesExposeRustPackIdentityAlertsAndReturnToTheCellMap() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val panel = PackPanel(activity)
        activity.setContentView(panel)
        try {
            val replay = productionReplayPresentation()
            val base = partialSnapshot()
            val firstGroup = base.groups.first().copy(
                packNumber = 2u,
                packReadingIndex = 7u,
                label = "Left pack",
                alertLevel = MobileBmsAlertLevelDto.WARNING,
                isBalancing = true,
                detail = "Voltage trend rising",
            )
            val snapshot = base.copy(groups = listOf(firstGroup, base.groups.last()))
            panel.render(replay.copy(bms = snapshot))

            val tile = checkNotNull(panel.findViewWithTag<ViewGroup>("pack.group.1"))
            assertEquals("P2 · 07", panel.findViewWithTag<TextView>("pack.group.title.1")?.text?.toString())
            assertEquals("!", panel.findViewWithTag<TextView>("pack.group.alert.1")?.text?.toString())
            val warningStroke = renderedTopBorderPixel(tile, activity)
            assertTrue(tile.contentDescription.contains("Left pack"))
            assertTrue(tile.contentDescription.contains(activity.getString(R.string.pack_cell_warning)))
            assertTrue(tile.contentDescription.contains(activity.getString(R.string.pack_balancing)))
            assertTrue(tile.contentDescription.contains("Voltage trend rising"))
            assertTrue((panel.findViewWithTag<View>("pack.group-grid") as GridLayout).columnCount >= 3)

            panel.render(replay.copy(bms = snapshot.copy(groups = listOf(
                firstGroup.copy(alertLevel = MobileBmsAlertLevelDto.CRITICAL), snapshot.groups.last(),
            ))))
            assertSame(tile, panel.findViewWithTag<View>("pack.group.1"))
            assertTrue("Critical alert should change the rendered tile outline", warningStroke != renderedTopBorderPixel(tile, activity))

            tile.performClick()
            buttons(panel).single { it.text == CutoutUi(activity).label("bms.pack.back") }.performClick()
            assertFalse(checkNotNull(panel.findViewWithTag<View>("pack.mode.overview")).isSelected)
            assertTrue(checkNotNull(panel.findViewWithTag<View>("pack.mode.cell-map")).isSelected)

            buttons(panel).single { it.text == activity.getString(R.string.pack_cell_map) }.performClick()
            checkNotNull(panel.findViewWithTag<View>("pack.group.1")).performClick()
            buttons(panel).single { it.text == CutoutUi(activity).label("bms.pack.back") }.performClick()
            assertFalse(checkNotNull(panel.findViewWithTag<View>("pack.mode.overview")).isSelected)
            assertTrue(checkNotNull(panel.findViewWithTag<View>("pack.mode.cell-map")).isSelected)
        } finally {
            host.pause().stop().destroy()
        }
    }

    private fun partialSnapshot(): MobileBmsSnapshotDto {
        fun reading(millivolts: Int) = VoltageReading(
            Voltage(millivolts), MobileValueSourceDto.REPORTED,
            MobileValueQualityDto.KNOWN, MobileVerificationStatusDto.SOURCE_VERIFIED,
        )
        fun group(index: Int, millivolts: Int) = MobileBmsGroupSnapshotDto(
            index.toUShort(), 1u, index.toUShort(), null, reading(millivolts), Voltage(millivolts),
            emptyList(), emptyList(), null, null, false, MobileBmsAlertLevelDto.NOMINAL, null,
        )
        return MobileBmsSnapshotDto(
            2u, 2u, MobileReadbackAvailabilityDto.AVAILABLE,
            MobileBmsTopologyDto("2S verified pack", 2u, 1u, 1u, 1u, MobileBmsTopologyConfidenceDto.VERIFIED),
            null, null, null, null, null, reading(48_000), null, null, null,
            VoltageDeltaReading(
                VoltageDelta(12), MobileValueSourceDto.CALCULATED,
                MobileValueQualityDto.KNOWN, MobileVerificationStatusDto.SOURCE_VERIFIED,
            ),
            1u, null, emptyList(), null, "Balancing is active", null,
            "Fault summary must not be repeated", null,
            listOf(group(1, 48_000), group(2, 48_012)), emptyList(),
            listOf(MobileBmsFaultDto("temperature", "Cell temperature high", MobileBmsAlertLevelDto.WARNING)),
            null, null,
        )
    }

    private fun texts(root: View): List<String> = descendants(root)
        .filterIsInstance<TextView>().map { it.text.toString() }.toList()

    private fun buttons(root: View): List<Button> = descendants(root).filterIsInstance<Button>().toList()

    private fun renderedTopBorderPixel(view: View, activity: Activity): Int {
        val width = maxOf(view.width, CutoutUi(activity).dp(120))
        view.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED),
        )
        val height = view.measuredHeight
        view.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        return try {
            view.draw(Canvas(bitmap))
            bitmap.getPixel(width / 2, 0)
        } finally {
            bitmap.recycle()
        }
    }

    private fun descendants(root: View): Sequence<View> = sequence {
        yield(root)
        if (root is ViewGroup) for (index in 0 until root.childCount) yieldAll(descendants(root.getChildAt(index)))
    }
}
