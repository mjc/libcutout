package lol.cutout.ffitest

import android.app.Activity
import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.CutoutApplication
import lol.cutout.CutoutUi
import lol.cutout.MainActivity
import lol.cutout.R
import org.junit.Test

open class ProductionAppTest {
    @Test
    fun launcherOpensDevicePickerAndGeneralSetupWithoutAConnectedPev() = withActivity { activity ->
        val context = activity
        check(visibleText(activity, context.getString(R.string.picker_title)))
        clickDescription(activity, context.getString(R.string.more))
        clickDescription(activity, context.getString(R.string.setup))
        check(visibleText(activity, context.getString(R.string.setup_ride_autostart)))
        clickDescription(activity, context.getString(R.string.devices))
        check(visibleText(activity, context.getString(R.string.picker_title)))
    }

    @Test
    fun rideRendersRustCaptureAndRetainsInstrumentsAcrossTelemetryUpdates() {
        val replay = productionReplayPresentation()
        withActivity { activity ->
            activity.render(replay)
            clickDescription(activity, activity.getString(R.string.configuration))
            check(visibleText(activity, CutoutUi(activity).label("controls.tune.title")))
            clickDescription(activity, activity.getString(R.string.ride))
            val speed = checkNotNull(activity.window.decorView.findViewWithTag<TextView>("ride.hero.speed"))
            val before = speed.text.toString()
            check(before.isNotBlank() && before != activity.getString(R.string.unavailable))
            check(visibleText(activity, checkNotNull(replay.session?.identity?.model)))
            activity.render(replay.copy())
            check(activity.window.decorView.findViewWithTag<TextView>("ride.hero.speed") === speed)
            check(speed.text.toString() == before)
            check(activity.window.decorView.findViewWithTag<View>("euc.ride.headroom") != null)
        }
    }

    private fun withActivity(block: (MainActivity) -> Unit) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        try {
            // Keep native discovery callbacks out of the deterministic presentation fixture.
            (activity.application as CutoutApplication).bluetooth.observe(null)
            (activity.application as CutoutApplication).bluetooth.stopScan()
            instrumentation.runOnMainSync { block(activity) }
        } finally { instrumentation.runOnMainSync { activity.finish() } }
    }

    private fun children(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(children(view.getChildAt(index)))
    }
    private fun clickDescription(activity: Activity, text: String) {
        val target = children(activity.window.decorView).single {
            it.isShown && it.isClickable && it.contentDescription?.toString() == text
        }
        check(target.performClick())
    }
    private fun visibleText(activity: Activity, text: String): Boolean =
        children(activity.window.decorView).filterIsInstance<TextView>().any { it.isShown && it.text.toString() == text }
}
