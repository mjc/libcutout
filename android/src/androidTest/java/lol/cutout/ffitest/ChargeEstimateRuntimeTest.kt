package lol.cutout.ffitest

import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import lol.cutout.ChargeEstimateRuntime
import lol.cutout.CutoutUi
import lol.cutout.RideDashboardView
import org.junit.Test
import uniffi.cutout_mobile_ffi.BatteryLevel
import uniffi.cutout_mobile_ffi.BatteryLevelReading
import uniffi.cutout_mobile_ffi.MobileChargeCapacitySourceDto
import uniffi.cutout_mobile_ffi.MobileChargeModeDto
import uniffi.cutout_mobile_ffi.MobileChargeModeReadingDto
import uniffi.cutout_mobile_ffi.MobileChargeProfileDto
import uniffi.cutout_mobile_ffi.MobileMonotonicMillisDto
import uniffi.cutout_mobile_ffi.Speed
import uniffi.cutout_mobile_ffi.SpeedReading
import uniffi.cutout_mobile_ffi.MobileRiderDashboardMetricDescriptorDto
import uniffi.cutout_mobile_ffi.MobileValueQualityDto
import uniffi.cutout_mobile_ffi.MobileValueSourceDto
import uniffi.cutout_mobile_ffi.MobileVerificationStatusDto
import uniffi.cutout_mobile_ffi.mobileRiderDashboardProjection
import uniffi.cutout_mobile_ffi.MobileChargeEstimateUnavailableReasonDto
import uniffi.cutout_mobile_ffi.MobileChargeEstimateStateKindDto
import uniffi.cutout_mobile_ffi.MobileConnectionReadinessDto
import uniffi.cutout_mobile_ffi.RideOperatingState

/** Exercises charge sampling and the dashboard using Rust replay telemetry only. */
open class ChargeEstimateRuntimeTest {
    @Test
    fun estimatorSamplesRustReplayAndDashboardDisplaysRustChargeState() {
        val replay = productionReplayPresentation()
        val runtime = ChargeEstimateRuntime()
        try {
            check(runtime.observe(null, null, null) == null) {
                "Disconnected sessions must not retain or publish a charge estimate"
            }
            val chargingTelemetry = checkNotNull(replay.telemetry).copy(operatingState = RideOperatingState.CHARGING)
            val originalSession = checkNotNull(replay.session)
            val verified = originalSession.copy(connection = originalSession.connection.copy(
                readiness = MobileConnectionReadinessDto.VERIFIED,
            ))
            val charging = replay.copy(session = verified, telemetry = chargingTelemetry)
            val state = runtime.observe(charging.session, charging.settings, charging.telemetry)
            checkNotNull(state) { "Connected Rust replay telemetry should be sampled by the Rust estimator" }
            if (charging.settings?.defaultChargeProfile == null) {
                check(state.kind == MobileChargeEstimateStateKindDto.UNAVAILABLE)
                check(state.unavailableReason == MobileChargeEstimateUnavailableReasonDto.CAPACITY_MISSING)
            }

            val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
            val dashboard = RideDashboardView(context, {}, {})
            dashboard.render(charging, state)
            val expected = when (state.kind) {
                MobileChargeEstimateStateKindDto.FULL -> CutoutUi(context).label("charge.estimate.value.full")
                MobileChargeEstimateStateKindDto.BALANCING -> CutoutUi(context).label("charge.estimate.value.balancing")
                else -> CutoutUi(context).label("euc.status.charging")
            }
            check(hasText(dashboard, expected)) { "Dashboard did not display the Rust charge state $expected" }
        } finally {
            runtime.close()
        }
    }

    @Test
    fun fullSnapshotsClearAndRestoreEstimatedBatteryAndChargeProjection() {
        val replay = productionReplayPresentation()
        val original = checkNotNull(replay.session)
        val session = original.copy(connection = original.connection.copy(
            readiness = MobileConnectionReadinessDto.VERIFIED,
        ))
        val profile = MobileChargeProfileDto(
            sessionId = 1UL,
            profileId = 1U,
            capacityMilliampHours = 10_000U,
            capacitySource = MobileChargeCapacitySourceDto.PROTOCOL_PROFILE,
            verification = MobileVerificationStatusDto.SOURCE_VERIFIED,
            chargeFlowVerification = MobileVerificationStatusDto.SOURCE_VERIFIED,
        )
        val settings = checkNotNull(replay.settings).copy(
            connection = session.connection,
            defaultChargeProfile = profile,
        )
        val snapshot = checkNotNull(replay.telemetry).copy(
            atMs = null,
            speed = SpeedReading(Speed(0), MobileValueSourceDto.REPORTED,
                MobileValueQualityDto.KNOWN, MobileVerificationStatusDto.SOURCE_VERIFIED),
            powerFlow = null,
            batteryCurrent = null,
            batteryTemperature = null,
            operatingState = RideOperatingState.CHARGING,
            chargeMode = MobileChargeModeReadingDto(
                MobileChargeModeDto.CHARGING,
                MobileValueSourceDto.REPORTED,
                MobileValueQualityDto.KNOWN,
                MobileVerificationStatusDto.SOURCE_VERIFIED,
            ),
            batteryLevelReported = null,
        )
        val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        val dashboard = RideDashboardView(context, {}, {})
        ChargeEstimateRuntime().use { runtime ->
            for (percent in listOf(71, null, 63)) {
                val level = percent?.let {
                    BatteryLevelReading(BatteryLevel(it.toUByte()), MobileValueSourceDto.ESTIMATED,
                        MobileValueQualityDto.INFERRED, MobileVerificationStatusDto.INFERRED)
                }
                val observedAt = MobileMonotonicMillisDto(SystemClock.elapsedRealtime().toULong())
                val current = snapshot.copy(
                    atMs = observedAt,
                    speedObservedAtMs = observedAt,
                    batteryLevelEstimated = level,
                )
                val presentation = replay.copy(session = session, settings = settings, telemetry = current)
                val state = checkNotNull(runtime.observe(session, settings, current))
                val evidence = "battery=$percent kind=${state.kind} reason=${state.unavailableReason}"
                dashboard.render(presentation, state)
                val battery = mobileRiderDashboardProjection(current).dashboardMetrics
                    .filterIsInstance<MobileRiderDashboardMetricDescriptorDto.BatteryLevel>()
                    .firstOrNull()?.reading
                check(battery == level)
                if (percent == null) {
                    check(state.kind == MobileChargeEstimateStateKindDto.UNAVAILABLE) { evidence }
                    check(state.unavailableReason == MobileChargeEstimateUnavailableReasonDto.BATTERY_LEVEL_MISSING) { evidence }
                    check(!hasText(dashboard, "≈71")) { "The dashboard retained the previous estimate" }
                } else {
                    check(state.kind == MobileChargeEstimateStateKindDto.COLLECTING_SAMPLES) { evidence }
                    check(state.unavailableReason == null) { evidence }
                    check(hasText(dashboard, "≈$percent")) { "The dashboard did not replace the battery estimate" }
                }
            }
        }
    }

    private fun hasText(view: View, expected: String): Boolean =
        (view is TextView && view.text.toString() == expected) ||
            (view is ViewGroup && (0 until view.childCount).any { hasText(view.getChildAt(it), expected) })
}
