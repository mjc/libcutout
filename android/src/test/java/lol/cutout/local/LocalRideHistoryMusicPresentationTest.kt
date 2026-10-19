package lol.cutout.local

import lol.cutout.shouldShowRideMusicTimeline
import org.junit.Test
import uniffi.cutout_mobile_ffi.MobileMusicHistoryStateDto

class LocalRideHistoryMusicPresentationTest {
    @Test
    fun onlyHumanReadableOrUnclassifiedEventsArePresentedAsListeningHistory() {
        check(shouldShowRideMusicTimeline(MobileMusicHistoryStateDto.HUMAN_READABLE, 1))
        check(shouldShowRideMusicTimeline(null, 1))
        check(!shouldShowRideMusicTimeline(MobileMusicHistoryStateDto.HUMAN_READABLE, 0))
        check(!shouldShowRideMusicTimeline(MobileMusicHistoryStateDto.MISSING, 1))
        check(!shouldShowRideMusicTimeline(MobileMusicHistoryStateDto.DISABLED, 1))
        check(!shouldShowRideMusicTimeline(MobileMusicHistoryStateDto.REDACTED, 1))
        check(!shouldShowRideMusicTimeline(MobileMusicHistoryStateDto.DELETED, 1))
    }
}
