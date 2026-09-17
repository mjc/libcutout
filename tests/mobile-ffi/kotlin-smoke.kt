import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.MobileMonotonicMillisDto
import uniffi.cutout_mobile_ffi.MobileSessionInputDto
import uniffi.cutout_mobile_ffi.MobileSessionInputKindDto
import uniffi.cutout_mobile_ffi.MobileTransportWriteLimitDto

fun main() {
    CutoutSessionStateHandle().use { state ->
        val attempt = state.beginConnectionAttempt("kotlin-smoke", 0UL)
        val token = checkNotNull(attempt.token)
        state.connectionLinkEstablished(token)
        val reply = listOf(
            2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115,
            104, 0, 38, 208, 3,
        ).map { it.toByte() }.toByteArray()
        state.observeConnectionNotification(token, reply)
        state.resolveDeviceSession(token, false, 1UL)

        val step = checkNotNull(
            state.ingestDeviceSession(
                token,
                MobileSessionInputDto(
                    MobileSessionInputKindDto.LINK_UP,
                    MobileMonotonicMillisDto(1UL),
                    MobileTransportWriteLimitDto(185U),
                    ByteArray(0),
                    ByteArray(0),
                ),
            ),
        )
        check(step.session.connection.token == token)
        check(state.settingsDescriptors().connection.token == token)
        check(state.settingsSnapshot().connection.token == token)
        check(state.deviceControlsSnapshot().connection.token == token)
    }
}
