package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.ScrollView
import uniffi.cutout_mobile_ffi.*
import java.math.BigDecimal
import java.util.Currency
import java.util.Locale

/** Native edit form. Rust owns supply inclusion, connector order, and price construction. */
class ChargingSpotEditorPanel(
    context: Context,
    private val spot: MobileChargingSpotDto?,
    initialCoordinate: MobileMapCoordinateDto,
    private val onCancel: () -> Unit,
    private val onSave: (MobileChargingInputDto) -> Unit,
    initialInput: MobileChargingInputDto? = spot?.input,
) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val body = ui.column()
    private val latitude = field(context.getString(R.string.charging_latitude), initialCoordinate.latitudeDegrees.toString(), true)
    private val longitude = field(context.getString(R.string.charging_longitude), initialCoordinate.longitudeDegrees.toString(), true)
    private val accessNote = field(context.getString(R.string.charging_access_note), initialInput?.accessNote.orEmpty(), false, true)
    private val name = field(context.getString(R.string.charging_name), initialInput?.name.orEmpty())
    private val hours = field(context.getString(R.string.charging_hours), initialInput?.hours.orEmpty())
    private val price = field(context.getString(R.string.charging_price), initialInput?.price?.let(::priceText).orEmpty(), true)
    private val currency = field(context.getString(R.string.charging_currency), initialInput?.price?.currency ?: "USD")
    private val issue = ui.text("", 14f, false, ui.red)
    private val venueButton = ui.action(context.getString(R.string.charging_venue_none)) { showVenueMenu() }
    private val termsButton = ui.action(context.getString(R.string.charging_terms)) { showTermsMenu() }
    private val basisButton = ui.action(context.getString(R.string.charging_price_basis)) { showBasisMenu() }
    private val supplies = ui.column()
    private val supplyRows = mutableListOf<SupplyRow>()
    private var cost = initialInput?.cost ?: MobileChargingCostDto.FREE
    private var venue = initialInput?.venue
    private var terms = initialInput?.terms
    private var priceBasis = initialInput?.price?.basis
    private var countryCode: String? = null
    private var orderedConnectorChoices: List<MobileChargingConnectorDto>? = null
    private val costButton = ui.action("", false) { toggleCost() }

    init {
        clipToPadding = false
        body.setPadding(ui.dp(18), ui.dp(12), ui.dp(18), ui.dp(20))
        addView(body, LayoutParams(-1, -2))
        body.addView(ui.text(context.getString(if (spot == null) R.string.charging_add else R.string.charging_edit), 24f, true))
        body.addView(latitude, spaced())
        body.addView(longitude, spaced())
        body.addView(accessNote, spaced())
        body.addView(name, spaced())
        body.addView(hours, spaced())
        body.addView(costButton, spaced())
        body.addView(venueButton, spaced())
        body.addView(ui.text(context.getString(R.string.charging_supply), 18f, true), spaced())
        body.addView(supplies, spaced())
        body.addView(ui.action(context.getString(R.string.charging_add_supply)) { addManualSupply() }, spaced())
        body.addView(ui.action(context.getString(R.string.charging_suggest_power)) { confirmSuggestedPower() }, spaced())
        body.addView(termsButton, spaced())
        body.addView(price, spaced())
        body.addView(currency, spaced())
        body.addView(basisButton, spaced())
        issue.visibility = View.GONE
        body.addView(issue, spaced())
        val actions = ui.row()
        actions.addView(ui.action(context.getString(R.string.charging_cancel)) { onCancel() },
            LinearLayout.LayoutParams(0, ui.dp(48), 1f))
        actions.addView(ui.action(context.getString(R.string.charging_save), true) { save() },
            LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(8) })
        body.addView(actions, spaced())
        initialInput?.supplies.orEmpty().forEach(::addRestoredSupply)
        if (initialInput?.supplies.isNullOrEmpty()) {
            price.visibility = if (cost == MobileChargingCostDto.PAID) View.VISIBLE else View.GONE
        }
        updateCost()
        updateSelectors()
    }

    fun close() {
        supplyRows.toList().forEach { it.close() }
    }

    /** GPS country. Manual rows survive; a replaced suggestion needs confirmation again. */
    fun setCountryCode(value: String?) {
        val normalized = normalizeCountry(value)
        if (countryCode == normalized) return
        countryCode = normalized
        supplyRows.forEach { it.setCountry(normalized) }
    }

    /** Rust's already ordered connector projection. Null keeps the country catalogue. */
    fun setConnectorChoices(choices: List<MobileChargingConnectorDto>?) {
        orderedConnectorChoices = choices
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

    private fun toggleCost() {
        cost = if (cost == MobileChargingCostDto.FREE) MobileChargingCostDto.PAID else MobileChargingCostDto.FREE
        if (cost == MobileChargingCostDto.FREE) {
            terms = null
            priceBasis = null
            price.setText("")
            currency.setText("USD")
        }
        updateCost()
        updateSelectors()
    }

    private fun updateCost() {
        costButton.text = context.getString(if (cost == MobileChargingCostDto.FREE) R.string.charging_cost_free else R.string.charging_cost_paid)
        price.visibility = if (cost == MobileChargingCostDto.PAID) View.VISIBLE else View.GONE
        currency.visibility = price.visibility
        termsButton.visibility = price.visibility
        basisButton.visibility = price.visibility
    }

    private fun updateSelectors() {
        venueButton.text = context.getString(venueString(venue))
        termsButton.text = context.getString(terms?.let(::termsString) ?: R.string.charging_terms)
        basisButton.text = context.getString(priceBasis?.let(::basisString) ?: R.string.charging_price_basis)
    }

    private fun showVenueMenu() {
        val popup = PopupMenu(context, venueButton)
        popup.menu.add(context.getString(R.string.charging_venue_none)).setOnMenuItemClickListener {
            selectVenue(null)
            true
        }
        val options: List<Pair<MobileChargingVenueDto, Int>> = listOf(
            MobileChargingVenueDto.RESTAURANT to R.string.charging_venue_restaurant,
            MobileChargingVenueDto.CAFE to R.string.charging_venue_cafe,
            MobileChargingVenueDto.BAR to R.string.charging_venue_bar,
            MobileChargingVenueDto.EV_CHARGING_STATION to R.string.charging_venue_ev,
            MobileChargingVenueDto.OTHER to R.string.charging_venue_other,
        )
        options.forEach { (value, label) ->
            popup.menu.add(context.getString(label)).setOnMenuItemClickListener {
                selectVenue(value)
                true
            }
        }
        popup.show()
    }

    private fun selectVenue(value: MobileChargingVenueDto?) {
        if (venue == value) return
        venue = value
        val ev = value == MobileChargingVenueDto.EV_CHARGING_STATION
        supplyRows.toList().forEach { it.setEv(ev) }
        if (ev) {
            val dropped = supplyRows.filter { it.isSuggested() }
            dropped.forEach { row ->
                supplies.removeView(row.root)
                supplyRows.remove(row)
                row.close()
            }
        }
        updateSelectors()
    }

    private fun showTermsMenu() {
        val popup = PopupMenu(context, termsButton)
        val options: List<Pair<MobileChargingTermsDto, Int>> = listOf(
            MobileChargingTermsDto.DIRECT_FEE to R.string.charging_terms_fee,
            MobileChargingTermsDto.PURCHASE_REQUIRED to R.string.charging_terms_purchase,
            MobileChargingTermsDto.BOTH to R.string.charging_terms_both,
        )
        options.forEach { (value, label) ->
            popup.menu.add(context.getString(label)).setOnMenuItemClickListener {
                terms = value
                updateSelectors()
                true
            }
        }
        popup.show()
    }

    private fun showBasisMenu() {
        val popup = PopupMenu(context, basisButton)
        val options: List<Pair<MobileChargingPriceBasisDto, Int>> = listOf(
            MobileChargingPriceBasisDto.PER_CHARGE to R.string.charging_basis_charge,
            MobileChargingPriceBasisDto.PER_HOUR to R.string.charging_basis_hour,
            MobileChargingPriceBasisDto.PER_KWH to R.string.charging_basis_kwh,
            MobileChargingPriceBasisDto.PURCHASE to R.string.charging_basis_purchase,
        )
        options.forEach { (value, label) ->
            popup.menu.add(context.getString(label)).setOnMenuItemClickListener {
                priceBasis = value
                updateSelectors()
                true
            }
        }
        popup.show()
    }

    private fun addManualSupply() {
        if (supplyRows.size >= 8) return
        val base = mobileChargingPowerDraft(countryCode, venue == MobileChargingVenueDto.EV_CHARGING_STATION)
        val draft = base.edit(null)
        base.close()
        addPowerRow(draft, null)
    }

    private fun addRestoredSupply(supply: MobileChargingSupplyDto) {
        val base = mobileChargingPowerDraft(countryCode, venue == MobileChargingVenueDto.EV_CHARGING_STATION)
        val draft = base.edit(supply)
        base.close()
        val snapshot = draft.snapshot()
        addPowerRow(draft, if (snapshot.includedSupply == null) supply else snapshot.selection)
    }

    private fun addPowerRow(draft: MobileChargingPowerDraft, display: MobileChargingSupplyDto?) {
        if (supplyRows.size >= 8) {
            draft.close()
            return
        }
        val row = SupplyRow(draft, display)
        supplyRows += row
        supplies.addView(row.root, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
    }

    private fun confirmSuggestedPower() {
        if (venue == MobileChargingVenueDto.EV_CHARGING_STATION || supplyRows.size >= 8) return
        val base = mobileChargingPowerDraft(countryCode, false)
        val summary = suggestionSummary(base.snapshot().selection)
        if (summary.isBlank()) {
            base.close()
            return
        }
        var consumed = false
        val dialog = AlertDialog.Builder(context)
            .setTitle(R.string.charging_suggest_power)
            .setMessage(context.getString(R.string.charging_suggest_power_confirm, summary))
            .setNegativeButton(R.string.charging_cancel) { _, _ ->
                if (!consumed) {
                    consumed = true
                    base.close()
                }
            }
            .setPositiveButton(R.string.charging_suggest_use) { _, _ ->
                if (consumed) return@setPositiveButton
                consumed = true
                val confirmed = base.confirm()
                base.close()
                addPowerRow(confirmed, confirmed.snapshot().selection)
            }
            .create()
        dialog.setOnCancelListener {
            if (!consumed) {
                consumed = true
                base.close()
            }
        }
        dialog.show()
    }

    private fun save() {
        val lat = latitude.text.toString().toDoubleOrNull()
        val lon = longitude.text.toString().toDoubleOrNull()
        if (lat == null || lon == null || !lat.isFinite() || !lon.isFinite() || lat !in -90.0..90.0 || lon !in -180.0..180.0) {
            showIssue("${context.getString(R.string.charging_latitude)} / ${context.getString(R.string.charging_longitude)}")
            return
        }
        val priceDto = try {
            priceDto()
        } catch (error: Exception) {
            showIssue(error.message ?: context.getString(R.string.charging_validation))
            return
        }
        val included = mutableListOf<MobileChargingSupplyDto>()
        for (row in supplyRows) {
            when (val piece = row.prepareSave()) {
                SupplySave.Skip -> Unit
                is SupplySave.Include -> included += piece.supply
                is SupplySave.Reject -> {
                    showIssue(piece.message)
                    return
                }
            }
        }
        val input = MobileChargingInputDto(
            MobileMapCoordinateDto(lat, lon), accessNote.text.toString(), cost,
            name.text.toString().trim().ifBlank { null }, venue,
            hours.text.toString().trim().ifBlank { null }, terms, priceDto,
            included,
        )
        when (val validation = mobileChargingValidateEditor(input)) {
            is MobileChargingValidationDto.Valid -> onSave(validation.input)
            is MobileChargingValidationDto.Invalid -> showIssue("${validation.issue.field}: ${validation.issue.reason}")
        }
    }

    private fun priceDto(): MobileChargingPriceDto? {
        if (cost == MobileChargingCostDto.FREE) return null
        val amount = price.text.toString().trim()
        val code = currency.text.toString().trim().uppercase(Locale.ROOT)
        if (amount.isEmpty() && (code.isEmpty() || code == "USD") && terms == null && priceBasis == null) return null
        val validation = context.getString(R.string.charging_validation)
        if (amount.isEmpty() || terms == null || priceBasis == null) error(validation)
        if (code.length != 3 || code.any { it !in 'A'..'Z' }) error(validation)
        val exponent = try {
            Currency.getInstance(code).defaultFractionDigits
        } catch (_: IllegalArgumentException) {
            Currency.getInstance(Locale.US).defaultFractionDigits
        }
        if (exponent !in 0..4) error(validation)
        val minor = try {
            val shifted = BigDecimal(amount).movePointRight(exponent).stripTrailingZeros()
            if (shifted.scale() > 0) error(validation)
            shifted.toBigIntegerExact().toString().toULongOrNull() ?: error(validation)
        } catch (_: NumberFormatException) {
            error(validation)
        } catch (_: ArithmeticException) {
            error(validation)
        }
        if (minor == 0UL || minor > 9_000_000_000_000_000UL) error(validation)
        return MobileChargingPriceDto(minor, code, exponent.toUByte(), checkNotNull(priceBasis))
    }

    private fun showIssue(value: String) { issue.text = value; issue.visibility = View.VISIBLE }

    private fun priceText(value: MobileChargingPriceDto): String =
        BigDecimal(value.amountMinor.toString()).movePointLeft(value.minorUnitExponent.toInt()).toPlainString()

    private fun suggestionSummary(selection: MobileChargingSupplyDto): String = listOfNotNull(
        selection.voltageVolts?.let { "$it V" },
        selection.connectors.joinToString(", ", transform = ::connectorText).takeIf(String::isNotBlank),
        selection.outletCount?.let { "$it ${context.getString(R.string.charging_outlet_count)}" },
    ).joinToString(" · ")

    private fun connectorChoices(): List<MobileChargingConnectorDto> =
        orderedConnectorChoices ?: mobileChargingConnectorChoices(countryCode)

    private fun normalizeCountry(value: String?): String? {
        val code = value?.trim() ?: return null
        if (code.length != 2 || code.any { it !in 'A'..'Z' && it !in 'a'..'z' }) return null
        return code.uppercase(Locale.ROOT)
    }

    private fun venueString(value: MobileChargingVenueDto?) = when (value) {
        null -> R.string.charging_venue_none
        MobileChargingVenueDto.RESTAURANT -> R.string.charging_venue_restaurant
        MobileChargingVenueDto.CAFE -> R.string.charging_venue_cafe
        MobileChargingVenueDto.BAR -> R.string.charging_venue_bar
        MobileChargingVenueDto.EV_CHARGING_STATION -> R.string.charging_venue_ev
        MobileChargingVenueDto.OTHER -> R.string.charging_venue_other
    }
    private fun termsString(value: MobileChargingTermsDto) = when (value) {
        MobileChargingTermsDto.DIRECT_FEE -> R.string.charging_terms_fee
        MobileChargingTermsDto.PURCHASE_REQUIRED -> R.string.charging_terms_purchase
        MobileChargingTermsDto.BOTH -> R.string.charging_terms_both
    }
    private fun basisString(value: MobileChargingPriceBasisDto) = when (value) {
        MobileChargingPriceBasisDto.PER_CHARGE -> R.string.charging_basis_charge
        MobileChargingPriceBasisDto.PER_HOUR -> R.string.charging_basis_hour
        MobileChargingPriceBasisDto.PER_KWH -> R.string.charging_basis_kwh
        MobileChargingPriceBasisDto.PURCHASE -> R.string.charging_basis_purchase
    }

    private sealed interface SupplySave {
        data object Skip : SupplySave
        data class Include(val supply: MobileChargingSupplyDto) : SupplySave
        data class Reject(val message: String) : SupplySave
    }

    private inner class SupplyRow(
        initialDraft: MobileChargingPowerDraft,
        display: MobileChargingSupplyDto?,
    ) {
        private var draft = initialDraft
        private var open = true
        private var applying = false
        private val voltage = field(context.getString(R.string.charging_voltage_v), "", true)
        private val outletCount = field(context.getString(R.string.charging_outlet_count), "", true)
        private val selectedConnectors = mutableListOf<MobileChargingConnectorDto>()
        val root = ui.column()
        private val connectorButton = ui.action(context.getString(R.string.charging_choose_connector)) { chooseConnector() }
        private val confirmButton = ui.action(context.getString(R.string.charging_suggest_use)) { confirmInclusion() }
        private val watcher = object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) = Unit
            override fun afterTextChanged(s: Editable?) {
                if (!applying) syncManualEdit()
            }
        }

        init {
            root.background = ui.rounded(12)
            root.setPadding(ui.dp(8), ui.dp(8), ui.dp(8), ui.dp(8))
            root.addView(voltage)
            root.addView(connectorButton, spaced())
            root.addView(outletCount, spaced())
            root.addView(confirmButton, spaced())
            root.addView(ui.action(context.getString(R.string.charging_delete)) { remove() }, spaced())
            if (display != null) loadSelection(display)
            voltage.addTextChangedListener(watcher)
            outletCount.addTextChangedListener(watcher)
            refreshConfirm()
        }

        fun close() {
            if (!open) return
            open = false
            draft.close()
        }

        fun isSuggested(): Boolean = draft.snapshot().origin == MobileChargingPowerDraftOriginDto.SUGGESTED

        fun setCountry(code: String?) {
            val next = draft.withCountry(code)
            replaceDraft(next)
            if (!hasIncompleteEdit() && !isEmpty()) loadSelection(draft.snapshot().selection)
            refreshConfirm()
        }

        fun setEv(ev: Boolean) {
            val next = draft.withKind(ev)
            replaceDraft(next)
            if (!hasIncompleteEdit() && !isEmpty()) loadSelection(draft.snapshot().selection)
            refreshConfirm()
        }

        fun prepareSave(): SupplySave {
            if (isEmpty()) return SupplySave.Skip
            val validation = context.getString(R.string.charging_validation)
            if (requiresConfirmation()) return SupplySave.Reject(validation)
            val included = draft.snapshot().includedSupply
            if (included != null && !hasInvalidVoltage() && !hasIncompleteEdit()) return SupplySave.Include(included)
            val probe = rawProbe()
            if (probe != null) {
                val inspected = mobileChargingValidateEditor(probeInput(probe))
                if (inspected is MobileChargingValidationDto.Invalid) {
                    return SupplySave.Reject("${inspected.issue.field}: ${inspected.issue.reason}")
                }
            }
            return SupplySave.Reject(validation)
        }

        private fun confirmInclusion() {
            if (isEmpty() || hasInvalidVoltage() || hasIncompleteEdit()) return
            replaceDraft(draft.confirm())
            refreshConfirm()
        }

        private fun remove() {
            supplies.removeView(root)
            supplyRows.remove(this)
            close()
        }

        private fun syncManualEdit() {
            if (!open || applying) return
            replaceDraft(draft.edit(currentSupply()))
            refreshConfirm()
        }

        private fun replaceDraft(next: MobileChargingPowerDraft) {
            val previous = draft
            draft = next
            if (open && previous !== next) previous.close()
        }

        private fun loadSelection(supply: MobileChargingSupplyDto) {
            applying = true
            voltage.setText(supply.voltageVolts?.toString().orEmpty())
            outletCount.setText(supply.outletCount?.toString().orEmpty())
            selectedConnectors.clear()
            selectedConnectors += supply.connectors
            setConnectorLabel()
            applying = false
        }

        private fun currentSupply(): MobileChargingSupplyDto? {
            val voltageText = voltage.text.toString().trim()
            val volts = if (voltageText.isEmpty()) null else voltageText.toUShortOrNull() ?: return null
            if (volts != null && !mobileChargingPowerVoltageIsValid(volts)) return null
            val outletText = outletCount.text.toString().trim()
            val outlets = if (outletText.isEmpty()) null else outletText.toUShortOrNull() ?: return null
            if (outlets == 0.toUShort()) return null
            val connectors = submittedConnectors()
            if (volts == null && connectors.isEmpty() && outlets == null) return null
            return MobileChargingSupplyDto(volts, connectors, outlets)
        }

        private fun rawProbe(): MobileChargingSupplyDto? {
            val voltageText = voltage.text.toString().trim()
            val volts = if (voltageText.isEmpty()) null else voltageText.toUShortOrNull() ?: return null
            val outletText = outletCount.text.toString().trim()
            val outlets = if (outletText.isEmpty()) null else outletText.toUShortOrNull() ?: return null
            return MobileChargingSupplyDto(volts, submittedConnectors(), outlets)
        }

        private fun probeInput(supply: MobileChargingSupplyDto) = MobileChargingInputDto(
            MobileMapCoordinateDto(0.0, 0.0),
            "Outlet",
            MobileChargingCostDto.FREE,
            null, null, null, null, null,
            listOf(supply),
        )

        private fun submittedConnectors(): List<MobileChargingConnectorDto> =
            if (MobileChargingConnectorDto.Unknown in selectedConnectors) listOf(MobileChargingConnectorDto.Unknown)
            else selectedConnectors.toList()

        private fun isEmpty(): Boolean =
            voltage.text.isNullOrBlank() && outletCount.text.isNullOrBlank() && selectedConnectors.isEmpty()

        private fun hasInvalidVoltage(): Boolean {
            val text = voltage.text.toString().trim()
            if (text.isEmpty()) return false
            val volts = text.toUShortOrNull() ?: return true
            return !mobileChargingPowerVoltageIsValid(volts)
        }

        private fun hasIncompleteEdit(): Boolean =
            !isEmpty() && draft.snapshot().origin == MobileChargingPowerDraftOriginDto.MANUAL
                && draft.snapshot().includedSupply == null

        private fun requiresConfirmation(): Boolean =
            isSuggested() && draft.snapshot().includedSupply == null && !hasIncompleteEdit()
                && !hasInvalidVoltage() && !isEmpty()

        private fun refreshConfirm() {
            confirmButton.visibility = if (requiresConfirmation()) View.VISIBLE else View.GONE
        }

        private fun chooseConnector() {
            val catalogue = connectorChoices()
            val extras = selectedConnectors.filterIsInstance<MobileChargingConnectorDto.Other>()
                .filter { other -> catalogue.none { it == other } }
            val options = catalogue + extras
            val labels = options.map(::connectorText) + root.context.getString(R.string.charging_other_connector)
            val checked = BooleanArray(labels.size) { index ->
                options.getOrNull(index)?.let(selectedConnectors::contains) ?: false
            }
            AlertDialog.Builder(root.context)
                .setTitle(R.string.charging_choose_connector)
                .setMultiChoiceItems(labels.toTypedArray(), checked) { _, which, isChecked ->
                    if (which < options.size) {
                        val item = options[which]
                        if (isChecked && item !in selectedConnectors) selectedConnectors += item
                        if (!isChecked) selectedConnectors.remove(item)
                        setConnectorLabel()
                        syncManualEdit()
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
                        selectedConnectors.remove(MobileChargingConnectorDto.Unknown)
                        val other = MobileChargingConnectorDto.Other(label)
                        if (other !in selectedConnectors) selectedConnectors += other
                        setConnectorLabel()
                        syncManualEdit()
                    }
                }.show()
        }

        private fun setConnectorLabel() {
            val shown = submittedConnectors()
            connectorButton.text = shown.takeIf { it.isNotEmpty() }?.joinToString(", ", transform = ::connectorText)
                ?: root.context.getString(R.string.charging_choose_connector)
            val connector = shown.firstOrNull { it != MobileChargingConnectorDto.Unknown }
            val icon = connector?.let { item ->
                root.context.getDrawable(ChargingConnectorArtwork.drawable(item))?.mutate()?.apply {
                    setTint(ui.brand)
                    setBounds(0, 0, ui.dp(28), ui.dp(28))
                }
            }
            connectorButton.setCompoundDrawables(icon, null, null, null)
            connectorButton.compoundDrawablePadding = if (icon == null) 0 else ui.dp(10)
        }
    }

    private fun connectorText(value: MobileChargingConnectorDto): String = ChargingConnectorArtwork.label(ui, value)
}
