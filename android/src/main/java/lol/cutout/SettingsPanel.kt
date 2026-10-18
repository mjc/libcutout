package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.os.Bundle
import android.text.InputType
import android.text.TextWatcher
import android.text.Editable
import android.view.Gravity
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import uniffi.cutout_mobile_ffi.MobileConnectionAttemptTokenDto
import uniffi.cutout_mobile_ffi.MobileConnectionReadinessDto
import uniffi.cutout_mobile_ffi.MobileConnectionTransportStateDto
import uniffi.cutout_mobile_ffi.MobileSettingAccessDto
import uniffi.cutout_mobile_ffi.MobileSettingApplyEffectDto
import uniffi.cutout_mobile_ffi.MobileSettingChoiceDto
import uniffi.cutout_mobile_ffi.MobileSettingControlDto
import uniffi.cutout_mobile_ffi.MobileSettingDescriptorDto
import uniffi.cutout_mobile_ffi.MobileSettingGroupDto
import uniffi.cutout_mobile_ffi.MobileSettingIdDto
import uniffi.cutout_mobile_ffi.MobileSettingSnapshotDto
import uniffi.cutout_mobile_ffi.MobileSettingStatusDto
import uniffi.cutout_mobile_ffi.MobileSettingTransportStatusDto
import uniffi.cutout_mobile_ffi.MobileSettingValueDto
import uniffi.cutout_mobile_ffi.MobileSettingValueSourceDto
import uniffi.cutout_mobile_ffi.MobileSettingUnitDto
import uniffi.cutout_mobile_ffi.MobileSettingsDto
import java.math.BigDecimal
import java.math.RoundingMode

