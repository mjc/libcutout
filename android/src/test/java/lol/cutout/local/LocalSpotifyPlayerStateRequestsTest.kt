package lol.cutout.local

import lol.cutout.AndroidSpotifyPlayerStateRequests
import lol.cutout.spotifyPlaybackTransport
import uniffi.cutout_mobile_ffi.MobileMusicProviderLifecycle
import uniffi.cutout_mobile_ffi.MobileMusicPlaybackTransport
import uniffi.cutout_mobile_ffi.MobileMusicRequestCompletion
import uniffi.cutout_mobile_ffi.MobileMusicRequestExpiration
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalSpotifyPlayerStateRequestsTest {
    @Test fun productionSpotifyTransportPrefersAnAvailableOfflineSession() {
        assertEquals(MobileMusicPlaybackTransport.APP_REMOTE, spotifyPlaybackTransport(true, false))
        assertEquals(MobileMusicPlaybackTransport.APP_REMOTE, spotifyPlaybackTransport(true, true))
        assertEquals(MobileMusicPlaybackTransport.WEB_API, spotifyPlaybackTransport(false, true))
        assertEquals(MobileMusicPlaybackTransport.UNAVAILABLE, spotifyPlaybackTransport(false, false))
    }

    @Test fun rustObservationSupersedesAnOutstandingWebPoll() {
        val lifecycle = MobileMusicProviderLifecycle()
        val requests = AndroidSpotifyPlayerStateRequests(lifecycle)
        try {
            val ticket = requireNotNull(requests.begin(10_000uL))

            requests.markObserved(10_100uL)

            assertEquals(MobileMusicRequestCompletion.STALE, requests.complete(ticket, 10_200uL))
            assertFalse("the superseded request must no longer own the native in-flight slot", requests.isCurrent(ticket))
        } finally {
            lifecycle.close()
        }
    }

    @Test fun lateExpiredPollCannotClearItsReplacement() {
        val lifecycle = MobileMusicProviderLifecycle()
        val requests = AndroidSpotifyPlayerStateRequests(lifecycle)
        try {
            val old = requireNotNull(requests.begin(10_000uL))
            assertEquals(MobileMusicRequestExpiration.EXPIRED, requests.expire(old, 100_000uL))
            val current = requireNotNull(requests.begin(100_001uL))

            assertEquals(MobileMusicRequestCompletion.STALE, requests.complete(old, 100_002uL))
            assertTrue("late completion must not retire the replacement ticket", requests.isCurrent(current))
        } finally {
            lifecycle.close()
        }
    }
}
