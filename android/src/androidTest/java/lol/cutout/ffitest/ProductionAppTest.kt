package lol.cutout.ffitest

import android.app.Activity
import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.CutoutApplication
import lol.cutout.CutoutUi
import lol.cutout.MainActivity
import lol.cutout.NativePhase
import lol.cutout.NearbyPev
import lol.cutout.R
import lol.cutout.AndroidRideSessionEffects
import lol.cutout.wireBytes
import java.util.UUID
import uniffi.cutout_mobile_ffi.MobileActivityProjectionStateDto
import uniffi.cutout_mobile_ffi.MobileConnectionReadinessDto
import uniffi.cutout_mobile_ffi.MobileProtocolFamilyDto
import uniffi.cutout_mobile_ffi.MobileDeviceIdentityDto
import uniffi.cutout_mobile_ffi.MobileRideSessionEndReasonDto
import uniffi.cutout_mobile_ffi.MobileRideSessionAppPresenceDto
import uniffi.cutout_mobile_ffi.MobileRideSessionPhaseDto
import uniffi.cutout_mobile_ffi.MobileRideSessionSnapshotDto
import uniffi.cutout_mobile_ffi.MobileVehicleKindDto
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.DiscoveryServiceUuid
import uniffi.cutout_mobile_ffi.MobileRideSessionInputDto
import uniffi.cutout_mobile_ffi.mobileDiscoveryCandidateFromAdvertisement
import org.junit.Test

open class ProductionAppTest {
    @Test
    fun launcherOpensDevicePickerAndGeneralSetupWithoutAConnectedPev() = withActivity { activity ->
        val context = activity
        check(visibleText(activity, context.getString(R.string.picker_title)))
        clickDescription(activity, context.getString(R.string.more))
        clickDescription(activity, context.getString(R.string.setup))
        check(visibleText(activity, context.getString(R.string.setup_ride_autostart)))
        clickDescription(activity, context.getString(R.string.devices))
        check(visibleText(activity, context.getString(R.string.picker_title)))
    }

    @Test
    fun rideRendersRustCaptureAndRetainsInstrumentsAcrossTelemetryUpdates() {
        val replay = productionReplayPresentation()
        withActivity { activity ->
            activity.render(replay)
            clickDescription(activity, activity.getString(R.string.configuration))
            check(visibleText(activity, CutoutUi(activity).label("controls.tune.title")))
            clickDescription(activity, activity.getString(R.string.ride))
            val speed = checkNotNull(activity.window.decorView.findViewWithTag<TextView>("ride.hero.speed"))
            val before = speed.text.toString()
            check(before.isNotBlank() && before != activity.getString(R.string.unavailable))
            check(visibleText(activity, checkNotNull(replay.session?.identity?.model)))
            activity.render(replay.copy())
            check(activity.window.decorView.findViewWithTag<TextView>("ride.hero.speed") === speed)
            check(speed.text.toString() == before)
            check(activity.window.decorView.findViewWithTag<View>("euc.ride.headroom") != null)
        }
    }

    @Test
    fun rideNavigationRequiresVerifiedIdentityAndReturnsToPickerWhenVerificationIsLost() {
        val replay = productionReplayPresentation()
        val originalSession = checkNotNull(replay.session)
        withActivity { activity ->
            val pickerTitle = activity.getString(R.string.picker_title)
            for (readiness in listOf(
                MobileConnectionReadinessDto.PENDING,
                MobileConnectionReadinessDto.RECORD_ONLY,
                MobileConnectionReadinessDto.FAILED,
                MobileConnectionReadinessDto.CONFLICTED,
            )) {
                activity.render(replay.copy(
                    phase = NativePhase.IDENTIFYING,
                    session = originalSession.copy(
                        connection = originalSession.connection.copy(readiness = readiness),
                    ),
                ))
                check(visibleText(activity, pickerTitle)) { "$readiness exposed the connected Ride route" }
                check(children(activity.window.decorView).none { it.tag == "ride.hero.speed" && it.isShown }) {
                    "$readiness rendered a connected Ride instrument"
                }
            }

            activity.render(replay)
            check(!visibleText(activity, pickerTitle)) { "Verified Rust identity did not open Ride" }
            check(children(activity.window.decorView).any { it.tag == "ride.hero.speed" && it.isShown })

            val failed = replay.copy(
                session = originalSession.copy(
                    connection = originalSession.connection.copy(readiness = MobileConnectionReadinessDto.FAILED),
                ),
            )
            activity.render(failed)
            check(visibleText(activity, pickerTitle)) { "Lost Rust verification did not return to Devices" }
        }
    }

    @Test
    fun verifiedRustReplayRouteSurvivesRestoreWithoutDiscoveryRows() {
        val restored = productionReplayPresentation().copy(devices = emptyList())
        check(restored.connectionCandidate?.connectionRoute != null)
        withActivity { activity ->
            activity.render(restored)
            check(children(activity.window.decorView).any { it.tag == "ride.hero.speed" && it.isShown }) {
                "A verified session with Rust's retained route projection stayed on the picker after restore"
            }
            check(!visibleText(activity, activity.getString(R.string.picker_title)))
        }
    }

