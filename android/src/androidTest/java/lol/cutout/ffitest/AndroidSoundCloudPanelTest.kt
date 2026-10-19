package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidSoundCloudPanel
import lol.cutout.AndroidSoundCloudPlayer
import lol.cutout.AndroidSoundCloudPresentation
import lol.cutout.CutoutUi
import uniffi.cutout_mobile_ffi.MobileSoundCloudSnapshot
import uniffi.cutout_mobile_ffi.MobileSoundCloudState
import uniffi.cutout_mobile_ffi.MobileSoundCloudTrack

/** Exercises the real SoundCloud panel with typed Rust player projections and no network/audio. */
open class AndroidSoundCloudPanelTest {
    @org.junit.Test
    fun panelShowsAdmittedTrackAttributionTimeAndQueueCapabilities() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val player = AndroidSoundCloudPlayer(context)
        try {
            instrumentation.runOnMainSync {
                val panel = AndroidSoundCloudPanel(context, player, CutoutUi(context))
                val track = MobileSoundCloudTrack(
                    urn = "soundcloud:tracks:42",
                    title = "Riding Song",
                    uploader = "Artist Name",
                    permalink = "https://soundcloud.com/artist/riding-song",
                    durationMs = 180_000uL,
                )
                panel.render(AndroidSoundCloudPresentation(
                    tracks = listOf(track),
                    snapshot = MobileSoundCloudSnapshot(
                        playbackId = 3uL,
                        state = MobileSoundCloudState.PLAYING,
                        track = track,
                        hasPrevious = true,
                        hasNext = false,
                    ),
                    elapsedSeconds = 91,
                ))
                val text = descendants(panel).filterIsInstance<TextView>().map { it.text.toString() }.toList()
                check(text.contains("Riding Song") && text.contains("Artist Name"))
                check(text.contains(context.getString(lol.cutout.R.string.music_soundcloud_elapsed, 1, 31)))
                check(panel.findViewWithTag<View>("music.soundcloud.track-link")?.visibility == View.VISIBLE)
                check(panel.findViewWithTag<View>("music.soundcloud.command.previous")?.isEnabled == true)
                check(panel.findViewWithTag<View>("music.soundcloud.command.next")?.isEnabled == false)
                check(panel.findViewWithTag<View>("music.soundcloud.command.pause") != null)
                capturePanel(panel)
                panel.dispose()
            }
        } finally {
            player.close()
        }
    }

    @org.junit.Test
    fun panelOmitsLinksForNonCanonicalTrackOrigins() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val player = AndroidSoundCloudPlayer(context)
        try {
            instrumentation.runOnMainSync {
                val panel = AndroidSoundCloudPanel(context, player)
                listOf("https://evil.example/track", "https://evil.soundcloud.com/track").forEachIndexed { index, link ->
                    val track = MobileSoundCloudTrack(
                        "soundcloud:tracks:${42 + index}", "Track", "Uploader", link, 10_000uL,
                    )
                    panel.render(AndroidSoundCloudPresentation(
                        snapshot = MobileSoundCloudSnapshot((index + 1).toULong(), MobileSoundCloudState.PAUSED, track, false, false),
                    ))
                    check(panel.findViewWithTag<View>("music.soundcloud.track-link")?.visibility == View.GONE)
                }
                panel.dispose()
            }
        } finally {
            player.close()
        }
    }

    protected open fun capturePanel(panel: View) = Unit

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }
}
