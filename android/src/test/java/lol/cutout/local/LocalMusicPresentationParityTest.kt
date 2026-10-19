package lol.cutout.local

import android.app.Activity
import android.content.ComponentName
import android.content.res.Configuration
import android.os.Looper
import android.provider.Settings
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.Spinner
import android.widget.ImageView
import android.widget.TextView
import lol.cutout.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalMusicPresentationParityTest {
    @Test
    fun compactPlayerKeepsOneReadableLineAtLargeTextSizes() {
        for (scale in listOf(1.5f, 2f)) {
            val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
            val base = host.get()
            val context = base.createConfigurationContext(Configuration(base.resources.configuration).apply { fontScale = scale })
            val player = MusicCompactPlayer(context, (base.application as CutoutApplication).music)
            base.setContentView(player)
            try {
                val value = presentation()
                player.render(value)
                val ui = CutoutUi(context)
                player.measure(View.MeasureSpec.makeMeasureSpec(ui.dp(393), View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(ui.dp(44), View.MeasureSpec.EXACTLY))
                player.layout(0, 0, player.measuredWidth, player.measuredHeight)
                val title = checkNotNull(player.findViewWithTag<TextView>("music.now-playing-title"))
                val metadata = title.parent as android.view.ViewGroup
                val expand = checkNotNull(player.findViewWithTag<View>("music.expand"))
                assertEquals(android.widget.Button::class.java.name, expand.createAccessibilityNodeInfo().className.toString())
                assertTrue(expand.contentDescription.toString().contains(value.snapshot!!.item!!.artist!!))
                assertEquals(View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS, metadata.importantForAccessibility)
                assertEquals(1, (0 until metadata.childCount).count { metadata.getChildAt(it).visibility == View.VISIBLE })
                assertEquals(value.snapshot!!.item!!.title, title.text.toString())
                assertTrue("Scaled text must fit the compact row", title.height <= player.height)
                assertTrue(title.textSize <= 14f * 1.35f * context.resources.displayMetrics.density + 1f)
                player.render(value.copy(snapshot = value.snapshot!!.copy(state = MobileMusicPlaybackStateDto.BUFFERING)))
                assertEquals(context.getString(R.string.music_state_buffering), title.text.toString())
            } finally { host.pause().stop().destroy() }
        }
    }

    @Test
    fun musicChoiceWaitsForPublishedSelectionAndCoalescesPendingWrites() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val requests = mutableListOf<Int>()
        val receipts = mutableListOf<() -> Unit>()
        val choice = MusicSettingsChoice(host.get(), CutoutUi(host.get()), "History",
            listOf(0 to "Off", 1 to "Identifiers", 2 to "Titles and artists")) { value, receipt ->
            requests.add(value); receipts.add(receipt)
        }
        host.get().setContentView(choice)
        try {
            choice.render(0)
            settle(choice)
            assertTrue(requests.isEmpty())
            choice.setSelection(1)
            settle(choice)
            assertEquals(listOf(1), requests)
            assertFalse(choice.isEnabled)
            assertEquals("Selection must retain the last owner receipt", 0, choice.selectedItemPosition)
            choice.render(1)
            settle(choice)
            assertFalse(choice.isEnabled)
            receipts[0]()
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(choice.isEnabled)
            assertEquals(1, choice.selectedItemPosition)
            choice.setSelection(2)
            settle(choice)
            assertEquals(listOf(1, 2), requests)
            receipts[0]()
            shadowOf(Looper.getMainLooper()).idle()
            assertFalse("Duplicate old receipt must not release a newer request", choice.isEnabled)
            receipts[1]()
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(choice.isEnabled)
            assertEquals("Rejected change leaves the previous published selection", 1, choice.selectedItemPosition)
        } finally { host.pause().stop().destroy() }
    }

    @Test
    fun expandedPlayerGroupsControlsAndUsesAccessibleIconsAtBothTextSizes() {
        for (scale in listOf(1f, 1.5f)) {
            val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
            val base = host.get()
            val context = base.createConfigurationContext(Configuration(base.resources.configuration).apply { fontScale = scale })
            val client = (base.application as CutoutApplication).music
            val panel = MusicPanel(context, client, onOpenSettings = {}, onHidePlayer = {}, onDone = {})
            base.setContentView(panel)
            try {
                shadowOf(Looper.getMainLooper()).idle()
                val value = presentation()
                panel.render(value)
                NativeSurfaceCapture.save(panel, if (scale > 1f) "music-expanded-large-text" else "music-expanded-dark")
                val pause = checkNotNull(panel.findViewWithTag<FrameLayout>("music.command.pause"))
                assertTrue(pause.getChildAt(0) is ImageView)
                assertTrue(pause.width >= CutoutUi(context).dp(44))
                assertEquals(context.getString(R.string.music_pause), pause.contentDescription)
                assertEquals(View.VISIBLE, panel.findViewWithTag<View>("music.spotify-content-link").visibility)
                assertEquals("The canonical content link replaces the generic provider link", View.GONE,
                    panel.findViewWithTag<View>("music.open-provider").visibility)
                val actions = checkNotNull(panel.findViewWithTag<View>("music.actions.section"))
                val timeline = checkNotNull(panel.findViewWithTag<View>("music.timeline.section"))
                assertTrue(actions.top < timeline.top)
                panel.render(value.copy(snapshot = value.snapshot!!.copy(
                    item = value.snapshot!!.item!!.copy(identifier = "not-a-spotify-identifier"))))
                assertEquals(View.GONE, panel.findViewWithTag<View>("music.spotify-content-link").visibility)
                assertEquals(View.VISIBLE, panel.findViewWithTag<View>("music.open-provider").visibility)
                assertSame(pause, panel.findViewWithTag("music.command.pause"))
            } finally { panel.dispose(); host.pause().stop().destroy() }
        }
    }

    @Test
    fun settingsUseNativePickersAndRespectProviderAndDisconnectAvailability() {
        for (scale in listOf(1f, 1.5f)) {
            val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
            val base = host.get()
            val context = base.createConfigurationContext(Configuration(base.resources.configuration).apply { fontScale = scale })
            val panel = MusicSettingsPanel(context, (base.application as CutoutApplication).music, onRequestListenerAccess = {})
            base.setContentView(panel)
            try {
                shadowOf(Looper.getMainLooper()).idle()
                val noSession = presentation().let { value ->
                    value.copy(
                        error = base.getString(R.string.music_no_active_session),
                        snapshot = value.snapshot!!.copy(
                            state = MobileMusicPlaybackStateDto.UNAVAILABLE,
                            item = null,
                            positionMilliseconds = null,
                            durationMilliseconds = null,
                            capabilities = MobileMusicCapabilitiesDto(false, false, false, false, true),
                        ),
                    )
                }
                panel.render(noSession)
                assertEquals(base.getString(R.string.music_state_unavailable),
                    panel.findViewWithTag<TextView>("music.connection-status").text.toString())
                assertEquals(base.getString(R.string.music_no_active_session),
                    panel.findViewWithTag<TextView>("music.provider-error").text.toString())
                val connect = checkNotNull(panel.findViewWithTag<android.widget.Button>("music.connect-provider"))
                assertEquals(View.VISIBLE, connect.visibility)
                assertTrue(connect.text.toString().startsWith("Connect "))
                assertEquals(Gravity.START or Gravity.CENTER_VERTICAL, connect.gravity)
                assertTrue(connect.minimumHeight >= CutoutUi(context).dp(44))
                val flatBackground = connect.background as android.graphics.drawable.RippleDrawable
                assertEquals("Flat Form actions have no filled or outlined content layer", 0, flatBackground.numberOfLayers)
                val open = checkNotNull(panel.findViewWithTag<android.widget.Button>("music.open-provider"))
                assertTrue(open.text.toString().startsWith("Open "))
                val disconnect = checkNotNull(panel.findViewWithTag<android.widget.Button>("music.disconnect-spotify"))
                assertEquals(CutoutUi(context).red, disconnect.currentTextColor)

                panel.render(presentation().copy(snapshot = presentation().snapshot!!.copy(state = MobileMusicPlaybackStateDto.STALE)))
                assertTrue("Stale sessions offer reconnect using the shared label", connect.text.toString().startsWith("Reconnect "))
                panel.render(presentation())
                assertEquals(presentation().snapshot!!.item!!.title,
                    panel.findViewWithTag<TextView>("music.connection-status").text.toString())

                panel.render(presentation().copy(selectedProvider = MobileMusicProviderDto.SOUND_CLOUD))
                val provider = checkNotNull(panel.findViewWithTag<Spinner>("music.provider-picker"))
                val history = checkNotNull(panel.findViewWithTag<Spinner>("music.history-picker"))
                assertEquals(3, provider.count)
                assertEquals(3, history.count)
                assertNull(panel.findViewWithTag<View>("music.history.forget"))
                assertFalse(history.isEnabled)
                assertEquals(View.GONE, panel.findViewWithTag<View>("music.connect-provider").visibility)
                panel.render(presentation().copy(spotifyDisconnectPending = true))
                assertFalse(provider.isEnabled)
                assertFalse(history.isEnabled)
                assertFalse(panel.findViewWithTag<View>("music.open-provider").isEnabled)
                panel.render(presentation())
                assertTrue(provider.isEnabled)
                assertTrue(history.isEnabled)
                NativeSurfaceCapture.save(panel, if (scale > 1f) "music-settings-large-text" else "music-settings-dark")
            } finally { panel.dispose(); host.pause().stop().destroy() }
        }
    }

    @Test
    fun connectWithNoActiveMediaSessionUsesExplicitProviderLaunchRecovery() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val base = host.get()
        val client = (base.application as CutoutApplication).music
        val previousAccess = Settings.Secure.getString(base.contentResolver, "enabled_notification_listeners")
        val previousProvider = client.latestPresentation.selectedProvider
        try {
            Settings.Secure.putString(
                base.contentResolver,
                "enabled_notification_listeners",
                ComponentName(base, AndroidMusicNotificationListenerService::class.java).flattenToString(),
            )
            client.selectProvider(MobileMusicProviderDto.SPOTIFY)
            client.connectOrRetrySelectedProvider()
            shadowOf(Looper.getMainLooper()).idle()

            // Robolectric has no Spotify package installed. The unavailable-app
            // result proves Connect/Retry reached the normal provider-launch
            // effect, rather than merely polling an absent MediaSession again.
            assertEquals(MobileMusicProviderDto.SPOTIFY, client.latestPresentation.selectedProvider)
            assertEquals(base.getString(R.string.music_provider_unavailable), client.latestPresentation.error)
            assertTrue(client.latestPresentation.listenerAccessGranted)
        } finally {
            Settings.Secure.putString(base.contentResolver, "enabled_notification_listeners", previousAccess)
            client.selectProvider(previousProvider)
            client.refreshSessions()
            host.pause().stop().destroy()
        }
    }

    private fun settle(view: View) {
        val ui = CutoutUi(view.context)
        view.measure(View.MeasureSpec.makeMeasureSpec(ui.dp(393), View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(ui.dp(100), View.MeasureSpec.AT_MOST))
        view.layout(0, 0, view.measuredWidth, view.measuredHeight)
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun presentation(): AndroidMusicPresentation {
        val item = MobileMusicItemDto("spotify:episode:512ojhOuo1ktJprKbVcKyQ", "A long podcast episode title for the ride home", "Example Podcast")
        return AndroidMusicPresentation(selectedProvider = MobileMusicProviderDto.SPOTIFY, listenerAccessGranted = true,
            snapshot = MobileMusicSnapshotDto(MobileMusicProviderDto.SPOTIFY, "fixture", MobileMusicPlaybackStateDto.PLAYING,
                item, 30_000UL, 3_600_000UL, 100UL, MobileMusicCapabilitiesDto(true, true, true, true, true)),
            history = MobileMusicHistoryDto(MobileMusicHistoryStatusDto.AVAILABLE, listOf(
                MobileMusicRideEventDto(0UL, MobileMusicProviderDto.SPOTIFY, item.identifier, item.title, item.artist,
                    MobileMusicRideEventKindDto.ITEM_CHANGED, 100UL, 100UL, 1_792_000_000_000UL, 0UL))))
    }
}
