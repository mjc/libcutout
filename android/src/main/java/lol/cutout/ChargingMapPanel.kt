package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.text.InputType
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import org.maplibre.android.geometry.LatLng
import uniffi.cutout_mobile_ffi.*

/** Charging directory UI. All spot facts and mutations come from the Rust-backed local database. */
class ChargingMapPanel(
    context: Context,
    private val client: AndroidChargingClient,
    private val mapView: RideMapSurface,
    private val onRequestLocation: () -> Unit,
) : ScrollView(context) {
    private enum class Mode { MAP, LIST, DETAILS, EDITOR, REVIEW }
    private val ui = CutoutUi(context)
    private val locationClient = AndroidMapLocationClient(context, onFix = { fix ->
        mapView.showCurrentLocation(fix.latitudeDegrees, fix.longitudeDegrees)
    }, onUnavailable = {
        syncMessage = context.getString(R.string.charging_location_unavailable)
        render()
    })
    private val content = ui.column()
    private val controls = ui.row()
    private val controller = ChargingMapController(client)
    private val reviewButton = ui.action(context.getString(R.string.charging_review_changes)) {
        mode = Mode.REVIEW
        controller.loadBlockedOperations()
    }
    private val deleteAccountButton = ui.action(context.getString(R.string.charging_account_delete)) { confirmDeleteAccount() }
    private val syncButton = ui.action(context.getString(R.string.charging_sync)) {
        synchronize(client.status.credentialState == MobileChargingSyncCredentialState.REGISTRATION_STARTED)
    }
    private var mode = Mode.MAP
    private var editorReturnMode = Mode.MAP
    private var editor: ChargingSpotEditorPanel? = null
    private var syncMessage: String? = null
    private val status = ui.text("", 14f, false, ui.muted)
    private val page = ui.column()
    var onLayoutModeChanged: (() -> Unit)? = null
    val isMapMode: Boolean get() = mode == Mode.MAP
    private var reportedMapMode: Boolean? = null

    init {
        isFillViewport = true
        clipToPadding = false
        content.setPadding(ui.dp(18), ui.dp(8), ui.dp(18), ui.dp(16))
        addView(content, LayoutParams(-1, -2))
        controls.addView(ui.action(context.getString(R.string.charging_view_map)) { setMode(Mode.MAP) }.apply { tag = "charging_view_map" },
            LinearLayout.LayoutParams(0, ui.dp(48), 1f))
        controls.addView(ui.action(context.getString(R.string.charging_view_list)) { setMode(Mode.LIST) }.apply { tag = "charging_view_list" },
            LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(8) })
        controls.addView(ui.action(context.getString(R.string.charging_filter)) { showFilters() },
            LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(8) })
        content.addView(controls)
        val actions = ui.row()
        actions.addView(ui.action(context.getString(R.string.charging_add)) { addSpot() },
            LinearLayout.LayoutParams(0, ui.dp(48), 1f))
        actions.addView(ui.action(context.getString(R.string.charging_refresh)) { refreshDirectory() },
            LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(8) })
        actions.addView(syncButton,
            LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(8) })
        content.addView(actions, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        content.addView(status, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        content.addView(reviewButton, LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(6) })
        content.addView(deleteAccountButton, LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(6) })
        content.addView(page, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        controller.onChanged = ::render
        mapView.onViewportChanged = { bounds -> controller.load(bounds) }
        mapView.onChargingSpotSelected = { id -> mode = Mode.DETAILS; controller.select(id); render() }
        controller.loadBlockedOperations()
        render()
    }

    fun updateLocation(sample: MobilePhoneLocationSampleDto?) {
        if (sample != null) locationClient.observe(sample)
    }

    fun dispose() {
        locationClient.close()
        controller.onChanged = null
        mapView.onViewportChanged = null
        mapView.onChargingSpotSelected = null
    }

    fun activate() {
        render()
        locationClient.refresh()
    }

    fun deactivate() { locationClient.cancel() }

    fun refreshLocation() { locationClient.refresh() }

    fun handleBack(): Boolean = when (mode) {
        Mode.DETAILS -> { mode = Mode.MAP; controller.select(null); render(); true }
        Mode.EDITOR -> { editor = null; mode = editorReturnMode; render(); true }
        Mode.REVIEW -> { mode = Mode.MAP; render(); true }
        else -> false
    }

    private fun setMode(value: Mode) {
        if (mode == Mode.EDITOR) return
        mode = value
        if (value == Mode.MAP || value == Mode.LIST) render()
    }

    private fun render() {
        syncButton.text = context.getString(if (client.status.credentialState == MobileChargingSyncCredentialState.REGISTRATION_STARTED)
            R.string.charging_retry_sync else R.string.charging_sync)
        reviewButton.visibility = if (controller.state.blockedOperations.isEmpty()
            && !controller.state.loadingBlockedOperations && controller.state.error == null) View.GONE else View.VISIBLE
        deleteAccountButton.visibility = if (client.status.credentialState in setOf(
                MobileChargingSyncCredentialState.READY,
                MobileChargingSyncCredentialState.DELETION_PENDING,
            )) View.VISIBLE else View.GONE
        if (mode != Mode.EDITOR || page.childCount != 1 || page.getChildAt(0) !== editor) {
            page.removeAllViews()
        }
        mapView.showChargingSpots(controller.state.spots, controller.state.selected?.id)
        status.text = listOfNotNull(
            controller.state.error,
            syncMessage,
            controller.state.blockedOperations.size.takeIf { it > 0 }?.let {
                context.getString(R.string.charging_review_pending, it)
            },
            controller.state.loading.takeIf { it }?.let { context.getString(R.string.charging_loading) },
        ).joinToString("\n")
        status.visibility = if (status.text.isNullOrBlank()) View.GONE else View.VISIBLE
        when (mode) {
            Mode.MAP -> Unit
            Mode.LIST -> renderDirectory()
            Mode.DETAILS -> controller.state.selected?.let(::renderDetails) ?: renderDirectory()
            Mode.EDITOR -> editor?.let { if (it.parent == null) page.addView(it) } ?: renderDirectory()
            Mode.REVIEW -> renderBlockedOperations()
        }
        val mapMode = isMapMode
        if (reportedMapMode != mapMode) {
            reportedMapMode = mapMode
            onLayoutModeChanged?.invoke()
        }
    }

    private fun renderBlockedOperations() {
        page.addView(ui.action(context.getString(R.string.charging_close)) { handleBack() },
            LinearLayout.LayoutParams(-1, ui.dp(48)))
        page.addView(ui.text(context.getString(R.string.charging_review_explanation), 15f, false, ui.muted), spaced())
        if (controller.state.loadingBlockedOperations) {
            page.addView(ui.text(context.getString(R.string.charging_loading), 14f, false, ui.muted), spaced())
        }
        if (!controller.state.loadingBlockedOperations && controller.state.blockedOperations.isEmpty()) {
            page.addView(ui.text(context.getString(R.string.charging_no_review), 15f, false, ui.muted), spaced())
        }
        controller.state.blockedOperations.forEach { conflict ->
            val card = ui.column().apply {
                background = ui.rounded(16)
                setPadding(ui.dp(14), ui.dp(12), ui.dp(14), ui.dp(12))
            }
            val title = conflict.input?.name?.takeIf(String::isNotBlank)
                ?: context.getString(R.string.charging_unknown)
            card.addView(ui.text(title, 18f, true))
            val reason = if (conflict.reason == MobileChargingBlockReasonDto.CONFLICT)
                R.string.charging_review_conflict else R.string.charging_review_rejected
            card.addView(ui.text(context.getString(reason), 14f, false, ui.orange), spaced())
            card.addView(ui.text(actionDescription(conflict.action), 14f, false, ui.muted), spaced())
            conflict.input?.let { card.addView(ui.text(inputDescription(it), 14f, false), spaced()) }
            conflict.remoteInput?.let {
                card.addView(ui.text(context.getString(R.string.charging_review_server_version), 14f, true), spaced())
                card.addView(ui.text(inputDescription(it), 14f, false, ui.muted), spaced())
            } ?: conflict.remoteStatus?.let { card.addView(ui.text(context.getString(statusString(it)), 14f, false, ui.muted), spaced()) }
            if (conflict.action == MobileChargingPendingActionDto.Create || conflict.action is MobileChargingPendingActionDto.Edit) {
                if (conflict.reason == MobileChargingBlockReasonDto.INVALID && conflict.input != null) {
                    card.addView(ui.action(context.getString(R.string.charging_review_edit)) {
                        openBlockedEditor(conflict)
                    }, spaced())
                }
                if (conflict.canRebase) card.addView(ui.action(context.getString(R.string.charging_review_keep)) {
                    resolveBlocked(conflict, MobileChargingResolutionDto.REBASE_KEEPING_LOCAL)
                }, spaced())
            }
            card.addView(ui.action(
                context.getString(if (conflict.remoteStatus == null) R.string.charging_review_discard else R.string.charging_review_use_server),
            ) { confirmResolve(conflict, MobileChargingResolutionDto.ACCEPT_SERVER) }, spaced())
            if (conflict.reason == MobileChargingBlockReasonDto.CONFLICT) {
                card.addView(ui.action(context.getString(R.string.charging_review_retry)) {
                    confirmResolve(conflict, MobileChargingResolutionDto.RETRY)
                }, spaced())
            }
            page.addView(card, spaced())
        }
    }

    private fun actionDescription(action: MobileChargingPendingActionDto): String = when (action) {
        MobileChargingPendingActionDto.Create -> context.getString(R.string.charging_review_create_action)
        is MobileChargingPendingActionDto.Edit -> context.getString(R.string.charging_review_edit_action)
        MobileChargingPendingActionDto.Withdraw -> context.getString(R.string.charging_review_withdraw_action)
        MobileChargingPendingActionDto.Verify -> context.getString(R.string.charging_review_verify_action)
        is MobileChargingPendingActionDto.Vote -> context.getString(R.string.charging_review_vote_action)
    }

    private fun inputDescription(input: MobileChargingInputDto): String = listOfNotNull(
        input.accessNote,
        context.getString(if (input.cost == MobileChargingCostDto.FREE) R.string.charging_cost_free else R.string.charging_cost_paid),
        input.hours,
        input.venue?.let { context.getString(venueString(it)) },
        input.terms?.let { context.getString(termsString(it)) },
        input.price?.let(::priceSummary),
        input.supplies.joinToString(" · ") { supplyText(it) }.takeIf(String::isNotBlank),
    ).joinToString("\n")

    private fun statusString(value: MobileChargingStatusDto) = when (value) {
        MobileChargingStatusDto.QUEUED -> R.string.charging_status_queued
        MobileChargingStatusDto.PUBLISHED -> R.string.charging_status_published
        MobileChargingStatusDto.HIDDEN -> R.string.charging_status_hidden
        MobileChargingStatusDto.WITHDRAWN -> R.string.charging_status_withdrawn
        MobileChargingStatusDto.REMOVED -> R.string.charging_status_removed
    }

    private fun openBlockedEditor(conflict: MobileChargingSyncConflictDto) {
        val input = conflict.input ?: return
        editorReturnMode = Mode.REVIEW
        editor = ChargingSpotEditorPanel(context, null, input.coordinate,
            onCancel = { editor = null; mode = Mode.REVIEW; render() },
            onSave = { updated ->
                controller.replaceBlocked(conflict, updated) { result ->
                    result.fold(
                        onSuccess = { editor = null; mode = Mode.REVIEW; render() },
                        onFailure = { syncMessage = it.message ?: context.getString(R.string.charging_validation); render() },
                    )
                }
            },
            initialInput = input,
        )
        mode = Mode.EDITOR
        render()
    }

    private fun confirmResolve(conflict: MobileChargingSyncConflictDto, resolution: MobileChargingResolutionDto) {
        val label = if (resolution == MobileChargingResolutionDto.RETRY) R.string.charging_review_retry
            else if (conflict.remoteStatus == null) R.string.charging_review_discard else R.string.charging_review_use_server
        AlertDialog.Builder(context)
            .setMessage(if (resolution == MobileChargingResolutionDto.RETRY) R.string.charging_review_retry_confirm
                else R.string.charging_review_discard_confirm)
            .setNegativeButton(R.string.charging_cancel, null)
            .setPositiveButton(label) { _, _ -> resolveBlocked(conflict, resolution) }
            .show()
    }

    private fun resolveBlocked(conflict: MobileChargingSyncConflictDto, resolution: MobileChargingResolutionDto) {
        controller.resolveBlocked(conflict, resolution) { result ->
            result.onFailure { syncMessage = it.message ?: context.getString(R.string.charging_conflict) }
        }
    }

    private fun renderDirectory() {
        if (controller.state.spots.isEmpty() && !controller.state.loading) {
            page.addView(ui.text(context.getString(R.string.charging_empty), 16f, false, ui.muted))
        }
        controller.state.spots.forEach { spot ->
            val input = spot.input
            val card = ui.column().apply {
                setPadding(ui.dp(14), ui.dp(12), ui.dp(14), ui.dp(12))
                background = ui.rounded(16)
                minimumHeight = ui.dp(72)
                isClickable = true
                isFocusable = true
                contentDescription = listOfNotNull(input?.name, input?.accessNote).joinToString(". ")
            }
            card.addView(ui.text(input?.name?.takeIf(String::isNotBlank) ?: context.getString(R.string.charging_unknown), 17f, true))
            card.addView(ui.text(input?.accessNote.orEmpty(), 14f, false, ui.muted),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(5) })
            card.addView(ui.text(supplySummary(input), 13f, false, ui.muted),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(5) })
            card.setOnClickListener { controller.select(spot.id); mode = Mode.DETAILS }
            page.addView(card, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = ui.dp(8) })
        }
        if (controller.state.cursor != null) {
            page.addView(ui.action(context.getString(R.string.charging_load_more)) { currentBounds()?.let { controller.load(it, true) } },
                LinearLayout.LayoutParams(-1, ui.dp(48)))
        }
    }

    private fun renderDetails(spot: MobileChargingSpotDto) {
        page.addView(ui.action(context.getString(R.string.charging_close)) { handleBack() },
            LinearLayout.LayoutParams(-1, ui.dp(48)))
        val input = spot.input
        page.addView(ui.text(input?.name?.takeIf(String::isNotBlank) ?: context.getString(R.string.charging_unknown), 24f, true), spaced())
        input?.let { value ->
            page.addView(ui.text(value.accessNote, 18f, false), spaced())
            addDetail(context.getString(if (value.cost == MobileChargingCostDto.FREE) R.string.charging_cost_free else R.string.charging_cost_paid))
            value.venue?.let { addDetail(context.getString(venueString(it))) }
            value.hours?.takeIf(String::isNotBlank)?.let { addDetail(it) }
            value.terms?.let { addDetail(context.getString(termsString(it))) }
            value.price?.let { addDetail(priceSummary(it)) }
            value.supplies.forEach { addDetail(supplyText(it)) }
        }
        if (spot.owned && (spot.status == MobileChargingStatusDto.QUEUED || spot.status == MobileChargingStatusDto.PUBLISHED)) {
            page.addView(ui.action(context.getString(R.string.charging_edit)) { openEditor(spot) }, spaced())
            page.addView(ui.action(context.getString(R.string.charging_delete)) { confirmDelete(spot) }, spaced())
        }
        if (spot.remoteId != null && spot.status == MobileChargingStatusDto.PUBLISHED) {
            page.addView(ui.action(context.getString(R.string.charging_report)) { promptReport(spot) }, spaced())
        }
        if (spot.pendingVote || controller.state.busySpotId == spot.id) {
            page.addView(ui.text(context.getString(R.string.charging_vote_queued), 14f, false, ui.muted), spaced())
        }
        val votes = ui.row()
        votes.addView(ui.action("${context.getString(R.string.charging_up_vote)} · ${spot.votesUp}") {
            controller.vote(spot, MobileChargingVoteDto.UP) { }
        }, LinearLayout.LayoutParams(0, ui.dp(48), 1f))
        votes.addView(ui.action("${context.getString(R.string.charging_down_vote)} · ${spot.votesDown}") {
            controller.vote(spot, MobileChargingVoteDto.DOWN) { }
        }, LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(8) })
        page.addView(votes, spaced())
    }

    private fun addDetail(value: String) { page.addView(ui.text(value, 15f, false, ui.muted), spaced()) }
    private fun spaced() = LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) }

    private fun addSpot() {
        val fix = locationClient.freshFix()
        val center = fix?.let { LatLng(it.latitudeDegrees, it.longitudeDegrees) } ?: mapView.currentMapCenter()
        if (center == null) {
            onRequestLocation()
            return
        }
        openEditor(null, MobileMapCoordinateDto(center.latitude, center.longitude))
    }

    private fun openEditor(spot: MobileChargingSpotDto?, coordinate: MobileMapCoordinateDto? = null) {
        val actual = coordinate ?: spot?.input?.coordinate ?: return
        editorReturnMode = if (spot == null) Mode.MAP else Mode.DETAILS
        editor = ChargingSpotEditorPanel(
            context = context,
            spot = spot,
            initialCoordinate = actual,
            onCancel = {
                editor = null
                mode = if (spot == null) Mode.MAP else Mode.DETAILS
                render()
            },
            onSave = { input ->
                controller.save(spot, input) { result ->
                    result.fold(
                        onSuccess = { saved ->
                            editor = null
                            if (saved == null) { mode = Mode.MAP; syncMessage = context.getString(R.string.charging_saved_queued) }
                            else { mode = Mode.DETAILS; controller.select(saved.id) }
                            render()
                        },
                        onFailure = { error ->
                            if (error is MobileChargingException.Conflict) showConflict(spot)
                            else syncMessage = error.message ?: context.getString(R.string.charging_validation)
                            render()
                        },
                    )
                }
            },
        )
        mode = Mode.EDITOR
        render()
    }

    private fun showConflict(spot: MobileChargingSpotDto?) {
        if (spot == null) return
        AlertDialog.Builder(context)
            .setMessage(R.string.charging_conflict)
            .setNegativeButton(R.string.charging_cancel, null)
            .setPositiveButton(R.string.charging_reload) { _, _ ->
                controller.reloadSpot(spot.id) { result -> result.onSuccess { fresh -> if (fresh != null) openEditor(fresh) } }
            }.show()
    }

    private fun confirmDelete(spot: MobileChargingSpotDto) {
        AlertDialog.Builder(context).setMessage(R.string.charging_delete_confirm)
            .setNegativeButton(R.string.charging_cancel, null)
            .setPositiveButton(R.string.charging_delete) { _, _ ->
                controller.withdraw(spot) { result ->
                    result.onSuccess { mode = Mode.MAP; render() }
                    result.onFailure { syncMessage = it.message; render() }
                }
            }.show()
    }

    private fun promptReport(spot: MobileChargingSpotDto) {
        val remoteId = spot.remoteId ?: return
        val message = EditText(context).apply {
            hint = context.getString(R.string.charging_report_details)
            minHeight = ui.dp(96)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
        }
        AlertDialog.Builder(context).setTitle(R.string.charging_report).setView(message)
            .setNegativeButton(R.string.charging_cancel, null)
            .setPositiveButton(R.string.charging_report_submit) { _, _ ->
                val reportText = message.text.toString()
                client.submitReport(remoteId, reportText) { result ->
                    result.fold(
                        onSuccess = { syncMessage = context.getString(R.string.charging_report_receipt) },
                        onFailure = { syncMessage = it.message ?: context.getString(R.string.charging_report_auth) },
                    )
                    render()
                }
            }.show()
    }

    private fun refreshDirectory() = client.refreshDirectory { result ->
        result.fold(
            onSuccess = { summary ->
                val outcome = summary.outcome
                syncMessage = if (outcome is MobileChargingSyncOutcome.Finished && outcome.result == MobileChargingSyncResult.SYNCED)
                    context.getString(R.string.charging_sync_complete)
                else context.getString(R.string.charging_sync_pending, summary.pendingCount.toInt())
                controller.refreshCurrentViewport()
            },
            onFailure = { syncMessage = it.message },
        )
        render()
    }

    private fun synchronize(explicitRetry: Boolean) = client.synchronize(explicitRetry) { result ->
        result.fold(
            onSuccess = { value ->
                syncMessage = value.message ?: when {
                    value.credentialState == MobileChargingSyncCredentialState.REGISTRATION_STARTED -> context.getString(R.string.charging_sync_uncertain)
                    value.result == MobileChargingSyncResult.SYNCED -> context.getString(R.string.charging_sync_complete)
                    else -> context.getString(R.string.charging_sync_pending, value.pendingCount.toInt())
                }
                controller.refreshCurrentViewport()
            },
            onFailure = { syncMessage = it.message },
        )
        controller.loadBlockedOperations()
        render()
    }

    private fun showFilters() {
        AlertDialog.Builder(context).setTitle(R.string.charging_filter)
            .setItems(arrayOf(context.getString(R.string.charging_cost), context.getString(R.string.charging_venue),
                context.getString(R.string.charging_connector), context.getString(R.string.charging_voltage))) { _, selected ->
                when (selected) {
                    0 -> selectCostFilter()
                    1 -> selectVenueFilter()
                    2 -> selectConnectorFilter()
                    3 -> selectVoltageFilter()
                }
            }.show()
    }

    private fun selectCostFilter() = choose(
        listOf(R.string.charging_all, R.string.charging_cost_free, R.string.charging_cost_paid),
    ) { index -> controller.setFilter(controller.filter.copy(cost = when (index) {
        1 -> MobileChargingCostDto.FREE
        2 -> MobileChargingCostDto.PAID
        else -> null
    })) }

    private fun selectVenueFilter() = choose(
        listOf(R.string.charging_all, R.string.charging_venue_restaurant, R.string.charging_venue_cafe,
            R.string.charging_venue_bar, R.string.charging_venue_ev, R.string.charging_venue_other),
    ) { index -> controller.setFilter(controller.filter.copy(venue = listOf(
        null, MobileChargingVenueDto.RESTAURANT, MobileChargingVenueDto.CAFE,
        MobileChargingVenueDto.BAR, MobileChargingVenueDto.EV_CHARGING_STATION, MobileChargingVenueDto.OTHER,
    )[index])) }

    private fun selectConnectorFilter() {
        val choices = controller.state.spots.flatMap { it.input?.supplies.orEmpty() }.flatMap { it.connectors }.distinct()
        val labels = listOf(context.getString(R.string.charging_all)) + choices.map(::connectorText)
        AlertDialog.Builder(context).setItems(labels.toTypedArray()) { _, index ->
            controller.setFilter(controller.filter.copy(connector = choices.getOrNull(index - 1)))
        }.show()
    }

    private fun selectVoltageFilter() {
        val choices = controller.state.spots.flatMap { it.input?.supplies.orEmpty() }.mapNotNull { it.voltageVolts }.distinct().sorted()
        val labels = listOf(context.getString(R.string.charging_all)) + choices.map { "$it V" }
        AlertDialog.Builder(context).setItems(labels.toTypedArray()) { _, index ->
            controller.setFilter(controller.filter.copy(voltageVolts = choices.getOrNull(index - 1)))
        }.show()
    }

    private fun choose(labels: List<Int>, picked: (Int) -> Unit) {
        AlertDialog.Builder(context).setItems(labels.map(context::getString).toTypedArray()) { _, index -> picked(index) }.show()
    }

    private fun currentBounds(): MobileGeoBoundsDto? = controller.currentBounds()

    private fun confirmDeleteAccount() {
        AlertDialog.Builder(context).setMessage(R.string.charging_account_delete_confirm)
            .setNegativeButton(R.string.charging_cancel, null)
            .setPositiveButton(R.string.charging_account_delete) { _, _ ->
                client.deleteAccount { result ->
                    syncMessage = result.fold(
                        onSuccess = { context.getString(R.string.charging_account_deleted) },
                        onFailure = { it.message ?: context.getString(R.string.charging_sync_uncertain) },
                    )
                    render()
                }
            }.show()
    }

    private fun supplySummary(input: MobileChargingInputDto?): String {
        if (input == null) return context.getString(R.string.charging_unknown)
        val cost = context.getString(if (input.cost == MobileChargingCostDto.FREE) R.string.charging_cost_free else R.string.charging_cost_paid)
        val power = input.supplies.joinToString(" · ") { supplyText(it) }
        return listOf(cost, power).filter(String::isNotBlank).joinToString(" · ")
    }

    private fun supplyText(value: MobileChargingSupplyDto) = listOfNotNull(
        value.voltageVolts?.let { "$it V" },
        value.connectors.joinToString(", ").takeIf(String::isNotBlank),
        value.outletCount?.let { "$it outlets" },
    ).joinToString(" · ")

    private fun priceSummary(price: MobileChargingPriceDto): String =
        "${java.math.BigDecimal(price.amountMinor.toString()).movePointLeft(price.minorUnitExponent.toInt()).toPlainString()} ${price.currency}"

    private fun venueString(value: MobileChargingVenueDto) = when (value) {
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

    private fun connectorText(value: MobileChargingConnectorDto): String = value.toString().substringAfterLast('.').replace('_', ' ')
}
