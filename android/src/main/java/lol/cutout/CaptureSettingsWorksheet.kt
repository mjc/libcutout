package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.graphics.Color
import android.os.Handler
import android.os.Looper
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.*
import uniffi.cutout_mobile_ffi.*
import java.text.DecimalFormatSymbols
import java.util.Locale

/** Native, transient editor for values the rider independently observed during a capture. */
class CaptureSettingsWorksheet(
    private val context: Context,
    private val save: (MobileCaptureSettingsDraftDto, (Result<MobileCaptureSettingsObservationDto>) -> Unit) -> Unit,
) {
    private val ui = CutoutUi(context)
    private val main = Handler(Looper.getMainLooper())
    private val profiles = captureResearchCatalogs()
    private val groups = listOf(
        MobileSettingGroupDto.LIMITS, MobileSettingGroupDto.INTERFACE, MobileSettingGroupDto.RIDING,
        MobileSettingGroupDto.MODES, MobileSettingGroupDto.DIAGNOSTICS,
    )
    private val values = linkedMapOf<String, String>()
    private val units = linkedMapOf<String, String>()
    private val states = linkedMapOf<String, String>()
    private var profileId = ""
    private var source = MobileCaptureObservationSourceDto.MANUFACTURER_APP
    private var note = ""
    private var earlierTime = ""
    private var useEarlierTime = false
    private var search = ""
    private var saving = false
    private var error: String? = null
    private var dialog: AlertDialog? = null
    private var fieldContainer: LinearLayout? = null
    private var errorView: TextView? = null
    private var saveButton: Button? = null
    private var cancelButton: Button? = null
    private var scroller: ScrollView? = null
    private var form: LinearLayout? = null
    private var earlierToggle: CheckBox? = null
    private var timeEditor: EditText? = null
    private var nowLabel: TextView? = null
    private var sourceSpinner: Spinner? = null
    private var noteEditor: EditText? = null
    private var profileDetail: TextView? = null

    fun show(): AlertDialog {
        check(dialog == null) { "Capture settings worksheet can only be shown once" }
        val body = ui.column().apply {
            setPadding(ui.dp(20), ui.dp(8), ui.dp(20), ui.dp(12))
            addView(ui.text(ui.label("capture.settings.worksheet.title"), 23f, true).apply { isAccessibilityHeading = true })
            addView(ui.space(12))
            addProfilePicker(this)
            addSourcePicker(this)
            addTimeControls(this)
            val searchBox = EditText(context).apply {
                tag = "capture.settings.search"
                hint = ui.label("capture.settings.search")
                singleLine = true
                inputType = InputType.TYPE_CLASS_TEXT
                setTextColor(ui.foreground)
                setHintTextColor(ui.muted)
                setCompoundDrawablesWithIntrinsicBounds(CutoutIcon(CutoutSymbol.INFO, ui.muted), null, null, null)
                setPadding(ui.dp(12), ui.dp(8), ui.dp(12), ui.dp(8))
                background = ui.rounded(12)
                addTextChangedListener(SimpleTextWatcher { search = it; renderFields() })
            }
            addView(searchBox, LinearLayout.LayoutParams(-1, ui.dp(48)).apply { bottomMargin = ui.dp(12) })
            fieldContainer = ui.column().also { addView(it, LinearLayout.LayoutParams(-1, -2)) }
            errorView = ui.text("", 14f, color = ui.red).apply {
                tag = "capture.settings.error"
                visibility = View.GONE
                setPadding(0, ui.dp(8), 0, ui.dp(8))
            }.also { addView(it) }
            noteEditor = EditText(context).apply {
                tag = "capture.settings.note"
                hint = ui.label("capture.settings.optional")
                minLines = 2
                maxLines = 5
                gravity = Gravity.TOP
                inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
                setTextColor(ui.foreground)
                setHintTextColor(ui.muted)
                setPadding(ui.dp(12), ui.dp(10), ui.dp(12), ui.dp(10))
                background = ui.rounded(12)
                addTextChangedListener(SimpleTextWatcher { note = it })
            }
            addView(ui.text(ui.label("capture.settings.note"), 17f, true))
            addView(ui.space(6))
            addView(noteEditor, LinearLayout.LayoutParams(-1, -2))
        }
        form = body
        renderFields()
        val scroll = ScrollView(context).apply {
            isFillViewport = false
            clipToPadding = false
            addView(body, ViewGroup.LayoutParams(-1, -2))
        }
        scroller = scroll
        val result = AlertDialog.Builder(context)
            .setView(scroll)
            .setNegativeButton(ui.label("capture.settings.cancel"), null)
            .setPositiveButton(ui.label("capture.settings.save"), null)
            .create()
        dialog = result
        result.setOnShowListener {
            saveButton = result.getButton(AlertDialog.BUTTON_POSITIVE).apply {
                tag = "capture.settings.save"
                setTextColor(Color.BLACK)
                background = ui.rounded(14, ui.yellow, outlined = false)
                setOnClickListener { submit() }
            }
            cancelButton = result.getButton(AlertDialog.BUTTON_NEGATIVE).apply { tag = "capture.settings.cancel" }
            result.window?.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, (context.resources.displayMetrics.heightPixels * .88f).toInt())
            updatePending()
        }
        result.show()
        return result
    }

    private fun addProfilePicker(parent: LinearLayout) {
        parent.addView(ui.text(ui.label("capture.settings.profile"), 15f, true, ui.muted))
        val choices = listOf(ui.label("capture.settings.choose_profile")) + profiles.map { it.model }
        val spinner = spinner(choices, "capture.settings.profile")
        spinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                val next = profiles.getOrNull(position - 1)?.profileId.orEmpty()
                if (next == profileId) return
                profileId = next
                values.clear(); units.clear(); states.clear(); error = null
                profileDetail?.text = catalog()?.let { "${it.pevCategory.uppercase(Locale.ROOT)} · ${it.model}" }.orEmpty()
                profileDetail?.visibility = if (profileId.isEmpty()) View.GONE else View.VISIBLE
                renderFields()
                updateError()
            }
        }
        parent.addView(spinner, matchHeight())
        profileDetail = ui.text("", 14f, color = ui.muted).apply {
            tag = "capture.settings.profile.detail"
            visibility = View.GONE
        }.also { parent.addView(it) }
        parent.addView(ui.space(10))
    }

    private fun addSourcePicker(parent: LinearLayout) {
        parent.addView(ui.text(ui.label("capture.settings.read_from"), 15f, true, ui.muted))
        val labels = listOf("capture.settings.source.manufacturer_app", "capture.settings.source.wheel_display", "capture.settings.source.other")
            .map(ui::label)
        sourceSpinner = spinner(labels, "capture.settings.source").apply {
            setSelection(0, false)
            onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
                override fun onNothingSelected(parent: AdapterView<*>?) = Unit
                override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                    source = listOf(MobileCaptureObservationSourceDto.MANUFACTURER_APP,
                        MobileCaptureObservationSourceDto.WHEEL_DISPLAY, MobileCaptureObservationSourceDto.OTHER)[position]
                }
            }
        }
        parent.addView(sourceSpinner, matchHeight())
        parent.addView(ui.space(8))
    }

    private fun addTimeControls(parent: LinearLayout) {
        earlierToggle = CheckBox(context).apply {
            tag = "capture.settings.earlier"
            text = ui.label("capture.settings.earlier")
            setTextColor(ui.foreground)
            buttonTintList = android.content.res.ColorStateList.valueOf(ui.yellow)
            setOnCheckedChangeListener { _, checked ->
                useEarlierTime = checked
                timeEditor?.visibility = if (checked) View.VISIBLE else View.GONE
                nowLabel?.visibility = if (checked) View.GONE else View.VISIBLE
            }
        }.also { parent.addView(it) }
        parent.addView(ui.text(ui.label("capture.settings.observed_at"), 15f, true, ui.muted))
        nowLabel = ui.text(ui.label("capture.settings.now"), 15f, color = ui.muted).apply {
            tag = "capture.settings.now"
        }.also { parent.addView(it) }
        timeEditor = EditText(context).apply {
            tag = "capture.settings.time"
            hint = ui.label("capture.settings.now")
            singleLine = true
            inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_FLAG_DECIMAL
            setTextColor(ui.foreground); setHintTextColor(ui.muted)
            addTextChangedListener(SimpleTextWatcher { earlierTime = it })
            visibility = View.GONE
        }.also { parent.addView(it, matchHeight()) }
        parent.addView(ui.space(10))
    }

    private fun renderFields() {
        val host = fieldContainer ?: return
        host.removeAllViews()
        val catalog = catalog()
        if (catalog == null) {
            host.addView(ui.text(ui.label("capture.settings.empty"), 15f, color = ui.muted).apply {
                tag = "capture.settings.empty"
                setPadding(0, ui.dp(8), 0, ui.dp(12))
            })
            return
        }
        groups.forEach { group ->
            val fields = catalog.fields.filter { it.group == group && !it.researchOnly && matches(it) }
            if (fields.isNotEmpty()) addFieldGroup(host, groupLabel(group), fields)
        }
        val research = catalog.fields.filter { it.researchOnly && matches(it) }
        if (research.isNotEmpty()) {
            val disclosure = TextView(context).apply {
                text = "${ui.label("capture.settings.research_fields")}  ▾"
                textSize = 17f; setTextColor(ui.foreground); isClickable = true; isFocusable = true
                setPadding(0, ui.dp(14), 0, ui.dp(10)); tag = "capture.settings.research.toggle"
            }
            val nested = ui.column().apply { visibility = View.GONE; tag = "capture.settings.research.fields" }
            disclosure.setOnClickListener {
                nested.visibility = if (nested.visibility == View.VISIBLE) View.GONE else View.VISIBLE
                disclosure.text = "${ui.label("capture.settings.research_fields")}  ${if (nested.visibility == View.VISIBLE) "▴" else "▾"}"
            }
            host.addView(disclosure); host.addView(nested)
            research.forEach { addFieldRow(nested, it) }
        }
        if (host.childCount == 0) host.addView(ui.text(ui.label("capture.settings.no_matches"), 15f, color = ui.muted))
    }

    private fun addFieldGroup(host: LinearLayout, title: String, fields: List<MobileCaptureResearchFieldDto>) {
        host.addView(ui.text(title, 20f, true).apply { isAccessibilityHeading = true })
        host.addView(ui.space(4))
        fields.forEach { addFieldRow(host, it) }
        host.addView(ui.space(12))
    }

    private fun addFieldRow(host: LinearLayout, field: MobileCaptureResearchFieldDto) {
        val row = ui.column().apply {
            tag = "capture.settings.field.${field.id}"
            setPadding(0, ui.dp(8), 0, ui.dp(8))
            addView(ui.text(ui.label(field.labelKey), 16f, true))
            addView(ui.space(4))
        }
        when (val control = field.control) {
            MobileSettingControlDto.Boolean -> addControlSpinner(row, field, listOf(
                ControlOption("", ui.label("capture.settings.unknown")),
                ControlOption("unobserved", ui.label("capture.settings.not_observed")),
                ControlOption("true", ui.label("capture.settings.on")),
                ControlOption("false", ui.label("capture.settings.off")),
            ))
            is MobileSettingControlDto.Choices -> addControlSpinner(row, field,
                listOf(ControlOption("", ui.label("capture.settings.unknown")),
                    ControlOption("unobserved", ui.label("capture.settings.not_observed"))) +
                    control.choices.map { ControlOption(it.id.toString(), ui.label(it.labelKey)) })
            is MobileSettingControlDto.Number -> {
                addStateSpinner(row, field, control.canDisable)
                addValueEditor(row, field)
            }
            MobileSettingControlDto.ReadOnly -> {
                addStateSpinner(row, field, false)
                addValueEditor(row, field)
            }
        }
        host.addView(row)
        host.addView(ui.divider())
    }

    private data class ControlOption(val value: String, val title: String)

    private fun addControlSpinner(parent: LinearLayout, field: MobileCaptureResearchFieldDto, options: List<ControlOption>) {
        val spinner = spinner(options.map { it.title }, "capture.settings.control.${field.id}")
        val selected = options.indexOfFirst { it.value == controlSelection(field.id) }.coerceAtLeast(0)
        spinner.setSelection(selected, false)
        spinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                val value = options[position].value
                states[field.id] = value
                if (value.isEmpty() || value == "unobserved" || value == "true" || value == "false") values.remove(field.id)
                else values[field.id] = value
            }
        }
        parent.addView(spinner, matchHeight())
    }

    private fun addStateSpinner(parent: LinearLayout, field: MobileCaptureResearchFieldDto, canDisable: Boolean) {
        val options = mutableListOf(ControlOption("value", ui.label("capture.settings.value")))
        if (canDisable) options += ControlOption("off", ui.label("capture.settings.off"))
        options += ControlOption("unobserved", ui.label("capture.settings.not_observed"))
        val spinner = spinner(options.map { it.title }, "capture.settings.state.${field.id}")
        spinner.setSelection(options.indexOfFirst { it.value == states[field.id] }.takeIf { it >= 0 } ?: 0, false)
        spinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                states[field.id] = options[position].value
            parent.findViewWithTag<View>("capture.settings.value.${field.id}")?.visibility =
                if (options[position].value == "off" || options[position].value == "unobserved") View.GONE else View.VISIBLE
            }
        }
        parent.addView(spinner, matchHeight())
    }

    private fun addValueEditor(parent: LinearLayout, field: MobileCaptureResearchFieldDto) {
        val editor = EditText(context).apply {
            tag = "capture.settings.value.${field.id}"
            hint = ui.label("capture.settings.unknown")
            singleLine = true
            inputType = InputType.TYPE_CLASS_TEXT
            setText(values[field.id].orEmpty())
            setTextColor(ui.foreground); setHintTextColor(ui.muted)
            visibility = if (states[field.id] == "off" || states[field.id] == "unobserved") View.GONE else View.VISIBLE
            addTextChangedListener(SimpleTextWatcher { values[field.id] = it })
        }
        val unit = defaultUnit(field)
        if (unit == "km/h") {
            val unitsList = listOf("km/h", "mph", "")
            val unitPicker = spinner(listOf("km/h", "mph", ui.label("capture.settings.unknown_unit")), "capture.settings.unit.${field.id}")
            unitPicker.setSelection(unitsList.indexOf(units[field.id] ?: unit).coerceAtLeast(0), false)
            unitPicker.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
                override fun onNothingSelected(parent: AdapterView<*>?) = Unit
                override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                    units[field.id] = unitsList[position]
                }
            }
            parent.addView(ui.row().apply {
                addView(editor, LinearLayout.LayoutParams(0, ui.dp(52), 1f))
                addView(unitPicker, LinearLayout.LayoutParams(ui.dp(112), ui.dp(52)))
            }, matchHeight())
        } else {
            parent.addView(ui.row().apply {
                addView(editor, LinearLayout.LayoutParams(0, ui.dp(52), 1f))
                if (unit.isNotEmpty()) addView(ui.text(unit, 15f, color = ui.muted).apply { setPadding(ui.dp(8), 0, ui.dp(8), 0) })
            }, matchHeight())
        }
    }

    private fun submit() {
        if (saving) return
        val draft = try { observation() } catch (failure: IllegalArgumentException) {
            error = failure.message
            updateError()
            return
        }
        saving = true
        error = null
        updatePending(); updateError()
        val submittedDialog = dialog
        try { save(draft) { result -> main.post {
            if (!saving || dialog !== submittedDialog || submittedDialog?.isShowing != true) return@post
            saving = false
            result.onSuccess {
                error = null
                dialog?.dismiss()
            }.onFailure {
                error = CaptureResearchPresentation.error(ui, it)
                updatePending(); updateError()
            }
        } } } catch (failure: Throwable) {
            saving = false
            error = CaptureResearchPresentation.error(ui, failure)
            updatePending(); updateError()
        }
    }

    private fun observation(): MobileCaptureSettingsDraftDto {
        val catalog = catalog() ?: throw IllegalArgumentException(ui.label("capture.settings.profile_required"))
        val atMs = if (useEarlierTime) {
            val separator = DecimalFormatSymbols.getInstance(Locale.getDefault()).decimalSeparator.toString()
            val seconds = earlierTime.trim().replace(separator, ".").toDoubleOrNull()
            require(seconds != null && seconds.isFinite() && seconds >= 0.0 && seconds < ULong.MAX_VALUE.toDouble() / 1000.0) {
                ui.label("capture.settings.invalid_time")
            }
            (seconds * 1000.0).toULong()
        } else null
        val reported = catalog.fields.mapNotNull { field ->
            when (states[field.id]) {
                "unobserved" -> MobileCaptureSettingObservationValueDto(field.id, MobileCaptureObservedValueDto.Unobserved,
                    units[field.id] ?: defaultUnit(field).takeIf(String::isNotEmpty), null)
                "off" -> MobileCaptureSettingObservationValueDto(field.id, MobileCaptureObservedValueDto.Off,
                    units[field.id] ?: defaultUnit(field).takeIf(String::isNotEmpty), null)
                "true" -> MobileCaptureSettingObservationValueDto(field.id, MobileCaptureObservedValueDto.Boolean(true), null, null)
                "false" -> MobileCaptureSettingObservationValueDto(field.id, MobileCaptureObservedValueDto.Boolean(false), null, null)
                else -> values[field.id]?.takeIf { it.trim().isNotEmpty() }?.let { entered ->
                    MobileCaptureSettingObservationValueDto(field.id, MobileCaptureObservedValueDto.Entered(entered),
                        units[field.id] ?: defaultUnit(field).takeIf(String::isNotEmpty), null)
                }
            }
        }
        return MobileCaptureSettingsDraftDto(
            MobileCaptureResearchProfileDto(catalog.profileId, catalog.pevCategory, catalog.model, null,
                MobileCaptureResearchProfileSelectionDto.MANUAL),
            atMs,
            source,
            reported,
            note.takeIf(String::isNotEmpty),
        )
    }

    private fun updatePending() {
        saveButton?.isEnabled = !saving
        cancelButton?.isEnabled = !saving
        dialog?.setCancelable(!saving)
        dialog?.setCanceledOnTouchOutside(!saving)
        scroller?.isEnabled = !saving
        form?.let { setEnabledRecursively(it, !saving) }
        saveButton?.text = ui.label(if (saving) "capture.settings.saving" else "capture.settings.save")
    }

    private fun setEnabledRecursively(view: View, enabled: Boolean) {
        view.isEnabled = enabled
        if (view is ViewGroup) for (index in 0 until view.childCount) setEnabledRecursively(view.getChildAt(index), enabled)
    }

    private fun updateError() {
        errorView?.let { view ->
            view.text = error.orEmpty()
            view.visibility = if (error == null) View.GONE else View.VISIBLE
        }
    }

    private fun matches(field: MobileCaptureResearchFieldDto) =
        search.isBlank() || ui.label(field.labelKey).contains(search, ignoreCase = true)
    private fun catalog() = profileId.takeIf(String::isNotEmpty)?.let(::captureResearchCatalog)

    private fun controlSelection(id: String): String = when (states[id]) {
        "unobserved", "true", "false" -> states[id].orEmpty()
        else -> values[id].orEmpty()
    }

    private fun groupLabel(group: MobileSettingGroupDto) = when (group) {
        MobileSettingGroupDto.LIMITS -> ui.label("controls.group.limits")
        MobileSettingGroupDto.INTERFACE -> ui.label("controls.group.interface")
        MobileSettingGroupDto.RIDING -> ui.label("controls.group.riding")
        MobileSettingGroupDto.MODES -> ui.label("capture.settings.group.modes")
        MobileSettingGroupDto.DIAGNOSTICS -> ui.label("capture.settings.group.other_observations")
        else -> group.name.lowercase().replaceFirstChar(Char::uppercase)
    }

    private fun defaultUnit(field: MobileCaptureResearchFieldDto) = CaptureResearchPresentation.defaultUnit(field)

    private fun spinner(items: List<String>, viewTag: String) = Spinner(context).apply {
        tag = viewTag
        adapter = ArrayAdapter(context, android.R.layout.simple_spinner_dropdown_item, items).apply {
            setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item)
        }
        minimumHeight = ui.dp(48)
    }

    private fun matchHeight() = LinearLayout.LayoutParams(-1, ui.dp(52)).apply { bottomMargin = ui.dp(6) }
}

