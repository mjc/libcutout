package lol.cutout.local

import android.content.Intent
import android.view.View
import android.view.ViewGroup
import lol.cutout.CutoutApplication
import lol.cutout.MainActivity
import lol.cutout.R
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.MobileMusicProviderDto

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalMusicNavigationTest {
    @Test
    fun providerSpecificPlayerRoutesReturnToTheirInvokingScreen() {
        val context = androidx.test.core.app.ApplicationProvider.getApplicationContext<CutoutApplication>()
        context.music.selectProvider(MobileMusicProviderDto.SPOTIFY)
        val host = Robolectric.buildActivity(MainActivity::class.java,
            Intent(context, MainActivity::class.java).putExtra("page", R.string.music)).setup().visible()
        val activity = host.get()
        try {
            val decor = activity.window.decorView
            assertTrue(checkNotNull(decor.findViewWithTag<View>("music.player.screen")).isShown)
            assertNull(decor.findViewWithTag<View>("music.soundcloud.player"))
            checkNotNull(decor.findViewWithTag<View>("music.open-settings")).performClick()
            context.music.selectProvider(MobileMusicProviderDto.SOUND_CLOUD)
            org.robolectric.Shadows.shadowOf(activity.mainLooper).idle()
            val choose = checkNotNull(decor.findViewWithTag<View>("music.soundcloud.choose-music"))
            assertTrue(choose.isShown)
            choose.performClick()
            assertTrue(checkNotNull(decor.findViewWithTag<View>("music.soundcloud.player")).isShown)
            assertFalse(checkNotNull(decor.findViewWithTag<View>("music.player.screen")).isShown)
            val back = descendants(decor).single {
                it.isShown && it.isClickable && it.contentDescription?.toString() == activity.getString(R.string.back)
            }
            back.performClick()
            assertTrue(choose.isShown)
            assertFalse(checkNotNull(decor.findViewWithTag<View>("music.soundcloud.player")).isShown)
            back.performClick()
            assertTrue(checkNotNull(decor.findViewWithTag<View>("music.soundcloud.player")).isShown)
            back.performClick()
            assertFalse(checkNotNull(decor.findViewWithTag<View>("music.soundcloud.player")).isShown)
        } finally { host.pause().stop().destroy() }
    }

    @Test
    fun hiddenMusicPlayerCanBeRestoredFromMoreWithoutOpeningAnotherPage() {
        val context = androidx.test.core.app.ApplicationProvider.getApplicationContext<CutoutApplication>()
        val preferences = context.getSharedPreferences("cutout.presentation", android.content.Context.MODE_PRIVATE)
        val previous = preferences.getBoolean("music.hidden", false)
        preferences.edit().putBoolean("music.hidden", true).commit()
        val host = Robolectric.buildActivity(MainActivity::class.java,
            Intent(context, MainActivity::class.java).putExtra("page", R.string.more)).setup().visible()
        try {
            val decor = host.get().window.decorView
            val musicRow = checkNotNull(decor.findViewWithTag<View>("more.music"))
            val restore = checkNotNull(decor.findViewWithTag<View>("music.restore"))
            assertTrue(restore.isShown)
            assertTrue(restore.performClick())
            assertFalse(preferences.getBoolean("music.hidden", true))
            assertEquals(View.GONE, restore.visibility)
            assertSame(musicRow, decor.findViewWithTag<View>("more.music"))
            assertFalse(restore.isShown)
            assertTrue(descendants(decor).any {
                it.isShown && it.isClickable && it.contentDescription?.toString() == context.getString(R.string.camera)
            })
        } finally {
            host.pause().stop().destroy()
            preferences.edit().putBoolean("music.hidden", previous).commit()
        }
    }

    @Test
    fun phoneAlarmsOpenFromTuneAndReturnToItsRetainedConfiguration() {
        val context = androidx.test.core.app.ApplicationProvider.getApplicationContext<CutoutApplication>()
        val host = Robolectric.buildActivity(MainActivity::class.java,
            Intent(context, MainActivity::class.java).putExtra("page", R.string.configuration)).setup().visible()
        try {
            val activity = host.get()
            val decor = activity.window.decorView
            val open = checkNotNull(decor.findViewWithTag<View>("settings.open-alarms"))
            assertTrue(open.isShown)
            open.performClick()
            assertTrue(checkNotNull(decor.findViewWithTag<View>("phone-alarm.configured")).visibility == View.GONE)
            val back = descendants(decor).single {
                it.isShown && it.isClickable && it.contentDescription?.toString() == activity.getString(R.string.back)
            }
            back.performClick()
            assertTrue(open.isShown)
            assertSame(open, decor.findViewWithTag<View>("settings.open-alarms"))
        } finally { host.pause().stop().destroy() }
    }

    @Test
    fun musicHistoryOptionsRemainReachableBelowTheInitialSettingsViewport() {
        val context = androidx.test.core.app.ApplicationProvider.getApplicationContext<CutoutApplication>()
        context.music.selectProvider(MobileMusicProviderDto.APPLE_MUSIC)
        val host = Robolectric.buildActivity(MainActivity::class.java,
            Intent(context, MainActivity::class.java).putExtra("page", R.string.music_settings)).setup().visible()
        try {
            val decor = host.get().window.decorView
            val panel = checkNotNull(decor.findViewWithTag<android.widget.ScrollView>("music.settings.screen"))
            val lastPolicy = checkNotNull(panel.findViewWithTag<View>("music.history.policy.human_readable"))
            assertTrue(panel.canScrollVertically(1))
            panel.fullScroll(View.FOCUS_DOWN)
            val visible = android.graphics.Rect()
            assertTrue(lastPolicy.getGlobalVisibleRect(visible))
            assertTrue(visible.height() > 0)
            assertTrue(panel.scrollY > 0)
        } finally { host.pause().stop().destroy() }
    }

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }
}