    @Test
    fun staleRustRideLifecycleRemovesHeadroomWithoutAddingAnotherRideSurface() {
        val replay = productionReplayPresentation()
        val ride = replay.copy(rideSession = MobileRideSessionSnapshotDto(
            identity = null,
            phase = MobileRideSessionPhaseDto.Stale,
            activity = MobileActivityProjectionStateDto.Absent,
            lastTelemetryAtMs = 1_000UL,
            staleAfterMs = 2_000UL,
            appPresence = MobileRideSessionAppPresenceDto.FOREGROUND,
        ))
        withActivity { activity ->
            activity.render(ride)
            val gauge = children(activity.window.decorView).single { it.tag == "euc.ride.headroom" && it.isShown }
            check(gauge.contentDescription.toString().endsWith(activity.getString(R.string.unavailable))) {
                "Stale Rust session retained a trustworthy headroom value: ${gauge.contentDescription}"
            }
            check(children(activity.window.decorView).none { it.tag == "ride.telemetry.stale" }) {
                "Staleness added an extra warning surface instead of updating the existing instrument"
            }
        }
    }

    @Test
    fun verifiedGenericVescWithRustCandidateMissingRouteStaysOnDevicePicker() {
        val replay = productionReplayPresentation()
        val session = checkNotNull(replay.session)
        val token = checkNotNull(session.connection.token)
        val candidate = mobileDiscoveryCandidateFromAdvertisement(
            token.platformIdentifier,
            "Generic VESC",
            listOf(DiscoveryServiceUuid(UUID.fromString("6e400001-b5a3-f393-e0a9-e50e24dcca9e").wireBytes())),
        )
        check(candidate.connectionRoute == null) { "Rust candidate unexpectedly assigned a VESC/Onewheel route" }
        val verifiedGenericVesc = replay.copy(
            connectionCandidate = candidate,
            devices = listOf(NearbyPev(
                id = token.platformIdentifier,
                name = candidate.displayName,
                rssi = -50,
                suggested = candidate.isPickerCandidate,
                candidate = candidate,
            )),
            session = session.copy(
                connection = session.connection.copy(readiness = MobileConnectionReadinessDto.VERIFIED),
                identity = MobileDeviceIdentityDto(
                    protocol = MobileProtocolFamilyDto.VESC,
                    vehicleKind = MobileVehicleKindDto.BOARD,
                    model = null,
                ),
            ),
        )
        withActivity { activity ->
            activity.render(verifiedGenericVesc)
            check(visibleText(activity, activity.getString(R.string.picker_title))) {
                "Verified VESC protocol without a Rust route exposed Ride"
            }
            check(children(activity.window.decorView).none { it.tag == "ride.hero.speed" && it.isShown })
        }
    }

    @Test
    fun productionRideLifecycleAdapterUsesRustFreshnessBoundaryAndTerminalReconnectReason() {
        val state = CutoutSessionStateHandle()
        try {
            val started = state.reduceRideSession(MobileRideSessionInputDto.Start("lifecycle-ui-test"))
            val identity = checkNotNull(started.snapshot.identity)
            state.reduceRideSession(MobileRideSessionInputDto.ActivityUnavailable(identity))
            state.reduceRideSession(MobileRideSessionInputDto.TelemetryObserved(1_000UL))

            check(AndroidRideSessionEffects.freshnessChecked(state, 2_999UL).phase == MobileRideSessionPhaseDto.Active) {
                "Rust freshness adapter marked the ride stale before its 2 second deadline"
            }
            check(AndroidRideSessionEffects.freshnessChecked(state, 3_000UL).phase == MobileRideSessionPhaseDto.Stale) {
                "Rust freshness adapter did not mark the ride stale at its deadline"
            }
            val ended = AndroidRideSessionEffects.reconnectExhausted(state)
            check(ended.phase == MobileRideSessionPhaseDto.Ended(MobileRideSessionEndReasonDto.RECONNECT_EXHAUSTED)) {
                "Rust retry exhaustion did not end the logical ride: ${ended.phase}"
            }
        } finally {
            state.close()
        }
    }

    private fun withActivity(block: (MainActivity) -> Unit) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        try {
            // Keep native discovery callbacks out of the deterministic presentation fixture.
            (activity.application as CutoutApplication).bluetooth.observe(null)
            (activity.application as CutoutApplication).bluetooth.stopScan()
            instrumentation.runOnMainSync { block(activity) }
        } finally { instrumentation.runOnMainSync { activity.finish() } }
    }

    private fun children(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(children(view.getChildAt(index)))
    }
    private fun clickDescription(activity: Activity, text: String) {
        val target = children(activity.window.decorView).single {
            it.isShown && it.isClickable && it.contentDescription?.toString() == text
        }
        check(target.performClick())
    }
    private fun visibleText(activity: Activity, text: String): Boolean =
        children(activity.window.decorView).filterIsInstance<TextView>().any { it.isShown && it.text.toString() == text }
}