/** Android presentation of the Rust-owned settings catalog, matching DeviceControlsForm. */
class SettingsPanel(
    context: Context,
    private val submit: (MobileConnectionAttemptTokenDto, MobileSettingIdDto, MobileSettingValueDto, () -> Unit) -> Unit,
) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val page = ui.column()
    private val header = ui.title(ui.label("controls.tune.title"))
    private var renderedGeneration: ULong? = null
    private var renderedToken: MobileConnectionAttemptTokenDto? = null
    private var descriptorKey: List<String> = emptyList()
    private var pendingUiState: Bundle? = null
    private val rows = linkedMapOf<MobileSettingIdDto, SettingRow>()
    private val emptyMessage = ui.text(ui.label("controls.empty"), 15f, color = ui.muted)
    private val groups = listOf(
        MobileSettingGroupDto.INTERFACE, MobileSettingGroupDto.HAPTICS,
        MobileSettingGroupDto.LIMITS, MobileSettingGroupDto.RIDING,
        MobileSettingGroupDto.MODES, MobileSettingGroupDto.HARDWARE,
        MobileSettingGroupDto.REMOTE, MobileSettingGroupDto.BATTERY,
        MobileSettingGroupDto.LIGHTING,
    )

    init {
        setBackgroundColor(ui.background)
        clipToPadding = false
        page.setPadding(ui.dp(24), ui.dp(24), ui.dp(24), ui.dp(32))
        page.addView(header)
        page.addView(ui.space(20))
        addView(page, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
    }

    /** Apply owner snapshots without replacing editors when only their observation age changes. */
    fun render(snapshot: MobileSettingsDto?) {
        if (snapshot == null) {
            if (renderedGeneration != null) rebuild(emptyList(), null)
            emptyMessage.text = ui.label("settings.readback.unavailable")
            if (emptyMessage.parent == null) page.addView(emptyMessage)
            return
        }
        val descriptors = groups.flatMap { group ->
            snapshot.settingDescriptors.filter {
                it.group == group && it.access != MobileSettingAccessDto.UNVERIFIED
            }.sortedBy { it.order.toInt() }
        }
        val key = descriptors.map { descriptorKey(it) }
        val token = snapshot.connection.token
        val stableToken = token != null && token.generation == snapshot.connection.generation
        if (renderedGeneration != snapshot.connection.generation || renderedToken != token || key != descriptorKey) {
            rebuild(descriptors, snapshot)
        }
        renderedGeneration = snapshot.connection.generation
        renderedToken = token
        descriptorKey = key
        val interactable = stableToken &&
            snapshot.connection.readiness == MobileConnectionReadinessDto.VERIFIED &&
            snapshot.connection.transport == MobileConnectionTransportStateDto.CONNECTED
        rows.values.forEach { row ->
            row.render(snapshot.settings.firstOrNull { it.id == row.descriptor.id }, token, interactable)
        }
        restorePendingState(token)
    }

    /** Persist local editing state only; Rust remains the owner of setting and request state. */
    fun saveUiState(): Bundle = Bundle().apply {
        val token = renderedToken ?: return@apply
        putString("attempt.generation", token.generation.toString())
        putString("attempt.platform", token.platformIdentifier)
        rows.forEach { (id, row) -> row.saveUiState()?.let { putBundle("row.${id.name}", it) } }
    }

    /** Restore local drafts only after the matching attempt snapshot has arrived. */
    fun restoreUiState(saved: Bundle?) {
        pendingUiState = saved?.takeIf { it.containsKey("attempt.generation") && it.containsKey("attempt.platform") }
        val token = renderedToken ?: return
        restorePendingState(token)
    }

    private fun restorePendingState(token: MobileConnectionAttemptTokenDto?) {
        val saved = pendingUiState ?: return
        if (token == null) return
        val matches = saved.getString("attempt.generation") == token.generation.toString() &&
            saved.getString("attempt.platform") == token.platformIdentifier
        pendingUiState = null
        if (matches) rows.forEach { (id, row) -> row.restoreUiState(saved.getBundle("row.${id.name}")) }
    }

    private fun descriptorKey(value: MobileSettingDescriptorDto) =
        "${value.id}:${value.group}:${value.order}:${value.control}:${value.access}:${value.labelKey}"

    private fun rebuild(descriptors: List<MobileSettingDescriptorDto>, snapshot: MobileSettingsDto?) {
        rows.values.forEach { it.dismissTransientUi() }
        renderedGeneration = snapshot?.connection?.generation
        renderedToken = snapshot?.connection?.token
        page.removeAllViews()
        rows.clear()
        page.addView(header)
        page.addView(ui.space(20))
        if (descriptors.isEmpty()) {
            page.addView(emptyMessage)
            return
        }
        groups.forEach { group ->
            val section = descriptors.filter { it.group == group }
            if (section.isNotEmpty()) {
                val title = ui.text(ui.label(groupKey(group)), 17f, true).apply {
                    isAccessibilityHeading = true
                }
                page.addView(title)
                page.addView(ui.space(12))
                val card = ui.column().apply {
                    background = ui.rounded(22)
                    setPadding(ui.dp(16), ui.dp(4), ui.dp(16), ui.dp(4))
                }
                section.forEachIndexed { index, descriptor ->
                    if (index > 0) card.addView(ui.divider())
                    val row = SettingRow(descriptor)
                    rows[descriptor.id] = row
                    card.addView(row.root)
                }
                page.addView(card)
                page.addView(ui.space(20))
            }
        }
    }

    private fun groupKey(group: MobileSettingGroupDto) = when (group) {
        MobileSettingGroupDto.INTERFACE -> "controls.group.interface"
        MobileSettingGroupDto.HAPTICS -> "controls.group.haptics"
        MobileSettingGroupDto.LIMITS -> "controls.group.limits"
        MobileSettingGroupDto.RIDING -> "controls.group.riding"
        MobileSettingGroupDto.MODES -> "controls.group.modes"
        MobileSettingGroupDto.HARDWARE -> "controls.group.hardware"
        MobileSettingGroupDto.REMOTE -> "controls.group.remote"
        MobileSettingGroupDto.BATTERY -> "controls.group.battery"
        MobileSettingGroupDto.LIGHTING -> "controls.group.lighting"
        MobileSettingGroupDto.DIAGNOSTICS -> "controls.group.diagnostics"
    }

    private inner class SettingRow(val descriptor: MobileSettingDescriptorDto) {
        val root = ui.column().apply {
            tag = descriptor.id
            setPadding(0, ui.dp(12), 0, ui.dp(12))
        }
        private val title = ui.text(ui.label(descriptor.labelKey), 17f, true)
        private val summary = ui.text("", 14f, color = ui.muted)
        private val controls = ui.column()
        private val help = descriptor.helpKey?.let { ui.text(ui.label(it), 14f, color = ui.muted) }
        private val semantics = descriptor.valueSemanticsKey?.let { ui.text(ui.label(it), 14f, color = ui.muted) }
        private val effect = effectLabel(descriptor.applyEffect)?.let { ui.text(it, 14f, color = ui.muted) }
        private val feedback = ui.text("", 14f, color = ui.muted)
        private var state: MobileSettingSnapshotDto? = null
        private val boundAttempt = renderedToken
        private var interactable = false
        private var draft: MobileSettingValueDto? = null
        private var submitted: MobileSettingValueDto? = null
        private var pendingLocalSubmission = false
        private var priorRequestId: ULong? = null
        private var failure: String? = null
        private var numericEdit: EditText? = null
        private var choiceButton: android.widget.Button? = null
        private var applyButton: android.widget.Button? = null
        private var decrementButton: android.widget.Button? = null
        private var incrementButton: android.widget.Button? = null
        private var choiceDialog: AlertDialog? = null
        private var invalidNumericInput = false
        private var suppressNumericWatcher = false
        private val control = descriptor.control

        init {
            val titleRow = ui.row()
            titleRow.addView(title, LinearLayout.LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            titleRow.addView(summary)
            root.addView(titleRow)
            root.addView(controls)
            listOf(help, semantics, effect, feedback).filterNotNull().forEach {
                root.addView(it)
                it.setPadding(0, ui.dp(5), 0, 0)
            }
            buildControls()
        }

        fun render(next: MobileSettingSnapshotDto?, attempt: MobileConnectionAttemptTokenDto?, enabled: Boolean) {
            if (next?.current != null && draft == next.current) {
                draft = null; submitted = null
            }
            state = next
            if (pendingLocalSubmission && next?.requestId != null && next.requestId != priorRequestId) {
                pendingLocalSubmission = false
            }
            interactable = enabled && attempt == boundAttempt && descriptor.access == MobileSettingAccessDto.WRITABLE
            setChildrenEnabled(controls, interactable)
            controls.alpha = if (interactable || descriptor.access == MobileSettingAccessDto.READ_ONLY) 1f else .55f
            updateSummary()
            updateFeedback()
        }

        fun dismissTransientUi() {
            choiceDialog?.dismiss()
            choiceDialog = null
        }

        private fun buildControls() {
            if (descriptor.access == MobileSettingAccessDto.READ_ONLY) return
            when (val shape = control) {
                is MobileSettingControlDto.Boolean -> {
                    val line = ui.row()
                    line.gravity = Gravity.END or Gravity.CENTER_VERTICAL
                    booleanButton(false, line)
                    booleanButton(true, line)
                    controls.addView(line)
                }
                is MobileSettingControlDto.Choices -> {
                    val writable = shape.choices.filter { it.writable }
                    val line = ui.row()
                    val selected = ui.action(context.getString(R.string.setting_unknown)) {
                        val values = writable.map { choiceLabel(it) }.toTypedArray()
                        choiceDialog = AlertDialog.Builder(context).setItems(values) { _, which ->
                            val choice = writable.getOrNull(which) ?: return@setItems
                            edit(MobileSettingValueDto.Choice(choice.id))
                        }.show()
                    }
                    choiceButton = selected
                    line.addView(selected, LinearLayout.LayoutParams(-1, LayoutParams.WRAP_CONTENT))
                    controls.addView(line)
                    if (writable.isEmpty()) selected.isEnabled = false
                    val buttons = ui.row().apply { gravity = Gravity.END }
                    val apply = ui.action(ui.label("controls.apply"), primary = true) {
                        draft?.let { send(it) }
                    }.apply {
                        tag = "settings.apply.${descriptor.id}"
                        contentDescription = formatLabel("controls.apply.accessibility", ui.label(descriptor.labelKey))
                    }
                    applyButton = apply
                    buttons.addView(apply)
                    controls.addView(buttons)
                }
                is MobileSettingControlDto.Number -> buildNumber(shape)
                is MobileSettingControlDto.ReadOnly -> Unit
            }
        }

        private fun booleanButton(value: Boolean, into: LinearLayout) {
            val button = ui.action(ui.label(if (value) "controls.on" else "settings.choice.off")) {
                send(MobileSettingValueDto.Boolean(value))
            }
            button.tag = value
            button.contentDescription = formatLabel(
                "controls.boolean.action", ui.label(descriptor.labelKey),
                ui.label(if (value) "controls.on" else "settings.choice.off"),
            )
            button.layoutParams = LinearLayout.LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT).apply {
                marginStart = ui.dp(6)
            }
            into.addView(button)
        }

        private fun buildNumber(shape: MobileSettingControlDto.Number) {
            val line = ui.row()
            val input = EditText(context).apply {
                textSize = 17f
                setTextColor(ui.foreground)
                setHintTextColor(ui.muted)
                hint = ui.label("settings.value.draft")
                tag = "settings.editor.${descriptor.id}"
                inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_FLAG_DECIMAL or InputType.TYPE_NUMBER_FLAG_SIGNED
                setSingleLine(true)
                addTextChangedListener(object : TextWatcher {
                    override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
                    override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                        if (suppressNumericWatcher) return
                        val parsed = parseFixed(s?.toString().orEmpty(), shape.precision.toInt())
                        val nextDraft = parsed?.takeIf { it in shape.minimum..shape.maximum }?.let { MobileSettingValueDto.Number(it) }
                        if (nextDraft != draft) {
                            draft = nextDraft
                            submitted = null
                            pendingLocalSubmission = false
                            failure = null
                        }
                        invalidNumericInput = parsed == null || parsed !in shape.minimum..shape.maximum
                        updateSummary(); updateFeedback()
                    }
                    override fun afterTextChanged(s: Editable?) = Unit
                })
            }
            numericEdit = input
            input.minimumHeight = ui.dp(52)
            input.contentDescription = ui.label(descriptor.labelKey)
            line.addView(input, LinearLayout.LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            val decrement = ui.action("−") { step(shape, false) }
                .apply { contentDescription = context.getString(R.string.setting_decrease, ui.label(descriptor.labelKey), "") }
            val increment = ui.action("+") { step(shape, true) }
                .apply { contentDescription = context.getString(R.string.setting_increase, ui.label(descriptor.labelKey), "") }
            decrementButton = decrement
            incrementButton = increment
            line.addView(decrement)
            line.addView(increment)
            controls.addView(line)
            val buttons = ui.row().apply { gravity = Gravity.END }
            if (shape.canDisable) buttons.addView(ui.action(ui.label("settings.choice.off")) {
                edit(MobileSettingValueDto.Disabled)
            }.apply {
                contentDescription = formatLabel("controls.disable.accessibility", ui.label(descriptor.labelKey))
            })
            val apply = ui.action(ui.label("controls.apply"), primary = true) {
                draft?.let { send(it) }
            }.apply {
                tag = "settings.apply.${descriptor.id}"
                contentDescription = formatLabel("controls.apply.accessibility", ui.label(descriptor.labelKey))
            }
            applyButton = apply
            buttons.addView(apply)
            controls.addView(buttons)
        }

        private fun step(shape: MobileSettingControlDto.Number, increasing: Boolean) {
            val precision = shape.precision.toInt()
            val lower = shape.minimum.toLong()
            val step = shape.step.toLong()
            val editor = shape.editorStep.toLong()
            if (step <= 0 || editor <= 0 || lower > shape.maximum) return
            val source = draft ?: state?.current
            if (source is MobileSettingValueDto.Disabled) {
                edit(MobileSettingValueDto.Number(shape.minimum))
                setNumericText(formatFixed(shape.minimum, precision), force = true)
                return
            }
            val current = when (source) {
                is MobileSettingValueDto.Number -> source
                else -> return // Unknown is not the minimum; only explicit Off returns to the numeric lattice.
            }
            val offset = current.value.toLong() - lower
            val last = (shape.maximum.toLong() - lower) / step
            val index = if (increasing) Math.floorDiv(offset + editor + step - 1, step) else Math.floorDiv(offset - editor, step)
            val next = (lower + index.coerceIn(0, last) * step).toInt()
            edit(MobileSettingValueDto.Number(next))
            setNumericText(formatFixed(next, precision), force = true)
        }

        private fun edit(value: MobileSettingValueDto) {
            draft = value; submitted = null; pendingLocalSubmission = false; failure = null; invalidNumericInput = false
            if (value is MobileSettingValueDto.Number) {
                (control as? MobileSettingControlDto.Number)?.let { setNumericText(formatFixed(value.value, it.precision.toInt()), force = true) }
            }
            updateSummary(); updateFeedback()
        }

        private fun send(value: MobileSettingValueDto) {
            val original = boundAttempt
            if (original == null || !interactable || original != renderedToken || original != stateAttempt()) {
                failure = context.getString(R.string.settings_stale_attempt); updateFeedback(); return
            }
            if (pendingLocalSubmission && submitted == value) return
            if (invalidNumericInput) {
                failure = context.getString(R.string.invalid_number); updateFeedback(); return
            }
            try {
                var refused = false
                val requestIdBeforeSubmit = state?.requestId
                submit(original, descriptor.id, value) {
                    if (renderedToken != original) return@submit
                    refused = true
                    submitted = null
                    pendingLocalSubmission = false
                    failure = context.getString(R.string.setting_refused)
                    updateSummary(); updateFeedback()
                }
                if (!refused) {
                    submitted = value
                    pendingLocalSubmission = true
                    priorRequestId = requestIdBeforeSubmit
                    failure = null
                }
            } catch (_: Exception) {
                submitted = null
                pendingLocalSubmission = false
                failure = context.getString(R.string.setting_refused)
            }
            updateSummary()
            updateFeedback()
        }

        fun saveUiState(): Bundle? {
            val hasLocalState = draft != null || numericEdit?.text?.isNotEmpty() == true
            if (!hasLocalState) return null
            return Bundle().apply {
                draft?.let { putValue("draft", it) }
                numericEdit?.let { editor ->
                    putString("editor.text", editor.text.toString())
                    putInt("editor.start", editor.selectionStart.coerceAtLeast(0))
                    putInt("editor.end", editor.selectionEnd.coerceAtLeast(0))
                    putBoolean("editor.focused", editor.hasFocus())
                }
            }
        }

        fun restoreUiState(saved: Bundle?) {
            if (saved == null) return
            draft = saved.getValue("draft")
            submitted = draft?.takeIf {
                it == state?.requested && state?.requestId != null &&
                    (state?.status == MobileSettingStatusDto.WAITING_FOR_CONFIRMATION ||
                        state?.status == MobileSettingStatusDto.SENT_WITHOUT_CONFIRMATION) &&
                    state?.transport != MobileSettingTransportStatusDto.REJECTED
            }
            pendingLocalSubmission = false
            failure = null
            numericEdit?.let { editor ->
                val text = saved.getString("editor.text") ?: return@let
                setNumericText(text, force = true)
                val parsed = parseFixed(text, (control as? MobileSettingControlDto.Number)?.precision?.toInt() ?: 0)
                val shape = control as? MobileSettingControlDto.Number
                invalidNumericInput = parsed == null || shape == null || parsed !in shape.minimum..shape.maximum
                editor.setSelection(
                    saved.getInt("editor.start").coerceIn(0, editor.length()),
                    saved.getInt("editor.end").coerceIn(0, editor.length()),
                )
                if (saved.getBoolean("editor.focused")) editor.requestFocus()
            }
            updateSummary()
            updateFeedback()
        }

        private fun Bundle.putValue(key: String, value: MobileSettingValueDto) {
            val item = Bundle()
            when (value) {
                is MobileSettingValueDto.Boolean -> { item.putString("kind", "boolean"); item.putBoolean("value", value.value) }
                is MobileSettingValueDto.Number -> { item.putString("kind", "number"); item.putInt("value", value.value) }
                is MobileSettingValueDto.Choice -> { item.putString("kind", "choice"); item.putInt("value", value.id.toInt()) }
                is MobileSettingValueDto.Disabled -> item.putString("kind", "disabled")
            }
            putBundle(key, item)
        }

        private fun Bundle.getValue(key: String): MobileSettingValueDto? = getBundle(key)?.let { item ->
            when (item.getString("kind")) {
                "boolean" -> MobileSettingValueDto.Boolean(item.getBoolean("value"))
                "number" -> MobileSettingValueDto.Number(item.getInt("value"))
                "choice" -> MobileSettingValueDto.Choice(item.getInt("value").toUShort())
                "disabled" -> MobileSettingValueDto.Disabled
                else -> null
            }
        }

        private fun stateAttempt() = renderedToken?.takeIf { it == boundAttempt }

        private fun setNumericText(text: String, force: Boolean = false) {
            val edit = numericEdit ?: return
            if ((!force && edit.hasFocus()) || edit.text.toString() == text) return
            suppressNumericWatcher = true
            try {
                edit.setText(text)
                edit.setSelection(edit.text.length)
            } finally {
                suppressNumericWatcher = false
            }
        }

        private fun updateSummary() {
            val current = state?.current
            val chosen = draft ?: current
            val primary = when {
                chosen == null -> context.getString(R.string.setting_unknown)
                draft != null && draft != current -> "${ui.label(if (draft == submitted && state?.requested == draft) "settings.value.requested" else "settings.value.draft")}: ${valueText(draft)}"
                else -> "${ui.label(sourceKey(state?.currentSource))}: ${valueText(chosen)}"
            }
            val requested = state?.requested?.takeIf {
                it != current && state?.status != MobileSettingStatusDto.IDLE && state?.status != MobileSettingStatusDto.CONFIRMED
            }
            summary.text = if (requested == null) primary else "$primary\n${ui.label("settings.value.requested")}: ${valueText(requested)}"
            if (control is MobileSettingControlDto.Number) {
                numericEdit?.contentDescription = "${ui.label(descriptor.labelKey)}: ${valueText(draft ?: current)}"
                decrementButton?.contentDescription = context.getString(
                    R.string.setting_decrease, ui.label(descriptor.labelKey), valueText(draft ?: current),
                )
                incrementButton?.contentDescription = context.getString(
                    R.string.setting_increase, ui.label(descriptor.labelKey), valueText(draft ?: current),
                )
                applyButton?.contentDescription = "${formatLabel("controls.apply.accessibility", ui.label(descriptor.labelKey))}: ${valueText(draft)}"
            }
            if (control is MobileSettingControlDto.Choices) {
                choiceButton?.contentDescription = "${ui.label(descriptor.labelKey)}: ${valueText(draft ?: current)}"
                applyButton?.contentDescription = "${formatLabel("controls.apply.accessibility", ui.label(descriptor.labelKey))}: ${valueText(draft)}"
                choiceButton?.text = valueText(draft ?: current)
                choiceButton?.isEnabled = interactable && control.choices.any { it.writable }
            }
            val selected = when {
                failure != null || state?.transport == MobileSettingTransportStatusDto.REJECTED -> current
                state?.status == MobileSettingStatusDto.WAITING_FOR_CONFIRMATION || state?.status == MobileSettingStatusDto.SENT_WITHOUT_CONFIRMATION -> state?.requested
                else -> current
            }
            (controls.getChildAt(0) as? LinearLayout)?.let { line ->
                for (index in 0 until line.childCount) {
                    val child = line.getChildAt(index)
                    if (child.tag is Boolean) {
                        val chosenBoolean = selected as? MobileSettingValueDto.Boolean
                        val isSelected = chosenBoolean?.value == child.tag
                        child.alpha = if (isSelected) 1f else .65f
                        child.isSelected = isSelected
                        val selectionState = when {
                            !isSelected -> null
                            state?.requested == selected &&
                                (state?.status == MobileSettingStatusDto.WAITING_FOR_CONFIRMATION ||
                                    state?.status == MobileSettingStatusDto.SENT_WITHOUT_CONFIRMATION) ->
                                ui.label("controls.accessibility.requested")
                            state?.current == selected -> when (state?.currentSource) {
                                MobileSettingValueSourceDto.LIVE_READBACK -> ui.label("controls.accessibility.reported")
                                MobileSettingValueSourceDto.CAPTURE_REPLAY -> ui.label("settings.value.replay")
                                MobileSettingValueSourceDto.USER_REQUEST -> ui.label("controls.accessibility.requested")
                                else -> null
                            }
                            else -> null
                        }
                        val option = ui.label(if (child.tag == true) "controls.on" else "settings.choice.off")
                        child.contentDescription = formatLabel(
                            "controls.boolean.action", ui.label(descriptor.labelKey), option,
                        ) + selectionState?.let { ", $it" }.orEmpty()
                    }
                }
            }
            val changed = draft != null && draft != current
            val hasNumericBase = draft != null || current != null
            decrementButton?.isEnabled = interactable && hasNumericBase
            incrementButton?.isEnabled = interactable && hasNumericBase
            val retryable = state?.status == MobileSettingStatusDto.FAILED ||
                state?.status == MobileSettingStatusDto.REFUSED || state?.status == MobileSettingStatusDto.TIMED_OUT ||
                state?.transport == MobileSettingTransportStatusDto.REJECTED
            applyButton?.visibility = if (changed) View.VISIBLE else View.GONE
            applyButton?.isEnabled = interactable && changed && !pendingLocalSubmission && (draft != submitted || retryable)
        }

        private fun updateFeedback() {
            val status = state?.status
            val transport = state?.transport
            val text = when {
                failure != null -> failure
                invalidNumericInput -> context.getString(R.string.invalid_number)
                draft != null && draft != state?.requested -> null
                status == MobileSettingStatusDto.CONFIRMED -> null
                status == MobileSettingStatusDto.REFUSED -> ui.label("settings.state.refused")
                transport == MobileSettingTransportStatusDto.REJECTED -> ui.label("settings.transport.rejected")
                transport == MobileSettingTransportStatusDto.CANCELLED -> ui.label("settings.transport.cancelled")
                status == MobileSettingStatusDto.FAILED -> ui.label("settings.state.failed")
                transport == MobileSettingTransportStatusDto.ACCEPTED -> ui.label("settings.transport.accepted")
                transport == MobileSettingTransportStatusDto.QUEUED -> ui.label("settings.transport.queued")
                status == MobileSettingStatusDto.WAITING_FOR_CONFIRMATION -> ui.label("settings.state.pending")
                else -> null
            }
            feedback.text = text.orEmpty()
            feedback.setTextColor(if (status == MobileSettingStatusDto.REFUSED || status == MobileSettingStatusDto.FAILED || transport == MobileSettingTransportStatusDto.REJECTED || transport == MobileSettingTransportStatusDto.CANCELLED || failure != null) ui.red else ui.muted)
            feedback.visibility = if (text.isNullOrEmpty()) View.GONE else View.VISIBLE
        }

        private fun valueText(value: MobileSettingValueDto?): String = when (value) {
            null -> "—"
            is MobileSettingValueDto.Boolean -> ui.label(if (value.value) "controls.on" else "settings.choice.off")
            is MobileSettingValueDto.Disabled -> ui.label("settings.choice.off")
            is MobileSettingValueDto.Choice -> (control as? MobileSettingControlDto.Choices)?.choices?.firstOrNull { it.id == value.id }?.let(::choiceLabel) ?: "—"
            is MobileSettingValueDto.Number -> (control as? MobileSettingControlDto.Number)?.let { formatQuantity(value.value, it) } ?: value.value.toString()
        }
    }

    private fun choiceLabel(choice: MobileSettingChoiceDto) = ui.label(choice.labelKey)

    private fun sourceKey(source: MobileSettingValueSourceDto?) = when (source) {
        MobileSettingValueSourceDto.CAPTURE_REPLAY -> "settings.value.replay"
        MobileSettingValueSourceDto.USER_REQUEST -> "settings.value.requested"
        else -> "settings.value.wheel"
    }

    private fun effectLabel(effect: MobileSettingApplyEffectDto) = when (effect) {
        MobileSettingApplyEffectDto.IMMEDIATE -> ui.label("settings.effect.immediate")
        MobileSettingApplyEffectDto.RESTART_REQUIRED -> ui.label("settings.effect.restart_required")
        MobileSettingApplyEffectDto.UNSPECIFIED -> null
    }

    private fun formatQuantity(raw: Int, descriptor: MobileSettingControlDto.Number): String {
        val amount = BigDecimal.valueOf(raw.toLong(), descriptor.precision.toInt())
        val number = amount.setScale(descriptor.precision.toInt(), RoundingMode.UNNECESSARY).toPlainString()
        return number + when (descriptor.unit) {
            MobileSettingUnitDto.PWM_DUTY_PERCENT, MobileSettingUnitDto.PERCENT -> "%"
            MobileSettingUnitDto.KILOMETRES_PER_HOUR -> " km/h"
            MobileSettingUnitDto.DEGREES -> "°"
            MobileSettingUnitDto.SECONDS -> " s"
            MobileSettingUnitDto.MINUTES -> " min"
            MobileSettingUnitDto.VOLTS -> " V"
            MobileSettingUnitDto.AMPERES -> " A"
            MobileSettingUnitDto.HERTZ -> " Hz"
            MobileSettingUnitDto.DEGREES_PER_SECOND -> "°/s"
            MobileSettingUnitDto.CELSIUS -> "°C"
            MobileSettingUnitDto.WATTS -> " W"
            MobileSettingUnitDto.MILLISECONDS -> " ms"
            MobileSettingUnitDto.REVOLUTIONS_PER_MINUTE -> " rpm"
            MobileSettingUnitDto.MILLIMETRES -> " mm"
            MobileSettingUnitDto.AMP_HOURS -> " Ah"
            else -> ""
        }
    }

    private fun formatLabel(key: String, vararg values: String): String {
        var text = ui.label(key)
        values.forEach { text = text.replaceFirst("%@", it) }
        return text
    }

    private fun formatFixed(raw: Int, precision: Int) = BigDecimal.valueOf(raw.toLong(), precision).toPlainString()
    private fun parseFixed(text: String, precision: Int): Int? = try {
        BigDecimal(text).movePointRight(precision).setScale(0, RoundingMode.UNNECESSARY).intValueExact()
    } catch (_: Exception) { null }

    private fun setChildrenEnabled(view: View, enabled: Boolean) {
        view.isEnabled = enabled
        if (view is LinearLayout) {
            for (index in 0 until view.childCount) setChildrenEnabled(view.getChildAt(index), enabled)
        }
    }
}
