package lol.cutout

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
import uniffi.cutout_mobile_ffi.*

/** Standalone MELK RGB route. Connection identity, profile validation and command framing stay Rust-owned. */
class LightingPanel(
    context: Context,
    private val client: AndroidMelkLightingClient,
    private val onRequestBluetooth: () -> Unit,
) : ScrollView(context) {
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
        if (value) client.resume() else client.stopIfUnpaired()
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
            else if (snapshot.state != MobileMelkLightingSessionStateDto.Ready) connectionActions.addView(ui.action(ui.label("lighting.choose_accessory"), primary = true, onClick = { showPairing() }))
            val error = value.error
            if (error != null) connectionActions.addView(ui.text(error, 13f, color = ui.orange).apply { setPadding(ui.dp(8), ui.dp(8), ui.dp(8), ui.dp(4)) })
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
        val color = LightingColorWheelView(context, hsv[0], hsv[1], onInteractionChanged = { tracking ->
            trackingColorWheel = tracking
            if (!tracking) scheduleControlRefreshAfterInteraction()
        }) { red, green, blue, final ->
            client.setSolidColor(red, green, blue, preview = !final)
        }.apply {
            tag = "lighting.color-wheel"
            contentDescription = ui.label("lighting.color_wheel"); isEnabled = ready()
            layoutParams = LinearLayout.LayoutParams(-1, ui.dp(248))
        }
        card.addView(color)
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
        val slider = SeekBar(context).apply {
            max = 100; progress = state.brightness.toInt(); isEnabled = ready()
            contentDescription = ui.label("lighting.brightness"); tag = "lighting.brightness"
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(seekBar: SeekBar?, progress: Int, fromUser: Boolean) { if (fromUser) (seekBar?.parent as? LinearLayout)?.let { (it.getChildAt(0) as? LinearLayout)?.let { row -> (row.getChildAt(1) as? TextView)?.text = context.getString(R.string.lighting_percent, progress) } } }
                override fun onStartTrackingTouch(seekBar: SeekBar?) { trackSlider(true) }
                override fun onStopTrackingTouch(seekBar: SeekBar?) {
                    trackSlider(false)
                    if (ready()) seekBar?.let { client.setBrightness(it.progress) }
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
        val selectedGroup = groups.firstOrNull { it.effectIds.any { id -> id.toUByte().toInt() == currentPattern } }?.name ?: groupNames.firstOrNull().orEmpty()
        val spinner = Spinner(context).apply {
            adapter = ArrayAdapter(context, android.R.layout.simple_spinner_dropdown_item, groupNames)
            setSelection(groupNames.indexOf(selectedGroup).coerceAtLeast(0)); isEnabled = ready(); contentDescription = ui.label("lighting.pattern_group")
        }
        card.addView(spinner)
        card.addView(ui.text(ui.label("lighting.quick_picks"), 15f, true, ui.muted))
        val favorites = listOf(1, 16, 22, 75).mapNotNull { id -> patterns.firstOrNull { it.id.toInt() == id } }
        addPatternRows(card, favorites, currentPattern)
        card.addView(ui.text(ui.label("lighting.group_patterns"), 15f, true, ui.muted))
        val rows = ui.column(); card.addView(rows)
        fun populate() {
            rows.removeAllViews()
            val groupName = spinner.selectedItem?.toString()
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
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(s: SeekBar?, p: Int, user: Boolean) = Unit
                override fun onStartTrackingTouch(s: SeekBar?) { trackSlider(true) }
                override fun onStopTrackingTouch(s: SeekBar?) {
                    trackSlider(false)
                    if (ready()) s?.let { client.setEffectSpeed(255 - it.progress) }
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
        card.addView(SeekBar(context).apply { max = 100; progress = sensitivity; isEnabled = ready(); contentDescription = ui.label("lighting.microphone_sensitivity"); tag = "lighting.music.sensitivity"
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(s: SeekBar?, p: Int, user: Boolean) = Unit
                override fun onStartTrackingTouch(s: SeekBar?) { trackSlider(true) }
                override fun onStopTrackingTouch(s: SeekBar?) {
                    trackSlider(false)
                    if (ready()) s?.let { client.applyPlayback(MobileLightingPlaybackDto.Music(selection.selectedItemPosition.toUByte(), it.progress.toUByte())) }
                    scheduleControlRefreshAfterInteraction()
                }
            }) })
        card.addView(ui.action(ui.label("lighting.stop_music")) { client.stopMusic() }.apply { isEnabled = ready() })
        controls.addView(card)
    }

    private fun renderPresets(state: MobileMelkLightingRestoreStateDto) {
        val card = card(); card.addView(ui.text(ui.label("lighting.preset.scenes"), 18f, true))
        val quick = listOf(Triple("lighting.preset.red", 255, 0 to 0), Triple("lighting.preset.blue", 0, 0 to 255), Triple("lighting.preset.night", 16, 20 to 32))
        card.addView(ui.row().apply { quick.forEach { (key, red, rest) -> addView(ui.action(ui.label(key)) { client.setSolidColor(red, rest.first, rest.second) }.apply { isEnabled = ready() }, LinearLayout.LayoutParams(0, -2, 1f)) } })
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
        }
        card.addView(ui.row().apply {
            addView(name, LinearLayout.LayoutParams(0, -2, 1f))
            addView(ui.action(ui.label("lighting.save")) { client.savePreset(name.text.toString()); name.text.clear() }.apply { isEnabled = ready() })
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

    private companion object { const val STATE_PAGE = "lighting.selected-page" }

    private fun showPairing() {
        val presentation = current ?: return
        if (presentation.permissionMissing) { onRequestBluetooth(); return }
        val candidates = presentation.candidates
        val labels = candidates.map { (it.name?.takeIf(String::isNotBlank) ?: ui.label("lighting.pairing.unknown_name")) + "  ·  ${it.rssi} dBm" }.toTypedArray()
        if (labels.isEmpty()) {
            android.app.AlertDialog.Builder(context).setTitle(ui.label("lighting.pairing.nearby_title"))
                .setMessage(ui.label("lighting.connection.ready_to_scan"))
                .setPositiveButton(ui.label("lighting.choose_accessory")) { _, _ -> client.resume() }.setNegativeButton(android.R.string.cancel, null).show()
        } else android.app.AlertDialog.Builder(context).setTitle(ui.label("lighting.pairing.nearby_title"))
            .setItems(labels) { _, which -> client.selectCandidate(candidates[which].platformIdentifier) }
            .setNegativeButton(android.R.string.cancel, null).show()
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
                if (event.action == android.view.MotionEvent.ACTION_DOWN) onInteractionChanged(true)
                val cx = width / 2f; val cy = height / 2f
                val dx = event.x - cx; val dy = event.y - cy
                val saturationValue = (kotlin.math.sqrt(dx * dx + dy * dy) / (diameter * .47f)).coerceIn(0f, 1f)
                hue = ((Math.toDegrees(kotlin.math.atan2(dy.toDouble(), dx.toDouble())) + 360.0) % 360.0).toFloat()
                saturation = saturationValue; invalidate()
                val rgb = Color.HSVToColor(floatArrayOf(hue, saturation, 1f))
                onColor(Color.red(rgb), Color.green(rgb), Color.blue(rgb), event.action == android.view.MotionEvent.ACTION_UP)
                if (event.action == android.view.MotionEvent.ACTION_UP) onInteractionChanged(false)
                return true
            }
            android.view.MotionEvent.ACTION_CANCEL -> { onInteractionChanged(false); return true }
        }
        return super.onTouchEvent(event)
    }
}
