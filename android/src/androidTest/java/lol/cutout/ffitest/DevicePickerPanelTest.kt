package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.ProgressBar
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import lol.cutout.BlePresentation
import lol.cutout.DevicePickerPanel
import lol.cutout.NativePhase
import lol.cutout.NearbyPev
import lol.cutout.wireBytes
import org.junit.Test
import uniffi.cutout_mobile_ffi.DiscoveryCandidate
import uniffi.cutout_mobile_ffi.DiscoveryServiceUuid
import uniffi.cutout_mobile_ffi.MobileConnectionReadinessDto
import uniffi.cutout_mobile_ffi.mobileDiscoveryCandidateFromAdvertisement

open class DevicePickerPanelTest {
    @Test
    fun recoveryActionsRequestPermissionAndOpenSettingsSeparately() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        var scans = 0
        var settings = 0
        var configuration = 0
        lateinit var panel: DevicePickerPanel
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            panel = DevicePickerPanel(context, { scans++ }, { settings++ }, { configuration++ }, {})
            panel.render(BlePresentation(scanning = true), permitted = false, enabled = false, hasScanned = false)
            check(tagged<ProgressBar>(panel, "picker_spinner").visibility == View.GONE) {
                "Permission recovery must take priority over a stale scan spinner"
            }

            check(tagged<Button>(panel, "picker_recovery_primary").performClick())
            check(scans == 1 && settings == 0) { "Permission action did not request Nearby access" }
            check(tagged<Button>(panel, "picker_recovery_settings").performClick())
            check(settings == 1) { "Secondary recovery action did not open app settings" }
            check(tagged<View>(panel, "picker_setup").performClick())
            check(configuration == 1) { "Setup gear did not open configuration" }

