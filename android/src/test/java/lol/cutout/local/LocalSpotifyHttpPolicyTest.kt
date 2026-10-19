package lol.cutout.local

import uniffi.cutout_mobile_ffi.MobileMusicCommandDto
import uniffi.cutout_mobile_ffi.MobileSpotifyHttpSession
import uniffi.cutout_mobile_ffi.MobileSpotifyPlaybackException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Exercises the real shared Rust policy through the generated Android FFI. */
class LocalSpotifyHttpPolicyTest {
    @Test
    fun playbackAndCommandsUseRustFixedOriginAndHonorRetryAfterAcrossEffects() {
        MobileSpotifyHttpSession().use { session ->
            val playback = session.playbackRequest(10UL)
            assertEquals("GET", playback.method)
            assertEquals("https://api.spotify.com/v1/me/player?additional_types=track%2Cepisode", playback.url)

            val limited = try {
                session.receiveResponse(429.toUShort(), "2", 0UL, 20UL)
                error("429 should record a Rust-owned retry deadline")
            } catch (error: MobileSpotifyPlaybackException.RateLimited) {
                error
            }
            assertEquals(2_000UL, limited.retryAfterMilliseconds)
            val blocked = try {
                session.commandRequest(MobileMusicCommandDto.NEXT, "device/one", 21UL)
                error("active retry deadline should block command admission")
            } catch (error: MobileSpotifyPlaybackException.RateLimited) {
                error
            }
            assertEquals(1_999UL, blocked.retryAfterMilliseconds)

            val command = session.commandRequest(MobileMusicCommandDto.NEXT, "device/one", 2_020UL)
            assertEquals("POST", command.method)
            assertTrue(command.url.startsWith("https://api.spotify.com/v1/me/player/next?device_id="))
            assertTrue(command.url.endsWith("device%2Fone"))
            assertFalse(session.receiveResponse(204.toUShort(), null, 0UL, 2_030UL))
            try {
                session.receiveResponse(401.toUShort(), null, 0UL, 2_040UL)
                error("401 must be returned as a typed authorization failure")
            } catch (_: MobileSpotifyPlaybackException.Unauthorized) {
                // Native code may refresh its stored AppAuth grant once.
            }
        }
    }
}
