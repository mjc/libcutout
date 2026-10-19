package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.Switch
import android.widget.TextView
import android.widget.EditText
import android.widget.SeekBar
import android.widget.Spinner
import android.view.MotionEvent
import android.os.Bundle
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
    fun effectsSeparatesPowerAndPatternsCardsAndRestoresNativePageSelection() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as Activity
        val client = AndroidMelkLightingClient(activity.applicationContext)
        try {
            instrumentation.runOnMainSync {
                val requested = MobileMelkLightingRestoreStateDto(false, 255u, 0u, 0u, 40u, null)
                val snapshot = LightingPresentation(
                    MobileMelkLightingSessionSnapshotDto(MobileMelkLightingSessionStateDto.Ready, "synthetic-light", "Test light", 0u, true),
                    emptyList(), "synthetic-light", null, null, false, requested, null, emptyList(),
                    permissionMissing = false,
                )
                val panel = LightingPanel(activity, client) {}
                panel.render(snapshot)
                activity.setContentView(panel)
                checkNotNull(panel.findViewWithTag<View>("lighting.tab.effects")).performClick()

                val powerCard = checkNotNull(panel.findViewWithTag<View>("lighting.effects.power-card"))
                val patternsCard = checkNotNull(panel.findViewWithTag<View>("lighting.effects.patterns-card"))
                val power = checkNotNull(panel.findViewWithTag<Switch>("lighting.power"))
                val speed = checkNotNull(panel.findViewWithTag<SeekBar>("lighting.effect.speed"))
                check(ancestorWithTag(power, "lighting.effects.power-card") === powerCard)
                check(ancestorWithTag(speed, "lighting.effects.patterns-card") === patternsCard)
                check(powerCard !== patternsCard) {
                    "Swift Effects places Power and the effect controls in separate cards"
                }

                val saved = panel.saveUiState()
                val restoredPanel = LightingPanel(activity, client) {}
                restoredPanel.render(snapshot)
                restoredPanel.restoreUiState(saved)
                check(checkNotNull(restoredPanel.findViewWithTag<View>("lighting.tab.effects")).isSelected)
                checkNotNull(restoredPanel.findViewWithTag<View>("lighting.effects.power-card"))
                checkNotNull(restoredPanel.findViewWithTag<View>("lighting.effects.patterns-card"))
            }
        } finally { client.stop(); instrumentation.runOnMainSync { activity.finish() } }
    }

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
                // Start on the native thumb for the requested 40% value. A track tap
                // outside the thumb can seek immediately without entering the drag
                // lifecycle that protects an in-progress user edit from telemetry.
                val thumbCenterX = slider.thumb.bounds.centerX().toFloat()
                val centerY = slider.height / 2f
                val now = android.os.SystemClock.uptimeMillis()
                val down = MotionEvent.obtain(now, now, MotionEvent.ACTION_DOWN, thumbCenterX, centerY, 0)
                slider.dispatchTouchEvent(down)
                down.recycle()
                val slop = android.view.ViewConfiguration.get(slider.context).scaledTouchSlop
                val moveTime = now + 16
                val movedX = (thumbCenterX + slop * 3f).coerceAtMost((slider.width - slider.paddingRight).toFloat())
                val move = MotionEvent.obtain(now, moveTime, MotionEvent.ACTION_MOVE, movedX, centerY, 0)
                slider.dispatchTouchEvent(move)
                move.recycle()
                check(slider.isPressed) { "Brightness slider did not enter its native tracking state" }
                val duringDrag = snapshot.copy(requested = requested.copy(brightness = 63u))
                panel.render(duringDrag)
                check(panel.findViewWithTag<SeekBar>("lighting.brightness") === slider && slider.isPressed) {
                    "A telemetry snapshot replaced the brightness control during a drag"
                }
                val cancelTime = moveTime + 16
                val cancel = MotionEvent.obtain(now, cancelTime, MotionEvent.ACTION_CANCEL, movedX, centerY, 0)
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
    fun presetSaveReceiptClearsOnlyTheDraftRevisionThatWasSubmitted() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as Activity
        val client = AndroidMelkLightingClient(activity.applicationContext)
        try {
            instrumentation.runOnMainSync {
                val requested = MobileMelkLightingRestoreStateDto(false, 255u, 0u, 0u, 40u, null)
                val snapshot = LightingPresentation(
                    MobileMelkLightingSessionSnapshotDto(
                        MobileMelkLightingSessionStateDto.Ready, "synthetic-light", "Test light", 0u, true,
                    ),
                    emptyList(), "synthetic-light", null, null, false, requested, null, emptyList(),
                    permissionMissing = false,
                )
                val submissions = mutableListOf<String>()
                val panel = LightingPanel(activity, client, {}, submissions::add)
                panel.render(snapshot)
                activity.setContentView(panel)

                var editor = checkNotNull(panel.findViewWithTag<EditText>("lighting.preset.name"))
                editor.isFocusableInTouchMode = true
                check(editor.requestFocus())
                editor.setText("Scene submitted")
                checkNotNull(panel.findViewWithTag<View>("lighting.preset.save")).performClick()
                check(submissions == listOf("Scene submitted"))

                // The user keeps typing while the addPreset receipt is pending.
                editor.setText("Newer unfinished draft")
                val save = checkNotNull(panel.findViewWithTag<Button>("lighting.preset.save"))
                check(!save.isEnabled) { "A second save must remain disabled until the first receipt arrives" }
                // Android's performClick invokes listeners even for disabled Views,
                // so this also exercises the production admission guard directly.
                save.performClick()
                check(submissions == listOf("Scene submitted")) { "Overlapping preset saves must be coalesced" }

                panel.render(snapshot.copy(error = "Preset store unavailable"))
                check(save.isEnabled) { "A failed save receipt should allow retry of the preserved draft" }
                save.performClick()
                check(submissions == listOf("Scene submitted", "Newer unfinished draft")) {
                    "Retry should submit the current draft after the earlier failure is received"
                }
                val firstReceipt = snapshot.copy(presets = listOf(MobileRgbLightingPresetDto("Scene submitted", requested)))
                panel.render(firstReceipt)
                check(editor.text.toString() == "Newer unfinished draft") {
                    "A successful save receipt cleared a newer draft typed after submission"
                }

                editor.clearFocus()
                panel.render(firstReceipt.copy(requested = requested.copy(brightness = 41u)))
                editor = checkNotNull(panel.findViewWithTag<EditText>("lighting.preset.name"))
                check(editor.text.toString() == "Newer unfinished draft") {
                    "A controls rebuild resurrected the submitted name or lost the newer draft"
                }

                editor.requestFocus()
                val secondReceipt = firstReceipt.copy(
                    presets = firstReceipt.presets + MobileRgbLightingPresetDto("Newer unfinished draft", requested),
                )
                panel.render(secondReceipt)
                check(editor.text.isEmpty()) {
                    "A receipt for the still-current submitted draft should clear its focused editor"
                }
                editor.clearFocus()
                panel.render(secondReceipt.copy(requested = requested.copy(brightness = 42u)))
                check(checkNotNull(panel.findViewWithTag<EditText>("lighting.preset.name")).text.isEmpty()) {
                    "The cleared draft reappeared after a controls rebuild"
                }
            }
        } finally { client.stop(); instrumentation.runOnMainSync { activity.finish() } }
    }

    @Test
    fun presetDraftSurvivesEditorFocusLossAndGroupNamesUseSharedLocalization() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as Activity
        val client = AndroidMelkLightingClient(activity.applicationContext)
        try {
            lateinit var panel: LightingPanel
            lateinit var originalEditor: EditText
            instrumentation.runOnMainSync {
                panel = LightingPanel(activity, client) {}
                val requested = MobileMelkLightingRestoreStateDto(false, 255u, 0u, 0u, 40u, null)
                val snapshot = LightingPresentation(
                    MobileMelkLightingSessionSnapshotDto(
                        MobileMelkLightingSessionStateDto.Ready, "synthetic-light", "Test light", 0u, true,
                    ),
                    emptyList(), "synthetic-light", null, null, false, requested, null, emptyList(),
                    permissionMissing = false,
                )
                activity.setContentView(panel)
                panel.render(snapshot)
                checkNotNull(panel.findViewWithTag<View>("lighting.tab.effects")).performClick()
                val spinner = checkNotNull(panel.findViewWithTag<Spinner>("lighting.pattern.group"))
                val labels = CutoutUi(activity)
                val rustGroups = mobileMelkLightingEffectGroups()
                val displayedGroups = (spinner.adapter as android.widget.ArrayAdapter<*>).let { adapter ->
                    (0 until adapter.count).map { adapter.getItem(it).toString() }
                }
                check(displayedGroups == rustGroups.map { group ->
                    val label = labels.label("lighting.effect.group." + group.name.lowercase(java.util.Locale.ROOT).replace(" ", "_"))
                    if (label == labels.label("setting_unknown")) group.name else label
                }) { "The effect picker must localize Rust group names without changing their Rust-owned order" }
                val heading = checkNotNull(panel.findViewWithTag<TextView>("lighting.pattern.group.heading"))
                check(heading.text.toString().contains(displayedGroups[spinner.selectedItemPosition])) {
                    "The group-pattern heading should use the same localized group name as the picker"
                }

                val name = checkNotNull(panel.findViewWithTag<EditText>("lighting.preset.name"))
                originalEditor = name
                name.isFocusableInTouchMode = true
                check(name.requestFocus())
                name.setText("Unfinished scene")
                name.clearFocus()
                activity.window?.decorView?.clearFocus()
                panel.render(snapshot.copy(requested = requested.copy(brightness = 81u)))
            }
            instrumentation.waitForIdleSync()
            instrumentation.runOnMainSync {
                val restored = checkNotNull(panel.findViewWithTag<EditText>("lighting.preset.name"))
                check(restored !== originalEditor) { "The focus-loss/snapshot path should exercise a rebuilt preset editor" }
                check(restored.text.toString() == "Unfinished scene") {
                    "An unfinished preset name must survive keyboard dismissal and a controls rebuild"
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

    private fun ancestorWithTag(view: View, expected: String): View? {
        var current: View? = view
        while (current != null) {
            if (current.tag == expected) return current
            current = current.parent as? View
        }
        return null
    }

    private fun descendants(view: View): Sequence<View> = children(view)
}
