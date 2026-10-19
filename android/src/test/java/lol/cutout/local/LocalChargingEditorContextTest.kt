package lol.cutout.local

import android.app.Activity
import android.view.View
import lol.cutout.ChargingEditorContextResolver
import lol.cutout.ChargingEditorCallbackFence
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
import uniffi.cutout_mobile_ffi.MobileChargingConnectorDto
import uniffi.cutout_mobile_ffi.MobileMapCoordinateDto
import uniffi.cutout_mobile_ffi.mobileChargingPowerSuggestion
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalChargingEditorContextTest {
    @Test
    fun editorSaveReceiptIsIgnoredAfterReplacementOrPanelDisposal() {
        val fence = ChargingEditorCallbackFence()
        val firstEditor = fence.capture()
        assertTrue(fence.accepts(firstEditor))

        fence.invalidate()
        assertFalse("a retired editor must not apply its late save result", fence.accepts(firstEditor))

        val replacementEditor = fence.capture()
        assertTrue(fence.accepts(replacementEditor))
        fence.dispose()
        assertFalse("a disposed panel must reject the replacement editor's late result", fence.accepts(replacementEditor))
    }

    @Test
    fun freshLocationFeedsCountryPowerSuggestionAndRustOrderedConnectorPicker() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val location = MobileMapCoordinateDto(40.0, -105.0)
        var geocodedCoordinate: Pair<Double, Double>? = null
        var databaseCoordinate: MobileMapCoordinateDto? = null
        var databaseCountry: String? = null
        val loaded = CountDownLatch(1)
        val resolver = ChargingEditorContextResolver(
            reverseGeocode = { latitude, longitude ->
                geocodedCoordinate = latitude to longitude
                "US"
            },
            loadChoices = { coordinate, country, completed ->
                databaseCoordinate = coordinate
                databaseCountry = country
                completed(Result.success(listOf(MobileChargingConnectorDto.TypeG, MobileChargingConnectorDto.Type2)))
                loaded.countDown()
            },
        )
        val panel = ChargingSpotEditorPanel(
            context = activity,
            spot = null,
            initialCoordinate = location,
            onCancel = {},
            onSave = {},
            initialLocation = location,
            contextResolver = resolver,
        )
        activity.setContentView(panel)
        checkNotNull(panel.findViewWithTag<android.widget.Button>("charging_editor.add_supply")).performClick()
        val voltageDraft = checkNotNull(panel.findViewWithTag<android.widget.EditText>("charging_editor.voltage"))
        try {
            assertTrue("regional context did not resolve", loaded.await(3, TimeUnit.SECONDS))
            shadowOf(activity.mainLooper).idle()
            assertEquals(location.latitudeDegrees to location.longitudeDegrees, geocodedCoordinate)
            assertEquals(location, databaseCoordinate)
            assertEquals("US", databaseCountry)
            val connectorButton = checkNotNull(panel.findViewWithTag<android.widget.Button>("charging_editor.connector"))
            val defaultLabel = CutoutUi(activity).let { lol.cutout.ChargingConnectorArtwork.label(it, MobileChargingConnectorDto.Nema515) }
            assertTrue("Rust's national default remains an unconfirmed suggestion", connectorButton.text.contains(defaultLabel))

            voltageDraft.setText("240")
            assertEquals("240", voltageDraft.text.toString())

            checkNotNull(panel.findViewWithTag<View>("charging_editor.suggest_power")).performClick()
            val dialog = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            val suggestion = mobileChargingPowerSuggestion("US", false)
            assertEquals("US power defaults come from the Rust projection", 120.toUShort(), suggestion.voltageVolts)
            val message = dialog.findViewById<android.widget.TextView>(android.R.id.message).text.toString()
            assertTrue("country context did not reach the Rust power suggestion", message.contains("120 V"))
            dialog.dismiss()

            connectorButton.performClick()
            val connectorDialog = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            assertEquals(
                "nearby Rust order should lead the selectable list",
                lol.cutout.ChargingConnectorArtwork.label(CutoutUi(activity), MobileChargingConnectorDto.TypeG),
                connectorDialog.listView.adapter.getItem(0).toString(),
            )
        } finally {
            panel.dispose()
            resolver.close()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun noFreshLocationDoesNotGeocodeOrInventCountryFromDeviceLocale() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        var geocoderCalls = 0
        var databaseCalls = 0
        var country: String? = "uninitialized"
        val resolver = ChargingEditorContextResolver(
            reverseGeocode = { _, _ -> geocoderCalls++; "US" },
            loadChoices = { _, _, completed -> databaseCalls++; completed(Result.success(emptyList())) },
        )
        val request = resolver.resolve(null) { result -> country = result.countryCode }
        try {
            shadowOf(activity.mainLooper).idle()
            assertEquals(null, country)
            assertEquals(0, geocoderCalls)
            assertEquals(0, databaseCalls)
        } finally {
            request.close()
            resolver.close()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun cancelledEditorContextCannotApplyAfterTheEditorIsDisposed() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val geocoderEntered = CountDownLatch(1)
        val geocoderFinished = CountDownLatch(1)
        val releaseGeocoder = CountDownLatch(1)
        val choicesLoaded = CountDownLatch(1)
        val resolver = ChargingEditorContextResolver(
            reverseGeocode = { _, _ ->
                geocoderEntered.countDown()
                releaseGeocoder.await(2, TimeUnit.SECONDS)
                geocoderFinished.countDown()
                "US"
            },
            loadChoices = { _, _, completed ->
                completed(Result.success(listOf(MobileChargingConnectorDto.TypeG)))
                choicesLoaded.countDown()
            },
        )
        var applied = false
        val request = resolver.resolve(MobileMapCoordinateDto(40.0, -105.0)) { applied = true }
        try {
            assertTrue(geocoderEntered.await(3, TimeUnit.SECONDS))
            request.close()
            releaseGeocoder.countDown()
            assertTrue(geocoderFinished.await(3, TimeUnit.SECONDS))
            shadowOf(activity.mainLooper).idle()
            assertFalse("cancelled editor context was applied", applied)
            assertEquals(1L, choicesLoaded.count)
        } finally {
            releaseGeocoder.countDown()
            resolver.close()
            host.pause().stop().destroy()
        }
    }
}
