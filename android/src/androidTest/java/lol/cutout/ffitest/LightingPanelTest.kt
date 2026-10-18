package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.Switch
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidMelkLightingClient
import lol.cutout.LightingPanel
import lol.cutout.LightingPresentation
import org.junit.Test
import uniffi.cutout_mobile_ffi.MobileMelkLightingSessionSnapshotDto
import uniffi.cutout_mobile_ffi.MobileMelkLightingSessionStateDto

/** Native presentation checks for the separate MELK controller route. No device writes are made. */
open class LightingPanelTest {
    @Test
    fun controlsAreNativeAndRemainUnavailableUntilRustReportsReady() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val client = AndroidMelkLightingClient(context.applicationContext)
        try {
            lateinit var panel: LightingPanel
            instrumentation.runOnMainSync {
                panel = LightingPanel(context, client) {}
                panel.render(
                    LightingPresentation(
                        MobileMelkLightingSessionSnapshotDto(
                            MobileMelkLightingSessionStateDto.Idle, null, null, 0u, false,
                        ),
                        emptyList(), null, null, null, false, null, null, emptyList(),
                        permissionMissing = false,
                    ),
                )
                val effects = panel.findViewWithTag<Button>("lighting.tab.effects")
                check(effects != null) { "Swift-equivalent Effects page tab was not rendered" }
                effects.performClick()
                check(children(panel).filterIsInstance<TextView>().any { it.text == lol.cutout.CutoutUi(context).label("lighting.patterns") })
                check(children(panel).filterIsInstance<Switch>().all { !it.isEnabled }) {
                    "Unverified/disconnected lighting state left power control enabled"
                }
                check(children(panel).filter { it.tag?.toString()?.startsWith("lighting.effect.") == true }
                    .all { !it.isEnabled }) {
                    "Unverified/disconnected lighting state left an effect action enabled"
                }
            }
        } finally { client.stop() }
    }

    private fun children(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(children(view.getChildAt(index)))
    }
}
