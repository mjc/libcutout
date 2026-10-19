package lol.cutout

import android.text.Editable
import android.text.TextWatcher
import android.content.Context
import android.graphics.Color
import android.graphics.Canvas
import android.graphics.Bitmap
import android.graphics.Paint
import android.graphics.Typeface
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.os.Bundle
import android.widget.FrameLayout
import android.os.Looper
import android.widget.*
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.accessibility.AccessibilityEvent
import androidx.core.view.ViewCompat
import androidx.core.view.accessibility.AccessibilityNodeInfoCompat
import androidx.customview.widget.ExploreByTouchHelper
import uniffi.cutout_mobile_ffi.*
import java.util.Locale

/** Standalone MELK RGB route. Connection identity, profile validation and command framing stay Rust-owned. */
class LightingPanel internal constructor(
    context: Context,
    private val client: AndroidMelkLightingClient,
    private val onRequestBluetooth: () -> Unit,
    private val submitPreset: (String) -> Unit,
    private val submitSolidColor: (Int, Int, Int, Boolean) -> Unit = client::setSolidColor,
    private val submitBrightness: (Int) -> Unit = client::setBrightness,
    private val submitMusicPlayback: (MobileLightingPlaybackDto) -> Unit = client::applyPlayback,
    private val submitCandidateSelection: (String) -> Unit = client::selectCandidate,
    private val submitEffectSpeed: (Int, Int?) -> Unit = client::setEffectSpeed,
) : ScrollView(context) {
    constructor(context: Context, client: AndroidMelkLightingClient, onRequestBluetooth: () -> Unit) :
        this(context, client, onRequestBluetooth, client::savePreset)

    private val ui = CutoutUi(context)
    private val root = ui.column().apply { setPadding(ui.dp(20), ui.dp(16), ui.dp(20), ui.dp(30)) }
    private val connectionCard = ui.column().apply { setPadding(ui.dp(16), ui.dp(14), ui.dp(16), ui.dp(14)); background = ui.rounded(20) }
    private val connectionName = ui.text("", 18f, true)
    private val connectionStatus = ui.text("", 14f, color = ui.muted)
    private val accessoryDetails = ImageButton(context).apply {
        tag = "lighting.accessory.details"
        contentDescription = ui.label("lighting.accessory.details")
        setImageDrawable(CutoutIcon(CutoutSymbol.INFO, ui.muted))
        setPadding(ui.dp(12), ui.dp(12), ui.dp(12), ui.dp(12))
        background = null
        isFocusable = true
        setOnClickListener { showDetails() }
    }
    private val connectionActions = ui.row()
    private val selector = ui.row().apply {
        tag = "lighting.page-picker"
        setPadding(ui.dp(3), ui.dp(3), ui.dp(3), ui.dp(3))
        background = ui.rounded(13, ui.card, outlined = false)
    }
    private val pageTabs = linkedMapOf<Page, TextView>()
    private val controls = ui.column()
    private var active = false
    private var page = Page.COLOR
    private var current: LightingPresentation? = null
    private var trackingSliderCount = 0
    private var trackingColorWheel = false
    private var rebuildingControls = false
    // Preset-name entry is native draft state. The control tree is rebuilt from
    // Rust snapshots, so keep the unfinished text independently of that tree.
    private var presetNameDraft = ""
    private var presetDraftRevision = 0L
    private var presetNameEditor: EditText? = null
    private var presetSaveButton: Button? = null
    private var pendingPresetSave: PendingPresetSave? = null
    private var pairingDialog: android.app.AlertDialog? = null
    private var pairingCandidates: PairingCandidateAdapter? = null
    private var pairingEmptyState: TextView? = null

    private data class PendingPresetSave(val name: String, val existingNames: Set<String>, val draftRevision: Long)

    private enum class Page { COLOR, EFFECTS, MUSIC }

    init {
        isFillViewport = true
        setBackgroundColor(ui.background)
        addView(root)
        root.addView(ui.title(ui.label("lighting.title")))
        root.addView(ui.space(14))
        connectionCard.addView(ui.row().apply {
            addView(TextView(context).apply { text = "●"; textSize = 22f; setTextColor(ui.cyan); gravity = Gravity.CENTER; contentDescription = null }, LinearLayout.LayoutParams(ui.dp(38), ui.dp(38)))
            addView(ui.space(4))
            addView(ui.column().apply {
                addView(connectionName)
                addView(connectionStatus)
            }, LinearLayout.LayoutParams(0, -2, 1f))
            addView(accessoryDetails, LinearLayout.LayoutParams(ui.dp(44), ui.dp(44)))
        })
        connectionCard.addView(connectionActions)
        root.addView(connectionCard)
        root.addView(ui.space(14))
        Page.values().forEach { item ->
            val tab = TextView(context).apply {
                tag = "lighting.tab.${item.name.lowercase()}"
                text = ui.label("lighting.page.${item.name.lowercase()}")
                textSize = 14f
                setAutoSizeTextTypeUniformWithConfiguration(12, 14, 1, android.util.TypedValue.COMPLEX_UNIT_SP)
                setTextColor(ui.foreground)
                gravity = Gravity.CENTER
                includeFontPadding = false
                setSingleLine(true)
                ellipsize = android.text.TextUtils.TruncateAt.END
                minHeight = ui.dp(44)
                setPadding(ui.dp(4), 0, ui.dp(4), 0)
                isClickable = true
                isFocusable = true
                setOnClickListener { page = item; updatePagePicker(); renderSelected() }
                accessibilityDelegate = object : View.AccessibilityDelegate() {
                    override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                        super.onInitializeAccessibilityNodeInfo(host, info)
                        info.className = Button::class.java.name
                        info.isSelected = host.isSelected
                    }
                }
            }
            pageTabs[item] = tab
            selector.addView(tab, LinearLayout.LayoutParams(0, ui.dp(44), 1f))
        }
        updatePagePicker()
        root.addView(selector)
        root.addView(ui.space(14))
        root.addView(controls)
        render(client.currentPresentation())
        client.observe { render(it) }
    }

    /** Called by the host when this route becomes visible. A paired accessory remains connected when hidden. */
    fun setActive(value: Boolean) {
        if (active == value) return
        active = value
        if (value) client.resume() else {
            pairingDialog?.dismiss()
            client.stopIfUnpaired()
        }
    }

    override fun onDetachedFromWindow() {
        pairingDialog?.dismiss()
        controls.findViewWithTag<LightingColorWheelView>("lighting.color-wheel")?.clearVirtualFocusForDetach()
        trackingColorWheel = false
        trackingSliderCount = 0
        super.onDetachedFromWindow()
    }

    /** Saves only native page selection; accessory state remains owned by the Rust-backed client. */
    fun saveUiState(): Bundle = Bundle().apply { putString(STATE_PAGE, page.name) }

    /** Restores the selected native page without issuing a device command. */
    fun restoreUiState(state: Bundle?) {
        val restored = state?.getString(STATE_PAGE)?.let { name -> Page.values().firstOrNull { it.name == name } } ?: return
        page = restored
        updatePagePicker()
        if (current?.permissionMissing != true) renderSelected()
    }

    fun render(value: LightingPresentation) {
        if (Looper.myLooper() != Looper.getMainLooper()) { post { render(value) }; return }
            current = value
            acknowledgePresetSave(value)
            val snapshot = value.snapshot
            connectionName.text = value.alias?.takeIf(String::isNotBlank) ?: snapshot.name ?: ui.label("lighting.default_name")
            connectionName.tag = "lighting.accessory.name"
            connectionStatus.text = when {
                value.permissionMissing -> context.getString(R.string.lighting_bluetooth_permission)
                snapshot.state is MobileMelkLightingSessionStateDto.Retrying -> (snapshot.state as MobileMelkLightingSessionStateDto.Retrying).let { context.getString(R.string.lighting_retrying, it.attempt.toInt(), maxOf(1, ((it.delayMilliseconds.toLong() + 999) / 1000).toInt())) }
                snapshot.state is MobileMelkLightingSessionStateDto.Failed -> context.getString(R.string.lighting_failed, (snapshot.state as MobileMelkLightingSessionStateDto.Failed).reason)
                else -> ui.label("lighting.state." + when (snapshot.state) {
                    MobileMelkLightingSessionStateDto.Idle -> "idle"
                    MobileMelkLightingSessionStateDto.Scanning -> "scanning"
                    MobileMelkLightingSessionStateDto.Connecting -> "connecting"
                    is MobileMelkLightingSessionStateDto.Retrying -> "retrying"
                    MobileMelkLightingSessionStateDto.Discovering -> "discovering"
                    MobileMelkLightingSessionStateDto.Ready -> "ready"
                    MobileMelkLightingSessionStateDto.Disconnected -> "disconnected"
                    is MobileMelkLightingSessionStateDto.Failed -> "failed"
                })
            }
            connectionStatus.tag = "lighting.accessory.status"
            connectionStatus.setTextColor(if (snapshot.state == MobileMelkLightingSessionStateDto.Ready) ui.green else if (snapshot.state is MobileMelkLightingSessionStateDto.Failed || value.error != null) ui.red else ui.muted)
            connectionActions.removeAllViews()
            if (value.permissionMissing) connectionActions.addView(ui.action(context.getString(R.string.lighting_enable_bluetooth), primary = true, onClick = onRequestBluetooth))
            else if (snapshot.state != MobileMelkLightingSessionStateDto.Ready) connectionActions.addView(ui.action(ui.label("lighting.choose_accessory"), primary = true, onClick = { showPairing() }).apply { tag = "lighting.choose-accessory" })
            val error = value.error
            if (error != null) connectionActions.addView(ui.text(error, 13f, color = ui.orange).apply { setPadding(ui.dp(8), ui.dp(8), ui.dp(8), ui.dp(4)) })
            updatePairingDialog(value)
            updatePagePicker()
            if (value.permissionMissing) { controls.removeAllViews(); return }
            if (!ready() || !hasActiveControlInteraction()) renderSelected()
    }

    private fun updatePagePicker() {
        pageTabs.forEach { (item, tab) ->
            val selected = page == item
            tab.isSelected = selected
            tab.typeface = Typeface.create("sans-serif", if (selected) Typeface.BOLD else Typeface.NORMAL)
            tab.background = if (selected) ui.rounded(10, ui.stroke, outlined = false) else null
        }
    }

    private fun ready() = current?.snapshot?.state == MobileMelkLightingSessionStateDto.Ready && current?.permissionMissing == false

    private fun hasActiveControlInteraction(): Boolean = trackingSliderCount > 0 || trackingColorWheel ||
        controls.anyView { it is EditText && it.hasFocus() }

    private fun ViewGroup.anyView(predicate: (View) -> Boolean): Boolean {
        for (index in 0 until childCount) {
            val child = getChildAt(index)
            if (predicate(child) || (child is ViewGroup && child.anyView(predicate))) return true
        }
        return false
    }

    private fun trackSlider(started: Boolean) {
        trackingSliderCount = (trackingSliderCount + if (started) 1 else -1).coerceAtLeast(0)
    }

    private fun scheduleControlRefreshAfterInteraction() {
        post {
            if (ready() && !hasActiveControlInteraction() && current != null) renderSelected()
        }
    }

    private fun renderSelected() {
        rebuildingControls = true
        try {
            trackingSliderCount = 0
            trackingColorWheel = false
            controls.removeAllViews()
            val state = current?.requested ?: current?.confirmed ?: defaultState()
            when (page) {
                Page.COLOR -> renderColor(state)
                Page.EFFECTS -> renderEffects(state)
                Page.MUSIC -> renderMusic(state)
            }
            renderBrightness(state)
            renderPresets(state)
            installFocusRefreshListeners(controls)
        } finally {
            rebuildingControls = false
        }
    }

    private fun installFocusRefreshListeners(parent: ViewGroup) {
        for (index in 0 until parent.childCount) {
            val child = parent.getChildAt(index)
            if (child is EditText) {
                child.onFocusChangeListener = View.OnFocusChangeListener { _, hasFocus ->
                    if (!hasFocus && !rebuildingControls) post { if (!hasActiveControlInteraction() && current != null) renderSelected() }
                }
            }
            if (child is ViewGroup) installFocusRefreshListeners(child)
        }
    }

    private fun renderColor(state: MobileMelkLightingRestoreStateDto) {
        val card = card()
        card.addView(powerControl(state))
        card.addView(ui.text(ui.label("lighting.solid_color"), 18f, true))
        val hsv = FloatArray(3).also { Color.RGBToHSV(state.red.toInt(), state.green.toInt(), state.blue.toInt(), it) }
        val wheel = LightingColorWheelView(context, hsv[0], hsv[1],
            onInteractionChanged = { tracking ->
                trackingColorWheel = tracking
                if (!tracking) scheduleControlRefreshAfterInteraction()
            },
            onColor = { red, green, blue, final -> submitSolidColor(red, green, blue, !final) },
        ).apply {
            tag = "lighting.color-wheel"
            contentDescription = ui.label("lighting.color_wheel"); isEnabled = ready()
            layoutParams = LinearLayout.LayoutParams(-1, ui.dp(248))
        }
        card.addView(wheel)
        card.addView(ui.action(ui.label("lighting.set_white")) { client.setSolidColor(255, 255, 255) }.apply { isEnabled = ready(); tag = "lighting.set-white" })
        controls.addView(card)
    }

    private fun powerControl(state: MobileMelkLightingRestoreStateDto): View = ui.row().apply {
        addView(ui.text(ui.label("lighting.power"), 18f, true), LinearLayout.LayoutParams(0, -2, 1f))
        val toggle = Switch(context).apply {
            isChecked = state.powerOn; isEnabled = ready(); contentDescription = ui.label("lighting.power"); tag = "lighting.power"
            setOnCheckedChangeListener { _, checked -> client.setPower(checked) }
        }
        addView(toggle)
    }

    private fun renderBrightness(state: MobileMelkLightingRestoreStateDto) {
        val card = card()
        card.addView(ui.row().apply {
            addView(ui.text(ui.label("lighting.brightness"), 18f, true), LinearLayout.LayoutParams(0, -2, 1f))
            addView(ui.text(context.getString(R.string.lighting_percent, state.brightness.toInt()), 14f, color = ui.muted))
        })
        var brightnessTouchTracking = false
        val slider = SeekBar(context).apply {
            max = 100; progress = state.brightness.toInt(); isEnabled = ready()
            contentDescription = ui.label("lighting.brightness"); tag = "lighting.brightness"
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(seekBar: SeekBar?, progress: Int, fromUser: Boolean) {
                    if (!fromUser) return
                    (seekBar?.parent as? LinearLayout)?.let { (it.getChildAt(0) as? LinearLayout)?.let { row -> (row.getChildAt(1) as? TextView)?.text = context.getString(R.string.lighting_percent, progress) } }
                    if (!brightnessTouchTracking && ready()) submitBrightness(progress)
                }
                override fun onStartTrackingTouch(seekBar: SeekBar?) { brightnessTouchTracking = true; trackSlider(true) }
                override fun onStopTrackingTouch(seekBar: SeekBar?) {
                    brightnessTouchTracking = false
                    trackSlider(false)
                    if (ready()) seekBar?.let { submitBrightness(it.progress) }
                    scheduleControlRefreshAfterInteraction()
                }
            })
        }
        card.addView(slider)
        controls.addView(card)
    }

    private fun renderEffects(state: MobileMelkLightingRestoreStateDto) {
        controls.addView(card().apply {
            tag = "lighting.effects.power-card"
            addView(powerControl(state))
        })
        controls.addView(ui.space(16))
        val card = card().apply { tag = "lighting.effects.patterns-card" }
        card.addView(ui.text(ui.label("lighting.patterns"), 18f, true))
        val patterns = mobileMelkLightingPatternCatalog().filter { it.verified }
        val groups = mobileMelkLightingEffectGroups()
        val currentPattern = (state.playback as? MobileLightingPlaybackDto.Effect)?.pattern?.toInt()
        val groupNames = groups.map { it.name }
        val groupLabels = groupNames.map(::localizedGroupName)
        val selectedGroup = groups.firstOrNull { it.effectIds.any { id -> id.toUByte().toInt() == currentPattern } }?.name ?: groupNames.firstOrNull().orEmpty()
        val spinner = Spinner(context).apply {
            adapter = ArrayAdapter(context, android.R.layout.simple_spinner_dropdown_item, groupLabels)
            setSelection(groupNames.indexOf(selectedGroup).coerceAtLeast(0)); isEnabled = ready(); contentDescription = ui.label("lighting.pattern_group"); tag = "lighting.pattern.group"
        }
        card.addView(spinner)
        card.addView(ui.text(ui.label("lighting.quick_picks"), 15f, true, ui.muted))
        val favorites = listOf(1, 16, 22, 75).mapNotNull { id -> patterns.firstOrNull { it.id.toInt() == id } }
        addPatternRows(card, favorites, currentPattern)
        val groupHeading = ui.text("", 15f, true, ui.muted).apply { tag = "lighting.pattern.group.heading" }
        card.addView(groupHeading)
        val rows = ui.column(); card.addView(rows)
        fun populate() {
            rows.removeAllViews()
            val groupName = groupNames.getOrNull(spinner.selectedItemPosition)
            groupHeading.text = formatLabel("lighting.group_patterns", groupName?.let(::localizedGroupName).orEmpty())
            val ids = groups.firstOrNull { it.name == groupName }?.effectIds?.map { it.toUByte().toInt() }?.toSet().orEmpty()
            addPatternRows(rows, patterns.filter { it.id.toInt() in ids }, currentPattern)
        }
        spinner.onItemSelectedListener = object : android.widget.AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: android.widget.AdapterView<*>?) = Unit
            override fun onItemSelected(parent: android.widget.AdapterView<*>?, view: View?, position: Int, id: Long) = populate()
        }
        populate()
        card.addView(ui.text(ui.label("lighting.effect.speed"), 18f, true))
        card.addView(ui.row().apply {
            addView(ui.text(ui.label("lighting.slower"), 13f, color = ui.muted)); addView(ui.space(4)); addView(ui.text(ui.label("lighting.faster"), 13f, color = ui.muted))
        })
        val speed = (state.playback as? MobileLightingPlaybackDto.Effect)?.speed?.toInt() ?: 128
        card.addView(SeekBar(context).apply { max = 255; progress = 255 - speed; isEnabled = ready(); contentDescription = ui.label("lighting.effect.speed"); tag = "lighting.effect.speed"
            val initialPattern = (state.playback as? MobileLightingPlaybackDto.Effect)?.pattern?.toInt()
            var touchTracking = false
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(s: SeekBar?, p: Int, user: Boolean) {
                    if (user && !touchTracking && ready()) submitEffectSpeed(255 - p, initialPattern)
                }
                override fun onStartTrackingTouch(s: SeekBar?) { touchTracking = true; trackSlider(true) }
                override fun onStopTrackingTouch(s: SeekBar?) {
                    touchTracking = false
                    trackSlider(false)
                    if (ready()) s?.let { submitEffectSpeed(255 - it.progress, initialPattern) }
                    scheduleControlRefreshAfterInteraction()
                }
            }) })
        controls.addView(card)
    }

    private fun addPatternRows(parent: LinearLayout, values: List<MobileMelkLightingPatternDto>, selected: Int?) {
        values.chunked(2).forEach { group ->
            parent.addView(ui.row().apply { group.forEach { pattern ->
                val isSelected = pattern.id.toInt() == selected
                val preview = EffectPreviewView(context, pattern.id.toInt(), pattern.group).apply { layoutParams = LinearLayout.LayoutParams(-1, ui.dp(70)) }
                val title = ui.text(pattern.name, 14f, true).apply { gravity = Gravity.CENTER; maxLines = 2 }
                val effectId = ui.text(context.getString(R.string.lighting_effect_id, pattern.id.toInt()), 11f, color = ui.muted).apply { gravity = Gravity.CENTER }
                val button = ui.column().apply {
                    tag = "lighting.effect.${pattern.id.toInt()}"; gravity = Gravity.CENTER
                    setPadding(ui.dp(8), ui.dp(8), ui.dp(8), ui.dp(8)); background = ui.rounded(14, if (isSelected) Color.rgb(74, 45, 105) else ui.card)
                    addView(preview); addView(ui.space(7)); addView(title); addView(effectId)
                    isEnabled = ready(); isClickable = true; isFocusable = true; contentDescription = "${pattern.name}, ${context.getString(R.string.lighting_effect_id, pattern.id.toInt())}"
                    accessibilityDelegate = object : View.AccessibilityDelegate() {
                        override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                            super.onInitializeAccessibilityNodeInfo(host, info); info.className = Button::class.java.name; info.isSelected = isSelected
                        }
                    }
                    setOnClickListener {
                        val speed = (current?.requested?.playback as? MobileLightingPlaybackDto.Effect)?.speed ?: 128u
                        client.applyPlayback(MobileLightingPlaybackDto.Effect(pattern.id, speed))
                    }
                }
                addView(button, LinearLayout.LayoutParams(0, ui.dp(132), 1f)); addView(ui.space(5))
            } })
        }
    }

    private fun renderMusic(state: MobileMelkLightingRestoreStateDto) {
        val card = card(); card.addView(powerControl(state)); card.addView(ui.text(ui.label("lighting.page.music"), 18f, true))
        val names = listOf("flow_flash", "flash", "rainbow", "snake", "rainbow_2", "pulse", "flow", "pulse_2")
        val labels = names.map { ui.label("lighting.music.effect.$it") }
        val playback = state.playback as? MobileLightingPlaybackDto.Music
        val selection = Spinner(context).apply { adapter = ArrayAdapter(context, android.R.layout.simple_spinner_dropdown_item, labels); setSelection((playback?.effect?.toInt() ?: 0).coerceIn(0, labels.lastIndex)); isEnabled = ready(); contentDescription = ui.label("lighting.music.effect") }
        card.addView(selection)
        selection.onItemSelectedListener = object : android.widget.AdapterView.OnItemSelectedListener {
            private var initial = true
            override fun onNothingSelected(parent: android.widget.AdapterView<*>?) = Unit
            override fun onItemSelected(parent: android.widget.AdapterView<*>?, view: View?, position: Int, id: Long) {
                if (initial) { initial = false; return }
                client.applyPlayback(MobileLightingPlaybackDto.Music(position.toUByte(), (playback?.sensitivity ?: 50u)))
            }
        }
        card.addView(ui.text(ui.label("lighting.sensitivity"), 17f, true))
        val sensitivity = playback?.sensitivity?.toInt() ?: 50
        var sensitivityTouchTracking = false
        card.addView(SeekBar(context).apply { max = 100; progress = sensitivity; isEnabled = ready(); contentDescription = ui.label("lighting.microphone_sensitivity"); tag = "lighting.music.sensitivity"
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(s: SeekBar?, p: Int, user: Boolean) {
                    if (user && !sensitivityTouchTracking && ready()) {
                        submitMusicPlayback(MobileLightingPlaybackDto.Music(selection.selectedItemPosition.toUByte(), p.toUByte()))
                    }
                }
                override fun onStartTrackingTouch(s: SeekBar?) { sensitivityTouchTracking = true; trackSlider(true) }
                override fun onStopTrackingTouch(s: SeekBar?) {
                    sensitivityTouchTracking = false
                    trackSlider(false)
                    if (ready()) s?.let { submitMusicPlayback(MobileLightingPlaybackDto.Music(selection.selectedItemPosition.toUByte(), it.progress.toUByte())) }
                    scheduleControlRefreshAfterInteraction()
                }
            }) })
        card.addView(ui.action(ui.label("lighting.stop_music")) { client.stopMusic() }.apply { isEnabled = ready() })
        controls.addView(card)
    }

    private fun renderPresets(state: MobileMelkLightingRestoreStateDto) {
        val card = card(); card.addView(ui.text(ui.label("lighting.preset.scenes"), 18f, true))
        val quick = listOf(Triple("lighting.preset.red", 255, 0 to 0), Triple("lighting.preset.blue", 0, 0 to 255), Triple("lighting.preset.night", 16, 20 to 32))
        card.addView(ui.row().apply {
            quick.forEach { (key, red, rest) ->
                val preset = LinearLayout(context).apply {
                    tag = "lighting.quick-preset.${key.substringAfterLast('.') }"
                    gravity = Gravity.CENTER
                    orientation = LinearLayout.VERTICAL
                    minimumHeight = ui.dp(64)
                    isClickable = true
                    isFocusable = true
                    isEnabled = ready()
                    contentDescription = ui.label(key)
                    addView(View(context).apply {
                        background = android.graphics.drawable.GradientDrawable().apply {
                            shape = android.graphics.drawable.GradientDrawable.OVAL
                            setColor(Color.rgb(red, rest.first, rest.second))
                            setStroke(ui.dp(1), ui.stroke)
                        }
                        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
                    }, LinearLayout.LayoutParams(ui.dp(42), ui.dp(42)))
                    addView(ui.text(ui.label(key), 12f, color = ui.muted).apply { gravity = Gravity.CENTER })
                    accessibilityDelegate = object : View.AccessibilityDelegate() {
                        override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                            super.onInitializeAccessibilityNodeInfo(host, info)
                            info.className = Button::class.java.name
                        }
                    }
                    setOnClickListener { if (ready()) client.setSolidColor(red, rest.first, rest.second) }
                }
                addView(preset, LinearLayout.LayoutParams(0, -2, 1f))
            }
        })
        current?.presets.orEmpty().forEach { preset ->
            val summary = presetSummary(preset.requested)
            val row = ui.column().apply {
                tag = "lighting.preset.${preset.name}"; setPadding(ui.dp(14), ui.dp(10), ui.dp(14), ui.dp(10))
                background = ui.rounded(14); isEnabled = ready(); isClickable = true; isFocusable = true
                addView(ui.text(preset.name, 16f, true))
                addView(ui.text(summary, 13f, color = ui.muted))
                contentDescription = context.getString(R.string.lighting_preset_accessibility, preset.name, summary)
                setOnClickListener { if (ready()) client.applyPreset(preset) }
                setOnLongClickListener { android.app.AlertDialog.Builder(context).setItems(arrayOf(ui.label("lighting.preset.replace"), ui.label("lighting.preset.delete"))) { _, which -> if (which == 0) client.replacePreset(preset.name) else client.removePreset(preset.name) }.show(); true }
                accessibilityDelegate = object : View.AccessibilityDelegate() {
                    override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                        super.onInitializeAccessibilityNodeInfo(host, info); info.className = Button::class.java.name
                    }
                }
            }
            card.addView(row, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = ui.dp(8) })
        }
        val name = EditText(context).apply {
            hint = ui.label("lighting.preset.name"); setSingleLine(true); contentDescription = ui.label("lighting.preset.name")
            tag = "lighting.preset.name"
            setText(presetNameDraft)
        }
        presetNameEditor = name
        val saveButton = ui.action(ui.label("lighting.save")) { requestPresetSave(name.text.toString()) }.apply {
            tag = "lighting.preset.save"
            isEnabled = ready() && presetNameDraft.isNotBlank() && pendingPresetSave == null
        }
        presetSaveButton = saveButton
        name.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(text: CharSequence?, start: Int, count: Int, after: Int) = Unit
            override fun onTextChanged(text: CharSequence?, start: Int, before: Int, count: Int) = Unit
            override fun afterTextChanged(text: Editable?) {
                val next = text?.toString().orEmpty()
                if (next != presetNameDraft) {
                    presetNameDraft = next
                    presetDraftRevision++
                }
                updatePresetSaveButton()
            }
        })
        card.addView(ui.row().apply {
            addView(name, LinearLayout.LayoutParams(0, -2, 1f))
            addView(saveButton)
        })
        val confirmed = current?.confirmed
        if (confirmed != null) card.addView(ui.text(context.getString(R.string.lighting_confirmed, confirmed.red.toInt(), confirmed.green.toInt(), confirmed.blue.toInt()), 13f, color = ui.muted))
        val switch = Switch(context).apply { text = ui.label("lighting.restore.toggle"); isChecked = current?.restoreEnabled == true }
        // Restore preference is Rust persisted; only expose the switch for a paired identity.
        if (current?.snapshot?.platformIdentifier != null) {
            card.addView(switch.apply { isEnabled = true; contentDescription = text; setOnCheckedChangeListener { _, checked -> client.setRestoreEnabled(checked) } })
        }
        current?.associatedVehicle?.let { card.addView(ui.text(context.getString(R.string.lighting_associated_vehicle, it), 13f, color = ui.muted)) }
        controls.addView(card)
    }

    private fun card() = ui.column().apply { setPadding(ui.dp(16), ui.dp(16), ui.dp(16), ui.dp(16)); background = ui.rounded(20); addView(ui.space(2)) }
    private fun defaultState() = MobileMelkLightingRestoreStateDto(false, 255u, 0u, 0u, 100u, null)

    private fun localizedGroupName(name: String): String {
        val key = name.lowercase(Locale.ROOT).replace(" ", "_")
        val translated = ui.label("lighting.effect.group.$key")
        return translated.takeUnless { it == ui.label("setting_unknown") } ?: name
    }

    private fun formatLabel(key: String, value: String): String = String.format(
        Locale.getDefault(), ui.label(key).replace("%@", "%s"), value,
    )

    private fun requestPresetSave(name: String) {
        val trimmed = name.trim()
        if (!ready() || trimmed.isEmpty() || pendingPresetSave != null) return
        pendingPresetSave = PendingPresetSave(
            trimmed,
            current?.presets.orEmpty().map { it.name }.toSet(),
            presetDraftRevision,
        )
        updatePresetSaveButton()
        submitPreset(trimmed)
    }

    private fun updatePresetSaveButton() {
        presetSaveButton?.isEnabled = ready() && presetNameDraft.isNotBlank() && pendingPresetSave == null
    }

    private fun acknowledgePresetSave(value: LightingPresentation) {
        val pending = pendingPresetSave ?: return
        if (value.error != null) {
            pendingPresetSave = null
            updatePresetSaveButton()
            return
        }
        if (pending.name !in pending.existingNames && value.presets.any { it.name == pending.name }) {
            // The client publishes its refreshed Rust-backed store only after
            // addPreset succeeds. Failed/duplicate requests leave the draft intact.
            if (presetDraftRevision == pending.draftRevision && presetNameDraft.trim() == pending.name) {
                val editor = presetNameEditor
                if (editor != null && editor.text.toString().trim() == pending.name) {
                    editor.setText("")
                } else {
                    presetNameDraft = ""
                    presetDraftRevision++
                }
            }
            pendingPresetSave = null
            updatePresetSaveButton()
        }
    }

    private companion object { const val STATE_PAGE = "lighting.selected-page" }

    private fun showPairing() {
        val presentation = current ?: return
        if (presentation.permissionMissing) { onRequestBluetooth(); return }
        val rows = ListView(context).apply { tag = "lighting.pairing.candidates" }
        val empty = ui.text("", 15f, color = ui.muted).apply {
            gravity = Gravity.CENTER
            setPadding(ui.dp(24), ui.dp(24), ui.dp(24), ui.dp(24))
        }
        val content = FrameLayout(context).apply {
            addView(rows, FrameLayout.LayoutParams(-1, ui.dp(320)))
            addView(empty, FrameLayout.LayoutParams(-1, ui.dp(320)))
        }
        val adapter = PairingCandidateAdapter()
        rows.adapter = adapter
        rows.emptyView = empty
        rows.setOnItemClickListener { _, view, _, _ ->
            val latest = current ?: return@setOnItemClickListener
            if (!canSelectCandidate(latest)) return@setOnItemClickListener
            val identifier = (view as? PairingCandidateAdapter.CandidateRow)?.platformIdentifier
                ?: return@setOnItemClickListener
            if (latest.candidates.any { it.platformIdentifier == identifier }) {
                submitCandidateSelection(identifier)
            }
        }
        val dialog = android.app.AlertDialog.Builder(context)
            .setTitle(ui.label("lighting.pairing.nearby_title"))
            .setView(content)
            .setPositiveButton(ui.label("lighting.retry"), null)
            .setNegativeButton(android.R.string.cancel, null)
            .create()
        dialog.setOnDismissListener {
            if (pairingDialog === dialog) {
                pairingDialog = null
                pairingCandidates = null
                pairingEmptyState = null
            }
        }
        pairingDialog = dialog
        pairingCandidates = adapter
        pairingEmptyState = empty
        dialog.show()
        dialog.getButton(android.app.AlertDialog.BUTTON_POSITIVE).setOnClickListener {
            val latest = current ?: return@setOnClickListener
            if (canReconnect(latest)) client.resume()
        }
        updatePairingDialog(presentation)
    }

    private fun canSelectCandidate(value: LightingPresentation): Boolean =
        value.snapshot.state == MobileMelkLightingSessionStateDto.Idle ||
            value.snapshot.state == MobileMelkLightingSessionStateDto.Scanning

    private fun canReconnect(value: LightingPresentation): Boolean =
        value.snapshot.state == MobileMelkLightingSessionStateDto.Disconnected ||
            value.snapshot.state is MobileMelkLightingSessionStateDto.Failed

    private fun updatePairingDialog(value: LightingPresentation) {
        val dialog = pairingDialog?.takeIf { it.isShowing } ?: return
        pairingCandidates?.replace(value.candidates, canSelectCandidate(value))
        pairingEmptyState?.text = when {
            value.snapshot.state == MobileMelkLightingSessionStateDto.Scanning -> ui.label("lighting.connection.scanning")
            else -> ui.label("lighting.connection.ready_to_scan")
        }
        val retry = dialog.getButton(android.app.AlertDialog.BUTTON_POSITIVE)
        retry?.visibility = if (canReconnect(value)) View.VISIBLE else View.GONE
        retry?.isEnabled = canReconnect(value)
    }

    private inner class PairingCandidateAdapter : android.widget.BaseAdapter() {
        private var items = emptyList<MobileMelkLightingSessionCandidateDto>()
        private var selectable = false
        private val stableIds = mutableMapOf<String, Long>()
        private var nextStableId = 1L

        inner class CandidateRow(context: Context) : LinearLayout(context) {
            var platformIdentifier: String? = null
        }

        fun replace(next: List<MobileMelkLightingSessionCandidateDto>, canSelect: Boolean) {
            items = next.toList()
            selectable = canSelect
            notifyDataSetChanged()
        }

        override fun getCount() = items.size
        override fun getItem(position: Int) = items.getOrNull(position)
        override fun getItemId(position: Int): Long = items.getOrNull(position)?.platformIdentifier?.let { identifier ->
            stableIds.getOrPut(identifier) { nextStableId++ }
        } ?: Long.MIN_VALUE + position
        override fun hasStableIds() = true
        override fun areAllItemsEnabled() = false
        override fun isEnabled(position: Int) = selectable && getItem(position) != null

        override fun getView(position: Int, convertView: View?, parent: ViewGroup): View {
            val candidate = checkNotNull(getItem(position))
            val row = (convertView as? CandidateRow) ?: CandidateRow(context).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                minimumHeight = ui.dp(56)
                setPadding(ui.dp(16), ui.dp(8), ui.dp(16), ui.dp(8))
                addView(ui.icon(CutoutSymbol.LIGHTING, ui.cyan, 22))
                val text = ui.column().apply {
                    addView(ui.text("", 16f, true))
                    addView(ui.text("", 13f, color = ui.muted))
                }
                text.tag = "lighting.pairing.row-text"
                addView(text, LinearLayout.LayoutParams(0, -2, 1f))
                addView(ui.icon(CutoutSymbol.CHEVRON, ui.muted, 16))
            }
            row.platformIdentifier = candidate.platformIdentifier
            row.tag = "lighting.candidate.${candidate.platformIdentifier}"
            row.isEnabled = isEnabled(position)
            row.alpha = if (row.isEnabled) 1f else 0.56f
            val text = row.findViewWithTag<LinearLayout>("lighting.pairing.row-text")
            (text.getChildAt(0) as TextView).text = candidate.name?.takeIf(String::isNotBlank)
                ?: ui.label("lighting.pairing.unknown_name")
            (text.getChildAt(1) as TextView).text = context.getString(R.string.signal, candidate.rssi)
            row.contentDescription = "${(text.getChildAt(0) as TextView).text}, ${(text.getChildAt(1) as TextView).text}"
            return row
        }
    }

    private fun showDetails() {
        val presentation = current ?: return
        if (presentation.pairedIdentifier == null) { showPairing(); return }
        val name = EditText(context).apply {
            setSingleLine(true); hint = ui.label("lighting.alias.placeholder"); setText(presentation.alias.orEmpty())
        }
        val content = ui.column().apply {
            setPadding(ui.dp(12), ui.dp(4), ui.dp(12), 0)
            addView(name)
            val restore = Switch(context).apply {
                text = ui.label("lighting.restore.toggle"); isChecked = presentation.restoreEnabled
                setOnCheckedChangeListener { _, checked -> client.setRestoreEnabled(checked) }
            }
            addView(restore)
            addView(ui.text(
                if (presentation.associatedVehicle != null) context.getString(R.string.lighting_associated_vehicle, presentation.associatedVehicle)
                else context.getString(R.string.lighting_association_none), 14f, color = ui.muted,
            ))
            val selectedVehicle = presentation.selectedVehicle
            if (selectedVehicle != null) {
                addView(ui.action(
                    context.getString(if (presentation.associatedVehicle == selectedVehicle) R.string.lighting_association_current else R.string.lighting_association_set_current),
                ) { client.setAssociatedVehicle(if (presentation.associatedVehicle == selectedVehicle) null else selectedVehicle) })
            }
            if (presentation.associatedVehicle != null && presentation.associatedVehicle != selectedVehicle) {
                addView(ui.action(context.getString(R.string.lighting_association_clear)) { client.setAssociatedVehicle(null) })
            }
        }
        android.app.AlertDialog.Builder(context).setTitle(presentation.alias ?: presentation.snapshot.name ?: ui.label("lighting.default_name"))
            .setView(content)
            .setPositiveButton(ui.label("lighting.save")) { _, _ -> client.setAlias(name.text.toString()) }
            .setNeutralButton(ui.label("lighting.forget")) { _, _ ->
                android.app.AlertDialog.Builder(context).setTitle(ui.label("lighting.forget.confirm_title")).setMessage(ui.label("lighting.forget.confirm_message"))
                    .setNegativeButton(android.R.string.cancel, null).setPositiveButton(ui.label("lighting.forget")) { _, _ -> client.forgetAccessory() }.show()
            }
            .setNegativeButton(android.R.string.cancel, null).show()
    }

    private fun presetSummary(requested: MobileMelkLightingRestoreStateDto): String = when (val playback = requested.playback) {
        is MobileLightingPlaybackDto.Effect -> context.getString(
            R.string.lighting_preset_effect_summary,
            mobileMelkLightingPatternCatalog().firstOrNull { it.id == playback.pattern }?.name
                ?: context.getString(R.string.lighting_unknown_effect, playback.pattern.toInt()),
            playback.speed.toInt(),
        )
        is MobileLightingPlaybackDto.Music -> context.getString(R.string.lighting_preset_music_summary, playback.sensitivity.toInt())
        MobileLightingPlaybackDto.Solid, null -> context.getString(R.string.lighting_preset_solid_summary, requested.brightness.toInt())
    }
}

