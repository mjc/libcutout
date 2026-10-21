package lol.cutout.ffitest

import android.app.Activity
import android.content.res.Configuration
import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.ScrollView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.BlePresentation
import lol.cutout.CutoutApplication
import lol.cutout.CutoutUi
import lol.cutout.MainActivity
import lol.cutout.RideDashboardView
import org.junit.Test
import java.util.Locale
import uniffi.cutout_mobile_ffi.MobileFootpadContactState
import uniffi.cutout_mobile_ffi.MobileFootpadTelemetryDto
import uniffi.cutout_mobile_ffi.MobileConnectionReadinessDto
import uniffi.cutout_mobile_ffi.MobileProtocolFamilyDto
import uniffi.cutout_mobile_ffi.MobileVehicleClassificationProvenanceDto
import uniffi.cutout_mobile_ffi.MobileVehicleClassificationRequirementDto
import uniffi.cutout_mobile_ffi.MobileVehicleKindDto
import uniffi.cutout_mobile_ffi.MobileVescApplicationDiscoveryDto
import uniffi.cutout_mobile_ffi.MobileVescRideOperatingModeDto
import uniffi.cutout_mobile_ffi.MobileVescRideStopReasonDto
import uniffi.cutout_mobile_ffi.MobileVescRideWarningDto
import uniffi.cutout_mobile_ffi.MobilePhoneLocationSampleDto
import uniffi.cutout_mobile_ffi.MobilePhoneLocationState
import uniffi.cutout_mobile_ffi.RideOperatingState

/** Presentation-only state copies over the existing Rust replay; no wire claim is made. */
open class RideDashboardProjectionTest {
    @Test
    fun rustOperatingStateControlsEucStatusAndTelemetryViewsStayStable() = withActivity { activity ->
        val replay = productionReplayPresentation()
        val ui = CutoutUi(activity)
        val ride = RideDashboardView(activity, {}, {})
        activity.setContentView(ride)

        // The replay's retained speed is intentionally left unchanged as status states vary.
        val states = listOf(
            RideOperatingState.PARKED to "euc.status.parked",
            RideOperatingState.STANDING to "euc.status.standing",
            RideOperatingState.RIDING to "euc.status.riding",
            RideOperatingState.CHARGING to "euc.status.charging",
        )
        var originalSpeed: String? = null
        var speedView: TextView? = null
        var gaugeView: View? = null
        for ((state, labelKey) in states) {
            ride.render(replay.withTelemetry(operatingState = state))
            val currentSpeed = tagged<TextView>(ride, "ride.hero.speed")
            if (speedView == null) {
                speedView = currentSpeed
                gaugeView = tagged(ride, "euc.ride.headroom")
                originalSpeed = currentSpeed.text.toString()
            } else {
                check(currentSpeed === speedView) { "State-only Rust update replaced the speed view" }
                check(tagged<View>(ride, "euc.ride.headroom") === gaugeView) { "State-only update replaced headroom gauge" }
            }
            check(currentSpeed.text.toString() == originalSpeed) {
                "Synthetic operating state changed the replay speed value"
            }
            val expectedStatus = ui.label(labelKey)
            check(expectedStatus != activity.getString(lol.cutout.R.string.setting_unknown)) {
                "Missing Ride localization key $labelKey"
            }
            check(hasText(ride, expectedStatus)) { "$state did not use localized $labelKey" }
        }
        check(ui.label("euc.status.standing") == "Stationary") {
            "Standing must use neutral stationary wording"
        }
        check(ui.label("euc.status.standing") != ui.label("euc.status.parked")) {
            "Standing must remain distinct from explicitly parked telemetry"
        }
        val speedUnit = activity.getString(lol.cutout.R.string.speed_unit)
        check(speedUnit == "mph") { "Ride speed unit no longer matches Swift RideUnits" }
        val replaySpeed = checkNotNull(replay.telemetry?.speed).value.value.toDouble()
        val expectedSpeed = String.format(Locale.getDefault(), "%.1f", replaySpeed * 0.0022369362920544)
        check(originalSpeed == expectedSpeed) { "Ride speed did not use Swift's millimeters-per-second to mph conversion" }
        check(children(ride).filterIsInstance<TextView>().any { it.text.toString().trim() == speedUnit }) {
            "The Ride hero did not render its localized speed unit"
        }
    }

