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

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }
}