/** Small static native rendering of the selected reference pattern, matching Swift's preview card role. */
private class EffectPreviewView(context: Context, private val patternId: Int, private val group: String) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val widthPx = this.width.toFloat(); val heightPx = this.height.toFloat()
        if (widthPx <= 0f || heightPx <= 0f) return
        paint.color = Color.rgb(18, 18, 22); paint.style = Paint.Style.FILL
        canvas.drawRoundRect(0f, 0f, widthPx, heightPx, 10f, 10f, paint)
        val count = 30
        val paddingX = widthPx * .08f; val paddingY = heightPx * .18f
        val usableW = widthPx - paddingX * 2; val usableH = heightPx - paddingY * 2
        for (index in 0 until count) {
            val progress = index.toFloat() / (count - 1)
            val (x, y) = when (group) {
                "Curtain" -> {
                    val column = (index % 5) / 4f; val fall = (index / 5) / 5f
                    paddingX + usableW * column to paddingY + usableH * (.18f + kotlin.math.abs(kotlin.math.sin(fall * Math.PI)).toFloat() * .72f)
                }
                "Run", "Run Back" -> {
                    val direction = if (group == "Run Back") 1f - progress else progress
                    paddingX + usableW * direction to paddingY + usableH * (.5f + kotlin.math.sin((progress * 3 + (patternId % 5) * .4f) * Math.PI).toFloat() * .3f)
                }
                "Water", "Flow" -> {
                    val angle = progress * (if (patternId % 2 == 0) 2.5 else 3.5) * 2 * Math.PI
                    val radius = minOf(usableW, usableH) * (.12f + progress * .35f)
                    widthPx / 2 + kotlin.math.cos(angle).toFloat() * radius to heightPx / 2 + kotlin.math.sin(angle).toFloat() * radius * .72f
                }
                "Tail" -> paddingX + usableW * progress to paddingY + usableH * (.8f - progress * .55f)
                else -> paddingX + usableW * progress to paddingY + usableH * (.5f + .3f * kotlin.math.sin(progress * 3 * Math.PI).toFloat())
            }
            val hue = ((patternId * 37 + index * 40) % 360).toFloat()
            paint.color = Color.HSVToColor(floatArrayOf(hue, .9f, 1f))
            canvas.drawCircle(x, y, uiRadius(), paint)
        }
    }
    private fun uiRadius() = 2.5f * resources.displayMetrics.density
}

