package lol.cutout.local

import android.app.Activity
import android.content.DialogInterface
import android.content.res.Configuration
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import lol.cutout.ChargingConnectorArtwork
import lol.cutout.ChargingEditorContextResolver
import lol.cutout.ChargingSpotEditorPanel
import lol.cutout.CutoutUi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalChargingEditorParityTest {
    @Test
    fun createRequiresAnExplicitKindAndNonblankNoteAndMapsAllThreeKindsToRustFields() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        try {
            val cases = listOf(
                Triple("charging", MobileChargingCostDto.FREE, null),
                Triple("food", MobileChargingCostDto.PAID, null),
                Triple("ev", MobileChargingCostDto.FREE, MobileChargingVenueDto.EV_CHARGING_STATION),
            )
            cases.forEach { (kind, expectedCost, expectedVenue) ->
                var saved: MobileChargingInputDto? = null
                val panel = editor(activity) { saved = it }
                activity.setContentView(panel)
                val save = checkNotNull(panel.findViewWithTag<Button>("charging.editor.save"))
                assertFalse("A new spot starts without a selected type", save.isEnabled)
                listOf("charging", "food", "ev").forEach { choice ->
                    assertFalse("New type choice $choice must start unselected",
                        checkNotNull(panel.findViewWithTag<View>("charging.editor.kind.$choice")).isSelected)
                }
                val details = checkNotNull(panel.findViewWithTag<View>("charging.editor.details-content"))
                assertEquals("New spot Details start collapsed", View.GONE, details.visibility)

                checkNotNull(panel.findViewWithTag<View>("charging.editor.kind.$kind")).performClick()
                val venue = checkNotNull(panel.findViewWithTag<View>("charging.editor.venue"))
                val evPaid = checkNotNull(panel.findViewWithTag<View>("charging.editor.ev-paid"))
                when (kind) {
                    "food" -> {
                        assertEquals(View.VISIBLE, venue.visibility)
                        assertEquals(View.GONE, evPaid.visibility)
                    }
                    "ev" -> {
                        assertEquals(View.GONE, venue.visibility)
                        assertEquals(View.VISIBLE, evPaid.visibility)
                    }
                    else -> {
                        assertEquals(View.GONE, venue.visibility)
                        assertEquals(View.GONE, evPaid.visibility)
                    }
                }
                assertFalse("The access note is still required after choosing a type", save.isEnabled)
                checkNotNull(panel.findViewWithTag<EditText>("charging.editor.access-note")).setText("Public outlet by the trail entrance")
                assertTrue("The optional place type or EV paid toggle must not gate Save", save.isEnabled)
                save.performClick()
                val input = checkNotNull(saved)
                assertEquals(expectedCost, input.cost)
                assertEquals(expectedVenue, input.venue)
                assertEquals("Public outlet by the trail entrance", input.accessNote)
            }

            val foodPanel = editor(activity) {}
            activity.setContentView(foodPanel)
            checkNotNull(foodPanel.findViewWithTag<View>("charging.editor.kind.food")).performClick()
            checkNotNull(foodPanel.findViewWithTag<EditText>("charging.editor.access-note")).setText("Cafe lobby")
            val placeType = checkNotNull(foodPanel.findViewWithTag<Button>("charging.editor.venue"))
            assertTrue("The optional place-type selector stays available", placeType.isClickable)
            assertEquals("Food place type remains optional", View.VISIBLE, placeType.visibility)
        } finally {
            host.pause().stop().destroy()
        }
    }

    @Test
    fun optionalPriceErrorsAreVisibleAndTermsRemainIndependentFromPrice() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        try {
            var saved: MobileChargingInputDto? = null
            val panel = editor(activity) { saved = it }
            activity.setContentView(panel)
            checkNotNull(panel.findViewWithTag<View>("charging.editor.kind.food")).performClick()
            checkNotNull(panel.findViewWithTag<EditText>("charging.editor.access-note")).setText("Ask before entering")
            checkNotNull(panel.findViewWithTag<Button>("charging.editor.add-details")).performClick()
            val amount = checkNotNull(panel.findViewWithTag<EditText>("charging.editor.price.amount"))
            amount.setText("2.50")
            val save = checkNotNull(panel.findViewWithTag<Button>("charging.editor.save"))
            assertFalse("Partial optional price metadata cannot be submitted", save.isEnabled)
            assertEquals(View.VISIBLE, panel.findViewWithTag<View>("charging.editor.validation-error").visibility)

            amount.setText("")
            checkNotNull(panel.findViewWithTag<EditText>("charging.editor.price.currency")).setText("")
            assertTrue("Food terms may be recorded without inventing a price", save.isEnabled)
            save.performClick()
            assertEquals(MobileChargingCostDto.PAID, checkNotNull(saved).cost)
            assertEquals(null, checkNotNull(saved).price)
        } finally {
            host.pause().stop().destroy()
        }
    }

    @Test
    fun unrelatedEditPreservesOriginalPriceMinorUnitsExactly() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        try {
            val coordinate = MobileMapCoordinateDto(40.0, -105.0)
            val originalPrice = MobileChargingPriceDto(12_345UL, "USD", 3.toUByte(), MobileChargingPriceBasisDto.PER_HOUR)
            val original = MobileChargingInputDto(
                coordinate,
                "Entry by the side door",
                MobileChargingCostDto.PAID,
                "Trail Cafe",
                MobileChargingVenueDto.CAFE,
                null,
                MobileChargingTermsDto.DIRECT_FEE,
                originalPrice,
                emptyList(),
            )
            var saved: MobileChargingInputDto? = null
            val panel = ChargingSpotEditorPanel(
                context = activity,
                spot = null,
                initialCoordinate = coordinate,
                onCancel = {},
                onSave = { saved = it },
                initialInput = original,
            )
            activity.setContentView(panel)
            checkNotNull(panel.findViewWithTag<EditText>("charging_editor.name")).setText("Updated cafe name")
            checkNotNull(panel.findViewWithTag<Button>("charging.editor.save")).performClick()
            assertEquals("USD's original nonstandard three-decimal amount must round-trip unchanged", originalPrice, checkNotNull(saved).price)
        } finally {
            host.pause().stop().destroy()
        }
    }

    @Test
    fun acceptedSaveDisablesEditorAndPreventsDuplicateSaveOrCancel() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        try {
            var saveCount = 0
            var cancelCount = 0
            val panel = ChargingSpotEditorPanel(
                context = activity,
                spot = null,
                initialCoordinate = MobileMapCoordinateDto(40.0, -105.0),
                onCancel = { cancelCount += 1 },
                onSave = { saveCount += 1 },
            )
            activity.setContentView(panel)
            checkNotNull(panel.findViewWithTag<View>("charging.editor.kind.charging")).performClick()
            val note = checkNotNull(panel.findViewWithTag<EditText>("charging.editor.access-note"))
            note.setText("Outlet by the entrance")
            note.requestFocus()
            val save = checkNotNull(panel.findViewWithTag<Button>("charging.editor.save"))
            val cancel = checkNotNull(panel.findViewWithTag<Button>("charging.editor.cancel"))

            save.performClick()

            assertEquals(1, saveCount)
            assertTrue(panel.isSaving)
            assertFalse("Accepted save retires text editing before waiting for storage", note.isFocused)
            assertFalse("Save is inert until its async owner reports completion", save.isEnabled)
            assertTrue("Disabled Save has a visible pending-state treatment", save.alpha < 1f)
            assertFalse("Cancel is inert while save is pending", cancel.isEnabled)
            save.performClick()
            cancel.performClick()
            assertEquals("A pending save must not submit twice", 1, saveCount)
            assertEquals("A pending save must not cancel", 0, cancelCount)

            panel.setSaving(false)
            assertFalse(panel.isSaving)
            assertTrue("The editor can be retried after the save owner reports failure", cancel.isEnabled)
        } finally {
            host.pause().stop().destroy()
        }
    }

    @Test
    fun evKindDropsConfirmedDomesticSuggestionButKeepsEditorSaveAvailable() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val coordinate = MobileMapCoordinateDto(40.0, -105.0)
        val choicesLoaded = CountDownLatch(1)
        val resolver = ChargingEditorContextResolver(
            reverseGeocode = { _, _ -> "US" },
            loadChoices = { _, _, completed ->
                completed(Result.success(emptyList()))
                choicesLoaded.countDown()
            },
        )
        try {
            var saved: MobileChargingInputDto? = null
            val panel = ChargingSpotEditorPanel(
                context = activity,
                spot = null,
                initialCoordinate = coordinate,
                onCancel = {},
                onSave = { saved = it },
                initialLocation = coordinate,
                contextResolver = resolver,
            )
            activity.setContentView(panel)
            assertTrue("regional power choice did not resolve", choicesLoaded.await(3, TimeUnit.SECONDS))
            shadowOf(activity.mainLooper).idle()
            checkNotNull(panel.findViewWithTag<View>("charging.editor.kind.charging")).performClick()
            checkNotNull(panel.findViewWithTag<EditText>("charging.editor.access-note")).setText("Outlet by the trail entrance")
            checkNotNull(panel.findViewWithTag<Button>("charging.editor.add-details")).performClick()

            val suggestion = mobileChargingPowerSuggestion("US", false)
            checkNotNull(panel.findViewWithTag<Button>("charging_editor.suggest_power")).performClick()
            val confirm = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            assertTrue(confirm.findViewById<android.widget.TextView>(android.R.id.message).text.contains("${suggestion.voltageVolts} V"))
            confirm.getButton(DialogInterface.BUTTON_POSITIVE).performClick()
            shadowOf(activity.mainLooper).idle()
            val supplies = checkNotNull(panel.findViewWithTag<LinearLayout>("charging.editor.supplies"))
            assertEquals("The accepted domestic suggestion is represented by a power row", 1, supplies.childCount)

            checkNotNull(panel.findViewWithTag<View>("charging.editor.kind.ev")).performClick()
            assertEquals("Switching to EV invalidates the domestic suggested row", 0, supplies.childCount)
            val save = checkNotNull(panel.findViewWithTag<Button>("charging.editor.save"))
            assertTrue(save.isEnabled)
            save.performClick()
            assertTrue("EV save must not retain domestic suggested power", checkNotNull(saved).supplies.isEmpty())
        } finally {
            resolver.close()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun suggestedOutletDefaultsAreNotPersistedUntilTheRiderEditsThem() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        try {
            var saved: MobileChargingInputDto? = null
            val panel = editor(activity) { saved = it }
            activity.setContentView(panel)
            checkNotNull(panel.findViewWithTag<View>("charging.editor.kind.charging")).performClick()
            checkNotNull(panel.findViewWithTag<EditText>("charging.editor.access-note")).setText("Outlet by the east door")
            checkNotNull(panel.findViewWithTag<Button>("charging.editor.add-details")).performClick()
            val plug = checkNotNull(panel.findViewWithTag<Button>("charging.editor.plug-type"))
            val count = checkNotNull(panel.findViewWithTag<Button>("charging.editor.outlet-count"))
            assertTrue("Rust's first connector is presented as a suggestion", plug.text.isNotBlank())
            assertTrue("The default outlet count is presented", count.text.endsWith("1"))
            val save = checkNotNull(panel.findViewWithTag<Button>("charging.editor.save"))
            assertTrue(save.isEnabled)
            save.performClick()
            assertTrue("Untouched defaults must not become asserted supply metadata", checkNotNull(saved).supplies.isEmpty())
            panel.setSaving(false) // The no-op owner reports that the editor may continue.

            count.performClick()
            val dialog = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            val list = dialog.listView
            list.performItemClick(list.adapter.getView(2, null, list), 2, list.adapter.getItemId(2))
            assertTrue(save.isEnabled)
            saved = null
            save.performClick()
            assertEquals(3.toUShort(), checkNotNull(saved).supplies.single().outletCount)
        } finally {
            host.pause().stop().destroy()
        }
    }

    @Test
    fun detailsDisclosurePreservesDraftAndExistingMetadataAndRegionalOverride() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val coordinate = MobileMapCoordinateDto(40.0, -105.0)
        val choices = mutableListOf<MobileChargingConnectorDto>()
        val resolver = ChargingEditorContextResolver(
            reverseGeocode = { _, _ -> "US" },
            loadChoices = { _, _, completed ->
                completed(Result.success(choices.ifEmpty { listOf(MobileChargingConnectorDto.TypeG, MobileChargingConnectorDto.Type2) }))
            },
        )
        try {
            var saved: MobileChargingInputDto? = null
            val input = MobileChargingInputDto(
                coordinate,
                "Door code at the east entrance",
                MobileChargingCostDto.PAID,
                "Riverside Cafe",
                MobileChargingVenueDto.CAFE,
                "Open all day",
                MobileChargingTermsDto.PURCHASE_REQUIRED,
                null,
                listOf(MobileChargingSupplyDto(120.toUShort(), listOf(MobileChargingConnectorDto.Type2), 2.toUShort())),
            )
            val panel = ChargingSpotEditorPanel(
                context = activity,
                spot = null,
                initialCoordinate = coordinate,
                onCancel = {},
                onSave = { saved = it },
                initialInput = input,
                initialLocation = coordinate,
                contextResolver = resolver,
            )
            activity.setContentView(panel)
            val details = checkNotNull(panel.findViewWithTag<View>("charging.editor.details-content"))
            assertEquals("Swift opens Details for an existing draft", View.VISIBLE, details.visibility)
            assertTrue(checkNotNull(panel.findViewWithTag<View>("charging.editor.kind.food")).isSelected)

            val name = checkNotNull(panel.findViewWithTag<EditText>("charging_editor.name"))
            name.setText("Changed name")
            val toggle = checkNotNull(panel.findViewWithTag<Button>("charging.editor.add-details"))
            toggle.performClick()
            assertEquals(View.GONE, details.visibility)
            toggle.performClick()
            assertEquals(View.VISIBLE, details.visibility)
            assertEquals("Changed name", name.text.toString())

            val connector = checkNotNull(panel.findViewWithTag<Button>("charging_editor.connector"))
            val selectedConnectorLabel = ChargingConnectorArtwork.label(CutoutUi(activity), MobileChargingConnectorDto.Type2)
            assertTrue(connector.text.toString().contains(selectedConnectorLabel))
            panel.updateFreshLocation(null)
            panel.updateFreshLocation(MobileMapCoordinateDto(39.9, -105.0))
            shadowOf(activity.mainLooper).idle()
            assertTrue("A context refresh may update suggestions without replacing a manual connector edit",
                connector.text.toString().contains(selectedConnectorLabel))

            val save = checkNotNull(panel.findViewWithTag<Button>("charging.editor.save"))
            assertTrue(save.isEnabled)
            save.performClick()
            val result = checkNotNull(saved)
            assertEquals("Changed name", result.name)
            assertEquals("Door code at the east entrance", result.accessNote)
            assertEquals("Open all day", result.hours)
            assertEquals(MobileChargingTermsDto.PURCHASE_REQUIRED, result.terms)
            assertEquals(MobileChargingVenueDto.CAFE, result.venue)
            assertEquals(input.supplies, result.supplies)
        } finally {
            resolver.close()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun normalAndAccessibilityTextLayoutsKeepChoicesAndActionsVisibleAndReachable() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        try {
            for (fontScale in listOf(1f, 1.5f)) {
                val context = activity.createConfigurationContext(
                    Configuration(activity.resources.configuration).apply { this.fontScale = fontScale },
                )
                val panel = editor(context) {}
                activity.setContentView(panel)
                val width = (393 * context.resources.displayMetrics.density).toInt()
                val height = (852 * context.resources.displayMetrics.density).toInt()
                panel.measure(
                    View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.AT_MOST),
                )
                panel.layout(0, 0, width, panel.measuredHeight)
                shadowOf(activity.mainLooper).idle()
                NativeSurfaceCapture.save(panel, if (fontScale > 1f) "charging-editor-accessibility" else "charging-editor-default")

                val choices = checkNotNull(panel.findViewWithTag<LinearLayout>("charging.editor.kind-choices"))
                assertEquals("Large text uses a full-width vertical choice stack", if (fontScale > 1f) LinearLayout.VERTICAL else LinearLayout.HORIZONTAL, choices.orientation)
                val buttons = listOf("charging", "food", "ev").map { kind ->
                    checkNotNull(panel.findViewWithTag<View>("charging.editor.kind.$kind"))
                }
                buttons.forEach { button ->
                    assertTrue("Type choice keeps at least a 44dp target", button.measuredHeight >= (44 * context.resources.displayMetrics.density).toInt())
                    assertTrue("Type choice has a visible accessible label", !button.contentDescription.isNullOrBlank())
                    assertTrue("Type choice fits the editor width", button.left >= 0 && button.right <= width)
                }
                val note = checkNotNull(panel.findViewWithTag<EditText>("charging.editor.access-note"))
                assertTrue("Required note remains a large multiline target", note.measuredHeight >= (72 * context.resources.displayMetrics.density).toInt())
                val save = checkNotNull(panel.findViewWithTag<Button>("charging.editor.save"))
                assertTrue("Save label stays laid out at text scale $fontScale", save.lineCount > 0)
                assertTrue("Save retains a native 44dp target", save.measuredHeight >= (44 * context.resources.displayMetrics.density).toInt())
                checkNotNull(panel.findViewWithTag<View>("charging.editor.add-details")).performClick()
                panel.measure(
                    View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
                )
                panel.layout(0, 0, width, height)
                val scroll = checkNotNull(panel.findViewWithTag<ScrollView>("charging.editor.scroll"))
                val beforeScroll = IntArray(2).also(save::getLocationOnScreen)
                scroll.scrollTo(0, scroll.getChildAt(0).height)
                shadowOf(activity.mainLooper).idle()
                val afterScroll = IntArray(2).also(save::getLocationOnScreen)
                assertTrue("Expanded details scroll independently of the navigation actions", scroll.scrollY > 0)
                assertTrue("Save remains fixed while optional details scroll", beforeScroll.contentEquals(afterScroll))
                assertTrue("Save remains visible above the scrolling form", save.isShown && save.bottom <= scroll.top)
            }
        } finally {
            host.pause().stop().destroy()
        }
    }

    private fun editor(context: android.content.Context, onSave: (MobileChargingInputDto) -> Unit) =
        ChargingSpotEditorPanel(
            context = context,
            spot = null,
            initialCoordinate = MobileMapCoordinateDto(40.0, -105.0),
            onCancel = {},
            onSave = onSave,
        )

}
