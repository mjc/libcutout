package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidMusicClient
import lol.cutout.AndroidMusicPresentation
import lol.cutout.CutoutApplication
import lol.cutout.CutoutUi
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
        val height = ui.dp(52)
        val host = FrameLayout(player.context)
        host.addView(player, FrameLayout.LayoutParams(width, height))
        host.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
        )
        host.layout(0, 0, width, height)

        check(player.height == height) { "Compact player should retain the Swift 52dp row height" }
        val title = checkNotNull(player.findViewWithTag<TextView>("music.now-playing-title"))
        check(title.maxLines == 1 && title.ellipsize == android.text.TextUtils.TruncateAt.END)
        val metadata = descendants(player).filterIsInstance<TextView>().filter { it !== title && it.text.isNotBlank() }
        check(metadata.isNotEmpty()) { "The long podcast artist should remain present in the compact metadata stack" }
        check(metadata.all { it.maxLines == 1 && it.ellipsize == android.text.TextUtils.TruncateAt.END })
        val transport = descendants(player).filter { it.tag?.toString()?.startsWith("music.command.") == true }
        check(transport.map { it.tag.toString() }.toSet() == setOf("music.command.pause", "music.command.next"))
        transport.forEach { target ->
            check(target.measuredHeight >= ui.dp(44)) { "${target.tag} must expose at least a 44dp touch target" }
        }
        fun rightOnScreen(view: View): Int = IntArray(2).also(view::getLocationOnScreen)[0] + view.width
        fun leftOnScreen(view: View): Int = IntArray(2).also(view::getLocationOnScreen)[0]
        check(metadata.maxOf(::rightOnScreen) <= transport.minOf(::leftOnScreen)) {
            "Long metadata must not overlap the compact transport controls"
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
        check(player.findViewWithTag<View>("music.command.play") == null)
        player.render(AndroidMusicPresentation(snapshot = presentation().snapshot, listenerAccessGranted = false))
        check(descendants(player).none { it.tag?.toString()?.startsWith("music.command.") == true })
    }

    @org.junit.Test
    fun compactPlayerExpandAndHideCallbacksRun() = withPlayer { context, _, client, _ ->
        var expanded = 0
        var hidden = 0
        val callbacks = MusicCompactPlayer(context, client, onOpenDetails = { expanded++ }, onDismiss = { hidden++ })
        callbacks.render(presentation())
        check(callbacks.findViewWithTag<View>("music.expand").performClick())
        val hide = descendants(callbacks).single { it.contentDescription == context.getString(lol.cutout.R.string.music_hide) }
        check(hide.performClick())
        check(expanded == 1 && hidden == 1)
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
