package lol.cutout.local

import android.app.Activity
import android.view.View
import android.widget.TextView
import lol.cutout.ffitest.DevicePickerPanelTest
import lol.cutout.BlePresentation
import lol.cutout.CutoutUi
import lol.cutout.DevicePickerPanel
import lol.cutout.NearbyPev
import lol.cutout.wireBytes
import java.util.UUID
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.shadows.ShadowAlertDialog
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.DiscoveryServiceUuid
import uniffi.cutout_mobile_ffi.mobileDiscoveryCandidateFromAdvertisement

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalDevicePickerPanelTest : DevicePickerPanelTest() {
    @Test
    fun detailsDialogUsesSwiftDetectionFieldsWithoutAddingSignalTelemetry() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val id = "picker-detail-device"
        val candidate = mobileDiscoveryCandidateFromAdvertisement(
            id,
            "NF2557",
            listOf(DiscoveryServiceUuid(UUID.fromString("6e400001-b5a3-f393-e0a9-e50e24dcca9e").wireBytes())),
        )
        val panel = DevicePickerPanel(activity, {}, {}, {}, {})

        try {
            activity.setContentView(panel)
            panel.render(
                BlePresentation(devices = listOf(NearbyPev(
                    id = id,
                    name = candidate.displayName,
                    rssi = -54,
                    suggested = candidate.isPickerCandidate,
                    candidate = candidate,
                    advertisedName = "Nearby board",
                ))),
                permitted = true,
                enabled = true,
                hasScanned = true,
            )
            checkNotNull(panel.findViewWithTag<View>("picker_details:$id")).performClick()

            val dialog = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            val message = checkNotNull(dialog.findViewById<TextView>(android.R.id.message)).text.toString()
            assertTrue(message.contains("Nearby board"))
            assertTrue(message.contains(id))
            assertTrue(message.contains(activity.getString(lol.cutout.R.string.picker_device_identifier)))
            assertTrue(message.contains(CutoutUi(activity).label("picker.detection_details")))
            assertTrue(message.contains(candidate.productCategory + " - " + candidate.evidence))
            assertTrue(message.contains(candidate.detail))
            assertFalse("Signal strength is absent from the current Swift details form", message.contains("Signal"))
            assertFalse(message.contains("-54 dBm"))
            dialog.dismiss()
        } finally {
            panel.removeAllViews()
            host.pause().stop().destroy()
        }
    }
}
