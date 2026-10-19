package lol.cutout

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.text.InputType
import android.text.TextUtils
import android.view.Gravity
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.ScrollView
import android.widget.Switch
import android.app.AlertDialog
import uniffi.cutout_mobile_ffi.*
import java.math.BigDecimal
import java.util.IdentityHashMap
import java.util.Currency
import java.util.Locale

/** Native edit form. Rust normalizes and validates the exact payload submitted to storage. */
class ChargingSpotEditorPanel(
    context: Context,
    private val spot: MobileChargingSpotDto?,
    initialCoordinate: MobileMapCoordinateDto,
    private val onCancel: () -> Unit,
    private val onSave: (MobileChargingInputDto) -> Unit,
    initialInput: MobileChargingInputDto? = spot?.input,
    initialLocation: MobileMapCoordinateDto? = null,
    private val contextResolver: ChargingEditorContextResolver? = null,
) : LinearLayout(context) {
    internal enum class Kind { CHARGING, FOOD, EV }

    private val ui = CutoutUi(context)
    private val body = ui.column()
    private val latitude = field(context.getString(R.string.charging_latitude), initialCoordinate.latitudeDegrees.toString(), true)
    private val longitude = field(context.getString(R.string.charging_longitude), initialCoordinate.longitudeDegrees.toString(), true)
    private val accessNote = field(context.getString(R.string.charging_access_note), initialInput?.accessNote.orEmpty(), false, true)
    private val name = field(context.getString(R.string.charging_name), initialInput?.name.orEmpty())
    private val hours = field(context.getString(R.string.charging_hours), initialInput?.hours.orEmpty())
    private val price = field(context.getString(R.string.charging_price), initialInput?.price?.let(::priceText).orEmpty(), true)
        .apply { tag = "charging.editor.price.amount" }
    private val currency = field(context.getString(R.string.charging_currency), initialInput?.price?.currency.orEmpty())
        .apply { tag = "charging.editor.price.currency" }
    private val issue = ui.text("", 14f, false, ui.red).apply {
        tag = "charging.editor.validation-error"
        visibility = View.GONE
    }
    private val venueButton = ui.action(ui.label("charging.editor.venue.choose")) { showVenueMenu() }.apply {
        tag = "charging.editor.venue"
    }
    private val termsButton = ui.action(ui.label("charging.editor.terms")) { showTermsMenu() }.apply { tag = "charging.editor.terms" }
    private val basisButton = ui.action(ui.label("charging.editor.price.basis")) { showBasisMenu() }.apply { tag = "charging.editor.price.basis" }
    private val supplies = ui.column().apply { tag = "charging.editor.supplies" }
    private val supplyRows = mutableListOf<SupplyRow>()
    private var selectedPlug = initialInput?.supplies?.firstOrNull()?.connectors?.firstOrNull()
    private var plugWasEdited = false
    private var outletCount = initialInput?.supplies?.firstOrNull()?.outletCount?.toInt() ?: 1
    private var outletCountWasEdited = false
    private val defaultSupplyControls = ui.column()
    private val suggestedPlugButton = ui.action("", false) { showPlugMenu() }.apply {
        tag = "charging.editor.plug-type"
    }
    private val outletCountButton = ui.action("", false) { showOutletCountMenu() }.apply {
        tag = "charging.editor.outlet-count"
    }
    private var defaultPowerDraft = mobileChargingPowerDraft(countryCode = null, evStation = false)
    private var draftEvKind = false
    private val kindButtons = linkedMapOf<Kind, View>()
    private val details = ui.column()
    private val detailsContent = ui.column().apply { tag = "charging.editor.details-content" }
    private val detailsButton = ui.action("", false) { setDetailsExpanded(!detailsExpanded) }.apply {
        tag = "charging.editor.add-details"
    }
    private val optionalDetailsError = ui.text(ui.label("charging.editor.error.optional_details"), 14f, false, ui.red).apply {
        tag = "charging.editor.optional-details-error"
        isClickable = true
        isFocusable = true
        setOnClickListener { setDetailsExpanded(true) }
        visibility = View.GONE
    }
    private lateinit var saveButton: android.widget.Button
    private val evPaid = Switch(context).apply {
        text = ui.label("charging.cost.paid")
        contentDescription = ui.label("charging.cost.paid")
        tag = "charging.editor.ev-paid"
    }
    private var cost = initialInput?.cost ?: MobileChargingCostDto.FREE
    private var venue = initialInput?.venue
    private var terms = initialInput?.terms
    private var priceBasis = initialInput?.price?.basis
    private var originalPrice = initialInput?.price
    private var selectedKind: Kind? = initialInput?.let(::kindFor)
    private var detailsExpanded = initialInput != null
    private var countryCode: String? = null
    private var orderedConnectorChoices: List<MobileChargingConnectorDto>? = null
    private var contextRequest: java.io.Closeable? = null
    private var disposed = false
    private var saving = false
    private var enabledBeforeSaving: IdentityHashMap<View, Boolean>? = null
    private var hadFreshLocation = initialLocation != null

    init {
        orientation = VERTICAL
        clipToPadding = false
        val header = ui.row().apply {
            tag = "charging.editor.navigation"
            setPadding(ui.dp(10), ui.dp(6), ui.dp(10), ui.dp(6))
            gravity = Gravity.CENTER_VERTICAL
        }
        header.addView(ui.action(ui.label("charging.editor.cancel")) {
            if (!saving && !disposed) onCancel()
        }.apply {
            tag = "charging.editor.cancel"
            setTextColor(ui.brand)
            background = android.graphics.drawable.RippleDrawable(
                android.content.res.ColorStateList.valueOf(Color.argb(40, 255, 214, 0)), null, null)
            minWidth = 0
            minimumWidth = 0
            setPadding(ui.dp(8), ui.dp(4), ui.dp(8), ui.dp(4))
        }, LayoutParams(-2, -2))
        header.addView(ui.text(ui.label(if (spot == null) "charging.editor.title" else "charging.editor.edit_title"), 17f, true).apply {
            tag = "charging.editor.title"
            gravity = Gravity.CENTER
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
        }, LayoutParams(0, -2, 1f).apply { leftMargin = ui.dp(4); rightMargin = ui.dp(4) })
        saveButton = ui.action(ui.label("charging.editor.save"), true) { save() }.apply {
            tag = "charging.editor.save"
            minWidth = ui.dp(64)
            minimumWidth = ui.dp(64)
            minHeight = ui.dp(44)
            minimumHeight = ui.dp(44)
            setPadding(ui.dp(12), ui.dp(4), ui.dp(12), ui.dp(4))
        }
        header.addView(saveButton, LayoutParams(-2, -2))
        addView(header, LayoutParams(-1, -2))
        body.setPadding(ui.dp(18), ui.dp(12), ui.dp(18), ui.dp(20))
        addView(ScrollView(context).apply {
            tag = "charging.editor.scroll"
            isFillViewport = true
            clipToPadding = false
            addView(body, android.widget.FrameLayout.LayoutParams(-1, -2))
        }, LayoutParams(-1, 0, 1f))

        val choices = ui.row().apply { tag = "charging.editor.kind-choices" }
        Kind.values().forEach { kind ->
            val choice = kindChoice(kind)
            kindButtons[kind] = choice
            val params = LinearLayout.LayoutParams(0, -2, 1f)
            if (kind != Kind.CHARGING) params.leftMargin = ui.dp(8)
            choices.addView(choice, params)
        }
        if (context.resources.configuration.fontScale >= ACCESSIBILITY_FONT_SCALE) {
            choices.orientation = LinearLayout.VERTICAL
            for (index in 0 until choices.childCount) {
                choices.getChildAt(index).layoutParams = LinearLayout.LayoutParams(-1, -2).apply {
                    if (index > 0) topMargin = ui.dp(8)
                }
            }
        }
        body.addView(choices, spaced())

        body.addView(venueButton, spaced())
        body.addView(evPaid, spaced())
        evPaid.setOnCheckedChangeListener { _, checked ->
            if (selectedKind == Kind.EV) setCost(if (checked) MobileChargingCostDto.PAID else MobileChargingCostDto.FREE)
        }

        val noteSection = ui.column().apply { tag = "charging.editor.required-note" }
        noteSection.addView(ui.text(ui.label("charging.editor.access_note"), 14f, color = ui.muted))
        accessNote.hint = ""
        accessNote.tag = "charging.editor.access-note"
        accessNote.gravity = Gravity.TOP or Gravity.START
        accessNote.minHeight = ui.dp(72)
        accessNote.maxLines = Int.MAX_VALUE
        noteSection.addView(accessNote, spaced())
        body.addView(noteSection, spaced())

        issue.visibility = View.GONE
        body.addView(issue, spaced())
        details.addView(detailsButton)
        details.addView(optionalDetailsError, spaced())
        details.addView(detailsContent)
        body.addView(details, spaced())

        detailsContent.addView(ui.text(ui.label("charging.editor.location.section"), 18f, true), spaced())
        detailsContent.addView(ui.text(ui.label("charging.editor.latitude"), 14f, color = ui.muted), spaced())
        latitude.tag = "charging_editor.latitude"
        detailsContent.addView(latitude)
        detailsContent.addView(ui.text(ui.label("charging.editor.longitude"), 14f, color = ui.muted), spaced())
        longitude.tag = "charging_editor.longitude"
        detailsContent.addView(longitude)
        detailsContent.addView(ui.text(ui.label("charging.editor.details.section"), 18f, true), spaced())
        name.hint = ""
        name.tag = "charging_editor.name"
        detailsContent.addView(ui.text(ui.label("charging.editor.name"), 14f, color = ui.muted), spaced())
        detailsContent.addView(name)
        hours.hint = ""
        hours.tag = "charging_editor.hours"
        hours.setSingleLine(false)
        hours.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
        hours.minLines = 1
        hours.maxLines = 4
        detailsContent.addView(ui.text(ui.label("charging.editor.hours"), 14f, color = ui.muted), spaced())
        detailsContent.addView(hours)
        detailsContent.addView(ui.text(ui.label("charging.editor.supply.section"), 18f, true), spaced())
        defaultSupplyControls.addView(suggestedPlugButton)
        defaultSupplyControls.addView(outletCountButton, spaced())
        detailsContent.addView(defaultSupplyControls, spaced())
        detailsContent.addView(supplies, spaced())
        detailsContent.addView(ui.action(context.getString(R.string.charging_add_supply)) { addSupply(null) }.apply {
            tag = "charging_editor.add_supply"
        }, spaced())
        detailsContent.addView(ui.action(context.getString(R.string.charging_suggest_power)) { confirmSuggestedPower() }.apply {
            tag = "charging_editor.suggest_power"
        }, spaced())
        detailsContent.addView(termsButton, spaced())
        detailsContent.addView(ui.text(ui.label("charging.editor.price.amount"), 14f, color = ui.muted), spaced())
        price.hint = ""
        detailsContent.addView(price)
        detailsContent.addView(ui.text(ui.label("charging.editor.price.currency"), 14f, color = ui.muted), spaced())
        currency.hint = ""
        detailsContent.addView(currency)
        detailsContent.addView(basisButton, spaced())
        initialInput?.supplies.orEmpty().forEach { addSupply(it) }
        if (initialInput?.supplies.isNullOrEmpty()) {
            price.visibility = if (cost == MobileChargingCostDto.PAID) View.VISIBLE else View.GONE
        }
        accessNote.addTextChangedListener(textWatcher(::updateSaveEnabled))
        price.addTextChangedListener(textWatcher { originalPrice = null; updateSaveEnabled() })
        currency.addTextChangedListener(textWatcher { originalPrice = null; updateSaveEnabled() })
        latitude.addTextChangedListener(textWatcher(::updateSaveEnabled))
        longitude.addTextChangedListener(textWatcher(::updateSaveEnabled))
        setDetailsExpanded(detailsExpanded)
        updateCost()
        updateSelectors()
        updateKindChoices()
        updateSaveEnabled()
        resolveRegionalContext(initialLocation)
    }

    /** Mirrors Swift's editor/location-presence task identity without re-geocoding each fix. */
    fun updateFreshLocation(location: MobileMapCoordinateDto?) {
        if (disposed || (location != null) == hadFreshLocation) return
        hadFreshLocation = location != null
        contextRequest?.close()
        countryCode = null
        orderedConnectorChoices = null
        replaceDefaultPowerDraft(defaultPowerDraft.withCountry(null))
        supplyRows.forEach { it.updateCountry(null) }
        updateDefaultSupplyControls()
        supplyRows.forEach(SupplyRow::refreshConnectorChoices)
        resolveRegionalContext(location)
    }

    private fun resolveRegionalContext(location: MobileMapCoordinateDto?) {
        contextRequest = contextResolver?.resolve(location) { regionalContext ->
            if (disposed) return@resolve
            countryCode = regionalContext.countryCode
            orderedConnectorChoices = regionalContext.connectorChoices
            replaceDefaultPowerDraft(defaultPowerDraft.withCountry(countryCode))
            supplyRows.forEach { it.updateCountry(countryCode) }
            updateDefaultSupplyControls()
            supplyRows.forEach(SupplyRow::refreshConnectorChoices)
        }
    }

    /** Cancels only this editor's pending native geocode/context result. */
    fun dispose() {
        disposed = true
        contextRequest?.close()
        contextRequest = null
        defaultPowerDraft.close()
        supplyRows.forEach(SupplyRow::dispose)
    }

    val isSaving: Boolean get() = saving

    /** Keeps editor actions inert until the asynchronous save owner reports its result. */
    fun setSaving(value: Boolean) {
        if (disposed || saving == value) return
        saving = value
        if (value) {
            findFocus()?.clearFocus()
            (context.getSystemService(Context.INPUT_METHOD_SERVICE) as? android.view.inputmethod.InputMethodManager)
                ?.hideSoftInputFromWindow(windowToken, 0)
            val states = IdentityHashMap<View, Boolean>()
            fun capture(view: View) {
                states[view] = view.isEnabled
                view.isEnabled = false
                if (view is android.view.ViewGroup) {
                    for (index in 0 until view.childCount) capture(view.getChildAt(index))
                }
            }
            capture(this)
            enabledBeforeSaving = states
            updateSaveAppearance()
        } else {
            val states = enabledBeforeSaving
            enabledBeforeSaving = null
            states?.forEach { (view, enabled) -> view.isEnabled = enabled }
            updateSaveEnabled()
        }
    }

    private fun field(label: String, value: String, numeric: Boolean = false, multiline: Boolean = false) = EditText(context).apply {
        hint = label
        contentDescription = label
        textSize = 16f
        setTextColor(ui.foreground)
        setHintTextColor(ui.muted)
        setText(value)
        inputType = if (numeric) InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_FLAG_DECIMAL or InputType.TYPE_NUMBER_FLAG_SIGNED
            else InputType.TYPE_CLASS_TEXT or if (multiline) InputType.TYPE_TEXT_FLAG_MULTI_LINE else InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
        if (multiline) setSingleLine(false) else setSingleLine(true)
        minHeight = ui.dp(if (multiline) 72 else 48)
        setPadding(ui.dp(12), ui.dp(10), ui.dp(12), ui.dp(10))
        background = ui.rounded(12)
    }

    private fun spaced() = LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) }

    private fun kindFor(input: MobileChargingInputDto): Kind = when {
        input.venue == MobileChargingVenueDto.EV_CHARGING_STATION -> Kind.EV
        input.cost == MobileChargingCostDto.PAID -> Kind.FOOD
        else -> Kind.CHARGING
    }

    private fun kindChoice(kind: Kind): View {
        val labelKey = when (kind) {
            Kind.CHARGING -> "charging.editor.kind.charging"
            Kind.FOOD -> "charging.editor.kind.food"
            Kind.EV -> "charging.editor.kind.ev"
        }
        val label = ui.label(labelKey)
        val view = ui.column().apply {
            tag = "charging.editor.kind.${kind.name.lowercase(Locale.ROOT)}"
            gravity = Gravity.CENTER
            orientation = LinearLayout.VERTICAL
            minimumHeight = ui.dp(68)
            setPadding(ui.dp(8), ui.dp(8), ui.dp(8), ui.dp(8))
            addView(ChargingEditorKindIcon(context, kind).apply {
                layoutParams = LinearLayout.LayoutParams(ui.dp(24), ui.dp(24))
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            })
            addView(ui.text(label, 14f, color = ui.foreground).apply {
                gravity = Gravity.CENTER
                maxLines = 2
                ellipsize = TextUtils.TruncateAt.END
            }, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
            isClickable = true
            isFocusable = true
            contentDescription = label
            accessibilityDelegate = object : View.AccessibilityDelegate() {
                override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                    super.onInitializeAccessibilityNodeInfo(host, info)
                    info.className = android.widget.Button::class.java.name
                    info.isSelected = host.isSelected
                }
            }
            setOnClickListener { chooseKind(kind) }
        }
        return view
    }

    private fun chooseKind(kind: Kind) {
        if (selectedKind == kind) return
        selectedKind = kind
        when (kind) {
            Kind.CHARGING -> {
                setCost(MobileChargingCostDto.FREE)
                venue = null
            }
            Kind.FOOD -> {
                setCost(MobileChargingCostDto.PAID)
                if (venue == MobileChargingVenueDto.EV_CHARGING_STATION) venue = null
            }
            Kind.EV -> {
                setCost(MobileChargingCostDto.FREE)
                venue = MobileChargingVenueDto.EV_CHARGING_STATION
            }
        }
        updateKindChoices()
        updateCost()
        updateSelectors()
        updateSaveEnabled()
    }

    private fun setCost(next: MobileChargingCostDto) {
        if (cost == next) return
        cost = next
        if (next == MobileChargingCostDto.FREE) {
            terms = null
            priceBasis = null
            originalPrice = null
            price.setText("")
            currency.setText("")
        }
        updateCost()
        updateSelectors()
        updateSaveEnabled()
    }

    private fun updateKindChoices() {
        kindButtons.forEach { (kind, view) ->
            val selected = selectedKind == kind
            view.isSelected = selected
            view.background = android.graphics.drawable.GradientDrawable().apply {
                setColor(if (selected) Color.argb(28, Color.red(ui.brand), Color.green(ui.brand), Color.blue(ui.brand)) else ui.card)
                cornerRadius = ui.dp(14).toFloat()
                setStroke(ui.dp(if (selected) 2 else 1), if (selected) ui.brand else ui.stroke)
            }
        }
    }

    private fun setDetailsExpanded(expanded: Boolean) {
        detailsExpanded = expanded
        detailsContent.visibility = if (expanded) View.VISIBLE else View.GONE
        optionalDetailsError.visibility = if (!expanded && inputProblem() != null && hasRequiredInput()) View.VISIBLE else View.GONE
        detailsButton.text = ui.label("charging.editor.add_details") + if (expanded) "  ▴" else "  ▾"
        detailsButton.contentDescription = ui.label("charging.editor.add_details")
        detailsButton.accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                super.onInitializeAccessibilityNodeInfo(host, info)
                info.className = android.widget.Button::class.java.name
                info.isSelected = expanded
            }
        }
        updateSaveEnabled()
    }

    private fun textWatcher(onChanged: () -> Unit) = object : android.text.TextWatcher {
        override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
        override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) = onChanged()
        override fun afterTextChanged(s: android.text.Editable?) = Unit
    }

    private fun updateSaveEnabled() {
        if (!hasRequiredInput()) {
            saveButton.isEnabled = false
            updateSaveAppearance()
            issue.visibility = View.GONE
            optionalDetailsError.visibility = View.GONE
            return
        }
        val problem = inputProblem()
        saveButton.isEnabled = !saving && problem == null
        updateSaveAppearance()
        issue.text = problem.orEmpty()
        issue.visibility = if (problem != null && detailsExpanded) View.VISIBLE else View.GONE
        optionalDetailsError.visibility = if (problem != null && !detailsExpanded) View.VISIBLE else View.GONE
    }

    private fun updateSaveAppearance() {
        saveButton.alpha = if (!saving && saveButton.isEnabled) 1f else 0.4f
    }

    private fun hasRequiredInput() = selectedKind != null && !accessNote.text.isNullOrBlank()

    private fun inputProblem(): String? {
        if (!validCoordinate()) return ui.label("charging.editor.error.coordinate")
        if (cost == MobileChargingCostDto.PAID && hasPartialPrice() && runCatching { priceDto() }.getOrNull() == null) {
            return ui.label("charging.editor.error.price")
        }
        val input = try {
            buildInput()
        } catch (_: Exception) {
            return if (supplyRows.any { it.hasInvalidInput() }) ui.label("charging.editor.error.connectors")
            else ui.label("charging.editor.error.metadata")
        }
        return when (val validation = mobileChargingValidateEditor(input)) {
            is MobileChargingValidationDto.Valid -> null
            is MobileChargingValidationDto.Invalid -> validationMessage(validation.issue)
        }
    }

    private fun validationMessage(issue: MobileChargingFieldIssueDto): String = when (issue.field) {
        MobileChargingValidationFieldDto.COORDINATE -> ui.label("charging.editor.error.coordinate")
        MobileChargingValidationFieldDto.ACCESS_NOTE -> when (issue.reason) {
            MobileChargingValidationReasonDto.REQUIRED -> ui.label("charging.editor.error.access_note_blank")
            MobileChargingValidationReasonDto.TOO_LONG -> ui.label("charging.editor.error.access_note_too_long")
            MobileChargingValidationReasonDto.CONTROL_CHARACTER -> ui.label("charging.editor.error.access_note_control_character")
            else -> ui.label("charging.editor.error.metadata")
        }
        MobileChargingValidationFieldDto.NAME -> ui.label("charging.editor.error.name")
        MobileChargingValidationFieldDto.HOURS -> ui.label("charging.editor.error.hours")
        MobileChargingValidationFieldDto.PAYMENT -> ui.label("charging.editor.error.price")
        MobileChargingValidationFieldDto.SUPPLY -> when {
            issue.reason == MobileChargingValidationReasonDto.TOO_MANY -> ui.label("charging.editor.error.supplies")
            issue.reason == MobileChargingValidationReasonDto.REQUIRED -> ui.label("charging.editor.error.supply_confirmation")
            else -> ui.label("charging.editor.error.connectors")
        }
    }

    private fun validCoordinate(): Boolean {
        val lat = latitude.text.toString().toDoubleOrNull()
        val lon = longitude.text.toString().toDoubleOrNull()
        return lat != null && lon != null && lat.isFinite() && lon.isFinite() && lat in -90.0..90.0 && lon in -180.0..180.0
    }

    private fun hasPartialPrice(): Boolean = price.text.toString().isNotBlank() ||
        currency.text.toString().isNotBlank() || priceBasis != null

    private fun updateCost() {
        venueButton.visibility = if (selectedKind == Kind.FOOD) View.VISIBLE else View.GONE
        evPaid.visibility = if (selectedKind == Kind.EV) View.VISIBLE else View.GONE
        if (selectedKind == Kind.EV) {
            evPaid.setOnCheckedChangeListener(null)
            evPaid.isChecked = cost == MobileChargingCostDto.PAID
            evPaid.setOnCheckedChangeListener { _, checked ->
                if (selectedKind == Kind.EV) setCost(if (checked) MobileChargingCostDto.PAID else MobileChargingCostDto.FREE)
            }
        }
        price.visibility = if (cost == MobileChargingCostDto.PAID) View.VISIBLE else View.GONE
        currency.visibility = price.visibility
        termsButton.visibility = price.visibility
        basisButton.visibility = price.visibility
        defaultSupplyControls.visibility = if (supplyRows.isEmpty()) View.VISIBLE else View.GONE
        updateDefaultSupplyControls()
    }

    private fun updateSelectors() {
        val isEv = venue == MobileChargingVenueDto.EV_CHARGING_STATION
        if (isEv != draftEvKind) {
            draftEvKind = isEv
            replaceDefaultPowerDraft(defaultPowerDraft.withKind(isEv))
            supplyRows.forEach { it.updateKind(isEv) }
            if (isEv) {
                supplyRows.filter { it.isSuggested }.toList().forEach { row ->
                    supplies.removeView(row.root)
                    supplyRows.remove(row)
                    row.dispose()
                }
            }
        }
        venueButton.text = ui.label(when (venue) {
            null -> "charging.editor.venue.choose"
            MobileChargingVenueDto.RESTAURANT -> "charging.venue.restaurant"
            MobileChargingVenueDto.CAFE -> "charging.venue.cafe"
            MobileChargingVenueDto.BAR -> "charging.venue.bar"
            MobileChargingVenueDto.OTHER -> "charging.venue.other"
            MobileChargingVenueDto.EV_CHARGING_STATION -> "charging.editor.venue.choose"
        })
        termsButton.text = ui.label(when (terms) {
            null -> "charging.editor.terms.choose"
            MobileChargingTermsDto.DIRECT_FEE -> "charging.terms.direct_fee"
            MobileChargingTermsDto.PURCHASE_REQUIRED -> "charging.terms.purchase"
            MobileChargingTermsDto.BOTH -> "charging.terms.both"
        })
        basisButton.text = ui.label(when (priceBasis) {
            null -> "charging.editor.price.basis.choose"
            MobileChargingPriceBasisDto.PER_CHARGE -> "charging.price.per_charge"
            MobileChargingPriceBasisDto.PER_HOUR -> "charging.price.per_hour"
            MobileChargingPriceBasisDto.PER_KWH -> "charging.price.per_kwh"
            MobileChargingPriceBasisDto.PURCHASE -> "charging.price.purchase"
        })
        updateDefaultSupplyControls()
        supplyRows.forEach(SupplyRow::refreshConnectorChoices)
    }

    private fun updateDefaultSupplyControls() {
        val snapshot = defaultPowerDraft.snapshot()
        val connector = if (plugWasEdited) selectedPlug else selectedPlug ?: snapshot.selection.connectors.firstOrNull()
        suggestedPlugButton.text = connector?.let(::connectorText) ?: ui.label("charging.editor.plug.choose")
        suggestedPlugButton.contentDescription = "${ui.label("charging.editor.plug_type")}: ${suggestedPlugButton.text}"
        outletCountButton.text = "${ui.label("charging.editor.outlet_count")}: $outletCount"
    }

    private fun connectorChoices(): List<MobileChargingConnectorDto> =
        orderedConnectorChoices ?: mobileChargingConnectorChoices(countryCode)

    private fun replaceDefaultPowerDraft(next: MobileChargingPowerDraft) {
        val old = defaultPowerDraft
        defaultPowerDraft = next
        old.close()
    }

    private fun newPowerDraft(supply: MobileChargingSupplyDto?, evStation: Boolean): MobileChargingPowerDraft {
        val fresh = mobileChargingPowerDraft(countryCode, evStation)
        if (supply == null) return fresh
        val edited = fresh.edit(supply)
        fresh.close()
        return edited
    }

    private fun syncDefaultPowerDraft() {
        val supply = MobileChargingSupplyDto(
            voltageVolts = null,
            connectors = if (plugWasEdited) listOfNotNull(selectedPlug) else emptyList(),
            outletCount = if (outletCountWasEdited) outletCount.toUShort() else null,
        )
        replaceDefaultPowerDraft(defaultPowerDraft.edit(supply.takeIf { plugWasEdited || outletCountWasEdited }))
    }

    private fun showPlugMenu() {
        val popup = PopupMenu(context, suggestedPlugButton)
        popup.menu.add(ui.label("charging.editor.plug.choose")).setOnMenuItemClickListener {
            selectedPlug = null
            plugWasEdited = true
            syncDefaultPowerDraft()
            updateDefaultSupplyControls()
            updateSaveEnabled()
            true
        }
        connectorChoices().forEach { connector ->
            popup.menu.add(connectorText(connector)).setOnMenuItemClickListener {
                selectedPlug = connector
                plugWasEdited = true
                syncDefaultPowerDraft()
                updateDefaultSupplyControls()
                updateSaveEnabled()
                true
            }
        }
        popup.show()
    }

    private fun showOutletCountMenu() {
        val options = (1..10).toList()
        AlertDialog.Builder(context)
            .setTitle(ui.label("charging.editor.outlet_count"))
            .setSingleChoiceItems(options.map { it.toString() }.toTypedArray(), options.indexOf(outletCount).coerceAtLeast(0)) { dialog, index ->
                outletCount = options[index]
                outletCountWasEdited = true
                syncDefaultPowerDraft()
                updateDefaultSupplyControls()
                updateSaveEnabled()
                dialog.dismiss()
            }
            .setNegativeButton(R.string.charging_cancel, null)
            .show()
    }

    private fun showVenueMenu() {
        val popup = PopupMenu(context, venueButton)
        popup.menu.add(ui.label("charging.editor.venue.choose")).setOnMenuItemClickListener {
            venue = null
            updateSelectors()
            updateSaveEnabled()
            true
        }
        val options = listOf(
            MobileChargingVenueDto.RESTAURANT to "charging.venue.restaurant",
            MobileChargingVenueDto.CAFE to "charging.venue.cafe",
            MobileChargingVenueDto.BAR to "charging.venue.bar",
            MobileChargingVenueDto.OTHER to "charging.venue.other",
        )
        options.forEach { (value, label) ->
            popup.menu.add(ui.label(label)).setOnMenuItemClickListener {
                venue = value
                updateSelectors()
                updateSaveEnabled()
                true
            }
        }
        popup.show()
    }

    private fun showTermsMenu() {
        val popup = PopupMenu(context, termsButton)
        popup.menu.add(ui.label("charging.editor.terms.choose")).setOnMenuItemClickListener {
            terms = null
            updateSelectors()
            updateSaveEnabled()
            true
        }
        val options = listOf(
            MobileChargingTermsDto.DIRECT_FEE to "charging.terms.direct_fee",
            MobileChargingTermsDto.PURCHASE_REQUIRED to "charging.terms.purchase",
            MobileChargingTermsDto.BOTH to "charging.terms.both",
        )
        options.forEach { (value, label) ->
            popup.menu.add(ui.label(label)).setOnMenuItemClickListener {
                terms = value
                updateSelectors()
                true
            }
        }
        popup.show()
    }

    private fun showBasisMenu() {
        val popup = PopupMenu(context, basisButton)
        popup.menu.add(ui.label("charging.editor.price.basis.choose")).setOnMenuItemClickListener {
            priceBasis = null
            originalPrice = null
            updateSelectors()
            updateSaveEnabled()
            true
        }
        val options = listOf(
            MobileChargingPriceBasisDto.PER_CHARGE to "charging.price.per_charge",
            MobileChargingPriceBasisDto.PER_HOUR to "charging.price.per_hour",
            MobileChargingPriceBasisDto.PER_KWH to "charging.price.per_kwh",
            MobileChargingPriceBasisDto.PURCHASE to "charging.price.purchase",
        )
        options.forEach { (value, label) ->
            popup.menu.add(ui.label(label)).setOnMenuItemClickListener {
                priceBasis = value
                originalPrice = null
                updateSelectors()
                true
            }
        }
        popup.show()
    }

    private fun addSupply(value: MobileChargingSupplyDto?, draft: MobileChargingPowerDraft? = null) {
        if (supplyRows.size >= 8) {
            draft?.close()
            return
        }
        val compactInput = if (value == null && draft == null && supplyRows.isEmpty() && (plugWasEdited || outletCountWasEdited)) {
            MobileChargingSupplyDto(
                voltageVolts = null,
                connectors = if (plugWasEdited) listOfNotNull(selectedPlug) else emptyList(),
                outletCount = if (outletCountWasEdited) outletCount.toUShort() else null,
            )
        } else null
        val row = SupplyRow(context, value ?: compactInput, draft)
        supplyRows += row
        if (supplyRows.size == 1) resetDefaultSupplyState()
        supplies.addView(row.root, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
        updateCost()
        updateSaveEnabled()
    }

    private fun resetDefaultSupplyState() {
        selectedPlug = null
        plugWasEdited = false
        outletCount = 1
        outletCountWasEdited = false
        replaceDefaultPowerDraft(mobileChargingPowerDraft(countryCode, venue == MobileChargingVenueDto.EV_CHARGING_STATION))
    }

    private fun confirmSuggestedPower() {
        val snapshot = defaultPowerDraft.snapshot()
        val suggestion = snapshot.selection
        val summary = listOfNotNull(
            suggestion.voltageVolts?.let { "$it V" },
            suggestion.connectors.joinToString(", ").takeIf(String::isNotBlank),
            suggestion.outletCount?.let { "$it ${context.getString(R.string.charging_outlet_count)}" },
        ).joinToString(" · ")
        if (summary.isBlank()) return
        AlertDialog.Builder(context)
            .setTitle(R.string.charging_suggest_power)
            .setMessage(context.getString(R.string.charging_suggest_power_confirm, summary))
            .setNegativeButton(R.string.charging_cancel, null)
            .setPositiveButton(R.string.charging_suggest_use) { _, _ ->
                val confirmed = defaultPowerDraft.confirm()
                val included = confirmed.snapshot().includedSupply
                if (included != null) addSupply(included, confirmed) else confirmed.close()
            }
            .show()
    }

    private fun save() {
        if (saving || disposed) return
        if (selectedKind == null) {
            showIssue(ui.label("charging.editor.error.metadata"))
            return
        }
        if (accessNote.text.isNullOrBlank()) {
            showIssue(ui.label("charging.editor.error.access_note_blank"))
            return
        }
        val input = try { buildInput() } catch (error: Exception) {
            showIssue(error.message ?: context.getString(R.string.charging_validation)); return
        }
        when (val validation = mobileChargingValidateEditor(input)) {
            is MobileChargingValidationDto.Valid -> {
                setSaving(true)
                try {
                    onSave(validation.input)
                } catch (error: Exception) {
                    setSaving(false)
                    showIssue(error.message ?: context.getString(R.string.charging_validation))
                }
            }
            is MobileChargingValidationDto.Invalid -> showIssue(validationMessage(validation.issue))
        }
    }

    private fun buildInput(): MobileChargingInputDto {
        val lat = latitude.text.toString().toDoubleOrNull()
        val lon = longitude.text.toString().toDoubleOrNull()
        require(lat != null && lon != null && lat.isFinite() && lon.isFinite() && lat in -90.0..90.0 && lon in -180.0..180.0) {
            ui.label("charging.editor.error.coordinate")
        }
        return MobileChargingInputDto(
            MobileMapCoordinateDto(lat, lon), accessNote.text.toString(), cost,
            name.text.toString().trim().ifBlank { null }, venue,
            hours.text.toString().trim().ifBlank { null }, terms, priceDto(),
            suppliesForSave(),
        )
    }

    private fun suppliesForSave(): List<MobileChargingSupplyDto> {
        val values = supplyRows.mapNotNull(SupplyRow::value).toMutableList()
        if (supplyRows.isEmpty() && (plugWasEdited || outletCountWasEdited)) {
            val connectors = if (plugWasEdited) listOfNotNull(selectedPlug) else emptyList()
            val count = if (outletCountWasEdited) outletCount.toUShort() else null
            if (connectors.isNotEmpty() || count != null) values += MobileChargingSupplyDto(null, connectors, count)
        }
        return values
    }

    private fun priceDto(): MobileChargingPriceDto? {
        if (cost == MobileChargingCostDto.FREE) return null
        originalPrice?.let { return it }
        val amount = price.text.toString().trim()
        val code = currency.text.toString().trim().uppercase(Locale.ROOT)
        if (amount.isEmpty() && code.isEmpty() && priceBasis == null) return null
        require(amount.isNotEmpty() && priceBasis != null) { context.getString(R.string.charging_validation) }
        val exponent = Currency.getInstance(code).defaultFractionDigits
        require(exponent in 0..4) { context.getString(R.string.charging_validation) }
        val decimalSeparator = java.text.DecimalFormatSymbols.getInstance(Locale.getDefault()).decimalSeparator
        val normalizedAmount = if (decimalSeparator == '.') amount else amount.replace(decimalSeparator, '.')
        val minor = BigDecimal(normalizedAmount).movePointRight(exponent).toBigIntegerExact().toString().toULongOrNull()
            ?: error(context.getString(R.string.charging_validation))
        require(minor in 1UL..9_000_000_000_000_000UL) { context.getString(R.string.charging_validation) }
        return MobileChargingPriceDto(minor, code, exponent.toUByte(), checkNotNull(priceBasis))
    }

    private fun showIssue(value: String) { issue.text = value; issue.visibility = View.VISIBLE }

    private fun priceText(value: MobileChargingPriceDto): String =
        BigDecimal(value.amountMinor.toString()).movePointLeft(value.minorUnitExponent.toInt()).toPlainString().let { amount ->
            val separator = java.text.DecimalFormatSymbols.getInstance(Locale.getDefault()).decimalSeparator
            if (separator == '.') amount else amount.replace('.', separator)
        }

    private inner class SupplyRow(
        context: Context,
        value: MobileChargingSupplyDto?,
        providedDraft: MobileChargingPowerDraft? = null,
    ) {
        private var powerDraft = providedDraft ?: newPowerDraft(value, venue == MobileChargingVenueDto.EV_CHARGING_STATION)
        private var applyingSnapshot = false
        private val initialSelection = powerDraft.snapshot().selection
        val isSuggested: Boolean get() = powerDraft.snapshot().origin == MobileChargingPowerDraftOriginDto.SUGGESTED
        private val showsInitialSelection = providedDraft != null || value != null
        private val voltage = field(context.getString(R.string.charging_voltage_v), if (showsInitialSelection) initialSelection.voltageVolts?.toString().orEmpty() else "", true).apply {
            tag = "charging_editor.voltage"
        }
        private val outletCount = field(context.getString(R.string.charging_outlet_count), if (showsInitialSelection) initialSelection.outletCount?.toString().orEmpty() else "", true)
        private val selectedConnectors = (if (showsInitialSelection) initialSelection.connectors else emptyList()).toMutableList()
        val root = ui.column()
        private val connectorButton = ui.action(context.getString(R.string.charging_choose_connector)) { chooseConnector() }.apply {
            tag = "charging_editor.connector"
        }
        init {
            voltage.addTextChangedListener(textWatcher { if (!applyingSnapshot) syncPowerDraft(); updateSaveEnabled() })
            outletCount.addTextChangedListener(textWatcher { if (!applyingSnapshot) syncPowerDraft(); updateSaveEnabled() })
            root.background = ui.rounded(12)
            root.setPadding(ui.dp(8), ui.dp(8), ui.dp(8), ui.dp(8))
            root.addView(voltage)
            root.addView(connectorButton, spaced())
            root.addView(outletCount, spaced())
            root.addView(ui.action(context.getString(R.string.charging_delete)) {
                supplies.removeView(root)
                supplyRows.remove(this)
                if (supplyRows.isEmpty()) {
                    resetDefaultSupplyState()
                }
                updateCost()
                updateSaveEnabled()
            }, spaced())
            setConnectorLabel()
        }
        fun value(): MobileChargingSupplyDto? {
            val voltageText = voltage.text.toString().trim()
            val outletText = outletCount.text.toString().trim()
            val volts = voltageText.takeIf(String::isNotEmpty)?.toUShortOrNull()
            val outlets = outletText.takeIf(String::isNotEmpty)?.toUShortOrNull()
            require(voltageText.isEmpty() || volts != null) { ui.label("charging.editor.error.voltage") }
            require(outletText.isEmpty() || outlets != null) { context.getString(R.string.charging_validation) }
            if (isEmpty()) return null
            return powerDraft.snapshot().includedSupply
        }
        fun hasInvalidInput(): Boolean = !isEmpty() && runCatching { value() == null }.getOrElse { true }
        fun dispose() = powerDraft.close()
        fun updateCountry(code: String?) = replacePowerDraft(powerDraft.withCountry(code))
        fun updateKind(evStation: Boolean) = replacePowerDraft(powerDraft.withKind(evStation))

        private fun isEmpty(): Boolean = voltage.text.isNullOrBlank() && outletCount.text.isNullOrBlank() && selectedConnectors.isEmpty()

        private fun syncPowerDraft() {
            val voltageText = voltage.text.toString().trim()
            val outletText = outletCount.text.toString().trim()
            val volts = voltageText.takeIf(String::isNotEmpty)?.toUShortOrNull()
            val outlets = outletText.takeIf(String::isNotEmpty)?.toUShortOrNull()
            val candidate = if ((voltageText.isEmpty() || volts != null) && (outletText.isEmpty() || outlets != null)) {
                MobileChargingSupplyDto(volts, selectedConnectors.toList(), outlets).takeUnless {
                    voltageText.isEmpty() && outletText.isEmpty() && selectedConnectors.isEmpty()
                }
            } else null
            replacePowerDraft(powerDraft.edit(candidate))
        }

        private fun replacePowerDraft(next: MobileChargingPowerDraft) {
            val old = powerDraft
            powerDraft = next
            old.close()
            if (powerDraft.snapshot().origin == MobileChargingPowerDraftOriginDto.SUGGESTED) applySuggestedSelection()
            setConnectorLabel()
        }

        private fun applySuggestedSelection() {
            val selection = powerDraft.snapshot().selection
            applyingSnapshot = true
            voltage.setText(selection.voltageVolts?.toString().orEmpty())
            outletCount.setText(selection.outletCount?.toString().orEmpty())
            selectedConnectors.clear()
            selectedConnectors.addAll(selection.connectors)
            applyingSnapshot = false
        }

        private fun chooseConnector() {
            val options = connectorOptions() + selectedConnectors.filterIsInstance<MobileChargingConnectorDto.Other>().filterNot { it in connectorOptions() }
            val labels = options.map(::connectorText) + root.context.getString(R.string.charging_other_connector)
            val checked = BooleanArray(labels.size) { index -> options.getOrNull(index)?.let(selectedConnectors::contains) ?: false }
            AlertDialog.Builder(root.context)
                .setTitle(R.string.charging_choose_connector)
                .setMultiChoiceItems(labels.toTypedArray(), checked) { _, which, isChecked ->
                    if (which < options.size) {
                        val item = options[which]
                        if (isChecked && item !in selectedConnectors) selectedConnectors += item
                        if (!isChecked) selectedConnectors.remove(item)
                        syncPowerDraft()
                        setConnectorLabel()
                        updateSaveEnabled()
                    }
                }
                .setPositiveButton(R.string.charging_done, null)
                .setNeutralButton(R.string.charging_other_connector) { _, _ -> askOtherConnector() }
                .show()
        }
        private fun askOtherConnector() {
            val input = EditText(root.context).apply { hint = root.context.getString(R.string.charging_other_connector_name) }
            AlertDialog.Builder(root.context).setTitle(R.string.charging_other_connector).setView(input)
                .setNegativeButton(R.string.charging_cancel, null)
                .setPositiveButton(R.string.charging_done) { _, _ ->
                    val label = input.text.toString().trim()
                    if (label.isNotEmpty()) {
                        MobileChargingConnectorDto.Other(label).also { if (it !in selectedConnectors) selectedConnectors += it }
                        syncPowerDraft()
                        setConnectorLabel()
                        updateSaveEnabled()
                    }
                }.show()
        }
        private fun setConnectorLabel() {
            val selected = selectedConnectors.takeIf { it.isNotEmpty() }
            val draft = powerDraft.snapshot()
            val suggested = draft.selection.connectors.firstOrNull()
            connectorButton.text = selected?.joinToString(", ", transform = ::connectorText)
                ?: suggested?.takeIf { draft.origin == MobileChargingPowerDraftOriginDto.SUGGESTED }
                    ?.let { root.context.getString(R.string.charging_connector_suggested, connectorText(it)) }
                ?: root.context.getString(R.string.charging_choose_connector)
            val connector = selectedConnectors.firstOrNull { it != MobileChargingConnectorDto.Unknown } ?: suggested
            val icon = connector?.let { item ->
                root.context.getDrawable(ChargingConnectorArtwork.drawable(item))?.mutate()?.apply {
                    setTint(ui.brand)
                    setBounds(0, 0, ui.dp(28), ui.dp(28))
                }
            }
            connectorButton.setCompoundDrawables(icon, null, null, null)
            connectorButton.compoundDrawablePadding = if (icon == null) 0 else ui.dp(10)
        }
        fun refreshConnectorChoices() = setConnectorLabel()
        private fun connectorOptions(): List<MobileChargingConnectorDto> {
            return this@ChargingSpotEditorPanel.connectorChoices()
        }
    }

    private fun connectorText(value: MobileChargingConnectorDto): String = ChargingConnectorArtwork.label(ui, value)

    private companion object {
        const val ACCESSIBILITY_FONT_SCALE = 1.5f
    }

}

