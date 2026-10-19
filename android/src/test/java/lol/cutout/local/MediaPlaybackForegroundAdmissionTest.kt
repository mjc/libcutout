package lol.cutout.local

import android.app.Application
import android.os.Looper
import lol.cutout.MediaPlaybackForegroundAdmission
import lol.cutout.AndroidSoundCloudPresentation
import uniffi.cutout_mobile_ffi.MobileSoundCloudSnapshot
import uniffi.cutout_mobile_ffi.MobileSoundCloudState
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.time.Duration

/** Android startup receipts must precede native audio focus and cannot settle a replacement request. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class MediaPlaybackForegroundAdmissionTest {
    @Test
    fun serviceSubmissionDoesNotAdmitPlaybackUntilForegroundReceipt() {
        var submissions = 0
        val admission = MediaPlaybackForegroundAdmission(RuntimeEnvironment.getApplication()) { submissions++; true }
        val results = mutableListOf<Result<Unit>>()
        try {
            admission.request(results::add)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(1, submissions)
            assertTrue(admission.hasPendingNativePlayback)
            assertTrue(results.isEmpty())
            admission.foregroundReady()
            admission.foregroundReady()
            assertEquals(1, results.size)
            assertTrue(results.single().isSuccess)
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(8))
            assertTrue(admission.hasPendingNativePlayback)
        } finally { admission.close(); shadowOf(Looper.getMainLooper()).idle() }
    }

    @Test
    fun bufferingRetainsAdmittedServiceAndPauseReleasesIt() {
        val admission = MediaPlaybackForegroundAdmission(RuntimeEnvironment.getApplication()) { true }
        try {
            admission.request { check(it.isSuccess) }
            shadowOf(Looper.getMainLooper()).idle()
            admission.foregroundReady()
            val loading = AndroidSoundCloudPresentation(snapshot = MobileSoundCloudSnapshot(
                1uL, MobileSoundCloudState.LOADING, null, false, false))
            admission.playbackChanged(loading, active = false)
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(8))
            assertTrue(admission.hasPendingNativePlayback)
            admission.playbackChanged(loading.copy(snapshot = loading.snapshot.copy(state = MobileSoundCloudState.PAUSED)), active = false)
            assertFalse(admission.hasPendingNativePlayback)
        } finally { admission.close(); shadowOf(Looper.getMainLooper()).idle() }
    }

    @Test
    fun rejectedServiceStartFailsWithoutAudioAdmission() {
        val admission = MediaPlaybackForegroundAdmission(RuntimeEnvironment.getApplication()) { false }
        val results = mutableListOf<Result<Unit>>()
        try {
            admission.request(results::add)
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(results.single().isFailure)
            assertFalse(admission.hasPendingNativePlayback)
            admission.foregroundReady()
            assertEquals(1, results.size)
        } finally { admission.close(); shadowOf(Looper.getMainLooper()).idle() }
    }

    @Test
    fun replacementKeepsForegroundClaimAndOnlyNewRequestCanBeAdmitted() {
        val admission = MediaPlaybackForegroundAdmission(RuntimeEnvironment.getApplication()) { true }
        val claims = mutableListOf<Boolean>()
        val observation = admission.observe { claims += admission.hasPendingNativePlayback }
        val first = mutableListOf<Result<Unit>>()
        val second = mutableListOf<Result<Unit>>()
        try {
            admission.request(first::add)
            shadowOf(Looper.getMainLooper()).idle()
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(3))
            admission.request(second::add)
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(first.single().isFailure)
            assertEquals(listOf(true, true), claims)
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(2))
            assertTrue(second.isEmpty())
            admission.foregroundReady()
            assertTrue(second.single().isSuccess)
        } finally { observation.close(); admission.close(); shadowOf(Looper.getMainLooper()).idle() }
    }

    @Test
    fun missingReceiptTimesOutOnceAndLateReceiptCannotRevivePlayback() {
        val admission = MediaPlaybackForegroundAdmission(RuntimeEnvironment.getApplication()) { true }
        val results = mutableListOf<Result<Unit>>()
        try {
            admission.request(results::add)
            shadowOf(Looper.getMainLooper()).idle()
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(4))
            assertTrue(results.single().isFailure)
            assertFalse(admission.hasPendingNativePlayback)
            admission.foregroundReady()
            admission.foregroundFailed()
            assertEquals(1, results.size)
        } finally { admission.close(); shadowOf(Looper.getMainLooper()).idle() }
    }
}
