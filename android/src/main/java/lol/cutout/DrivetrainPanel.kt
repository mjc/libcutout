package lol.cutout

import android.content.Context
import android.os.Bundle
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.text.method.DigitsKeyListener
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import uniffi.cutout_mobile_ffi.MobileConnectionAttemptTokenDto
import uniffi.cutout_mobile_ffi.MobileVescDrivetrainDraft
import uniffi.cutout_mobile_ffi.MobileVescDrivetrainException

/** Text editors for one captured attempt. Rust validates and persists every draft. */
class DrivetrainPanel(
    context: Context,
    private val load: (MobileConnectionAttemptTokenDto, (Result<MobileVescDrivetrainDraft?>) -> Unit) -> Unit,
    private val save: (MobileConnectionAttemptTokenDto, MobileVescDrivetrainDraft, (Result<MobileVescDrivetrainDraft>) -> Unit) -> Unit,
) : LinearLayout(context) {
    private val ui = CutoutUi(context)
    private var token: MobileConnectionAttemptTokenDto? = null
    private var pendingEditors: Bundle? = null
    private var pendingAttempt: MobileConnectionAttemptTokenDto? = null
    private var revision = 0L
    private var loaded = false
    private var busy = false
    private val diameter = field("wheel-diameter", "wheel_diameter", decimal = true)
    private val poles = field("motor-pole-pairs", "motor_pole_pairs", decimal = false)
    private val reduction = field("reduction", "reduction", decimal = true)
    private val feedback = ui.text("", 14f, color = ui.red).apply {
        tag = "vesc.drivetrain.error"; visibility = View.GONE
    }
    private val retry = ui.action(ui.label("vesc.drivetrain.retry_load")) { reload() }.apply {
        tag = "vesc.drivetrain.retry-load"; visibility = View.GONE
    }
    private val saveButton = ui.action(ui.label("vesc.drivetrain.save"), primary = true) { submit() }.apply {
        tag = "vesc.drivetrain.save"; isEnabled = false
    }

    init {
        orientation = VERTICAL
        tag = "vesc.drivetrain.editor"
        background = ui.rounded(22)
        setPadding(ui.dp(16), ui.dp(16), ui.dp(16), ui.dp(16))
        addView(ui.text(ui.label("vesc.drivetrain.title"), 20f, true))
        for ((editor, key) in listOf(diameter to "wheel_diameter", poles to "motor_pole_pairs", reduction to "reduction")) {
            addView(ui.space(16))
            addView(ui.text(ui.label("vesc.drivetrain.$key"), 15f, true))
            addView(editor, LayoutParams(-1, -2))
        }
        addView(ui.space(12))
        addView(ui.text(ui.label("vesc.drivetrain.description"), 14f, color = ui.muted))
        addView(feedback, LayoutParams(-1, -2))
        addView(retry, LayoutParams(-1, -2))
        addView(ui.space(16))
        addView(saveButton, LayoutParams(-1, -2))
        updateEnabled()
    }

    /** Unrelated telemetry updates leave the same editors and their drafts in place. */
    fun bind(attempt: MobileConnectionAttemptTokenDto?) {
        if (token == attempt) return
        if (attempt != null && pendingAttempt != null && attempt != pendingAttempt) {
            pendingEditors = null
            pendingAttempt = null
        }
        token = attempt
        revision++
        loaded = false
        busy = false
        clearFailure()
        applyDraft(null)
        updateEnabled()
        if (attempt != null) reload()
    }

    /** Editor text for this attempt. Held across the reload that follows activity recreation. */
    fun saveUiState(attempt: MobileConnectionAttemptTokenDto): Bundle? {
        if (pendingAttempt == attempt && pendingEditors != null) return pendingEditors
        if (token == attempt && loaded) return editorBundle()
        return null
    }

    /** Keep a restored draft until this attempt finishes loading. A stored draft must not replace it. */
    fun restoreUiState(attempt: MobileConnectionAttemptTokenDto, saved: Bundle?) {
        if (token != null && token != attempt) return
        pendingEditors = saved
        pendingAttempt = if (saved == null) null else attempt
        if (loaded && token == attempt) applyPendingEditors()
    }

    private fun reload() {
        val attempt = token ?: return
        if (busy) return
        val request = ++revision
        busy = true
        clearFailure()
        updateEnabled()
        load(attempt) { result ->
            if (request != revision || token != attempt) return@load
            busy = false
            result.onSuccess { draft ->
                loaded = true
                applyDraft(draft)
                applyPendingEditors()
            }.onFailure { showFailure(it, loading = true) }
            updateEnabled()
        }
    }

    private fun submit() {
        val attempt = token ?: return
        if (busy || !loaded) return
        pendingEditors = null
        pendingAttempt = null
        val draft = MobileVescDrivetrainDraft(decimalText(diameter), poles.text.toString(), decimalText(reduction))
        val request = ++revision
        busy = true
        clearFailure()
        updateEnabled()
        save(attempt, draft) { result ->
            if (request != revision || token != attempt) return@save
            busy = false
            result.onSuccess(::applyDraft).onFailure { showFailure(it, loading = false) }
            updateEnabled()
        }
    }

    private fun applyDraft(draft: MobileVescDrivetrainDraft?) {
        diameter.setText(draft?.wheelDiameterMm.orEmpty())
        poles.setText(draft?.motorPolePairs.orEmpty())
        reduction.setText(draft?.motorToWheelRatio.orEmpty())
    }

    private fun applyPendingEditors() {
        val saved = pendingEditors ?: return
        if (!loaded || token == null || token != pendingAttempt) return
        listOf("diameter" to diameter, "poles" to poles, "reduction" to reduction).forEach { (key, editor) ->
            val item = saved.getBundle(key) ?: return@forEach
            val text = item.getString("text") ?: return@forEach
            editor.setText(text)
            val length = editor.length()
            editor.setSelection(
                item.getInt("start").coerceIn(0, length),
                item.getInt("end").coerceIn(0, length),
            )
            if (item.getBoolean("focused")) editor.requestFocus()
        }
        pendingEditors = null
        pendingAttempt = null
    }

    private fun editorBundle() = Bundle().apply {
        putEditor("diameter", diameter)
        putEditor("poles", poles)
        putEditor("reduction", reduction)
    }

    private fun Bundle.putEditor(key: String, editor: EditText) {
        putBundle(key, Bundle().apply {
            putString("text", editor.text.toString())
            putInt("start", editor.selectionStart.coerceAtLeast(0))
            putInt("end", editor.selectionEnd.coerceAtLeast(0))
            putBoolean("focused", editor.hasFocus())
        })
    }

    private fun updateEnabled() {
        val enabled = loaded && !busy && token != null
        listOf(diameter, poles, reduction).forEach { it.isEnabled = enabled }
        saveButton.isEnabled = enabled
        saveButton.text = ui.label(if (busy && loaded) "vesc.drivetrain.saving" else "vesc.drivetrain.save")
        retry.isEnabled = !busy && token != null
    }

    private fun field(id: String, key: String, decimal: Boolean) = EditText(context).apply {
        tag = "vesc.drivetrain.$id"
        contentDescription = ui.label("vesc.drivetrain.$key")
        textSize = 17f
        setTextColor(ui.foreground)
        setSingleLine(true)
        minHeight = ui.dp(48)
        if (decimal) keyListener = DecimalKeyListener() else inputType = InputType.TYPE_CLASS_NUMBER
        addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) { if (loaded && !busy) clearFailure() }
            override fun afterTextChanged(s: Editable?) {}
        })
    }

    private fun decimalText(editor: EditText) = editor.text.toString().replace(',', '.')

    private fun clearFailure() {
        diameter.error = null; poles.error = null; reduction.error = null
        feedback.visibility = View.GONE
        retry.visibility = View.GONE
    }

    private fun showFailure(error: Throwable, loading: Boolean) {
        val field = when (error) {
            is MobileVescDrivetrainException.InvalidWheelDiameter -> diameter to "wheel_diameter"
            is MobileVescDrivetrainException.InvalidMotorPolePairs -> poles to "motor_pole_pairs"
            is MobileVescDrivetrainException.InvalidReduction -> reduction to "reduction"
            else -> null
        }
        if (field != null) {
            field.first.error = ui.label("vesc.drivetrain.error.${field.second}")
            field.first.requestFocus()
        } else {
            val key = when (error) {
                is MobileVescDrivetrainException.ConnectionUnavailable -> "connection"
                is MobileVescDrivetrainException.UnsupportedDevice -> "unsupported"
                else -> if (loading) "load" else "persistence"
            }
            feedback.text = ui.label("vesc.drivetrain.error.$key")
            feedback.visibility = View.VISIBLE
        }
        retry.visibility = if (loading) View.VISIBLE else View.GONE
    }
}

/** Digits plus comma and period. A locale listener would drop the stored period when a draft reloads. */
@Suppress("DEPRECATION")
private class DecimalKeyListener : DigitsKeyListener(false, true) {
    override fun getAcceptedChars(): CharArray = ACCEPTED

    private companion object {
        val ACCEPTED = charArrayOf('0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', ',')
    }
}