    @Test
    fun identifiedEucRetainsInstrumentsWhileRustReadinessIsPendingWithoutRetainingReadings() = withActivity { activity ->
        val replay = productionReplayPresentation()
        val ride = RideDashboardView(activity, {}, {})
        activity.setContentView(ride)
        ride.render(replay)
        val speed = tagged<TextView>(ride, "ride.hero.speed")
        val gauge = tagged<View>(ride, "euc.ride.headroom")
        val verifiedSpeed = speed.text.toString()
        check(verifiedSpeed.isNotBlank() && verifiedSpeed != activity.getString(lol.cutout.R.string.unavailable))

        val pending = replay.copy(
            phase = lol.cutout.NativePhase.IDENTIFYING,
            session = checkNotNull(replay.session).copy(
                connection = checkNotNull(replay.session).connection.copy(
                    readiness = MobileConnectionReadinessDto.PENDING,
                ),
            ),
        )
        ride.render(pending)
        check(tagged<TextView>(ride, "ride.hero.speed") === speed) {
            "Pending Rust readiness replaced the identified EUC speed instrument"
        }
        check(tagged<View>(ride, "euc.ride.headroom") === gauge) {
            "Pending Rust readiness replaced the identified EUC headroom instrument"
        }
        check(speed.text.toString() == activity.getString(lol.cutout.R.string.unavailable)) {
            "Unverified readiness retained a previously verified speed reading"
        }
        check(speed.text.toString() != verifiedSpeed)

        ride.render(replay)
        check(tagged<TextView>(ride, "ride.hero.speed") === speed)
        check(speed.text.toString() == verifiedSpeed) {
            "Verified readiness did not restore the current Rust speed on the retained instrument"
        }
    }

    @Test
    fun vescPresentationUsesRustModeWarningsStopsAndFootpadEnums() = withActivity { activity ->
        val replay = productionReplayPresentation()
        val ui = CutoutUi(activity)
        val ride = RideDashboardView(activity, {}, {})
        activity.setContentView(ride)

        val darkride = replay.withVesc(
            operatingState = RideOperatingState.UNKNOWN,
            mode = MobileVescRideOperatingModeDto.DARKRIDE,
            warning = MobileVescRideWarningDto.NONE,
            stopReason = MobileVescRideStopReasonDto.NONE,
        )
        ride.render(darkride)
        check(hasLocalizedText(ride, ui, activity, "vesc.mode.darkride"))

        val bothFootpads = darkride.withVesc(
            warning = MobileVescRideWarningDto.LOW_VOLTAGE,
            footpad = MobileFootpadTelemetryDto(1u, MobileFootpadContactState.BOTH, null, null),
        )
        ride.render(bothFootpads)
        check(hasLocalizedText(ride, ui, activity, "vesc.warning.low_voltage"))
        check(tagged<TextView>(ride, "vesc.ride.footpad").text.toString() == localized(ui, activity, "footpad.contact.both"))

        val neitherFootpad = bothFootpads.withVesc(
            warning = MobileVescRideWarningDto.NONE,
            stopReason = MobileVescRideStopReasonDto.PITCH,
            footpad = MobileFootpadTelemetryDto(0u, MobileFootpadContactState.NONE, null, null),
        )
        ride.render(neitherFootpad)
        check(hasLocalizedText(ride, ui, activity, "vesc.stop.pitch"))
        check(tagged<TextView>(ride, "vesc.ride.footpad").text.toString() == localized(ui, activity, "footpad.contact.none"))

        // A decoded warning wins over a simultaneous stop reason, as on SwiftUI.
        val warningWins = neitherFootpad.withVesc(warning = MobileVescRideWarningDto.CURRENT)
        ride.render(warningWins)
        check(hasLocalizedText(ride, ui, activity, "vesc.warning.current"))
        check(!hasText(ride, localized(ui, activity, "vesc.stop.pitch")))
    }

    @Test
    fun resizeRebuildsGeometryAndImmediatelyReappliesLatestRustValues() = withActivity { activity ->
        val presentation = productionReplayPresentation()
        val ride = RideDashboardView(activity, {}, {})
        activity.setContentView(ride)
        ride.render(presentation)
        val width = (360 * activity.resources.displayMetrics.density).toInt()
        val height = (800 * activity.resources.displayMetrics.density).toInt()
        layout(ride, width, height)
        val expected = tagged<TextView>(ride, "ride.hero.speed").text.toString()
        check(expected.isNotBlank() && expected != activity.getString(lol.cutout.R.string.unavailable))
        checkLaidOutInstrumentBounds(ride)

        // Force a landscape/size-class transition without waiting for another telemetry event.
        layout(ride, height, width)
        val afterResize = tagged<TextView>(ride, "ride.hero.speed").text.toString()
        check(afterResize == expected) { "Geometry rebuild discarded the most recent Rust speed" }
        checkLaidOutInstrumentBounds(ride)
        check(tagged<View>(ride, "euc.ride.headroom").contentDescription?.isNotBlank() == true) {
            "Geometry rebuild did not restore the current Rust headroom value"
        }
    }