            panel.render(BlePresentation(scanning = true), permitted = true, enabled = false, hasScanned = false)
            check(tagged<ProgressBar>(panel, "picker_spinner").visibility == View.GONE) {
                "Bluetooth-off recovery must take priority over a stale scan spinner"
            }
        }
    }

    @Test
    fun emptyRecoveryAppearsOnlyAfterScanCompletesAndErrorsRemainActionable() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        var scans = 0
        lateinit var panel: DevicePickerPanel
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            panel = DevicePickerPanel(context, { scans++ }, {}, {}, {})

            // The host marks hasScanned when a scan starts. Do not show a
            // retry action until that active scan has actually returned empty.
            panel.render(
                BlePresentation(scanning = true),
                permitted = true,
                enabled = true,
                hasScanned = true,
            )
            val recovery = tagged<View>(panel, "picker_recovery")
            check(recovery.visibility == View.GONE) {
                "Empty recovery was shown while scanning; its action would stop the current scan"
            }

            panel.render(
                BlePresentation(scanning = false),
                permitted = true,
                enabled = true,
                hasScanned = true,
            )
            check(recovery.visibility == View.VISIBLE) {
                "Completed empty scan did not expose the retry action"
            }
            check(tagged<Button>(panel, "picker_recovery_primary").performClick())
            check(scans == 1) { "Completed-empty retry did not request another scan" }

            // Scan failures retain their dedicated recovery path.
            panel.render(
                BlePresentation(scanning = false, problem = true),
                permitted = true,
                enabled = true,
                hasScanned = true,
            )
            check(recovery.visibility == View.VISIBLE) {
                "Scan error recovery disappeared after the empty-state change"
            }
            check(context.getString(lol.cutout.R.string.picker_try_again) ==
                tagged<Button>(panel, "picker_recovery_primary").text.toString()) {
                "Scan error no longer offers its normal retry action"
            }
        }
    }

    @Test
    fun noiseIsFilteredAndRowsSurviveSignalUpdates() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val nordic = candidate("nordic-device", "VESC", "6e400001-b5a3-f393-e0a9-e50e24dcca9e")
        val ffe0 = candidate("ffe0-device", "Wheel", "0000ffe0-0000-1000-8000-00805f9b34fb")
        val noise = candidate("noise-device", "Fitness band", "0000180f-0000-1000-8000-00805f9b34fb")
        val first = BlePresentation(devices = listOf(
            nearby(nordic, -54), nearby(ffe0, -61), nearby(noise, -37),
        ))
        lateinit var panel: DevicePickerPanel
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            panel = DevicePickerPanel(context, {}, {}, {}, {})
            panel.render(first, permitted = true, enabled = true, hasScanned = true)
            val nordicRow = tagged<View>(panel, "picker_row:nordic-device")
            val ffe0Row = tagged<View>(panel, "picker_row:ffe0-device")
            check(findTag(panel, "picker_row:noise-device") == null) { "Unrelated Bluetooth noise appeared in the picker" }

            panel.render(first.copy(devices = listOf(
                nearby(nordic, -81), nearby(ffe0, -42), nearby(noise, -26),
            )), permitted = true, enabled = true, hasScanned = true)

            check(tagged<View>(panel, "picker_row:nordic-device") === nordicRow) { "RSSI update replaced the Nordic row" }
            check(tagged<View>(panel, "picker_row:ffe0-device") === ffe0Row) { "RSSI update replaced the FFE0 row" }
            check(findTag(panel, "picker_row:noise-device") == null) { "RSSI update exposed unrelated Bluetooth noise" }
        }
    }

    @Test
    fun pickerCardShowsOnlySwiftSummaryAndAction() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val candidate = candidate("device-internal-id", "VESC", "6e400001-b5a3-f393-e0a9-e50e24dcca9e")
        val nearby = nearby(candidate, -54)
        lateinit var panel: DevicePickerPanel
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            panel = DevicePickerPanel(context, {}, {}, {}, {})
            panel.render(BlePresentation(devices = listOf(nearby)), permitted = true, enabled = true, hasScanned = true)

            val card = tagged<View>(panel, "picker_row:device-internal-id")
            val text = children(card).filterIsInstance<TextView>().map { it.text.toString() }.toList()
            check(text.contains(candidate.displayName))
            check(!text.contains(nearby.id)) { "Platform identifier leaked into the primary picker card" }
            check(!text.contains(candidate.productCategory) && !text.contains(candidate.evidence) &&
                !text.contains(candidate.detail)) { "Detection diagnostics appeared outside the details action" }
            val connect = children(card).filterIsInstance<Button>().single()
            check(connect.text == context.getString(lol.cutout.R.string.picker_connect))
            check(connect.contentDescription == context.getString(lol.cutout.R.string.picker_connect_device, candidate.displayName))

            val advertised = nearby.copy(advertisedName = "Advertised VESC board")
            panel.render(BlePresentation(devices = listOf(advertised)), permitted = true, enabled = true, hasScanned = true)
            check(tagged<TextView>(card, "picker_identity:device-internal-id").text == "Advertised VESC board") {
                "A meaningful advertised identity should remain visible as secondary text"
            }
        }
    }

    @Test
    fun verifiedRustReadinessStopsSpinnerWhileNativePhaseRemainsIdentifying() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val replay = productionReplayPresentation()
        check(replay.session?.connection?.readiness?.name == "VERIFIED") {
            "Production replay did not produce a verified Rust snapshot"
        }
        lateinit var panel: DevicePickerPanel
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            panel = DevicePickerPanel(context, {}, {}, {}, {})
            panel.render(replay.copy(scanning = true, phase = NativePhase.IDENTIFYING), permitted = true, enabled = true, hasScanned = true)
            check(tagged<ProgressBar>(panel, "picker_spinner").visibility == View.GONE) {
                "Verified connection remained in a connecting state after native phase lagged"
            }
            check(tagged<TextView>(panel, "picker_status_text").text == context.getString(lol.cutout.R.string.picker_status_live)) {
                "Verified readiness must remain the visible status when a scan flag lags"
            }
            for (readiness in listOf(
                MobileConnectionReadinessDto.FAILED,
                MobileConnectionReadinessDto.RECORD_ONLY,
                MobileConnectionReadinessDto.CONFLICTED,
            )) {
                val terminal = replay.copy(
                    scanning = true,
                    phase = NativePhase.IDENTIFYING,
                    session = replay.session!!.copy(
                        connection = replay.session!!.connection.copy(readiness = readiness),
                    ),
                )
                panel.render(terminal, permitted = true, enabled = true, hasScanned = true)
                check(tagged<ProgressBar>(panel, "picker_spinner").visibility == View.GONE) {
                    "$readiness readiness remained in a connecting/scanning state"
                }
            }
        }
    }

    private fun candidate(id: String, name: String, serviceUuid: String): DiscoveryCandidate =
        mobileDiscoveryCandidateFromAdvertisement(
            id,
            name,
            listOf(DiscoveryServiceUuid(UUID.fromString(serviceUuid).wireBytes())),
        )

    private fun nearby(candidate: DiscoveryCandidate, rssi: Int) = NearbyPev(
        id = candidate.platformIdentifier,
        name = candidate.displayName,
        rssi = rssi,
        suggested = candidate.isPickerCandidate,
        candidate = candidate,
        advertisedName = candidate.displayName,
    )

    private fun children(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(children(view.getChildAt(index)))
    }

    private inline fun <reified T : View> tagged(root: View, tag: String): T =
        findTag(root, tag) as? T ?: error("Missing ${T::class.java.simpleName} tagged $tag")

    private fun findTag(root: View, tag: String): View? = children(root).firstOrNull { it.tag == tag }
}
