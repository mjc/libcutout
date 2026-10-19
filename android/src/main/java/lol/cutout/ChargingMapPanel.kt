package lol.cutout

import android.app.AlertDialog
import android.app.Dialog
import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.PopupMenu
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
    private var centerWhenFixArrives = false
    private val locationClient = AndroidMapLocationClient(context, onFix = { fix ->
        if (active) {
            mapView.showCurrentLocation(fix.latitudeDegrees, fix.longitudeDegrees)
            if (centerWhenFixArrives) {
                centerWhenFixArrives = false
                mapView.recenter()
            }
            updateAddButtonEnabled()
        }
    }, onUnavailable = {
        centerWhenFixArrives = false
        syncMessage = context.getString(R.string.charging_location_unavailable)
        render()
    })
    private val content = ui.column()
    private val controller = ChargingMapController(client)
    private val modeSelector = ChargingDisplayModeSelector(
        context,
        context.getString(R.string.charging_view_map),
        context.getString(R.string.charging_view_list),
    ) { mapSelected -> setMode(if (mapSelected) Mode.MAP else Mode.LIST) }
    private val filterButton = chargingToolbarIconButton(
        context, ui, ChargingToolbarGlyph.FILTER,
        context.getString(R.string.charging_filter), "charging_filter", ::showFilters,
    )
    private val addButton = chargingToolbarIconButton(
        context, ui, ChargingToolbarGlyph.PLUS,
        context.getString(R.string.charging_add), "charging_add", ::addSpot,
    )
    private val moreButton = chargingToolbarIconButton(
        context, ui, ChargingToolbarGlyph.MORE,
        context.getString(R.string.charging_more_actions), "charging_more_actions", ::showActionsMenu,
    )
    private val controls = ChargingToolbar(context, modeSelector, filterButton, addButton, moreButton)
    private var mode = Mode.MAP
    private var directoryMode = Mode.MAP
    private var active = false
    private var lastViewport: MobileGeoBoundsDto? = null
    private var editorReturnMode = Mode.MAP
    private var editor: ChargingSpotEditorPanel? = null
    private var modalDialog: Dialog? = null
    private var modalBodyContainer: FrameLayout? = null
    private var modalFeedback: android.widget.TextView? = null
    private var modalMode: Mode? = null
    private var modalSpotID: String? = null
    private var modalSpotSnapshot: MobileChargingSpotDto? = null
    private var modalTitle: android.widget.TextView? = null
    private var modalCostBadge: android.widget.TextView? = null
    private var editorStatus: String? = null
    private var syncRequestedOnActivation = false
    private var syncMessage: String? = null
    private val status = ui.text("", 14f, false, ui.muted)
    private val communityActions = ui.row().apply { visibility = View.GONE }
    private val emptyAction = ui.action(context.getString(R.string.charging_empty)) { addSpot() }.apply {
        tag = "charging_empty_add"
        visibility = View.GONE
    }
    private val page = ui.column()
    var onLayoutModeChanged: (() -> Unit)? = null
    val isMapMode: Boolean get() = directoryMode == Mode.MAP
    private var reportedMapMode: Boolean? = null

    init {
        isFillViewport = true
        clipToPadding = false
        content.setPadding(ui.dp(18), ui.dp(8), ui.dp(18), ui.dp(16))
        addView(content, LayoutParams(-1, -2))
        content.addView(controls)
        content.addView(page, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        content.addView(emptyAction, LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(8) })
        content.addView(status, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        content.addView(communityActions, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
        controller.onChanged = ::render
        mapView.onViewportChanged = { bounds ->
            lastViewport = bounds
            updateAddButtonEnabled()
            if (active && directoryMode == Mode.MAP) controller.load(bounds)
        }
        mapView.onChargingSpotSelected = { id ->
            if (active && directoryMode == Mode.MAP) controller.select(id)
        }
        controller.loadBlockedOperations()
        render()
    }

    fun updateLocation(sample: MobilePhoneLocationSampleDto?) {
        if (sample != null) locationClient.observe(sample)
    }

    fun dispose() {
        modalDialog?.dismiss()
        modalDialog = null
        mapView.showChargingPopup(null)
        locationClient.close()
        controller.onChanged = null
        mapView.onViewportChanged = null
        mapView.onChargingSpotSelected = null
    }

    fun activate() {
        active = true
        render()
        locationClient.refresh()
        if (directoryMode == Mode.MAP) {
            (lastViewport ?: controller.currentBounds())?.let { bounds ->
                lastViewport = bounds
                controller.load(bounds)
            }
        }
        if (!syncRequestedOnActivation) {
            syncRequestedOnActivation = true
            synchronize(explicitRetry = false)
        }
    }

    fun deactivate() {
        active = false
        centerWhenFixArrives = false
        locationClient.cancel()
        syncRequestedOnActivation = false
        mapView.showChargingPopup(null)
        mapView.showChargingSpots(emptyList(), null)
    }

    fun refreshLocation() { if (active) locationClient.refresh() }

    /** Centers on a fresh native fix, requesting the Activity's location permission when needed. */
    fun centerOnUserLocation() {
        val fix = locationClient.freshFix()
        if (fix != null) {
            mapView.showCurrentLocation(fix.latitudeDegrees, fix.longitudeDegrees)
            mapView.recenter()
        } else {
            centerWhenFixArrives = true
            onRequestLocation()
        }
    }

    fun handleBack(): Boolean = when (mode) {
        Mode.DETAILS -> { closeSheet(Mode.DETAILS); true }
        Mode.EDITOR -> { closeSheet(Mode.EDITOR); true }
        Mode.REVIEW -> { closeSheet(Mode.REVIEW); true }
        else -> if (directoryMode == Mode.MAP && controller.state.selected != null) {
            controller.select(null)
            true
        } else false
    }

    private fun setMode(value: Mode) {
        if (mode == Mode.EDITOR) return
        mode = value
        directoryMode = value
        render()
        if (active && value == Mode.MAP) {
            (lastViewport ?: controller.currentBounds())?.let { bounds ->
                lastViewport = bounds
                controller.load(bounds)
            }
        }
    }

    private fun render() {
        if ((mode == Mode.MAP || mode == Mode.LIST) && modalDialog != null) clearSheet()
        updateAddButtonEnabled()
        modeSelector.selectMap(directoryMode == Mode.MAP)
        page.removeAllViews()
        val showMapContent = active && directoryMode == Mode.MAP
        mapView.showChargingSpots(
            if (showMapContent) controller.state.spots else emptyList(),
            if (showMapContent) controller.state.selected?.id else null,
        )
        mapView.showChargingPopup(
            controller.state.selected?.takeIf { showMapContent && mode == Mode.MAP }?.let(::chargingPopup),
        )
        status.text = listOfNotNull(
            controller.state.error,
            syncMessage,
            controller.state.blockedOperations.size.takeIf { it > 0 }?.let { context.getString(R.string.charging_review_pending, it) },
            controller.state.loading.takeIf { it }?.let { context.getString(R.string.charging_loading) },
        ).joinToString("\n")
        emptyAction.visibility = if (controller.state.spots.isEmpty() && !controller.state.loading &&
            controller.state.error == null) View.VISIBLE else View.GONE
        status.visibility = if (status.text.isNullOrBlank()) View.GONE else View.VISIBLE
        val syncNeedsAttention = client.status.credentialState == MobileChargingSyncCredentialState.REGISTRATION_STARTED ||
            client.status.pendingCount > 0u || client.status.result in setOf(
                MobileChargingSyncResult.AUTHENTICATION_REQUIRED,
                MobileChargingSyncResult.OFFLINE,
                MobileChargingSyncResult.CONFLICT,
                MobileChargingSyncResult.REJECTED,
                MobileChargingSyncResult.THROTTLED,
                MobileChargingSyncResult.STORAGE_UNAVAILABLE,
            )
        val reviewNeedsAttention = controller.state.blockedOperations.isNotEmpty() || controller.state.error != null
        communityActions.removeAllViews()
        if (syncNeedsAttention || reviewNeedsAttention) {
            communityActions.visibility = View.VISIBLE
            if (syncNeedsAttention) communityActions.addView(
                ui.action(context.getString(if (client.status.credentialState == MobileChargingSyncCredentialState.REGISTRATION_STARTED)
                    R.string.charging_retry_sync else R.string.charging_sync)) {
                    synchronize(client.status.credentialState == MobileChargingSyncCredentialState.REGISTRATION_STARTED)
                }, LinearLayout.LayoutParams(0, ui.dp(48), 1f),
            )
            if (reviewNeedsAttention) communityActions.addView(
                ui.action(context.getString(R.string.charging_review_changes)) {
                    mode = Mode.REVIEW
                    controller.loadBlockedOperations()
                    render()
                }, LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply {
                    if (communityActions.childCount > 0) leftMargin = ui.dp(8)
                },
            )
        } else communityActions.visibility = View.GONE
        when (mode) {
            Mode.MAP -> Unit
            Mode.LIST -> renderDirectory()
            Mode.DETAILS -> controller.state.selected?.let { spot ->
                if (directoryMode == Mode.LIST) renderDirectory()
                if (modalMode != Mode.DETAILS || modalSpotID != spot.id) {
                    showChargingSheet(Mode.DETAILS, spot.id, spot.input?.name ?: context.getString(R.string.charging_unknown),
                        detailContent(spot), spot.input?.cost)
                } else updateDetailSheet(spot)
            } ?: closeSheet(Mode.DETAILS)
            Mode.EDITOR -> editor?.let { panel ->
                if (directoryMode == Mode.LIST) renderDirectory()
                if (modalMode != Mode.EDITOR) showChargingSheet(
                    Mode.EDITOR, null, context.getString(if (editorReturnMode == Mode.REVIEW)
                        R.string.charging_review_edit else R.string.charging_edit), panel,
                ) else updateEditorFeedback()
            } ?: closeSheet(Mode.EDITOR)
            Mode.REVIEW -> {
                if (directoryMode == Mode.LIST) renderDirectory()
                if (modalMode != Mode.REVIEW) showChargingSheet(
                    Mode.REVIEW, null, context.getString(R.string.charging_review_changes), reviewContent(),
                ) else updateChargingSheetBody(reviewContent())
            }
        }
        val mapMode = directoryMode == Mode.MAP
        if (reportedMapMode != mapMode) {
            reportedMapMode = mapMode
            onLayoutModeChanged?.invoke()
        }
    }

    private fun reviewContent(): ScrollView {
        val scroll = ScrollView(context)
        val body = ui.column()
        body.setPadding(0, ui.dp(4), 0, ui.dp(12))
        scroll.addView(body, FrameLayout.LayoutParams(-1, -2))
        renderBlockedOperations(body)
        return scroll
    }

    private fun showActionsMenu() {
        PopupMenu(context, moreButton).apply {
            menu.add(context.getString(R.string.charging_refresh)).setOnMenuItemClickListener {
                refreshDirectory(); true
            }
            menu.add(context.getString(if (client.status.credentialState == MobileChargingSyncCredentialState.REGISTRATION_STARTED)
                R.string.charging_retry_sync else R.string.charging_sync)).setOnMenuItemClickListener {
                synchronize(client.status.credentialState == MobileChargingSyncCredentialState.REGISTRATION_STARTED); true
            }
            val hasReview = controller.state.blockedOperations.isNotEmpty() ||
                controller.state.loadingBlockedOperations || controller.state.error != null
            if (hasReview) menu.add(context.getString(R.string.charging_review_changes)).setOnMenuItemClickListener {
                mode = Mode.REVIEW
                controller.loadBlockedOperations()
                render()
                true
            }
            show()
        }
    }

    private fun showChargingSheet(
        sheetMode: Mode,
        spotID: String?,
        title: String,
        body: View,
        cost: MobileChargingCostDto? = null,
    ) {
        modalDialog?.setOnDismissListener(null)
        modalDialog?.dismiss()
        val dialog = Dialog(context)
        modalDialog = dialog
        modalMode = sheetMode
        modalSpotID = spotID
        modalSpotSnapshot = if (sheetMode == Mode.DETAILS) controller.state.selected else null
        val sheet = ui.column().apply {
            background = ui.rounded(28)
            setPadding(ui.dp(20), ui.dp(16), ui.dp(20), ui.dp(20))
        }
        val header = ui.row()
        val titleArea = ui.row().apply { gravity = Gravity.CENTER_VERTICAL }
        modalTitle = ui.text(title, 22f, true)
        titleArea.addView(modalTitle, LinearLayout.LayoutParams(0, -2, 1f))
        modalCostBadge = ui.text("", 12f, true).apply { visibility = View.GONE }
        titleArea.addView(modalCostBadge, LinearLayout.LayoutParams(-2, -2).apply { leftMargin = ui.dp(8) })
        updateCostBadge(cost)
        header.addView(titleArea, LinearLayout.LayoutParams(0, ui.dp(48), 1f))
        header.addView(ui.action("×") { closeSheet(sheetMode) }.apply {
            contentDescription = context.getString(R.string.charging_close)
            tag = "charging_sheet_close"
        }, LinearLayout.LayoutParams(ui.dp(48), ui.dp(48)))
        sheet.addView(header)
        val feedback = ui.text("", 14f, false, ui.red).apply { visibility = View.GONE }
        modalFeedback = feedback
        sheet.addView(feedback, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
        val bodyContainer = FrameLayout(context)
        modalBodyContainer = bodyContainer
        bodyContainer.addView(body, FrameLayout.LayoutParams(-1, -1))
        sheet.addView(bodyContainer, LinearLayout.LayoutParams(-1, 0, 1f).apply { topMargin = ui.dp(8) })
        dialog.setContentView(sheet)
        dialog.setOnDismissListener {
            if (modalDialog === dialog) {
                modalDialog = null
                modalBodyContainer = null
                modalFeedback = null
                modalMode = null
                modalSpotID = null
                modalSpotSnapshot = null
                modalTitle = null
                modalCostBadge = null
                if (mode == sheetMode) {
                    mode = when (sheetMode) {
                        Mode.EDITOR -> editorReturnMode
                        else -> directoryMode
                    }
                    if (mode == Mode.MAP || mode == Mode.LIST) directoryMode = mode
                    if (sheetMode == Mode.EDITOR) editor = null
                    if (sheetMode == Mode.DETAILS) controller.select(null)
                    render()
                }
            }
        }
        dialog.window?.apply {
            setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
            setGravity(Gravity.BOTTOM)
            setLayout(WindowManager.LayoutParams.MATCH_PARENT,
                (context.resources.displayMetrics.heightPixels * 0.86f).toInt())
            attributes = attributes.apply { dimAmount = 0.32f }
        }
        dialog.setCancelable(true)
        dialog.show()
        dialog.window?.setLayout(WindowManager.LayoutParams.MATCH_PARENT,
            (context.resources.displayMetrics.heightPixels * 0.86f).toInt())
    }

    private fun closeSheet(expected: Mode) {
        if (mode != expected) return
        mode = when (expected) {
            Mode.EDITOR -> editorReturnMode
            else -> directoryMode
        }
        if (mode == Mode.MAP || mode == Mode.LIST) directoryMode = mode
        if (expected == Mode.EDITOR) editor = null
        if (expected == Mode.DETAILS) controller.select(null)
        val dialog = modalDialog
        modalDialog = null
        modalBodyContainer = null
        modalFeedback = null
        modalMode = null
        modalSpotID = null
        modalSpotSnapshot = null
        modalTitle = null
        modalCostBadge = null
        dialog?.setOnDismissListener(null)
        dialog?.dismiss()
        render()
    }

    private fun clearSheet() {
        val dialog = modalDialog
        modalDialog = null
        modalBodyContainer = null
        modalFeedback = null
        modalMode = null
        modalSpotID = null
        modalSpotSnapshot = null
        modalTitle = null
        modalCostBadge = null
        dialog?.setOnDismissListener(null)
        dialog?.dismiss()
    }

    private fun updateChargingSheetBody(body: View) {
        val container = modalBodyContainer ?: return
        container.removeAllViews()
        container.addView(body, FrameLayout.LayoutParams(-1, -1))
    }

    private fun updateDetailSheet(spot: MobileChargingSpotDto) {
        if (modalSpotSnapshot == spot) return
        modalSpotSnapshot = spot
        modalTitle?.text = spot.input?.name ?: context.getString(R.string.charging_unknown)
        updateCostBadge(spot.input?.cost)
        updateChargingSheetBody(detailContent(spot))
    }

    private fun updateCostBadge(cost: MobileChargingCostDto?) {
        val badge = modalCostBadge ?: return
        if (cost == null) {
            badge.visibility = View.GONE
            return
        }
        val free = cost == MobileChargingCostDto.FREE
        badge.text = context.getString(if (free) R.string.charging_cost_free else R.string.charging_cost_paid)
        badge.setTextColor(if (free) ui.green else ui.yellow)
        badge.background = ui.rounded(18, if (free) Color.rgb(34, 73, 44) else Color.rgb(75, 60, 18), false)
        badge.setPadding(ui.dp(9), ui.dp(5), ui.dp(9), ui.dp(5))
        badge.visibility = View.VISIBLE
    }

    private fun updateEditorFeedback() {
        modalFeedback?.let {
            it.text = editorStatus.orEmpty()
            it.visibility = if (editorStatus.isNullOrBlank()) View.GONE else View.VISIBLE
        }
    }

    private fun renderBlockedOperations(target: LinearLayout) {
        target.addView(ui.text(context.getString(R.string.charging_review_explanation), 15f, false, ui.muted), spaced())
        if (controller.state.loadingBlockedOperations) {
            target.addView(ui.text(context.getString(R.string.charging_loading), 14f, false, ui.muted), spaced())
        }
        if (!controller.state.loadingBlockedOperations && controller.state.blockedOperations.isEmpty()) {
            target.addView(ui.text(context.getString(R.string.charging_no_review), 15f, false, ui.muted), spaced())
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
            target.addView(card, spaced())
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
        editorStatus = null
        editorReturnMode = Mode.REVIEW
        editor = ChargingSpotEditorPanel(context, null, input.coordinate,
            onCancel = { editor = null; mode = Mode.REVIEW; render() },
            onSave = { updated ->
                controller.replaceBlocked(conflict, updated) { result ->
                    result.fold(
                        onSuccess = { editor = null; mode = Mode.REVIEW; render() },
                        onFailure = { editorStatus = it.message ?: context.getString(R.string.charging_validation); render() },
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

    private fun detailContent(spot: MobileChargingSpotDto): ScrollView {
        val scroll = ScrollView(context)
        val body = ui.column().apply { setPadding(ui.dp(24), ui.dp(8), ui.dp(24), ui.dp(16)) }
        scroll.addView(body, FrameLayout.LayoutParams(-1, -2))
        val input = spot.input
        input?.let { value ->
            body.addView(ui.text(value.accessNote, 20f, false), spaced(16))
            value.supplies.forEach { supply ->
                val supplyRow = ui.row()
                firstKnownConnector(supply)?.let { connector ->
                    val icon = android.widget.ImageView(context).apply {
                        setImageResource(ChargingConnectorArtwork.drawable(connector))
                        imageTintList = ColorStateList.valueOf(ui.brand)
                        background = ui.rounded(14, Color.argb(26, 255, 204, 0), false)
                        setPadding(ui.dp(8), ui.dp(8), ui.dp(8), ui.dp(8))
                        contentDescription = null
                        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
                    }
                    supplyRow.addView(icon, LinearLayout.LayoutParams(ui.dp(56), ui.dp(56)).apply {
                        marginEnd = ui.dp(14)
                    })
                }
                supplyRow.addView(ui.text(supplyText(supply), 16f, true).apply {
                    typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
                    contentDescription = supplyText(supply)
                }, LinearLayout.LayoutParams(0, -2, 1f))
                body.addView(supplyRow, spaced(16))
            }
            value.venue?.takeIf { it != MobileChargingVenueDto.OTHER }?.let {
                addDetailLine(body, context.getString(venueString(it)), 12)
            }
            value.hours?.takeIf(String::isNotBlank)?.let {
                addDetailLine(body, it, 12)
            }
            value.terms?.let { addLabeledDetail(body, R.string.charging_terms, context.getString(termsString(it))) }
            value.price?.let { addLabeledDetail(body, R.string.charging_price, priceSummary(it)) }
        }
        controller.state.error?.let { body.addView(ui.text(it, 14f, false, ui.red), spaced(10)) }
        if (spot.pendingVote || controller.state.busySpotId == spot.id) {
            body.addView(ui.text(context.getString(R.string.charging_vote_queued), 14f, false, ui.muted), spaced(12))
        }
        if (spot.owned && (spot.status == MobileChargingStatusDto.QUEUED || spot.status == MobileChargingStatusDto.PUBLISHED)) {
            val actions = ui.row()
            actions.addView(ui.action(context.getString(R.string.charging_edit), true) { openEditor(spot) },
                LinearLayout.LayoutParams(0, ui.dp(48), 1f))
            actions.addView(ui.action(context.getString(R.string.charging_delete)) { confirmDelete(spot) },
                LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(10) })
            body.addView(actions, spaced(20))
        }
        if (spot.remoteId != null && spot.status == MobileChargingStatusDto.PUBLISHED) {
            body.addView(ui.action(context.getString(R.string.charging_report)) { promptReport(spot) }, spaced(12))
        }
        return scroll
    }

    private fun chargingPopup(spot: MobileChargingSpotDto): View {
        val popup = ui.column().apply {
            tag = "charging_selected_popup"
            background = ui.rounded(16, ui.card, outlined = false)
            elevation = ui.dp(5).toFloat()
            setPadding(ui.dp(8), ui.dp(4), ui.dp(8), ui.dp(8))
        }
        val heading = ui.row()
        val note = ui.text(spot.input?.accessNote?.takeIf(String::isNotBlank).orEmpty(), 13f, color = ui.muted).apply {
            tag = "charging_selected_access_note"
            maxLines = 2
            ellipsize = android.text.TextUtils.TruncateAt.END
        }
        heading.addView(note, LinearLayout.LayoutParams(0, -2, 1f))
        heading.addView(ui.action("×") { controller.select(null) }.apply {
            tag = "charging_selected_close"
            contentDescription = context.getString(R.string.charging_close)
            minWidth = ui.dp(44)
            minimumHeight = ui.dp(44)
        }, LinearLayout.LayoutParams(ui.dp(44), ui.dp(44)))
        popup.addView(heading)

        val votes = ui.row()
        votes.addView(voteButton(spot, MobileChargingVoteDto.UP),
            LinearLayout.LayoutParams(0, ui.dp(44), 1f))
        votes.addView(voteButton(spot, MobileChargingVoteDto.DOWN),
            LinearLayout.LayoutParams(0, ui.dp(44), 1f).apply { leftMargin = ui.dp(8) })
        popup.addView(votes, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
        if (spot.pendingVote || controller.state.busySpotId == spot.id) {
            popup.addView(ui.text(context.getString(R.string.charging_vote_queued), 12f, false, ui.muted).apply {
                tag = "charging_pending_vote"
            }, LinearLayout.LayoutParams(-1, ui.dp(36)))
        }
        popup.addView(ui.action(context.getString(R.string.charging_details)) {
            mode = Mode.DETAILS
            render()
        }.apply {
            tag = "charging_details_open"
            contentDescription = context.getString(R.string.charging_details)
        }, LinearLayout.LayoutParams(-1, ui.dp(44)).apply { topMargin = ui.dp(4) })
        return popup
    }

    private fun voteButton(spot: MobileChargingSpotDto, vote: MobileChargingVoteDto): View {
        val selected = spot.vote == vote
        val up = vote == MobileChargingVoteDto.UP
        val label = context.getString(if (up) R.string.charging_up_vote else R.string.charging_down_vote)
        val symbol = if (up) "↑" else "↓"
        val count = if (up) spot.votesUp else spot.votesDown
        return ui.action(if (spot.remoteId == null) symbol else "$symbol  $count") {
            controller.vote(spot, vote) { }
        }.apply {
            tag = if (up) "charging_vote_up" else "charging_vote_down"
            contentDescription = if (spot.remoteId == null) label else "$label, $count"
            isEnabled = controller.state.busySpotId != spot.id
            background = ui.rounded(22, ui.card, outlined = true).apply {
                setStroke(ui.dp(if (selected) 2 else 1), if (selected) {
                    if (up) ui.green else ui.red
                } else ui.stroke)
            }
        }
    }

    private fun addDetailLine(parent: LinearLayout, value: String, top: Int) {
        parent.addView(ui.text(value, 14f, false, ui.muted), spaced(top))
    }

    private fun addLabeledDetail(parent: LinearLayout, label: Int, value: String) {
        val column = ui.column()
        column.addView(ui.text(context.getString(label), 12f, false, ui.muted))
        column.addView(ui.text(value, 14f, false), LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
        parent.addView(column, spaced(12))
    }

    private fun spaced(top: Int = 8) = LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(top) }

    private fun firstKnownConnector(supply: MobileChargingSupplyDto): MobileChargingConnectorDto? =
        supply.connectors.firstOrNull { it != MobileChargingConnectorDto.Unknown }

    private fun addSpot() {
        if (!addButton.isEnabled) return
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
        editorStatus = null
        editorReturnMode = if (spot == null) directoryMode else Mode.DETAILS
        editor = ChargingSpotEditorPanel(
            context = context,
            spot = spot,
            initialCoordinate = actual,
            onCancel = {
                editor = null
                mode = if (spot == null) editorReturnMode else Mode.DETAILS
                directoryMode = if (mode == Mode.MAP || mode == Mode.LIST) mode else directoryMode
                render()
            },
            onSave = { input ->
                controller.save(spot, input) { result ->
                    result.fold(
                        onSuccess = { saved ->
                            editor = null
                            if (saved == null) {
                                mode = editorReturnMode
                                syncMessage = context.getString(R.string.charging_saved_queued)
                            } else { mode = Mode.DETAILS; controller.select(saved.id) }
                            render()
                        },
                        onFailure = { error ->
                            if (error is MobileChargingException.Conflict) showConflict(spot)
                            else editorStatus = error.message ?: context.getString(R.string.charging_validation)
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
                    result.onSuccess {
                        mode = directoryMode
                        clearSheet()
                        controller.select(null)
                        render()
                    }
                    result.onFailure { syncMessage = it.message; render() }
                }
            }.show()
    }

    private fun promptReport(spot: MobileChargingSpotDto) {
        val remoteId = spot.remoteId ?: return
        val retainedMessage = client.pendingReportMessage(remoteId)
        val message = EditText(context).apply {
            hint = context.getString(R.string.charging_report_details)
            setText(retainedMessage.orEmpty())
            isEnabled = retainedMessage == null
            minHeight = ui.dp(96)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
            setTextColor(ui.foreground)
            setHintTextColor(ui.muted)
            background = ui.rounded(14)
            setPadding(ui.dp(12), ui.dp(10), ui.dp(12), ui.dp(10))
        }
        val state = ui.text("", 14f, false, ui.muted)
        val form = ui.column().apply {
            setPadding(ui.dp(20), ui.dp(8), ui.dp(20), ui.dp(4))
            if (retainedMessage == null) {
                addView(ui.text(context.getString(R.string.charging_report_explanation), 14f, false, ui.muted),
                    LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = ui.dp(10) })
            }
            if (retainedMessage != null) {
                addView(ui.text(context.getString(R.string.charging_report_retry_note), 13f, false, ui.muted),
                    LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = ui.dp(8) })
            }
            addView(message, LinearLayout.LayoutParams(-1, -2))
            addView(state, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        }
        val dialog = AlertDialog.Builder(context).setTitle(R.string.charging_report).setView(form)
            .setNegativeButton(R.string.charging_cancel, null)
            .setPositiveButton(if (retainedMessage == null) R.string.charging_report_submit else R.string.charging_report_retry, null)
            .create()
        var submitting = false
        var received = false
        dialog.setOnShowListener {
            val cancel = dialog.getButton(AlertDialog.BUTTON_NEGATIVE)
            val submit = dialog.getButton(AlertDialog.BUTTON_POSITIVE)
            cancel.setOnClickListener {
                if (!submitting) dialog.dismiss()
            }
            submit.setOnClickListener {
                if (received) {
                    dialog.dismiss()
                    return@setOnClickListener
                }
                if (submitting) return@setOnClickListener
                submitting = true
                dialog.setCancelable(false)
                message.isEnabled = false
                cancel.isEnabled = false
                submit.isEnabled = false
                state.text = context.getString(R.string.charging_report_sending)
                client.submitReport(remoteId, message.text.toString()) { result ->
                    submitting = false
                    result.fold(
                        onSuccess = { receipt ->
                            received = true
                            state.text = context.getString(R.string.charging_report_receipt_id, receipt.reportId)
                            submit.text = context.getString(R.string.charging_close)
                            submit.isEnabled = true
                            cancel.visibility = View.GONE
                            dialog.setCancelable(true)
                            syncMessage = context.getString(R.string.charging_report_receipt)
                        },
                        onFailure = { error ->
                            state.text = error.message ?: context.getString(R.string.charging_report_auth)
                            submit.text = context.getString(R.string.charging_report_retry)
                            submit.isEnabled = true
                            cancel.isEnabled = true
                            dialog.setCancelable(true)
                        },
                    )
                    render()
                }
            }
        }
        dialog.setCancelable(true)
        dialog.show()
    }

    private fun refreshDirectory() = client.refreshDirectory { result ->
        result.fold(
            onSuccess = { summary ->
                val outcome = summary.outcome
                syncMessage = if (outcome is MobileChargingSyncOutcome.Finished && outcome.result == MobileChargingSyncResult.SYNCED)
                    context.getString(R.string.charging_sync_complete)
                else context.getString(R.string.charging_sync_pending, summary.pendingCount.toInt())
                if (active) controller.refreshCurrentViewport()
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
                if (active) controller.refreshCurrentViewport()
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
        val choices = ChargingConnectorArtwork.catalog
        val labels = listOf(context.getString(R.string.charging_all)) + choices.map(::connectorText)
        val selectedIndex = controller.filter.connector?.let { selected -> choices.indexOf(selected) + 1 } ?: 0
        AlertDialog.Builder(context).setTitle(R.string.charging_connector)
            .setSingleChoiceItems(labels.toTypedArray(), selectedIndex) { dialog, index ->
                controller.setFilter(controller.filter.copy(connector = choices.getOrNull(index - 1)))
                dialog.dismiss()
            }.setNegativeButton(R.string.charging_cancel, null).show()
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

    private fun currentBounds(): MobileGeoBoundsDto? = controller.currentBounds() ?: lastViewport

    private fun updateAddButtonEnabled() {
        val center = mapView.currentMapCenter()
        val hasUsableCenter = center != null && center.latitude.isFinite() && center.longitude.isFinite() &&
            center.latitude in -90.0..90.0 && center.longitude in -180.0..180.0
        addButton.isEnabled = editor == null && (hasUsableCenter || locationClient.freshFix() != null)
    }

    private fun supplySummary(input: MobileChargingInputDto?): String {
        if (input == null) return context.getString(R.string.charging_unknown)
        val cost = context.getString(if (input.cost == MobileChargingCostDto.FREE) R.string.charging_cost_free else R.string.charging_cost_paid)
        val power = input.supplies.joinToString(" · ") { supplyText(it) }
        return listOf(cost, power).filter(String::isNotBlank).joinToString(" · ")
    }

    private fun supplyText(value: MobileChargingSupplyDto): String {
        val voltage = value.voltageVolts?.let { "$it V" } ?: ui.label("charging.spot.power.unknown")
        val connectors = value.connectors.joinToString(", ") { ChargingConnectorArtwork.label(ui, it) }
        val power = if (connectors.isBlank()) voltage else "$voltage · $connectors"
        val outlets = value.outletCount?.let {
            if (it == 1.toUShort()) context.getString(R.string.charging_outlet_one)
            else context.getString(R.string.charging_outlet_many, it.toInt())
        }
        return listOfNotNull(outlets, power).joinToString(" · ")
    }

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

    private fun connectorText(value: MobileChargingConnectorDto): String = ChargingConnectorArtwork.label(ui, value)
}