/** Shared unit mapping for capture observation review and editing. */
internal object CaptureResearchPresentation {
    fun defaultUnit(field: MobileCaptureResearchFieldDto): String {
        field.unitHint?.let { return it }
        val unit = (field.control as? MobileSettingControlDto.Number)?.unit
        return when (unit) {
            MobileSettingUnitDto.PERCENT, MobileSettingUnitDto.PWM_DUTY_PERCENT -> "%"
            MobileSettingUnitDto.KILOMETRES_PER_HOUR -> "km/h"
            MobileSettingUnitDto.DEGREES -> "°"
            MobileSettingUnitDto.LEVEL -> "level"
            MobileSettingUnitDto.SECONDS -> "s"
            MobileSettingUnitDto.MINUTES -> "min"
            MobileSettingUnitDto.VOLTS -> "V"
            MobileSettingUnitDto.AMPERES -> "A"
            MobileSettingUnitDto.HERTZ -> "Hz"
            MobileSettingUnitDto.DEGREES_PER_SECOND -> "°/s"
            MobileSettingUnitDto.CELSIUS -> "°C"
            MobileSettingUnitDto.WATTS -> "W"
            MobileSettingUnitDto.MILLISECONDS -> "ms"
            MobileSettingUnitDto.REVOLUTIONS_PER_MINUTE -> "rpm"
            MobileSettingUnitDto.MILLIMETRES -> "mm"
            MobileSettingUnitDto.AMP_HOURS -> "Ah"
            else -> if (field.id == "displayed_pack_voltage") "V" else ""
        }
    }

    fun error(ui: CutoutUi, error: Throwable): String = when (error) {
        is MobileCaptureResearchException.InvalidObservation -> error.reason
        is MobileCaptureResearchException.NotRecording -> ui.label("capture.settings.not_recording")
        is MobileCaptureResearchException.AdmissionLost -> ui.label("capture.settings.not_saved")
        is MobileCaptureResearchException.WriterFailed -> ui.label("capture.settings.save_unconfirmed")
        else -> error.message ?: ui.label("capture.settings.save_error")
    }
}

private class SimpleTextWatcher(private val changed: (String) -> Unit) : android.text.TextWatcher {
    override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
    override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) { changed(s?.toString().orEmpty()) }
    override fun afterTextChanged(s: android.text.Editable?) = Unit
}
