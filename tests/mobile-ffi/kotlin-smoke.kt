import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.MobileMonotonicMillisDto
import uniffi.cutout_mobile_ffi.MobileSessionInputDto
import uniffi.cutout_mobile_ffi.MobileSessionInputKindDto
import uniffi.cutout_mobile_ffi.MobileTransportWriteLimitDto
import uniffi.cutout_mobile_ffi.MobilePevcapCaptureCursorDto
import uniffi.cutout_mobile_ffi.MobileRideDatabaseException
import uniffi.cutout_mobile_ffi.openRideDatabase
import java.nio.file.Files

fun main() {
    checkCaptureHistoryBoundary()
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
        check(state.settings().connection.token == token)
    }
}

private fun checkCaptureHistoryBoundary() {
    val directory = Files.createTempDirectory("cutout-capture-history-smoke-")
    try {
        openRideDatabase(directory.resolve("ride.sqlite").toString()).use { database ->
            try {
                val page = database.listPevcapCaptures(null, 1U)
                check(page.captures.isEmpty())
                check(page.nextCursor == null)
                try {
                    database.listPevcapCaptures(null, 0U)
                    error("An unbounded capture query was accepted")
                } catch (_: MobileRideDatabaseException.InvalidQueryLimit) {
                    // Rust rejected the query before dispatching it.
                }
                try {
                    database.listPevcapCaptures(MobilePevcapCaptureCursorDto(0UL, "invalid"), 1U)
                    error("An invalid capture cursor was accepted")
                } catch (_: MobileRideDatabaseException.InvalidPevcapCaptureCursor) {
                    // Generated bindings preserve the typed cursor error.
                }
            } finally {
                database.shutdown()
            }
        }
    } finally {
        check(directory.toFile().deleteRecursively())
    }
}