    @Test
    fun speedAutoSizeFitsTextWithinConstrainedRideWidth() = withActivity { activity ->
        val ride = RideDashboardView(activity, {}, {})
        activity.setContentView(ride)
        ride.render(productionReplayPresentation())
        val width = (320 * activity.resources.displayMetrics.density).toInt()
        val height = (760 * activity.resources.displayMetrics.density).toInt()
        layout(ride, width, height)
        ride.viewTreeObserver.dispatchOnPreDraw()

        val speed = tagged<TextView>(ride, "ride.hero.speed")
        val line = checkNotNull(speed.layout) { "Speed has no measured text layout after pre-draw" }
        val availableWidth = speed.width - speed.compoundPaddingLeft - speed.compoundPaddingRight
        check(line.lineCount == 1) { "Speed should stay on a single line at constrained phone width" }
        check(line.getLineWidth(0) <= availableWidth + 1f) {
            "Autosized speed text width ${line.getLineWidth(0)} exceeds its available width $availableWidth"
        }
    }

    @Test
    fun phoneGpsSpeedUsesRustReadbackWithoutReplacingWheelSpeed() = withActivity { activity ->
        val replay = productionReplayPresentation()
        val ride = RideDashboardView(activity, {}, {})
        activity.setContentView(ride)
        ride.render(replay)
        val wheelSpeed = tagged<TextView>(ride, "ride.hero.speed")
        val originalWheelSpeed = wheelSpeed.text.toString()
        val gauge = tagged<View>(ride, "euc.ride.headroom")
        val gpsSpeed = tagged<TextView>(ride, "ride.gps.speed")

        val state = MobilePhoneLocationState()
        val sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs = 1_700_000_000_000UL,
            sourceTimestampUnixSeconds = null,
            latitudeDegrees = 39.7392358,
            longitudeDegrees = -104.990251,
            altitudeMeters = 1_609.344,
            horizontalAccuracyMeters = 1.0,
            verticalAccuracyMeters = 1.0,
            speedMetersPerSecond = 4.47040025,
            speedAccuracyMetersPerSecond = 0.2,
            courseDegrees = 271.5,
            courseAccuracyDegrees = 3.0,
        )
        state.ingestAt(sample, 1_000UL)
        val fresh = state.currentReadbackAt(2_000UL)
        ride.renderPhoneSpeed(fresh)
        val normalizedGpsMph = checkNotNull(fresh.snapshot.gpsSpeed).value.value.toDouble() * 0.0022369362920544
        check(gpsSpeed.text.toString() == String.format(Locale.getDefault(), "GPS %.1f", normalizedGpsMph)) {
            "GPS speed did not use Rust's normalized phone speed"
        }
        check(gpsSpeed.contentDescription.toString().contains(CutoutUi(activity).label("gps.detail.fresh")))
        check(tagged<TextView>(ride, "ride.hero.speed") === wheelSpeed)
        check(wheelSpeed.text.toString() == originalWheelSpeed) { "Phone speed replaced Rust-selected wheel speed" }
        check(tagged<View>(ride, "euc.ride.headroom") === gauge)

        val stale = state.currentReadbackAt(3_001UL)
        ride.renderPhoneSpeed(stale)
        check(gpsSpeed.text.toString() == "GPS 10.0") { "Stale GPS value should remain visible with stale status" }
        check(gpsSpeed.contentDescription.toString().contains(CutoutUi(activity).label("gps.detail.stale")))

