package lol.cutout.local

import android.content.Context
import android.content.res.Configuration
import android.graphics.drawable.RippleDrawable
import android.view.View
import android.widget.ImageButton
import android.widget.LinearLayout
import androidx.test.core.app.ApplicationProvider
import lol.cutout.ChargingDisplayModeSelector
import lol.cutout.ChargingToolbar
import lol.cutout.CutoutUi
import lol.cutout.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ChargingDisplayModeSelectorTest {
    @Test
    fun mapAndListAreOneAccessibleCapsuleSegmentWithFortyFourDpTargets() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val ui = CutoutUi(context)
        val selector = ChargingDisplayModeSelector(context, "Map", "List") { }
        selector.measure(
            View.MeasureSpec.makeMeasureSpec(ui.dp(160), View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(ui.dp(48), View.MeasureSpec.EXACTLY),
        )
        selector.layout(0, 0, selector.measuredWidth, selector.measuredHeight)

        assertEquals(2, selector.childCount)
        assertTrue(selector.mapButton.isSelected)
        assertFalse(selector.listButton.isSelected)
        assertTrue(selector.mapButton.background is RippleDrawable)
        assertTrue(selector.listButton.background is RippleDrawable)
        assertEquals(ui.dp(44), selector.mapButton.height)
        assertEquals(ui.dp(44), selector.listButton.height)
        assertEquals("Map, ${context.getString(R.string.charging_selected)}", selector.mapButton.contentDescription)
        assertEquals("List", selector.listButton.contentDescription)
        assertEquals(View.IMPORTANT_FOR_ACCESSIBILITY_YES, selector.mapButton.importantForAccessibility)

        selector.selectMap(false)
        assertFalse(selector.mapButton.isSelected)
        assertTrue(selector.listButton.isSelected)
        assertEquals("List, ${context.getString(R.string.charging_selected)}", selector.listButton.contentDescription)
        assertNotNull(selector.background)
    }

    @Test
    fun toolbarFits320DpAndAccessibleFontKeepsLabelsAndTargetsVisible() {
        val base = ApplicationProvider.getApplicationContext<Context>()
        val regular = createToolbar(base, CutoutUi(base))
        val ui = CutoutUi(base)
        val contentWidth = ui.dp(320 - 36)
        regular.measure(
            View.MeasureSpec.makeMeasureSpec(contentWidth, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(ui.dp(56), View.MeasureSpec.AT_MOST),
        )
        regular.layout(0, 0, regular.measuredWidth, regular.measuredHeight)
        assertTrue(regular.width <= contentWidth)
        val selector = regular.getChildAt(0) as ChargingDisplayModeSelector
        assertTrue(selector.mapButton.width >= ui.dp(64))
        assertTrue(selector.listButton.width >= ui.dp(64))
        assertEquals(1, selector.mapButton.lineCount)
        assertEquals(1, selector.listButton.lineCount)

        val largeConfig = Configuration(base.resources.configuration).apply { fontScale = 1.5f }
        val largeContext = base.createConfigurationContext(largeConfig)
        val largeUi = CutoutUi(largeContext)
        val stacked = createToolbar(largeContext, largeUi)
        val largeWidth = largeUi.dp(320 - 36)
        stacked.measure(
            View.MeasureSpec.makeMeasureSpec(largeWidth, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(largeUi.dp(140), View.MeasureSpec.AT_MOST),
        )
        stacked.layout(0, 0, stacked.measuredWidth, stacked.measuredHeight)
        val largeSelector = stacked.getChildAt(0) as ChargingDisplayModeSelector
        assertEquals(1, largeSelector.mapButton.lineCount)
        assertEquals(1, largeSelector.listButton.lineCount)
        assertTrue(largeSelector.mapButton.height >= largeUi.dp(44))
        val actions = stacked.getChildAt(1) as LinearLayout
        for (index in 0 until actions.childCount) {
            val button = actions.getChildAt(index)
            if (button is ImageButton) {
                assertTrue("Charging action needs a 44dp target", button.width >= largeUi.dp(44))
                assertTrue("Charging actions must fit at narrow width", button.right <= actions.width)
            }
        }
    }

    private fun createToolbar(context: Context, ui: CutoutUi): ChargingToolbar {
        val selector = ChargingDisplayModeSelector(context, "Map", "List") { }
        val filter = lol.cutout.chargingToolbarIconButton(
            context, ui, lol.cutout.ChargingToolbarGlyph.FILTER, "Filters", "charging_filter", {},
        )
        val add = lol.cutout.chargingToolbarIconButton(
            context, ui, lol.cutout.ChargingToolbarGlyph.PLUS, "Add spot", "charging_add", {},
        )
        val more = lol.cutout.chargingToolbarIconButton(
            context, ui, lol.cutout.ChargingToolbarGlyph.MORE, "More actions", "charging_more_actions", {},
        )
        return ChargingToolbar(context, selector, filter, add, more)
    }
}
