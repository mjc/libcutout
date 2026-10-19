package lol.cutout.local

import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.Button
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
@Config(sdk = [35], qualifiers = "w393dp-h852dp-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalMapSubmodeRestoreTest {
    @Test
    fun historyTabSurvivesActivityRecreation() {
        val context = androidx.test.core.app.ApplicationProvider.getApplicationContext<LocalTestApplication>()
        val controller = Robolectric.buildActivity(MainActivity::class.java,
            Intent(context, MainActivity::class.java).putExtra("page", R.string.map)).setup().visible()
        try {
            var activity = controller.get()
            assertTrue(activity.window.decorView.findViewWithTag<Button>("ride_map_mode_history").performClick())

            var recreated = controller.recreate()
            activity = recreated.get()
            assertTrue("Map page should restore the selected History tab",
                activity.window.decorView.findViewWithTag<Button>("ride_map_mode_history").isSelected)

        } finally {
            controller.pause().stop().destroy()
        }
    }

    @Test
    fun anUnopenedMapKeepsItsRestoredTabThroughAnotherRecreation() {
        val context = androidx.test.core.app.ApplicationProvider.getApplicationContext<LocalTestApplication>()
        val controller = Robolectric.buildActivity(MainActivity::class.java,
            Intent(context, MainActivity::class.java).putExtra("page", R.string.map)).setup().visible()
        try {
            var activity = controller.get()
            activity.window.decorView.findViewWithTag<Button>("ride_map_mode_history").performClick()
            fun navigate(label: String) {
                checkNotNull(descendants(activity.window.decorView).firstOrNull {
                    it.isShown && it.isClickable && it.contentDescription?.toString() == label
                }).performClick()
            }
            navigate(context.getString(R.string.more))
            controller.recreate(); activity = controller.get()
            controller.recreate(); activity = controller.get()
            navigate(context.getString(R.string.map))
            assertTrue(activity.window.decorView.findViewWithTag<Button>("ride_map_mode_history").isSelected)
        } finally { controller.pause().stop().destroy() }
    }

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }

}
