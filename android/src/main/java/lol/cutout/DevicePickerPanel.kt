package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
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

        val brand = ui.text("CutOut", 17f, true)
        header.addView(brand, LinearLayout.LayoutParams(0, ui.dp(44), 1f))
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
        content.addView(title, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(20) })
        content.addView(ui.text(appContext.getString(R.string.picker_subtitle), 15f, false, ui.muted).apply {
            typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
        }, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(7) })

        status.gravity = Gravity.CENTER_VERTICAL
        val indicator = FrameLayout(appContext)
        indicator.addView(PickerStatusGlyph(appContext).apply { tag = "picker_status_glyph" },
            FrameLayout.LayoutParams(ui.dp(18), ui.dp(18), Gravity.CENTER))
        status.addView(indicator, LinearLayout.LayoutParams(ui.dp(20), ui.dp(20)))
        val statusText = ui.text("", 15f, false, ui.muted)
        statusText.tag = "picker_status_text"
        status.addView(statusText, LinearLayout.LayoutParams(0, -2, 1f).apply { leftMargin = ui.dp(10) })
        val spinner = ProgressBar(appContext).apply {
            tag = "picker_spinner"
            isIndeterminate = true
            indeterminateTintList = android.content.res.ColorStateList.valueOf(ui.yellow)
        }
        indicator.addView(spinner, FrameLayout.LayoutParams(ui.dp(18), ui.dp(18), Gravity.CENTER))
        content.addView(status, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(20) })

        recoveryPanel.addView(recoveryText)
        recoveryPanel.addView(recoveryAction, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(14) })
        recoveryPanel.addView(settingsAction, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(10) })
        content.addView(recoveryPanel, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(16) })
        content.addView(rows, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(20) })
    }

    /** Refreshes visible presentation while retaining row views for stable device identities. */
    fun render(value: BlePresentation, permitted: Boolean, enabled: Boolean, hasScanned: Boolean) {
        val statusText = status.findViewWithTag<TextView>("picker_status_text")
        val spinner = status.findViewWithTag<ProgressBar>("picker_spinner")
        val statusGlyph = status.findViewWithTag<View>("picker_status_glyph")
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
        val terminalReadiness = readiness in setOf(
            MobileConnectionReadinessDto.VERIFIED,
            MobileConnectionReadinessDto.FAILED,
            MobileConnectionReadinessDto.RECORD_ONLY,
            MobileConnectionReadinessDto.CONFLICTED,
        )
        val showsActivity = !permissionNeeded && !bluetoothOff && !terminalReadiness &&
            (connecting || value.scanning)
        spinner.visibility = if (showsActivity) View.VISIBLE else View.GONE
        statusGlyph.visibility = if (showsActivity) View.GONE else View.VISIBLE

        val discovered = relevantDevices
        val activeIds = discovered.mapTo(hashSetOf()) { it.id }
        val staleIds = rowViews.keys.filterNot(activeIds::contains)
        staleIds.forEach { id -> rowViews.remove(id)?.let(rows::removeView) }
        discovered.forEach { device ->
            val row = rowViews.getOrPut(device.id) {
                DeviceRow(device.id).also {
                    rows.addView(it, LinearLayout.LayoutParams(-1, -2).apply {
                        if (rows.childCount > 0) topMargin = ui.dp(12)
                    })
                }
            }
            row.update(device)
        }
        reorderRowsIfNeeded(discovered.map { it.id })
        // An empty result is only actionable after the scan has finished. While
        // scanning, the recovery action would call the host's scan toggle and
        // stop the in-flight scan instead of retrying it.
        updateRecovery(
            permissionNeeded,
            bluetoothOff,
            value.problem,
            hasScanned && !value.scanning && discovered.isEmpty(),
        )
    }

    private fun reorderRowsIfNeeded(order: List<String>) {
        if (order == renderedOrder) return
        order.forEachIndexed { index, id ->
            val row = rowViews[id] ?: return@forEachIndexed
            if (rows.getChildAt(index) !== row) {
                rows.removeView(row)
                rows.addView(row, index, LinearLayout.LayoutParams(-1, -2).apply {
                    if (index > 0) topMargin = ui.dp(12)
                })
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

    private inner class DeviceRow(private val platformId: String) : LinearLayout(appContext) {
        private val name = ui.text("", 20f).apply {
            typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
            tag = "picker_name:$platformId"
        }
        private val identity = ui.text("", 15f, false, ui.muted).apply { tag = "picker_identity:$platformId" }
        private val info = FrameLayout(context)
        private val connect = ui.action(appContext.getString(R.string.picker_connect), primary = true) { }
        private var latest: NearbyPev? = null
        private val glyph = PickerGlyph(appContext)

        init {
            tag = "picker_row:$platformId"
            orientation = VERTICAL
            setPadding(ui.dp(18), ui.dp(18), ui.dp(18), ui.dp(18))
            background = ui.rounded(24)
            val summary = ui.row().apply { gravity = Gravity.TOP }
            glyph.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            summary.addView(glyph, LinearLayout.LayoutParams(ui.dp(56), ui.dp(56)))
            val labels = ui.column()
            labels.addView(name)
            labels.addView(identity, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
            summary.addView(labels, LinearLayout.LayoutParams(0, -2, 1f).apply { leftMargin = ui.dp(14); rightMargin = ui.dp(14) })
            info.apply {
                tag = "picker_details:$platformId"
                isClickable = true; isFocusable = true; minimumWidth = ui.dp(48); minimumHeight = ui.dp(48)
                this.contentDescription = appContext.getString(R.string.picker_details)
                addView(ui.icon(CutoutSymbol.INFO, ui.muted, 21), FrameLayout.LayoutParams(ui.dp(21), ui.dp(21), Gravity.CENTER))
                background = ui.rounded(12, ui.card, false)
                setOnClickListener { latest?.let(::showDetails) }
            }
            summary.addView(info, LinearLayout.LayoutParams(ui.dp(48), ui.dp(48)))
            addView(summary)
            connect.minHeight = ui.dp(46)
            connect.minimumHeight = ui.dp(46)
            connect.typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
            addView(connect, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(18) })
        }

        fun update(device: NearbyPev) {
            latest = device
            val candidate = device.candidate ?: return
            name.text = candidate.displayName
            val supported = candidate.support == uniffi.cutout_mobile_ffi.DiscoveryCandidateSupport.SUPPORTED
            name.setTextColor(if (supported) ui.foreground else Color.argb(
                148, Color.red(ui.foreground), Color.green(ui.foreground), Color.blue(ui.foreground),
            ))
            identity.setTextColor(if (supported) ui.muted else Color.argb(
                122, Color.red(ui.foreground), Color.green(ui.foreground), Color.blue(ui.foreground),
            ))
            val secondaryIdentity = device.advertisedName?.takeIf {
                it.isNotBlank() && it != candidate.displayName
            }
            identity.text = secondaryIdentity.orEmpty()
            identity.visibility = if (secondaryIdentity == null) View.GONE else View.VISIBLE
            glyph.supported = supported
            glyph.invalidate()
            val action = candidate.recommendedAction
            val canAct = action == DiscoveryCandidateAction.USE || action == DiscoveryCandidateAction.PROBE
            connect.visibility = if (canAct) View.VISIBLE else View.GONE
            connect.text = appContext.getString(R.string.picker_connect)
            connect.contentDescription = appContext.getString(R.string.picker_connect_device, candidate.displayName)
            connect.setOnClickListener { onConnect(device.id) }
            info.contentDescription = appContext.getString(R.string.picker_details_for_device, candidate.displayName)
        }

        private fun showDetails(device: NearbyPev) {
            val candidate = device.candidate ?: return
            val advertisedName = device.advertisedName?.takeIf { it.isNotBlank() && it != candidate.displayName }
                ?.let { "${appContext.getString(R.string.picker_advertised_name)}: $it\n" }.orEmpty()
            AlertDialog.Builder(appContext)
                .setTitle(candidate.displayName)
                .setMessage("$advertisedName${appContext.getString(R.string.picker_device_identifier)}: ${device.id}\n\n${ui.label("picker.detection_details")}\n${candidate.productCategory} - ${candidate.evidence}\n${candidate.detail}")
                .setPositiveButton(appContext.getString(R.string.picker_done), null)
                .show()
        }
    }

    private inner class PickerGlyph(context: Context) : View(context) {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        var supported = false

        override fun onDraw(canvas: android.graphics.Canvas) {
            super.onDraw(canvas)
            val side = minOf(width, height).toFloat()
            val center = side / 2f
            val yellow = ui.yellow
            if (supported) {
                paint.style = Paint.Style.FILL
                paint.color = ui.background
                canvas.drawCircle(center, center, side / 2f, paint)
                paint.style = Paint.Style.STROKE
                paint.strokeWidth = maxOf(ui.dp(2).toFloat(), side * 0.08f)
                paint.color = yellow
                canvas.drawCircle(center, center, center - paint.strokeWidth / 2f, paint)
                paint.style = Paint.Style.FILL
                paint.color = ui.card
                canvas.drawCircle(center, center, side * 0.21f, paint)
                paint.color = yellow
                for (index in 0 until 8) {
                    val angle = index * Math.PI / 4.0 - Math.PI / 2.0
                    canvas.drawCircle(
                        center + (side * 0.16f * kotlin.math.cos(angle)).toFloat(),
                        center + (side * 0.16f * kotlin.math.sin(angle)).toFloat(),
                        side * 0.0425f,
                        paint,
                    )
                }
                canvas.drawCircle(center, center, side * 0.05f, paint)
            } else {
                paint.style = Paint.Style.FILL
                paint.color = Color.argb(41, Color.red(yellow), Color.green(yellow), Color.blue(yellow))
                canvas.drawCircle(center, center, side / 2f, paint)
                paint.style = Paint.Style.STROKE
                paint.strokeWidth = ui.dp(2).toFloat()
                paint.color = yellow
                canvas.drawCircle(center, center, side * 0.30f, paint)
                paint.style = Paint.Style.FILL
                paint.typeface = Typeface.create("sans-serif", Typeface.BOLD)
                paint.textSize = side * 0.42f
                val mark = "?"
                val bounds = android.graphics.Rect()
                paint.getTextBounds(mark, 0, mark.length, bounds)
                canvas.drawText(mark, center - bounds.exactCenterX(), center - bounds.exactCenterY(), paint)
            }
        }
    }

    private inner class PickerStatusGlyph(context: Context) : View(context) {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = ui.yellow
            style = Paint.Style.STROKE
            strokeWidth = ui.dp(2).toFloat()
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
        }

        init { importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO }

        override fun onDraw(canvas: android.graphics.Canvas) {
            super.onDraw(canvas)
            val side = minOf(width, height).toFloat()
            val center = side / 2f
            canvas.drawCircle(center, center, center - paint.strokeWidth / 2f, paint)
            val path = android.graphics.Path().apply {
                moveTo(side * 0.33f, side * 0.36f)
                lineTo(side * 0.67f, side * 0.36f)
                lineTo(side * 0.52f, side * 0.50f)
                lineTo(side * 0.72f, side * 0.50f)
                lineTo(side * 0.32f, side * 0.69f)
                lineTo(side * 0.46f, side * 0.54f)
                lineTo(side * 0.28f, side * 0.54f)
                close()
            }
            canvas.drawPath(path, paint)
        }
    }
}
