package lol.cutout.local

import android.content.Intent
import android.view.View
import android.view.ViewGroup
import lol.cutout.MainActivity
import lol.cutout.R
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalLightingNavigationTest {
    @Test
    fun effectsSelectionSurvivesRecreationAndAnUnopenedLightingRoute() {
        val context = androidx.test.core.app.ApplicationProvider.getApplicationContext<LocalTestApplication>()
        val host = Robolectric.buildActivity(MainActivity::class.java,
            Intent(context, MainActivity::class.java).putExtra("page", R.string.lighting)).setup().visible()
        try {
            var activity = host.get()
            checkNotNull(activity.window.decorView.findViewWithTag<View>("lighting.tab.effects")).performClick()
            host.recreate(); activity = host.get()
            assertTrue(checkNotNull(activity.window.decorView.findViewWithTag<View>("lighting.tab.effects")).isSelected)
            fun navigate(label: String) {
                descendants(activity.window.decorView).first {
                    it.isShown && it.isClickable && it.contentDescription?.toString() == label
                }.performClick()
            }
            navigate(context.getString(R.string.more))
            host.recreate(); activity = host.get()
            host.recreate(); activity = host.get()
            navigate(context.getString(R.string.lighting))
            assertTrue(checkNotNull(activity.window.decorView.findViewWithTag<View>("lighting.tab.effects")).isSelected)
        } finally { host.pause().stop().destroy() }
    }

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }
}
