package lol.cutout.ffitest

import android.app.Activity
import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.MainActivity
import lol.cutout.R
import org.junit.Test

class ProductionAppTest {
    @Test
    fun launcherAndNavigationUseTheRealApp() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            instrumentation.runOnMainSync {
                check(visibleText(activity, context.getString(R.string.speed_unit)))
                click(activity, context.getString(R.string.devices))
                check(visibleText(activity, context.getString(R.string.nearby_devices)))
                click(activity, context.getString(R.string.configuration))
                check(visibleText(activity, context.getString(R.string.no_settings)))
                click(activity, context.getString(R.string.ride))
                check(visibleText(activity, context.getString(R.string.speed_unit)))
            }
        } finally { instrumentation.runOnMainSync { activity.finish() } }
    }

    private fun children(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(children(view.getChildAt(index)))
    }
    private fun click(activity: Activity, text: String) {
        val button = children(activity.window.decorView).filterIsInstance<Button>()
            .single { it.isShown && it.text.toString() == text }
        check(button.performClick())
    }
    private fun visibleText(activity: Activity, text: String): Boolean =
        children(activity.window.decorView).filterIsInstance<TextView>().any { it.isShown && it.text.toString() == text }
}
