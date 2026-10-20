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
import uniffi.cutout_mobile_ffi.*
import uniffi.cutout_mobile_ffi.MobileFootpadContactState
import uniffi.cutout_mobile_ffi.MobileFootpadTelemetryDto
import uniffi.cutout_mobile_ffi.MobileConnectionReadinessDto
import uniffi.cutout_mobile_ffi.MobileProtocolFamilyDto
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
        check(ui.label("euc.status.standing") == ui.label("euc.status.parked")) {
            "Standing wording should follow the current localized Parked label"
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
        check(speed.text.toString() == "--") {
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
    fun vescBalanceInstrumentsRequireRefloatEvidenceRegardlessOfPhysicalKind() = withActivity { activity ->
        val replay = productionReplayPresentation().withVesc(
            footpad = MobileFootpadTelemetryDto(1u, MobileFootpadContactState.BOTH, null, null),
        )
        val session = checkNotNull(replay.session)
        val identity = checkNotNull(session.identity)
        for (fontScale in listOf(1f, 2f)) {
            val context = activity.createConfigurationContext(
                Configuration(activity.resources.configuration).apply { this.fontScale = fontScale },
            )
            val ui = CutoutUi(context)
            val ride = RideDashboardView(context, {}, {})
            activity.setContentView(ride)
            ride.render(replay)
            val speed = tagged<TextView>(ride, "ride.hero.speed").text.toString()
            check(children(ride).any { it.tag == "vesc.ride.footpad" }) {
                "Validated Refloat lost its existing balance surface at font scale $fontScale"
            }
            val balanceLabels = setOf(
                context.getString(lol.cutout.R.string.pwm_headroom),
                ui.label("vesc.duty_headroom.label"),
                ui.label("vesc.metric.board_angle"),
                ui.label("footpad.title"),
            )
            val balanceTags = setOf(
                "euc.ride.headroom", "vesc.ride.footpad", "ride.accessible.metric.headroom",
            )
            for (kind in MobileVehicleKindDto.entries) {
                for (application in listOf(null, MobileVescApplicationDiscoveryDto.PENDING, MobileVescApplicationDiscoveryDto.ABSENT)) {
                    ride.render(replay.copy(session = session.copy(identity = identity.copy(
                        vehicleKind = kind, vescApplication = application,
                    ))))
                    layout(ride, 1080, 1920)
                    check(children(ride).none { view -> balanceTags.any { tag -> view.tag == tag } }) {
                        "$kind/$application exposed a balance instrument at font scale $fontScale"
                    }
                    check(children(ride).filterIsInstance<TextView>().none { it.text.toString() in balanceLabels }) {
                        "$kind/$application exposed a balance label at font scale $fontScale"
                    }
                    check(tagged<TextView>(ride, "ride.hero.speed").text.toString() == speed) {
                        "Suppressing unsupported balance instruments discarded reported speed"
                    }
                }
            }
            ride.render(replay)
            check(children(ride).any { it.tag == "vesc.ride.footpad" }) {
                "A later validated Refloat projection did not restore its balance surface"
            }
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
        check(gpsSpeed.text.toString() == "GPS --")
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

    @Test
    fun standardVescUsesEnergyAndRecordingInstrumentsWithoutBalanceBoardReadings() = withActivity { activity ->
        val replay = productionReplayPresentation().withStandardVesc()
        val ride = RideDashboardView(activity, {}, {})
        val ui = CutoutUi(activity)
        activity.setContentView(ride)
        ride.render(replay)
        check(tagged<TextView>(ride, "ride.hero.speed").text.toString() == "--") {
            "Standard VESC without Rust drivetrain speed must remain unavailable"
        }
        check(hasText(ride, ui.label("vesc.metric.pack_voltage")))
        val voltage = checkNotNull(replay.telemetry?.voltage).value.value
        val expectedVoltage = String.format(Locale.getDefault(), "%.1f V", voltage.toDouble() / 1000)
        check(tagged<TextView>(ride, "ride.metric.energy").text.toString() == expectedVoltage)
        listOf("vesc.metric.trip", "vesc.metric.ride_time", "vesc.metric.battery_current",
            "vesc.metric.controller_temperature", "vesc.metric.motor_temperature").forEach { key ->
            check(hasText(ride, localized(ui, activity, key))) { "Standard Ride omitted $key" }
        }
        assertNoBalanceInstruments(ride, ui)
        check(children(ride).none { it is ScrollView }) { "Standard Ride must fit its normal-size instrument surface" }

        // A retained estimated SOC is presented as an estimate; voltage is the fallback.
        val replayTelemetry = checkNotNull(productionReplayPresentation().telemetry)
        val battery = checkNotNull(replayTelemetry.batteryLevelReported ?: replayTelemetry.batteryLevelEstimated)
        ride.render(replay.copy(telemetry = checkNotNull(replay.telemetry).copy(batteryLevelEstimated =
            battery.copy(source = MobileValueSourceDto.ESTIMATED))))
        check(tagged<TextView>(ride, "ride.metric.energy").text.toString() == "≈${battery.value.value} %")
        check(hasText(ride, activity.getString(lol.cutout.R.string.battery)))
        ride.render(replay)
        check(tagged<TextView>(ride, "ride.metric.energy").text.toString() == expectedVoltage)
    }

    @Test
    fun genericVescAndPendingApplicationNeverAcquireRefloatInstrumentsFromVehicleClass() = withActivity { activity ->
        val ride = RideDashboardView(activity, {}, {})
        activity.setContentView(ride)
        val replay = productionReplayPresentation().withStandardVesc()
        val session = checkNotNull(replay.session)
        val identity = checkNotNull(session.identity)
        for (kind in listOf(MobileVehicleKindDto.UNKNOWN, MobileVehicleKindDto.ESKATE, MobileVehicleKindDto.ONEWHEEL, MobileVehicleKindDto.ELECTRIC_UNICYCLE)) {
            for (application in listOf(null, MobileVescApplicationDiscoveryDto.PENDING, MobileVescApplicationDiscoveryDto.ABSENT)) {
                ride.render(replay.copy(session = session.copy(identity = identity.copy(
                    vehicleKind = kind, vescApplication = application))))
                assertNoBalanceInstruments(ride, CutoutUi(activity))
            }
        }
        // Refloat is the explicit shared application fact that permits its native surface.
        ride.render(replay.withVesc())
        check(children(ride).any { it.tag == "vesc.ride.footpad" })
    }

    @Test
    fun standardVescClearsUnverifiedReadingsAndPreservesGeometryAcrossTextSizes() = withActivity { activity ->
        val replay = productionReplayPresentation().withStandardVesc()
        for (fontScale in listOf(1f, 2f)) {
            val context = activity.createConfigurationContext(Configuration(activity.resources.configuration).apply {
                this.fontScale = fontScale
            })
            val ride = RideDashboardView(context, {}, {})
            activity.setContentView(ride)
            ride.render(replay)
            val energyTag = if (fontScale >= 1.5f) "ride.accessible.metric.energy" else "ride.metric.energy"
            val width = (360 * context.resources.displayMetrics.density).toInt()
            val height = (760 * context.resources.displayMetrics.density).toInt()
            layout(ride, width, height)
            val before = tagged<TextView>(ride, energyTag).text.toString()
            check(before != context.getString(lol.cutout.R.string.unavailable))
            layout(ride, height, width)
            check(tagged<TextView>(ride, energyTag).text.toString() == before)
            assertNoBalanceInstruments(ride, CutoutUi(context))
            val session = checkNotNull(replay.session)
            ride.render(replay.copy(session = session.copy(connection = session.connection.copy(
                readiness = MobileConnectionReadinessDto.PENDING))))
            check(tagged<TextView>(ride, energyTag).text.toString() == context.getString(lol.cutout.R.string.unavailable))
            ride.render(replay)
            check(tagged<TextView>(ride, energyTag).text.toString() == before)
        }
    }

    @Test
    fun standardVescRecordingSummaryUsesRustStatisticsAndClearsTerminalValues() = withActivity { activity ->
        val ride = RideDashboardView(activity, {}, {})
        activity.setContentView(ride)
        ride.render(productionReplayPresentation().withStandardVesc())
        val recording = recordingProjection()
        ride.renderRecording(recording)
        check(tagged<TextView>(ride, "ride.metric.ride_time").text.toString() == "1:05")
        val trip = tagged<TextView>(ride, "ride.metric.trip").text.toString()
        check(trip != activity.getString(lol.cutout.R.string.unavailable))
        val width = (360 * activity.resources.displayMetrics.density).toInt()
        val height = (760 * activity.resources.displayMetrics.density).toInt()
        layout(ride, height, width)
        check(tagged<TextView>(ride, "ride.metric.ride_time").text.toString() == "1:05")
        check(tagged<TextView>(ride, "ride.metric.trip").text.toString() == trip)
        ride.renderRecording(recording.copy(summary = recording.summary.copy(pointCount = 0UL)))
        check(tagged<TextView>(ride, "ride.metric.trip").text.toString() == activity.getString(lol.cutout.R.string.unavailable))
        ride.renderRecording(recording.copy(state = MobileRideLifecycleStateDto.STOPPED, liveActivityStatistics = null))
        check(tagged<TextView>(ride, "ride.metric.ride_time").text.toString() == activity.getString(lol.cutout.R.string.unavailable))
        ride.renderRecording(null)
        check(tagged<TextView>(ride, "ride.metric.trip").text.toString() == activity.getString(lol.cutout.R.string.unavailable))
    }

    @Test
    fun standardRideActionsUseTheExistingTuneAndVehicleChoiceRoutes() = withActivity { activity ->
        var openedTune = 0
        var choseKind = 0
        val ride = RideDashboardView(activity, {}, {}, onOpenTune = { openedTune++ },
            onChooseVehicleKind = { choseKind++ })
        activity.setContentView(ride)
        val replay = productionReplayPresentation().withStandardVesc()
        ride.render(replay)
        tagged<View>(ride, "ride.open-tune").performClick()
        check(openedTune == 1)
        val session = checkNotNull(replay.session)
        ride.render(replay.copy(session = session.copy(identity = checkNotNull(session.identity).copy(
            vehicleKind = MobileVehicleKindDto.UNKNOWN,
            classificationRequirement = MobileVehicleClassificationRequirementDto.USER_SELECTION_REQUIRED))))
        tagged<View>(ride, "ride.choose-vehicle-kind").performClick()
        check(choseKind == 1)
        assertNoBalanceInstruments(ride, CutoutUi(activity))
    }

    @Test
    fun unresolvedVescClassificationShowsSetupUntilRustResolvesTheVehicle() = withActivity { activity ->
        val replay = productionReplayPresentation().withStandardVesc()
        val session = checkNotNull(replay.session)
        val identity = checkNotNull(session.identity)
        val ui = CutoutUi(activity)
        for (fontScale in listOf(1f, 2f)) {
            var choices = 0
            var tuneOpens = 0
            val context = activity.createConfigurationContext(Configuration(activity.resources.configuration).apply {
                this.fontScale = fontScale
            })
            val ride = RideDashboardView(context, {}, {}, onOpenTune = { tuneOpens++ },
                onChooseVehicleKind = { choices++ })
            activity.setContentView(ride)
            ride.render(replay)
            check(children(ride).any { it.tag == "ride.hero.speed" })
            val energyTag = if (fontScale >= 1.5f) "ride.accessible.metric.energy" else "ride.metric.energy"
            val verifiedEnergy = tagged<TextView>(ride, energyTag).text.toString()

            for (requirement in listOf(MobileVehicleClassificationRequirementDto.PENDING,
                MobileVehicleClassificationRequirementDto.USER_SELECTION_REQUIRED)) {
                ride.render(replay.copy(session = session.copy(identity = identity.copy(
                    vehicleKind = MobileVehicleKindDto.UNKNOWN,
                    classificationRequirement = requirement,
                    vescApplication = if (requirement == MobileVehicleClassificationRequirementDto.PENDING)
                        MobileVescApplicationDiscoveryDto.PENDING else MobileVescApplicationDiscoveryDto.ABSENT))))
                val width = (360 * context.resources.displayMetrics.density).toInt()
                val height = (760 * context.resources.displayMetrics.density).toInt()
                for ((w, h) in listOf(width to height, height to width)) {
                    layout(ride, w, h)
                    check(children(ride).none { it.tag == "ride.hero.speed" || it.tag == energyTag ||
                        it.tag == "ride.readings.open" || it.tag == "ride.open-tune" ||
                        it.tag == "ride.accessible.readings.scroll" }) {
                        "$requirement exposed Ride instruments before Rust resolved vehicle classification"
                    }
                }
                if (requirement == MobileVehicleClassificationRequirementDto.PENDING) {
                    check(hasText(ride, localized(ui, activity, "device.type.discovery_pending")))
                    check(children(ride).none { it.tag == "ride.choose-vehicle-kind" }) {
                        "Pending discovery offered physical classification before Rust requested it"
                    }
                } else {
                    check(hasText(ride, localized(ui, activity, "device.type.choose.explanation")))
                    val choiceButton = tagged<TextView>(ride, "ride.choose-vehicle-kind")
                    check(choiceButton.text.isNotBlank() && choiceButton.text.toString() != "device.type.choose") {
                        "Vehicle choice displayed a raw localization key"
                    }
                    choiceButton.performClick()
                    check(choices == 1)
                    check(tuneOpens == 0) { "Classification choice opened Tune before vehicle resolution" }
                }
            }

            // A saved/restored classification is already resolved in Rust; no fresh native choice.
            ride.render(replay)
            check(tagged<TextView>(ride, energyTag).text.toString() == verifiedEnergy)
            tagged<View>(ride, "ride.open-tune").performClick()
            check(tuneOpens == 1 && choices == 1)
            check(children(ride).none { it.tag == "ride.choose-vehicle-kind" })
        }
    }

    private fun BlePresentation.withStandardVesc(): BlePresentation {
        val session = checkNotNull(session)
        return copy(session = session.copy(identity = checkNotNull(session.identity).copy(
            protocol = MobileProtocolFamilyDto.VESC, vehicleKind = MobileVehicleKindDto.ESKATE,
            vescApplication = MobileVescApplicationDiscoveryDto.ABSENT,
            classificationRequirement = MobileVehicleClassificationRequirementDto.RESOLVED)),
            telemetry = checkNotNull(telemetry).copy(speed = null, batteryLevelReported = null,
                batteryLevelEstimated = null, vescOperatingMode = null, vescWarning = null,
                vescStopReason = null, pitch = null, footpad = null))
    }

    private fun assertNoBalanceInstruments(ride: RideDashboardView, ui: CutoutUi) {
        check(children(ride).none { it.tag == "vesc.ride.footpad" || it.tag == "euc.ride.headroom" ||
            it.tag == "ride.accessible.metric.headroom" }) { "Standard VESC acquired balance-board instruments" }
        listOf("vesc.metric.board_angle", "vesc.duty_headroom.label", "footpad.title", "ride.safety.pwm_headroom").forEach {
            check(!hasText(ride, ui.label(it))) { "Generic/standard VESC displayed balance-specific $it" }
        }
    }

    /** Native presentation fixture over exported Rust projection fields; not wire/clock validation. */
    private fun recordingProjection() = MobileRideMapCoreSnapshotDto(
        rideId = "ride-projection", revision = 1UL, recordingToken = null, commandToken = null,
        locationAcquisition = MobileRideMapSnapshotLocationAcquisitionDto(MobileRideMapAvailabilityDto.READY,
            MobileRideMapLocationDemandDto.RECORD, MobileRideMapLocationRecoveryActionDto.NONE),
        state = MobileRideLifecycleStateDto.ACTIVE, allowedActions = emptyList(),
        telemetryState = MobileRideMapCoreTelemetryStateDto.GPS_ONLY,
        summary = MobileRideMapCoreSummaryDto(2UL, 1609.344, 65_000UL,
            MobileRideMovingTimeDto.Unavailable, MobileRideElapsedTimeDto.Unavailable),
        liveActivityStatistics = MobileRideMapLiveActivityStatisticsDto(65_000UL, 1_609_344UL),
        liveSpeed = MobileRideMapSpeedDto(0, MobileRideMapSpeedSourceDto.UNAVAILABLE),
        segmentCount = 1UL, associatedVehicle = null, recordedBoundsAvailable = true,
    )

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
            vescApplication = MobileVescApplicationDiscoveryDto.REFLOAT,
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
