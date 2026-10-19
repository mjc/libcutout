package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.Switch
import android.widget.TextView
import android.widget.EditText
import android.widget.SeekBar
import android.view.MotionEvent
import android.app.Activity
import android.content.Intent
import android.content.res.Configuration
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
                val effects = panel.findViewWithTag<View>("lighting.tab.effects")
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

    @Test
    fun accessoryDetailsStaysACompactButtonAndPreservesNameWidthAtLargeFontScale() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as Activity
        val client = AndroidMelkLightingClient(activity.applicationContext)
        try {
            instrumentation.runOnMainSync {
                val base = activity.resources.configuration
                listOf(1f, 2f).forEach { fontScale ->
                    val configuration = Configuration(base).apply { this.fontScale = fontScale }
                    val context = activity.createConfigurationContext(configuration)
                    val panel = LightingPanel(context, client) {}
                    panel.render(LightingPresentation(
                        MobileMelkLightingSessionSnapshotDto(
                            MobileMelkLightingSessionStateDto.Idle, null, null, 0u, false,
                        ),
                        emptyList(), null, null, null, false, null, null, emptyList(),
                        permissionMissing = false,
                    ))
                    activity.setContentView(panel)
                    val width = (393 * context.resources.displayMetrics.density).toInt()
                    val heightLimit = (1600 * context.resources.displayMetrics.density).toInt()
                    panel.measure(
                        View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                        View.MeasureSpec.makeMeasureSpec(heightLimit, View.MeasureSpec.AT_MOST),
                    )
                    panel.layout(0, 0, width, panel.measuredHeight)

                    val details = checkNotNull(panel.findViewWithTag<View>("lighting.accessory.details"))
                    val name = checkNotNull(panel.findViewWithTag<TextView>("lighting.accessory.name"))
                    val status = checkNotNull(panel.findViewWithTag<TextView>("lighting.accessory.status"))
                    val minNameWidth = (100 * context.resources.displayMetrics.density).toInt()
                    check(details.measuredWidth == (44 * context.resources.displayMetrics.density).toInt()) {
                        "Details action should stay a 44dp icon target at font scale $fontScale, got ${details.measuredWidth}px"
                    }
                    check(name.measuredWidth >= minNameWidth && status.measuredWidth >= minNameWidth) {
                        "Accessory name/status were squeezed by the details action at font scale $fontScale: ${name.measuredWidth}/${status.measuredWidth}px"
                    }
                    val node = details.createAccessibilityNodeInfo()
                    check(node.className == android.widget.ImageButton::class.java.name &&
                        node.contentDescription == CutoutUi(context).label("lighting.accessory.details")) {
                        "The compact details icon must remain a named button for accessibility: ${node.className}/${node.contentDescription}"
                    }
                }
            }
        } finally { client.stop(); instrumentation.runOnMainSync { activity.finish() } }
    }

    @Test
    fun segmentedPagesFitNarrowLargeTextAndRetainTabViewsAcrossSnapshots() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as Activity
        val client = AndroidMelkLightingClient(activity.applicationContext)
        try {
            instrumentation.runOnMainSync {
                val base = activity.resources.configuration
                listOf(1f, 1.5f, 2f).forEach { fontScale ->
                    val configuration = Configuration(base).apply { this.fontScale = fontScale }
                    val context = activity.createConfigurationContext(configuration)
                    val panel = LightingPanel(context, client) {}
                    val snapshot = LightingPresentation(
                        MobileMelkLightingSessionSnapshotDto(
                            MobileMelkLightingSessionStateDto.Idle, null, null, 0u, false,
                        ),
                        emptyList(), null, null, null, false, null, null, emptyList(),
                        permissionMissing = false,
                    )
                    panel.render(snapshot)
                    activity.setContentView(panel)

                    val density = context.resources.displayMetrics.density
                    val width = (353 * density).toInt()
                    val heightLimit = (1800 * density).toInt()
                    repeat(4) {
                        panel.measure(
                            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                            View.MeasureSpec.makeMeasureSpec(heightLimit, View.MeasureSpec.AT_MOST),
                        )
                        panel.layout(0, 0, width, panel.measuredHeight)
                        panel.viewTreeObserver.dispatchOnPreDraw()
                    }

                    val tabs = listOf("color", "effects", "music").map { name ->
                        checkNotNull(panel.findViewWithTag<TextView>("lighting.tab.$name"))
                    }
                    val selector = checkNotNull(panel.findViewWithTag<ViewGroup>("lighting.page-picker"))
                    check(selector.childCount == 3)
                    tabs.forEach { tab ->
                        check(tab.measuredHeight == (44 * density).toInt()) {
                            "Segmented page target must remain 44dp at font scale $fontScale"
                        }
                        check(tab.lineCount == 1 && tab.layout?.getEllipsisCount(0) == 0) {
                            "Segmented page label wrapped or clipped at font scale $fontScale: ${tab.text}"
                        }
                        check(tab.createAccessibilityNodeInfo().className == android.widget.Button::class.java.name)
                    }
                    check(tabs.sumOf { it.measuredWidth } == selector.measuredWidth - selector.paddingLeft - selector.paddingRight) {
                        "The three segmented targets must share the available width at font scale $fontScale"
                    }

                    val effects = tabs[1]
                    effects.performClick()
                    effects.isFocusableInTouchMode = true
                    check(effects.requestFocus()) { "Tab must receive input focus before testing its retention" }
                    panel.render(snapshot.copy(alias = "Updated accessory"))
                    check(panel.findViewWithTag<View>("lighting.tab.effects") === effects) {
                        "Snapshot updates must keep the existing tab view so accessibility focus survives"
                    }
                    check(effects.isSelected && effects.hasFocus()) {
                        "Rerendering a snapshot should preserve the selected page and focused tab"
                    }
                }
            }
        } finally { client.stop(); instrumentation.runOnMainSync { activity.finish() } }
    }

    @Test
    fun telemetrySnapshotsDoNotReplaceAnActiveBrightnessGesture() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as Activity
        val client = AndroidMelkLightingClient(activity.applicationContext)
        try {
            instrumentation.runOnMainSync {
                val panel = LightingPanel(activity, client) {}
                val requested = MobileMelkLightingRestoreStateDto(false, 255u, 0u, 0u, 40u, null)
                val snapshot = LightingPresentation(
                    MobileMelkLightingSessionSnapshotDto(
                        MobileMelkLightingSessionStateDto.Ready, "synthetic-light", "Test light", 0u, true,
                    ),
                    emptyList(), "synthetic-light", null, null, false, requested, null, emptyList(),
                    permissionMissing = false,
                )
                panel.render(snapshot)
                activity.setContentView(panel)
                val width = (393 * activity.resources.displayMetrics.density).toInt()
                val heightLimit = (2200 * activity.resources.displayMetrics.density).toInt()
                panel.measure(
                    View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(heightLimit, View.MeasureSpec.AT_MOST),
                )
                panel.layout(0, 0, width, panel.measuredHeight)
                panel.viewTreeObserver.dispatchOnPreDraw()

                val slider = checkNotNull(panel.findViewWithTag<SeekBar>("lighting.brightness"))
                check(slider.isEnabled && slider.width > 0 && slider.height > 0)
                val now = android.os.SystemClock.uptimeMillis()
                val down = MotionEvent.obtain(now, now, MotionEvent.ACTION_DOWN, slider.width * .7f, slider.height / 2f, 0)
                slider.dispatchTouchEvent(down)
                down.recycle()
                check(slider.isPressed) { "Brightness slider did not enter its native tracking state" }
                val duringDrag = snapshot.copy(requested = requested.copy(brightness = 63u))
                panel.render(duringDrag)
                check(panel.findViewWithTag<SeekBar>("lighting.brightness") === slider && slider.isPressed) {
                    "A telemetry snapshot replaced the brightness control during a drag"
                }
                val cancelTime = now + 20
                val cancel = MotionEvent.obtain(now, cancelTime, MotionEvent.ACTION_CANCEL, slider.width * .7f, slider.height / 2f, 0)
                slider.dispatchTouchEvent(cancel)
                cancel.recycle()
                check(!slider.isPressed) { "The synthetic slider gesture must finish before the next interaction test" }
            }
        } finally { client.stop(); instrumentation.runOnMainSync { activity.finish() } }
    }

    @Test
    fun telemetrySnapshotsPreserveFocusedPresetDraftAndCaret() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as Activity
        val client = AndroidMelkLightingClient(activity.applicationContext)
        try {
            instrumentation.runOnMainSync {
                val panel = LightingPanel(activity, client) {}
                val requested = MobileMelkLightingRestoreStateDto(false, 255u, 0u, 0u, 40u, null)
                val snapshot = LightingPresentation(
                    MobileMelkLightingSessionSnapshotDto(
                        MobileMelkLightingSessionStateDto.Ready, "synthetic-light", "Test light", 0u, true,
                    ),
                    emptyList(), "synthetic-light", null, null, false, requested, null, emptyList(),
                    permissionMissing = false,
                )
                panel.render(snapshot)
                activity.setContentView(panel)
                val width = (393 * activity.resources.displayMetrics.density).toInt()
                panel.measure(
                    View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec((2200 * activity.resources.displayMetrics.density).toInt(), View.MeasureSpec.AT_MOST),
                )
                panel.layout(0, 0, width, panel.measuredHeight)
                panel.viewTreeObserver.dispatchOnPreDraw()

                val name = checkNotNull(panel.findViewWithTag<EditText>("lighting.preset.name"))
                name.isFocusableInTouchMode = true
                check(name.requestFocus())
                name.setText("Weekend ride")
                name.setSelection(7)
                panel.render(snapshot.copy(requested = requested.copy(brightness = 78u)))
                val retained = checkNotNull(panel.findViewWithTag<EditText>("lighting.preset.name"))
                check(retained === name && retained.text.toString() == "Weekend ride" && retained.selectionStart == 7) {
                    "A telemetry snapshot replaced the focused preset editor or changed its draft/caret"
                }
            }
        } finally { client.stop(); instrumentation.runOnMainSync { activity.finish() } }
    }

    @Test
    fun focusOnAnOrdinaryButtonDoesNotFreezeNewConfirmedState() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as Activity
        val client = AndroidMelkLightingClient(activity.applicationContext)
        try {
            instrumentation.runOnMainSync {
                val panel = LightingPanel(activity, client) {}
                val confirmed = MobileMelkLightingRestoreStateDto(false, 255u, 0u, 0u, 40u, null)
                val snapshot = LightingPresentation(
                    MobileMelkLightingSessionSnapshotDto(
                        MobileMelkLightingSessionStateDto.Ready, "synthetic-light", "Test light", 0u, true,
                    ),
                    emptyList(), "synthetic-light", null, null, false, null, confirmed, emptyList(),
                    permissionMissing = false,
                )
                panel.render(snapshot)
                activity.setContentView(panel)
                val width = (393 * activity.resources.displayMetrics.density).toInt()
                panel.measure(
                    View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec((2200 * activity.resources.displayMetrics.density).toInt(), View.MeasureSpec.AT_MOST),
                )
                panel.layout(0, 0, width, panel.measuredHeight)
                panel.viewTreeObserver.dispatchOnPreDraw()

                val white = checkNotNull(panel.findViewWithTag<View>("lighting.set-white"))
                white.isFocusableInTouchMode = true
                check(white.requestFocus())
                panel.render(snapshot.copy(confirmed = confirmed.copy(brightness = 78u)))
                check(panel.findViewWithTag<View>("lighting.set-white") !== white)
                check(panel.findViewWithTag<SeekBar>("lighting.brightness")?.progress == 78) {
                    "Keyboard focus on a command button must not prevent a newer Rust-confirmed value from rendering"
                }
            }
        } finally { client.stop(); instrumentation.runOnMainSync { activity.finish() } }
    }

    private fun children(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(children(view.getChildAt(index)))
    }

    private fun descendants(view: View): Sequence<View> = children(view)
}
