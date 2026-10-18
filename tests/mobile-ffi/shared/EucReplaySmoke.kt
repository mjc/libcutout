package lol.cutout.smoke

import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.MobileCaptureNotificationEvidenceDto
import uniffi.cutout_mobile_ffi.MobileNotificationIngestOutcomeKindDto
import uniffi.cutout_mobile_ffi.MobileSessionInputKindDto
import uniffi.cutout_mobile_ffi.MobileSessionOutputKindDto
import uniffi.cutout_mobile_ffi.MobileTelemetrySnapshotDto
import uniffi.cutout_mobile_ffi.MobileVehicleKindDto

/** Executable Kotlin usage: one ordered worker owns a generic, Rust-selected PEV session. */
fun checkEucCaptureReplay() {
    val worker = Executors.newSingleThreadExecutor()
    try {
        val snapshot = worker.submit<MobileTelemetrySnapshotDto> {
            val inputs = eucReplayInputs()
            val notifications = inputs.filter { it.kind == MobileSessionInputKindDto.NOTIFICATION }
            check(notifications.size == 13)
            CutoutSessionStateHandle().use { state ->
                val token = checkNotNull(state.beginConnectionAttempt("replay-device", 0UL).token)
                state.connectionLinkEstablished(token)

                // Identification pass: the capture's model label does not select the decoder.
                for (notification in notifications) {
                    state.observeConnectionNotification(token, notification.bytes.copyOf())
                }
                val session = state.resolveDeviceSession(token, true, 1UL)
                check(session.identity?.vehicleKind == MobileVehicleKindDto.ELECTRIC_UNICYCLE)
                check(session.identity?.model == "NOSFET Aero")

                var semantic = 0
                var buffered = 0
                var stationary = 0
                var subscribed = false
                var latest: MobileTelemetrySnapshotDto? = null
                // Replay pass: preserve the captured timestamps, channels and chunk boundaries.
                for (input in inputs) {
                    val step = checkNotNull(state.ingestDeviceSession(token, input))
                    check(step.session.connection.token == token)
                    check(step.result.error == null)
                    // Each result owns this input's ordered outputs. Consume each exactly once;
                    // replay observes transport requests but never executes them on hardware.
                    for (output in step.result.outputs) {
                        when (output.ingest?.kind) {
                            MobileNotificationIngestOutcomeKindDto.SEMANTIC_EVENTS -> semantic++
                            MobileNotificationIngestOutcomeKindDto.BUFFERED_FRAGMENT -> buffered++
                            else -> Unit
                        }
                        if (output.kind == MobileSessionOutputKindDto.SUBSCRIBE) subscribed = true
                    }
                    if (step.captureNotificationEvidence ==
                        MobileCaptureNotificationEvidenceDto.STATIONARY_TELEMETRY) stationary++
                    latest = step.telemetry
                }
                check(subscribed) { "Resolved EUC session did not request a subscription" }
                check(semantic > 0 && buffered > 0) {
                    "Capture did not exercise both complete and fragmented parsing: $semantic/$buffered"
                }
                check(stationary == 0) {
                    "Fragmented notifications must retain every chunk, including at zero speed"
                }
                val owned = checkNotNull(latest)
                check(checkNotNull(owned.voltage).value.value > 0)
                check(owned.speed != null)

                val oldNotification = notifications.first()
                state.disconnectConnectionAttempt()
                val replacement = checkNotNull(
                    state.beginConnectionAttempt("replay-device", 2_000UL).token,
                )
                state.connectionLinkEstablished(replacement)
                check(replacement != token)
                // A callback retains the attempt captured when the native connection began.
                check(state.observeConnectionNotification(token, oldNotification.bytes) == null)
                check(state.ingestDeviceSession(token, oldNotification) == null)
                check(state.deviceSessionSnapshot().identity == null)
                check(state.connectionAttemptIsCurrent(replacement))
                state.disconnectConnectionAttempt()
                owned
            }
        }.get(30, TimeUnit.SECONDS)
        // The DTO survives native handle disposal and the handoff back to the caller.
        check(checkNotNull(snapshot.voltage).value.value > 0)
        check(snapshot.speed != null)
    } finally {
        worker.shutdownNow()
        check(worker.awaitTermination(5, TimeUnit.SECONDS))
    }
}