        state.clear()
        ride.renderPhoneSpeed(state.currentReadbackAt(4_000UL))
        check(gpsSpeed.text.toString() == "GPS ${activity.getString(lol.cutout.R.string.unavailable)}")
        check(gpsSpeed.contentDescription.toString().contains(CutoutUi(activity).label("gps.detail.unavailable")))
    }

    @Test
    fun largeTextUsesScrollableSingleColumnReadingsAndKeepsRustValuesAfterResize() = withActivity { activity ->
        val configuration = Configuration(activity.resources.configuration).apply { fontScale = 2f }
        val accessibleContext = activity.createConfigurationContext(configuration)
        val ride = RideDashboardView(accessibleContext, {}, {})
        activity.setContentView(ride)
        ride.render(productionReplayPresentation())
        val width = (360 * activity.resources.displayMetrics.density).toInt()
        val height = (640 * activity.resources.displayMetrics.density).toInt()
        layout(ride, width, height)

        val scroll = tagged<ScrollView>(ride, "ride.accessible.readings.scroll")
        val essentialKeys = listOf("headroom", "range", "battery", "voltage", "power", "temperature")
        val values = essentialKeys.associateWith { key ->
            tagged<TextView>(ride, "ride.accessible.metric.$key").apply {
                check(maxLines > 1) { "$key should wrap in the large-text presentation" }
            }.text.toString()
        }
        check(values.values.all(String::isNotBlank)) { "Large-text EUC projection omitted an essential reading" }
        val gpsState = MobilePhoneLocationState()
        gpsState.ingestAt(
            MobilePhoneLocationSampleDto(
                wallClockUnixMs = 1_700_000_000_000UL,
                sourceTimestampUnixSeconds = null,
                latitudeDegrees = 39.7392358,
                longitudeDegrees = -104.990251,
                altitudeMeters = 1_609.344,
                horizontalAccuracyMeters = 1.0,
                verticalAccuracyMeters = 1.0,
                speedMetersPerSecond = 4.47040025,
                speedAccuracyMetersPerSecond = 0.2,
                courseDegrees = 271.5,
                courseAccuracyDegrees = 3.0,
            ),
            1_000UL,
        )
        ride.renderPhoneSpeed(gpsState.currentReadbackAt(1_000UL))
        check(tagged<TextView>(ride, "ride.gps.speed").text.toString().startsWith("GPS ")) {
            "Large-text EUC hero omitted the independent phone speed readback"
        }
        check(scroll.childCount == 1 && scroll.getChildAt(0) is LinearLayout) {
            "Large-text Ride should use the scrollable single-column readings surface"
        }

        layout(ride, height, width)
        essentialKeys.forEach { key ->
            val value = tagged<TextView>(ride, "ride.accessible.metric.$key").text.toString()
            check(value == values[key]) { "Resize discarded the latest Rust $key value" }
        }
    }

    private fun BlePresentation.withTelemetry(operatingState: RideOperatingState): BlePresentation = copy(
        telemetry = checkNotNull(telemetry).copy(operatingState = operatingState),
    )

    private fun BlePresentation.withVesc(
        operatingState: RideOperatingState = RideOperatingState.RIDING,
        mode: MobileVescRideOperatingModeDto = MobileVescRideOperatingModeDto.NORMAL,
        warning: MobileVescRideWarningDto = MobileVescRideWarningDto.NONE,
        stopReason: MobileVescRideStopReasonDto = MobileVescRideStopReasonDto.NONE,
        footpad: MobileFootpadTelemetryDto? = null,
    ): BlePresentation {
        val session = checkNotNull(session)
        val identity = checkNotNull(session.identity).copy(
            protocol = MobileProtocolFamilyDto.VESC,
            vehicleKind = MobileVehicleKindDto.ONEWHEEL,
            classificationProvenance = MobileVehicleClassificationProvenanceDto.PROTOCOL,
            classificationRequirement = MobileVehicleClassificationRequirementDto.RESOLVED,
            vescApplication = MobileVescApplicationDiscoveryDto.REFLOAT,
            vescApplicationQuery = null,
            model = null,
        )
        return copy(
            session = session.copy(identity = identity),
            telemetry = checkNotNull(telemetry).copy(
                operatingState = operatingState,
                vescOperatingMode = mode,
                vescWarning = warning,
                vescStopReason = stopReason,
                footpad = footpad,
            ),
        )
    }

    private fun layout(view: View, width: Int, height: Int) {
        view.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
        )
        view.layout(0, 0, width, height)
    }

    private fun checkLaidOutInstrumentBounds(ride: RideDashboardView) {
        val speed = tagged<TextView>(ride, "ride.hero.speed")
        val gauge = tagged<View>(ride, "euc.ride.headroom")
        check(speed.width > 0 && speed.height > 0) {
            "Speed view had empty bounds on the first measured frame or after resize"
        }
        check(gauge.width > 0 && gauge.height > 0) {
            "Headroom gauge had empty bounds on the first measured frame or after resize"
        }
    }

    private fun withActivity(block: (MainActivity) -> Unit) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(
            Intent(instrumentation.targetContext, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        ) as MainActivity
        try {
            (activity.application as CutoutApplication).bluetooth.observe(null)
            (activity.application as CutoutApplication).bluetooth.stopScan()
            instrumentation.runOnMainSync { block(activity) }
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    private fun hasText(root: View, expected: String): Boolean =
        children(root).filterIsInstance<TextView>().any { it.text.toString() == expected }

    private fun hasLocalizedText(root: View, ui: CutoutUi, activity: Activity, key: String): Boolean =
        hasText(root, localized(ui, activity, key))

    private fun localized(ui: CutoutUi, activity: Activity, key: String): String = ui.label(key).also {
        check(it != activity.getString(lol.cutout.R.string.setting_unknown)) { "Missing Ride localization key $key" }
    }

    private fun children(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(children(view.getChildAt(index)))
    }

    private inline fun <reified T : View> tagged(root: View, tag: String): T =
        children(root).firstOrNull { it.tag == tag } as? T ?: error("Missing ${T::class.java.simpleName} tagged $tag")
}
