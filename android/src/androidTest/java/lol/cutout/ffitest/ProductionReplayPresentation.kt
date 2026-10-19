package lol.cutout.ffitest

import lol.cutout.BlePresentation
import lol.cutout.NativePhase
import lol.cutout.PevSession
import lol.cutout.smoke.eucReplayGatt
import lol.cutout.smoke.eucReplayInputs
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.MobileSessionInputKindDto

/** The existing public capture through the production adapter; no Bluetooth effects execute. */
internal fun productionReplayPresentation(): BlePresentation = CutoutSessionStateHandle().use { state ->
    var clock = 0UL
    val token = checkNotNull(state.beginConnectionAttempt("ui-replay", clock).token)
    state.connectionLinkEstablished(token)
    state.observeConnectionGatt(token, eucReplayGatt())
    var latest = BlePresentation()
    val session = PevSession(state, token, { clock }, {}) { snapshot, telemetry, settings ->
        latest = BlePresentation(phase = NativePhase.IDENTIFYING, session = snapshot,
            telemetry = telemetry, settings = settings)
    }
    for (input in eucReplayInputs().filter { it.kind == MobileSessionInputKindDto.NOTIFICATION }) {
        clock = input.monotonicMs.milliseconds
        session.notification(input.channel, input.bytes)
    }
    check(latest.telemetry?.voltage != null)
    val identity = checkNotNull(latest.session?.identity)
    val candidate = state.connectionAdmissionCandidate(token.platformIdentifier, identity.model ?: "Replay device", false)
    check(candidate.connectionRoute == uniffi.cutout_mobile_ffi.DiscoveryConnectionRoute.ELECTRIC_UNICYCLE)
    latest.copy(connectionCandidate = candidate)
}
