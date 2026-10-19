package lol.cutout

import android.content.Context
import android.text.InputType
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.ScrollView
import android.widget.TextView
import android.app.AlertDialog
import uniffi.cutout_mobile_ffi.*
import java.math.BigDecimal
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
        body.addView(ui.action(context.getString(R.string.charging_add_supply)) { addSupply(null) }, spaced())
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
        initialInput?.supplies.orEmpty().forEach(::addSupply)
        if (initialInput?.supplies.isNullOrEmpty()) {
            price.visibility = if (cost == MobileChargingCostDto.PAID) View.VISIBLE else View.GONE
        }
        updateCost()
        updateSelectors()
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
            venue = null
            updateSelectors()
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
                venue = value
                updateSelectors()
                true
            }
        }
        popup.show()
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

    private fun addSupply(value: MobileChargingSupplyDto?) {
        if (supplyRows.size >= 8) return
        val row = SupplyRow(context, value)
        supplyRows += row
        supplies.addView(row.root, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
    }

    private fun confirmSuggestedPower() {
        val suggestion = mobileChargingPowerSuggestion(countryCode = null, evStation = venue == MobileChargingVenueDto.EV_CHARGING_STATION)
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
            .setPositiveButton(R.string.charging_suggest_use) { _, _ -> addSupply(suggestion) }
            .show()
    }

    private fun save() {
        val lat = latitude.text.toString().toDoubleOrNull()
        val lon = longitude.text.toString().toDoubleOrNull()
        if (lat == null || lon == null || !lat.isFinite() || !lon.isFinite() || lat !in -90.0..90.0 || lon !in -180.0..180.0) {
            showIssue("${context.getString(R.string.charging_latitude)} / ${context.getString(R.string.charging_longitude)}")
            return
        }
        val priceDto = try { priceDto() } catch (error: Exception) {
            showIssue(error.message ?: context.getString(R.string.charging_validation)); return
        }
        val input = MobileChargingInputDto(
            MobileMapCoordinateDto(lat, lon), accessNote.text.toString(), cost,
            name.text.toString().trim().ifBlank { null }, venue,
            hours.text.toString().trim().ifBlank { null }, terms, priceDto,
            supplyRows.mapNotNull(SupplyRow::value),
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
        if (amount.isEmpty() && code == "USD" && terms == null && priceBasis == null) return null
        require(amount.isNotEmpty() && terms != null && priceBasis != null) { context.getString(R.string.charging_validation) }
        val exponent = Currency.getInstance(code).defaultFractionDigits
        require(exponent in 0..9) { context.getString(R.string.charging_validation) }
        val minor = BigDecimal(amount).movePointRight(exponent).toBigIntegerExact().toString().toULongOrNull()
            ?: error(context.getString(R.string.charging_validation))
        return MobileChargingPriceDto(minor, code, exponent.toUByte(), checkNotNull(priceBasis))
    }

    private fun showIssue(value: String) { issue.text = value; issue.visibility = View.VISIBLE }

    private fun priceText(value: MobileChargingPriceDto): String =
        BigDecimal(value.amountMinor.toString()).movePointLeft(value.minorUnitExponent.toInt()).toPlainString()

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

    private inner class SupplyRow(context: Context, value: MobileChargingSupplyDto?) {
        private val voltage = field(context.getString(R.string.charging_voltage_v), value?.voltageVolts?.toString().orEmpty(), true)
        private val outletCount = field(context.getString(R.string.charging_outlet_count), value?.outletCount?.toString().orEmpty(), true)
        private val selectedConnectors = value?.connectors?.toMutableList() ?: mutableListOf()
        val root = ui.column()
        private val connectorButton = ui.action(context.getString(R.string.charging_choose_connector)) { chooseConnector() }
        init {
            root.background = ui.rounded(12)
            root.setPadding(ui.dp(8), ui.dp(8), ui.dp(8), ui.dp(8))
            root.addView(voltage)
            root.addView(connectorButton, spaced())
            root.addView(outletCount, spaced())
            root.addView(ui.action(context.getString(R.string.charging_delete)) { supplies.removeView(root); supplyRows.remove(this) }, spaced())
            setConnectorLabel()
        }
        fun value(): MobileChargingSupplyDto? {
            val volts = voltage.text.toString().trim().takeIf(String::isNotEmpty)?.toUShortOrNull()
            val outlets = outletCount.text.toString().trim().takeIf(String::isNotEmpty)?.toUShortOrNull()
            val empty = voltage.text.isNullOrBlank() && outletCount.text.isNullOrBlank() && selectedConnectors.isEmpty()
            if (empty) return null
            return MobileChargingSupplyDto(volts, selectedConnectors.toList(), outlets)
        }
        private fun chooseConnector() {
            val options = ChargingConnectorArtwork.catalog + selectedConnectors.filterIsInstance<MobileChargingConnectorDto.Other>()
            val labels = options.map(::connectorText) + root.context.getString(R.string.charging_other_connector)
            val checked = BooleanArray(labels.size) { index -> options.getOrNull(index)?.let(selectedConnectors::contains) ?: false }
            AlertDialog.Builder(root.context)
                .setTitle(R.string.charging_choose_connector)
                .setMultiChoiceItems(labels.toTypedArray(), checked) { _, which, isChecked ->
                    if (which < options.size) {
                        val item = options[which]
                        if (isChecked && item !in selectedConnectors) selectedConnectors += item
                        if (!isChecked) selectedConnectors.remove(item)
                        setConnectorLabel()
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
                        setConnectorLabel()
                    }
                }.show()
        }
        private fun setConnectorLabel() {
            connectorButton.text = selectedConnectors.takeIf { it.isNotEmpty() }?.joinToString(", ", transform = ::connectorText)
                ?: root.context.getString(R.string.charging_choose_connector)
            val connector = selectedConnectors.firstOrNull { it != MobileChargingConnectorDto.Unknown }
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