/** HSV color wheel with a movable hue/saturation marker and coalesced Rust-owned preview writes. */
private class LightingColorWheelView(
    context: Context,
    initialHue: Float,
    initialSaturation: Float,
    private val onInteractionChanged: (Boolean) -> Unit,
    private val onColor: (Int, Int, Int, Boolean) -> Unit,
) : View(context) {
    private val hueLabel = CutoutUi(context).label("lighting.hue")
    private val saturationLabel = CutoutUi(context).label("lighting.saturation")
    private val bitmapSize = 240
    private val bitmap: Bitmap = Bitmap.createBitmap(bitmapSize, bitmapSize, Bitmap.Config.ARGB_8888).also { image ->
        val pixels = IntArray(bitmapSize * bitmapSize)
        val center = (bitmapSize - 1) / 2f
        val radius = center * .94f
        for (y in 0 until bitmapSize) for (x in 0 until bitmapSize) {
            val dx = (x - center) / radius; val dy = (y - center) / radius
            val distance = kotlin.math.sqrt(dx * dx + dy * dy)
            if (distance <= 1f) {
                val hue = ((Math.toDegrees(kotlin.math.atan2(dy.toDouble(), dx.toDouble())) + 360.0) % 360.0).toFloat()
                pixels[y * bitmapSize + x] = Color.HSVToColor(floatArrayOf(hue, distance, 1f))
            } else pixels[y * bitmapSize + x] = Color.TRANSPARENT
        }
        image.setPixels(pixels, 0, bitmapSize, 0, 0, bitmapSize, bitmapSize)
    }
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val markerPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; style = Paint.Style.STROKE; strokeWidth = 3f * resources.displayMetrics.density }
    private var hue = initialHue
    private var saturation = initialSaturation
    private var touchTracking = false
    private var interactionReported = false

    private fun updateInteractionState() {
        val active = touchTracking ||
            colorAxes.accessibilityFocusedVirtualViewId != ExploreByTouchHelper.INVALID_ID ||
            colorAxes.keyboardFocusedVirtualViewId != ExploreByTouchHelper.INVALID_ID
        if (interactionReported == active) return
        interactionReported = active
        onInteractionChanged(active)
    }

    fun clearVirtualFocusForDetach() {
        val provider = accessibilityNodeProvider
        val accessibilityFocus = colorAxes.accessibilityFocusedVirtualViewId
        if (accessibilityFocus != ExploreByTouchHelper.INVALID_ID) {
            provider?.performAction(
                accessibilityFocus,
                AccessibilityNodeInfoCompat.ACTION_CLEAR_ACCESSIBILITY_FOCUS,
                null,
            )
        }
        val keyboardFocus = colorAxes.keyboardFocusedVirtualViewId
        if (keyboardFocus != ExploreByTouchHelper.INVALID_ID) colorAxes.clearKeyboardFocusForVirtualView(keyboardFocus)
        touchTracking = false
        updateInteractionState()
    }

    private val colorAxes = object : ExploreByTouchHelper(this) {
        override fun getVirtualViewAt(x: Float, y: Float): Int {
            if (!isEnabled || width <= 0 || height <= 0) return INVALID_ID
            val diameter = minOf(width, height).toFloat()
            val left = (width - diameter) / 2f
            val top = (height - diameter) / 2f
            if (x < left || x >= left + diameter || y < top || y >= top + diameter) return INVALID_ID
            val dx = (x - (left + diameter / 2f)) / (diameter / 2f)
            val dy = (y - (top + diameter / 2f)) / (diameter / 2f)
            if (dx * dx + dy * dy > 1f) return INVALID_ID
            return if (x < left + diameter / 2f) HUE_VIEW_ID else SATURATION_VIEW_ID
        }

        override fun getVisibleVirtualViews(virtualViewIds: MutableList<Int>) {
            if (width > 0 && height > 0) {
                virtualViewIds += HUE_VIEW_ID
                virtualViewIds += SATURATION_VIEW_ID
            }
        }

        override fun onPopulateNodeForVirtualView(virtualViewId: Int, node: AccessibilityNodeInfoCompat) {
            node.className = SeekBar::class.java.name
            node.contentDescription = if (virtualViewId == HUE_VIEW_ID) hueLabel else saturationLabel
            node.text = if (virtualViewId == HUE_VIEW_ID) String.format(Locale.getDefault(), "%.0f°", hue)
                else String.format(Locale.getDefault(), "%.0f%%", saturation * 100f)
            node.isEnabled = this@LightingColorWheelView.isEnabled
            node.isFocusable = true
            node.isScrollable = true
            node.isVisibleToUser = isShown
            node.setBoundsInParent(axisBounds(virtualViewId))
            node.rangeInfo = AccessibilityNodeInfoCompat.RangeInfoCompat.obtain(
                AccessibilityNodeInfoCompat.RangeInfoCompat.RANGE_TYPE_FLOAT, 0f, 1f, axisValue(virtualViewId),
            )
            node.addAction(AccessibilityNodeInfoCompat.AccessibilityActionCompat.ACTION_SET_PROGRESS)
            node.addAction(AccessibilityNodeInfoCompat.AccessibilityActionCompat.ACTION_SCROLL_FORWARD)
            node.addAction(AccessibilityNodeInfoCompat.AccessibilityActionCompat.ACTION_SCROLL_BACKWARD)
        }

        override fun onPopulateNodeForHost(node: AccessibilityNodeInfoCompat) {
            node.contentDescription = contentDescription
            node.stateDescription = accessibilityValue()
            node.hintText = CutoutUi(context).label("lighting.color_wheel.hint")
        }

        override fun onPopulateEventForVirtualView(virtualViewId: Int, event: AccessibilityEvent) {
            event.contentDescription = if (virtualViewId == HUE_VIEW_ID) hueLabel else saturationLabel
            if (event.eventType == AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED ||
                event.eventType == AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED
            ) updateInteractionState()
        }

        override fun onVirtualViewKeyboardFocusChanged(virtualViewId: Int, hasFocus: Boolean) {
            updateInteractionState()
        }

        override fun onPerformActionForVirtualView(virtualViewId: Int, action: Int, arguments: Bundle?): Boolean {
            if (!isEnabled || (virtualViewId != HUE_VIEW_ID && virtualViewId != SATURATION_VIEW_ID)) return false
            val current = axisValue(virtualViewId)
            val next = when (action) {
                AccessibilityNodeInfoCompat.AccessibilityActionCompat.ACTION_SET_PROGRESS.id -> arguments
                    ?.getFloat(AccessibilityNodeInfoCompat.ACTION_ARGUMENT_PROGRESS_VALUE)
                    ?.coerceIn(0f, 1f) ?: return false
                AccessibilityNodeInfoCompat.ACTION_SCROLL_FORWARD -> (current + axisStep(virtualViewId)).coerceAtMost(1f)
                AccessibilityNodeInfoCompat.ACTION_SCROLL_BACKWARD -> (current - axisStep(virtualViewId)).coerceAtLeast(0f)
                else -> return false
            }
            if (next == current) return false
            setAccessibleAxis(virtualViewId, next)
            invalidateVirtualView(virtualViewId)
            sendEventForVirtualView(virtualViewId, AccessibilityEvent.TYPE_VIEW_SELECTED)
            sendEventForVirtualView(HOST_ID, AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED)
            return true
        }

    }

    init {
        isFocusable = true
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
        ViewCompat.setAccessibilityDelegate(this, colorAxes)
    }

    override fun dispatchHoverEvent(event: MotionEvent): Boolean =
        colorAxes.dispatchHoverEvent(event) || super.dispatchHoverEvent(event)

    override fun dispatchKeyEvent(event: KeyEvent): Boolean =
        colorAxes.dispatchKeyEvent(event) || super.dispatchKeyEvent(event)

    override fun onFocusChanged(gainFocus: Boolean, direction: Int, previouslyFocusedRect: android.graphics.Rect?) {
        super.onFocusChanged(gainFocus, direction, previouslyFocusedRect)
        colorAxes.onFocusChanged(gainFocus, direction, previouslyFocusedRect)
    }

    private fun axisBounds(id: Int): android.graphics.Rect {
        val diameter = minOf(width, height)
        val left = (width - diameter) / 2
        val top = (height - diameter) / 2
        val midpoint = left + diameter / 2
        return if (id == HUE_VIEW_ID) android.graphics.Rect(left, top, midpoint, top + diameter)
        else android.graphics.Rect(midpoint, top, left + diameter, top + diameter)
    }

    private fun axisValue(id: Int): Float = if (id == HUE_VIEW_ID) hue / 359f else saturation
    private fun axisStep(id: Int): Float = if (id == HUE_VIEW_ID) 1f / 359f else 0.01f

    private fun accessibilityValue(): String = String.format(
        Locale.getDefault(),
        "%s %d°, %s %d%%",
        hueLabel,
        hue.toInt(),
        saturationLabel,
        (saturation * 100f).toInt(),
    )

    private fun setAccessibleAxis(id: Int, value: Float) {
        val selectedHue = if (id == HUE_VIEW_ID) value * 359f else hue
        val selectedSaturation = if (id == SATURATION_VIEW_ID) value else saturation
        setSelection(selectedHue, selectedSaturation)
        val rgb = Color.HSVToColor(floatArrayOf(hue, saturation, 1f))
        onColor(Color.red(rgb), Color.green(rgb), Color.blue(rgb), true)
    }

    fun setSelection(hueDegrees: Float, saturationValue: Float) {
        hue = hueDegrees.coerceIn(0f, 359f)
        saturation = saturationValue.coerceIn(0f, 1f)
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val diameter = minOf(width, height).toFloat()
        if (diameter <= 0f) return
        val left = (width - diameter) / 2f; val top = (height - diameter) / 2f
        val wheel = android.graphics.RectF(left, top, left + diameter, top + diameter)
        canvas.drawBitmap(bitmap, null, wheel, paint)
        val angle = Math.toRadians(hue.toDouble())
        val radius = diameter * .47f * saturation
        val x = wheel.centerX() + kotlin.math.cos(angle).toFloat() * radius
        val y = wheel.centerY() + kotlin.math.sin(angle).toFloat() * radius
        canvas.drawCircle(x, y, 9f * resources.displayMetrics.density, markerPaint)
    }

    override fun onTouchEvent(event: android.view.MotionEvent): Boolean {
        if (!isEnabled) return false
        when (event.action) {
            android.view.MotionEvent.ACTION_DOWN, android.view.MotionEvent.ACTION_MOVE, android.view.MotionEvent.ACTION_UP -> {
                val diameter = minOf(width, height).toFloat(); if (diameter <= 0f) return false
                if (event.action == android.view.MotionEvent.ACTION_DOWN) {
                    touchTracking = true
                    updateInteractionState()
                }
                val cx = width / 2f; val cy = height / 2f
                val dx = event.x - cx; val dy = event.y - cy
                val saturationValue = (kotlin.math.sqrt(dx * dx + dy * dy) / (diameter * .47f)).coerceIn(0f, 1f)
                hue = ((Math.toDegrees(kotlin.math.atan2(dy.toDouble(), dx.toDouble())) + 360.0) % 360.0).toFloat()
                saturation = saturationValue; invalidate()
                val rgb = Color.HSVToColor(floatArrayOf(hue, saturation, 1f))
                onColor(Color.red(rgb), Color.green(rgb), Color.blue(rgb), event.action == android.view.MotionEvent.ACTION_UP)
                if (event.action == android.view.MotionEvent.ACTION_UP) {
                    touchTracking = false
                    updateInteractionState()
                }
                return true
            }
            android.view.MotionEvent.ACTION_CANCEL -> { touchTracking = false; updateInteractionState(); return true }
        }
        return super.onTouchEvent(event)
    }

    private companion object {
        const val HOST_VIEW_ID = -1
        const val HUE_VIEW_ID = 1
        const val SATURATION_VIEW_ID = 2
    }
}