/** Small, decorative equivalents of the three Swift kind-choice symbols. */
private class ChargingEditorKindIcon(context: Context, private val kind: ChargingSpotEditorPanel.Kind) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.rgb(255, 204, 0)
        style = Paint.Style.STROKE
        strokeWidth = context.resources.displayMetrics.density * 2f
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val scale = minOf(width, height) / 24f
        val save = canvas.save()
        canvas.translate((width - 24f * scale) / 2f, (height - 24f * scale) / 2f)
        canvas.scale(scale, scale)
        when (kind) {
            ChargingSpotEditorPanel.Kind.CHARGING -> {
                val bolt = Path().apply {
                    moveTo(14f, 1.5f); lineTo(5.5f, 13f); lineTo(11f, 13f)
                    lineTo(9.5f, 22.5f); lineTo(18.5f, 9.5f); lineTo(13f, 9.5f); close()
                }
                canvas.drawPath(bolt, paint.apply { style = Paint.Style.FILL })
            }
            ChargingSpotEditorPanel.Kind.FOOD -> {
                canvas.drawLine(5f, 3f, 5f, 21f, paint)
                canvas.drawLine(2f, 3f, 2f, 8f, paint)
                canvas.drawLine(8f, 3f, 8f, 8f, paint)
                canvas.drawLine(2f, 8f, 8f, 8f, paint)
                canvas.drawLine(19f, 3f, 19f, 21f, paint)
                canvas.drawArc(14f, 3f, 22f, 13f, -90f, 180f, false, paint)
            }
            ChargingSpotEditorPanel.Kind.EV -> {
                canvas.drawRoundRect(2f, 9f, 22f, 18f, 2f, 2f, paint)
                canvas.drawLine(5f, 9f, 8f, 5f, paint)
                canvas.drawLine(8f, 5f, 17f, 5f, paint)
                canvas.drawLine(17f, 5f, 20f, 9f, paint)
                canvas.drawCircle(7f, 18f, 1.8f, paint)
                canvas.drawCircle(17f, 18f, 1.8f, paint)
                val bolt = Path().apply {
                    moveTo(13.5f, 7f); lineTo(10.5f, 12.5f); lineTo(13f, 12.5f)
                    lineTo(11.5f, 17f); lineTo(16f, 11f); lineTo(13.5f, 11f); close()
                }
                canvas.drawPath(bolt, paint.apply { style = Paint.Style.FILL })
            }
        }
        paint.style = Paint.Style.STROKE
        canvas.restoreToCount(save)
    }
}
