package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.graphics.Rect
import android.widget.FrameLayout
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidMusicClient
import lol.cutout.AndroidMusicPresentation
import lol.cutout.CutoutApplication
import lol.cutout.CutoutUi
import lol.cutout.availableCommands
import lol.cutout.showsCompactPlayer
import lol.cutout.MusicPanel
import lol.cutout.MusicCompactPlayer
import uniffi.cutout_mobile_ffi.*

/** Exercises the production compact player against Rust-shaped music presentation DTOs. */
open class MusicCompactPlayerTest {
    @org.junit.Test
    fun compactPlayerFitsSwiftHeightAndKeepsLongMetadataOutOfTransportTargets() = withPlayer { _, ui, _, player ->
        val presentation = presentation(
            state = MobileMusicPlaybackStateDto.PLAYING,
            capabilities = MobileMusicCapabilitiesDto(previous = false, play = false, pause = true, next = true, openProvider = false),
        )
        player.render(presentation)
        val width = ui.dp(393)
        val height = ui.dp(44)
        val host = FrameLayout(player.context)
        host.addView(player, FrameLayout.LayoutParams(width, height))
        host.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
        )
        host.layout(0, 0, width, height)

        check(player.height == height) { "Compact player should retain the Swift 44dp row height" }
        val expandTrack = checkNotNull(player.findViewWithTag<View>("music.expand"))
        val title = checkNotNull(expandTrack.findViewWithTag<TextView>("music.now-playing-title"))
        check(title.maxLines == 1 && title.ellipsize == android.text.TextUtils.TruncateAt.END)
        val metadataContainer = title.parent as ViewGroup
        val metadata = (0 until metadataContainer.childCount).map { metadataContainer.getChildAt(it) as TextView }
        val artist = metadata.single { it !== title }
        check(artist.text.isNotBlank()) { "The long podcast artist should remain present in the compact metadata stack" }
        check(metadata.all { it.maxLines == 1 && it.ellipsize == android.text.TextUtils.TruncateAt.END })
        check(title.layout.getEllipsisCount(0) > 0 && artist.layout.getEllipsisCount(0) > 0) {
            "Long title and artist should truncate within the compact metadata column"
        }
        val transport = descendants(player).filter { it.tag?.toString()?.startsWith("music.command.") == true }
        check(transport.map { it.tag.toString() }.toSet() == setOf("music.command.pause", "music.command.next"))
        val pauseButton = player.findViewWithTag<View>("music.command.pause")
        player.render(presentation.copy(snapshot = presentation.snapshot?.copy(positionMilliseconds = 73_000uL)))
        check(player.findViewWithTag<View>("music.command.pause") === pauseButton) {
            "A position tick must update the existing transport view instead of replacing its accessibility target"
        }
        transport.forEach { target ->
            check(target.measuredWidth >= ui.dp(44) && target.measuredHeight >= ui.dp(44)) {
                "${target.tag} measured ${target.measuredWidth}×${target.measuredHeight}px; require a 44dp touch target"
            }
        }
        fun rectInHost(view: View) = Rect(0, 0, view.width, view.height).apply {
            host.offsetDescendantRectToMyCoords(view, this)
        }
        val metadataBounds = rectInHost(metadataContainer)
        val firstTransportBounds = rectInHost(transport.minBy { rectInHost(it).left })
        check(metadataBounds.right <= firstTransportBounds.left) {
            "Metadata ends at ${metadataBounds.right}px while transport begins at ${firstTransportBounds.left}px in the $width×$height host"
        }
    }

    @org.junit.Test
    fun compactPlayerOnlyShowsCommandsGrantedByCurrentCapabilities() = withPlayer { _, _, _, player ->
        player.render(presentation(
            state = MobileMusicPlaybackStateDto.PAUSED,
            capabilities = MobileMusicCapabilitiesDto(previous = false, play = true, pause = false, next = false, openProvider = true),
        ))
        check(player.findViewWithTag<View>("music.command.play") != null)
        check(player.findViewWithTag<View>("music.command.pause") == null)
        check(player.findViewWithTag<View>("music.command.next") == null)
        check(player.findViewWithTag<View>("music.command.open_provider") == null)

        player.render(presentation(
            state = MobileMusicPlaybackStateDto.UNAVAILABLE,
            capabilities = MobileMusicCapabilitiesDto(previous = false, play = false, pause = false, next = false, openProvider = true),
        ))
        check(player.findViewWithTag<View>("music.command.open_provider") != null)
        check(player.findViewWithTag<View>("music.command.play")?.visibility != View.VISIBLE)
        player.render(AndroidMusicPresentation(snapshot = presentation().snapshot, listenerAccessGranted = false))
        check(descendants(player).none { it.visibility == View.VISIBLE && it.tag?.toString()?.startsWith("music.command.") == true })
    }

    @org.junit.Test
    fun compactPlayerExpandCallbackRunsWithoutAndroidOnlyDismissControl() = withPlayer { context, _, client, _ ->
        var expanded = 0
        val callbacks = MusicCompactPlayer(context, client, onOpenDetails = { expanded++ })
        callbacks.render(presentation())
        check(callbacks.findViewWithTag<View>("music.expand").performClick())
        check(descendants(callbacks).none { it.contentDescription == context.getString(lol.cutout.R.string.music_hide) })
        check(expanded == 1)
    }

    @org.junit.Test
    fun compactVisibilityMatchesSwiftTransientAndStoppedItemRules() {
        fun state(state: MobileMusicPlaybackStateDto, hasItem: Boolean, canPlay: Boolean = false) = AndroidMusicPresentation(
            snapshot = MobileMusicSnapshotDto(
                provider = MobileMusicProviderDto.SPOTIFY,
                sessionId = "visibility-test",
                state = state,
                item = if (hasItem) MobileMusicItemDto("item", "Title", "Artist") else null,
                positionMilliseconds = 0uL,
                durationMilliseconds = null,
                observedAtMs = 0uL,
                capabilities = MobileMusicCapabilitiesDto(false, canPlay, false, false, false),
            ),
        )
        check(state(MobileMusicPlaybackStateDto.PLAYING, false).showsCompactPlayer())
        check(state(MobileMusicPlaybackStateDto.BUFFERING, false).showsCompactPlayer())
        check(state(MobileMusicPlaybackStateDto.STALE, true).showsCompactPlayer())
        check(!state(MobileMusicPlaybackStateDto.STALE, false).showsCompactPlayer())
        check(state(MobileMusicPlaybackStateDto.UNAVAILABLE, true, canPlay = true).showsCompactPlayer())
        check(!state(MobileMusicPlaybackStateDto.UNAVAILABLE, true, canPlay = false).showsCompactPlayer())
        check(!state(MobileMusicPlaybackStateDto.UNAUTHORIZED, true, canPlay = true).showsCompactPlayer())
    }

    @org.junit.Test
    fun transportCommandsFollowSwiftPlaybackStateGatesInsteadOfCapabilitiesAlone() {
        fun commands(state: MobileMusicPlaybackStateDto, caps: MobileMusicCapabilitiesDto) = AndroidMusicPresentation(
            snapshot = presentation().snapshot?.copy(state = state, capabilities = caps),
            listenerAccessGranted = true,
        ).availableCommands(includePrevious = true, includeOpenProvider = false).toSet()
        val all = MobileMusicCapabilitiesDto(previous = true, play = true, pause = true, next = true, openProvider = true)
        check(commands(MobileMusicPlaybackStateDto.BUFFERING, all) == setOf(MobileMusicCommandDto.PAUSE))
        check(commands(MobileMusicPlaybackStateDto.STALE, all) == setOf(MobileMusicCommandDto.PLAY))
        check(commands(MobileMusicPlaybackStateDto.STOPPED, all) == setOf(MobileMusicCommandDto.PLAY))
        check(commands(MobileMusicPlaybackStateDto.PLAYING, all) == setOf(
            MobileMusicCommandDto.PREVIOUS, MobileMusicCommandDto.PAUSE, MobileMusicCommandDto.NEXT,
        ))
        check(commands(MobileMusicPlaybackStateDto.UNAUTHORIZED, all).isEmpty())
        check(commands(MobileMusicPlaybackStateDto.INTERRUPTED, all).isEmpty())
    }

    @org.junit.Test
    fun playPauseTransitionsKeepTransportOrderAndExistingSkipTargets() = withPlayer { context, _, client, compact ->
        val expanded = MusicPanel(context, client, onOpenSettings = {}, onHidePlayer = {}, onDone = {})
        try {
            val caps = MobileMusicCapabilitiesDto(previous = true, play = true, pause = true, next = true, openProvider = true)
            fun visibleCommands(view: View) = descendants(view).filter {
                it.visibility == View.VISIBLE && it.tag?.toString()?.startsWith("music.command.") == true
            }.map { it.tag.toString().removePrefix("music.command.") }.toList()
            val playing = presentation(state = MobileMusicPlaybackStateDto.PLAYING, capabilities = caps)
            compact.render(playing); expanded.render(playing)
            val compactNext = compact.findViewWithTag<View>("music.command.next")
            val expandedNext = expanded.findViewWithTag<View>("music.command.next")
            listOf(MobileMusicPlaybackStateDto.PAUSED, MobileMusicPlaybackStateDto.PLAYING).forEach { state ->
                val value = playing.copy(snapshot = playing.snapshot?.copy(state = state))
                compact.render(value); expanded.render(value)
                val middle = if (state == MobileMusicPlaybackStateDto.PAUSED) "play" else "pause"
                check(visibleCommands(compact) == listOf(middle, "next"))
                check(visibleCommands(expanded) == listOf("previous", middle, "next"))
                check(compact.findViewWithTag<View>("music.command.next") === compactNext)
                check(expanded.findViewWithTag<View>("music.command.next") === expandedNext)
            }
        } finally { expanded.dispose() }
    }

    @org.junit.Test
    fun compactPlayerShowsOnlyRustValidatedSpotifyLinks() = withPlayer { _, _, _, player ->
        val base = presentation()
        val valid = base.copy(snapshot = base.snapshot?.copy(
            provider = MobileMusicProviderDto.SPOTIFY,
            item = base.snapshot?.item?.copy(identifier = "spotify:episode:512ojhOuo1ktJprKbVcKyQ"),
        ))
        player.render(valid)
        val link = checkNotNull(player.findViewWithTag<View>("music.spotify-content-link"))
        check(link.visibility == View.VISIBLE) { "A Rust-approved Spotify episode URI should expose its content link" }

        val invalid = valid.copy(snapshot = valid.snapshot?.copy(
            item = valid.snapshot?.item?.copy(identifier = "https://evil.example/track"),
        ))
        player.render(invalid)
        check(link.visibility == View.GONE) { "An arbitrary URL must not become a Spotify content link" }
    }

    private fun presentation(
        state: MobileMusicPlaybackStateDto = MobileMusicPlaybackStateDto.PLAYING,
        capabilities: MobileMusicCapabilitiesDto = MobileMusicCapabilitiesDto(false, false, false, false, false),
    ) = AndroidMusicPresentation(
        snapshot = MobileMusicSnapshotDto(
            provider = MobileMusicProviderDto.SPOTIFY,
            sessionId = "presentation-test-session",
            state = state,
            item = MobileMusicItemDto(
                identifier = "podcast-episode-42",
                title = "An unusually long podcast episode title about devices, road trips, and the people who ride them",
                artist = "A very long podcast creator and show name that should be truncated before it can collide with transport controls",
            ),
            positionMilliseconds = 72_000uL,
            durationMilliseconds = 3_600_000uL,
            observedAtMs = 1uL,
            capabilities = capabilities,
        ),
        listenerAccessGranted = true,
    )

    private fun withPlayer(block: (android.content.Context, CutoutUi, AndroidMusicClient, MusicCompactPlayer) -> Unit) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val app = context.applicationContext as CutoutApplication
        val lifecycle = MobileMusicProviderLifecycle()
        val client = AndroidMusicClient(context, lifecycle, app.rides)
        lateinit var player: MusicCompactPlayer
        try {
            instrumentation.runOnMainSync {
                player = MusicCompactPlayer(context, client, CutoutUi(context))
                block(context, CutoutUi(context), client, player)
            }
        } finally {
            client.close()
        }
    }

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }
}
