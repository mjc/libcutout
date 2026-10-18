package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.Switch
import android.widget.TextView
import android.app.Activity
import android.content.Intent
import lol.cutout.MainActivity
import lol.cutout.R
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidMelkLightingClient
import lol.cutout.CutoutUi
import lol.cutout.LightingPanel
import lol.cutout.LightingPresentation
import org.junit.Test
import uniffi.cutout_mobile_ffi.*

/** Native presentation checks for the separate MELK controller route. No device writes are made. */
open class LightingPanelTest {
    @Test
    fun presetSummariesDescribeSolidEffectAndAccessoryMusicPlayback() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as Activity
        val client = AndroidMelkLightingClient(activity.applicationContext)
        try {
            instrumentation.runOnMainSync {
                val solid = MobileMelkLightingRestoreStateDto(false, 255u, 0u, 0u, 64u, MobileLightingPlaybackDto.Solid)
                val effect = MobileMelkLightingRestoreStateDto(false, 0u, 0u, 0u, 100u, MobileLightingPlaybackDto.Effect(1u, 91u))
                val music = MobileMelkLightingRestoreStateDto(false, 0u, 0u, 0u, 100u, MobileLightingPlaybackDto.Music(2u, 73u))
                val presets = listOf(
                    MobileRgbLightingPresetDto("Desk", solid),
                    MobileRgbLightingPresetDto("Wave", effect),
                    MobileRgbLightingPresetDto("Ride", music),
                )
                val panel = LightingPanel(activity, client) {}
                panel.render(LightingPresentation(
                    MobileMelkLightingSessionSnapshotDto(MobileMelkLightingSessionStateDto.Idle, null, null, 0u, false),
                    emptyList(), "accessory-id", null, "ride-id", false, solid, null, presets,
                    permissionMissing = false, selectedVehicle = "ride-id",
                ))
                activity.setContentView(panel)
                val labels = CutoutUi(activity)
                val summaries = descendants(panel).filterIsInstance<TextView>().map { it.text.toString() }.toSet()
                check(summaries.contains(activity.getString(R.string.lighting_preset_solid_summary, 64)))
                check(summaries.contains(activity.getString(R.string.lighting_preset_effect_summary,
                    mobileMelkLightingPatternCatalog().first { it.id.toInt() == 1 }.name, 91)))
                check(summaries.contains(activity.getString(R.string.lighting_preset_music_summary, 73)))
                check(labels.label("lighting.preset.scenes").isNotBlank())
            }
        } finally { client.stop(); instrumentation.runOnMainSync { activity.finish() } }
    }

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

    private fun descendants(view: View): Sequence<View> = children(view)
}
