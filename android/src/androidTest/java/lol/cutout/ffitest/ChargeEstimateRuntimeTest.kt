package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import lol.cutout.ChargeEstimateRuntime
import lol.cutout.CutoutUi
import lol.cutout.RideDashboardView
import org.junit.Test
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

    private fun hasText(view: View, expected: String): Boolean =
        (view is TextView && view.text.toString() == expected) ||
            (view is ViewGroup && (0 until view.childCount).any { hasText(view.getChildAt(it), expected) })
}
