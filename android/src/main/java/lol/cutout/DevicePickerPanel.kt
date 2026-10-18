package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.graphics.Color
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.TextView
import uniffi.cutout_mobile_ffi.DiscoveryCandidateAction
import uniffi.cutout_mobile_ffi.DiscoveryCandidateSection
import uniffi.cutout_mobile_ffi.MobileConnectionReadinessDto

/** Native Android rendering of the SwiftUI device picker. The bottom navigation belongs to the host. */
class DevicePickerPanel(
    context: Context,
    private val onScan: () -> Unit,
    private val onOpenSettings: () -> Unit,
    private val onOpenConfiguration: () -> Unit,
    private val onConnect: (String) -> Unit,
) : ScrollView(context) {
    private val appContext = context
    private val ui = CutoutUi(appContext)
    private val content = ui.column()
    private val header = ui.row()
    private val status = ui.row()
    private val rows = ui.column()
    private val rowViews = linkedMapOf<String, DeviceRow>()
    private val recoveryPanel = ui.column().apply {
        tag = "picker_recovery"
        setPadding(ui.dp(18), ui.dp(18), ui.dp(18), ui.dp(18))
        background = ui.rounded(24)
    }
    private val recoveryText = ui.text("", 14f, false, ui.muted)
    private val recoveryAction = ui.action("", primary = true) { }.apply { tag = "picker_recovery_primary" }
    private val settingsAction = ui.action(appContext.getString(R.string.picker_open_settings)) { onOpenSettings() }
        .apply { tag = "picker_recovery_settings" }
    private var renderedOrder: List<String> = emptyList()
    private var renderedRecovery: List<Boolean>? = null

    init {
        isFillViewport = true
        clipToPadding = false
        setBackgroundColor(ui.background)
        content.setPadding(ui.dp(24), ui.dp(8), ui.dp(24), ui.dp(24))
        addView(content, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))

        val brand = ui.text("CutOut", 18f, true)
        header.addView(brand, LinearLayout.LayoutParams(0, ui.dp(48), 1f))
        val gear = FrameLayout(appContext).apply {
            tag = "picker_setup"
            isClickable = true
            isFocusable = true
            contentDescription = appContext.getString(R.string.picker_open_setup)
            addView(ui.icon(CutoutSymbol.GEAR, ui.foreground, 22), FrameLayout.LayoutParams(ui.dp(22), ui.dp(22), Gravity.CENTER))
            setOnClickListener { onOpenConfiguration() }
            minimumWidth = ui.dp(48); minimumHeight = ui.dp(48)
            background = ui.rounded(12, ui.background, false)
        }
        header.addView(gear, LinearLayout.LayoutParams(ui.dp(48), ui.dp(48)))
        content.addView(header)

        val title = ui.title(appContext.getString(R.string.picker_title))
        title.contentDescription = appContext.getString(R.string.picker_title)
        content.addView(title, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(14) })
        content.addView(ui.text(appContext.getString(R.string.picker_subtitle), 17f, false, ui.muted), LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })

        status.gravity = Gravity.CENTER_VERTICAL
        status.addView(ui.icon(CutoutSymbol.BLUETOOTH, ui.yellow, 18))
        val statusText = ui.text("", 15f, false, ui.muted)
        statusText.tag = "picker_status_text"
        status.addView(statusText, LinearLayout.LayoutParams(0, -2, 1f).apply { leftMargin = ui.dp(10) })
        val spinner = ProgressBar(appContext).apply {
            tag = "picker_spinner"
            isIndeterminate = true
            indeterminateTintList = android.content.res.ColorStateList.valueOf(ui.yellow)
        }
        status.addView(spinner, LinearLayout.LayoutParams(ui.dp(20), ui.dp(20)).apply { leftMargin = ui.dp(12) })
        content.addView(status, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(18); bottomMargin = ui.dp(6) })

        recoveryPanel.addView(recoveryText)
        recoveryPanel.addView(recoveryAction, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(14) })
        recoveryPanel.addView(settingsAction, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(10) })
        content.addView(recoveryPanel, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(16) })
        content.addView(rows, LinearLayout.LayoutParams(-1, -2))
    }

    /** Refreshes visible presentation while retaining row views for stable device identities. */
    fun render(value: BlePresentation, permitted: Boolean, enabled: Boolean, hasScanned: Boolean) {
        val statusText = status.findViewWithTag<TextView>("picker_status_text")
        val spinner = status.findViewWithTag<ProgressBar>("picker_spinner")
        val permissionNeeded = !permitted
        val bluetoothOff = permitted && !enabled
        val readiness = value.session?.connection?.readiness
        val connecting = readiness == MobileConnectionReadinessDto.PENDING ||
            readiness == null && value.phase != NativePhase.IDLE
        val connected = readiness == MobileConnectionReadinessDto.VERIFIED
        val relevantDevices = value.devices.filter { device ->
            val candidate = device.candidate ?: return@filter false
            candidate.isPickerCandidate && candidate.section in setOf(
                DiscoveryCandidateSection.SUPPORTED,
                DiscoveryCandidateSection.PROBE_FIRST,
            )
        }
        val message = when {
            permissionNeeded -> appContext.getString(R.string.picker_status_permission_denied)
            bluetoothOff -> appContext.getString(R.string.picker_status_bluetooth_unavailable)
            connecting -> appContext.getString(R.string.picker_status_connecting)
            connected -> appContext.getString(R.string.picker_status_live)
            readiness == MobileConnectionReadinessDto.FAILED -> appContext.getString(R.string.picker_status_connection_failed)
            readiness == MobileConnectionReadinessDto.RECORD_ONLY -> appContext.getString(R.string.picker_status_record_only)
            readiness == MobileConnectionReadinessDto.CONFLICTED -> appContext.getString(R.string.picker_status_conflicted)
            value.scanning -> appContext.getString(R.string.picker_status_scanning)
            value.problem -> appContext.getString(R.string.picker_status_scan_problem)
            hasScanned && relevantDevices.isEmpty() -> appContext.getString(R.string.picker_status_no_devices)
            hasScanned -> appContext.getString(R.string.picker_status_scan_complete)
            else -> appContext.getString(R.string.picker_status_starting)
        }
        statusText.text = message
        spinner.visibility = if (value.scanning || connecting) View.VISIBLE else View.GONE

        val discovered = relevantDevices
        val activeIds = discovered.mapTo(hashSetOf()) { it.id }
        val staleIds = rowViews.keys.filterNot(activeIds::contains)
        staleIds.forEach { id -> rowViews.remove(id)?.let(rows::removeView) }
        discovered.forEach { device ->
            val row = rowViews.getOrPut(device.id) {
                DeviceRow(device.id).also {
                    rows.addView(it, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(12) })
                }
            }
            row.update(device)
        }
        reorderRowsIfNeeded(discovered.map { it.id })
        updateRecovery(permissionNeeded, bluetoothOff, value.problem, hasScanned && discovered.isEmpty())
    }

    private fun reorderRowsIfNeeded(order: List<String>) {
        if (order == renderedOrder) return
        order.forEachIndexed { index, id ->
            val row = rowViews[id] ?: return@forEachIndexed
            if (rows.getChildAt(index) !== row) {
                rows.removeView(row)
                rows.addView(row, index, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(12) })
            }
        }
        renderedOrder = order
    }

    private fun updateRecovery(permissionNeeded: Boolean, bluetoothOff: Boolean, failed: Boolean, empty: Boolean) {
        val key = listOf(permissionNeeded, bluetoothOff, failed, empty)
        if (key == renderedRecovery) return
        renderedRecovery = key
        val visible = permissionNeeded || bluetoothOff || failed || empty
        recoveryPanel.visibility = if (visible) View.VISIBLE else View.GONE
        if (!visible) return
        val message = when {
            permissionNeeded -> appContext.getString(R.string.picker_permission_detail)
            bluetoothOff -> appContext.getString(R.string.picker_bluetooth_detail)
            failed -> appContext.getString(R.string.picker_problem_detail)
            else -> appContext.getString(R.string.picker_empty_detail)
        }
        recoveryText.text = message
        recoveryText.visibility = View.VISIBLE
        recoveryAction.text = when {
            permissionNeeded -> appContext.getString(R.string.picker_allow_nearby)
            bluetoothOff -> appContext.getString(R.string.picker_enable_bluetooth)
            failed -> appContext.getString(R.string.picker_try_again)
            else -> appContext.getString(R.string.picker_scan_again)
        }
        recoveryAction.setOnClickListener {
            when {
                permissionNeeded || bluetoothOff || failed || empty -> onScan()
            }
        }
        settingsAction.visibility = if (permissionNeeded) View.VISIBLE else View.GONE
    }

    private inner class DeviceRow(private val id: String) : LinearLayout(appContext) {
        private val name = ui.text("", 18f, true)
        private val identity = ui.text("", 14f, false, ui.muted)
        private val detail = ui.text("", 14f, false, ui.muted)
        private val info = FrameLayout(context)
        private val connect = ui.action(appContext.getString(R.string.picker_connect), primary = true) { }
        private var latest: NearbyPev? = null

        init {
            tag = "picker_row:$id"
            orientation = VERTICAL
            setPadding(ui.dp(18), ui.dp(18), ui.dp(18), ui.dp(18))
            background = ui.rounded(24)
            val summary = ui.row().apply { gravity = Gravity.TOP }
            val glyphFill = FrameLayout(appContext).apply {
                background = ui.rounded(16, Color.argb(28, Color.red(ui.yellow), Color.green(ui.yellow), Color.blue(ui.yellow)), false)
                addView(ui.icon(CutoutSymbol.BLUETOOTH, ui.yellow, 27), FrameLayout.LayoutParams(ui.dp(27), ui.dp(27), Gravity.CENTER))
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            }
            summary.addView(glyphFill, LinearLayout.LayoutParams(ui.dp(56), ui.dp(56)))
            val labels = ui.column()
            labels.addView(name)
            labels.addView(identity, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
            labels.addView(detail, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
            summary.addView(labels, LinearLayout.LayoutParams(0, -2, 1f).apply { leftMargin = ui.dp(14); rightMargin = ui.dp(4) })
            info.apply {
                tag = "picker_details:$id"
                isClickable = true; isFocusable = true; minimumWidth = ui.dp(48); minimumHeight = ui.dp(48)
                this.contentDescription = appContext.getString(R.string.picker_details)
                addView(ui.icon(CutoutSymbol.INFO, ui.muted, 21), FrameLayout.LayoutParams(ui.dp(21), ui.dp(21), Gravity.CENTER))
                background = ui.rounded(12, ui.card, false)
                setOnClickListener { latest?.let(::showDetails) }
            }
            summary.addView(info, LinearLayout.LayoutParams(ui.dp(48), ui.dp(48)))
            addView(summary)
            addView(connect, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(16) })
        }

        fun update(device: NearbyPev) {
            latest = device
            val candidate = device.candidate ?: return
            name.text = candidate.displayName
            identity.text = device.id
            identity.visibility = if (device.id.isNotBlank()) View.VISIBLE else View.GONE
            detail.text = if (candidate.recommendedAction == DiscoveryCandidateAction.USE || candidate.recommendedAction == DiscoveryCandidateAction.PROBE) {
                "${candidate.productCategory} · ${candidate.evidence}"
            } else {
                candidate.detail
            }
            val action = candidate.recommendedAction
            val canAct = action == DiscoveryCandidateAction.USE || action == DiscoveryCandidateAction.PROBE
            connect.visibility = if (canAct) View.VISIBLE else View.GONE
            connect.text = if (action == DiscoveryCandidateAction.PROBE) appContext.getString(R.string.picker_probe) else appContext.getString(R.string.picker_connect)
            connect.setOnClickListener { onConnect(device.id) }
            contentDescription = "${candidate.displayName}, ${device.id}, ${candidate.productCategory}, ${candidate.evidence}"
            info.contentDescription = appContext.getString(R.string.picker_details_for_device, candidate.displayName)
        }

        private fun showDetails(device: NearbyPev) {
            val candidate = device.candidate ?: return
            val advertisedName = device.advertisedName?.takeIf { it != candidate.displayName }
                ?.let { "\n${appContext.getString(R.string.picker_advertised_name)}: $it" }.orEmpty()
            AlertDialog.Builder(appContext)
                .setTitle(candidate.displayName)
                .setMessage("${candidate.productCategory}\n\n${candidate.evidence}\n\n${candidate.detail}\n\n${appContext.getString(R.string.picker_device_identifier)}: ${device.id}$advertisedName\n${appContext.getString(R.string.picker_signal)}: ${device.rssi} dBm")
                .setPositiveButton(appContext.getString(R.string.picker_done), null)
                .show()
        }
    }
}
