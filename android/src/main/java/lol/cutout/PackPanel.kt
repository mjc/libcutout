package lol.cutout

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.graphics.drawable.RippleDrawable
import android.widget.EditText
import android.widget.GridLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import uniffi.cutout_mobile_ffi.*
import java.util.Locale

/** Rust-projected pack/BMS readings with the same overview, map, and group-detail flow as SwiftUI. */
class PackPanel(context: Context) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val page = ui.column()
    private val bindings = mutableListOf<() -> Unit>()
    private var latest = BlePresentation()
    private var controllerBattery: MobileControllerBatteryProjectionDto? = null
    private var selectedGroup: UShort? = null
    private var showCells = false
    private var readingQuery = ""
    private var renderedStructure: List<Any?>? = null
    private val verified get() = latest.session?.connection?.readiness == MobileConnectionReadinessDto.VERIFIED
    private val bms get() = latest.bms.takeIf { verified }
    private val telemetry get() = latest.telemetry.takeIf { verified }
    private val allGroups get() = bms?.groups.orEmpty()
    // Swift's cell map includes every Rust-observed group, including one whose current
    // voltage is unavailable. Keep that group addressable so its retained metadata and
    // detail state are not silently lost just because this sample has no voltage.
    private val visibleGroups get() = allGroups.filter { readingQuery.isBlank() || it.index.toString().contains(readingQuery) || it.label?.contains(readingQuery, true) == true }

    init {
        setBackgroundColor(ui.background)
        page.setPadding(ui.dp(23), ui.dp(20), ui.dp(23), ui.dp(32))
        addView(page)
        rebuild()
    }

    fun render(value: BlePresentation) {
        latest = value
        controllerBattery = if (verified) mobileControllerBatteryProjection(
            MobileControllerBatteryInputDto.Live(
                MobileControllerBatteryReadingsDto(
                    telemetry?.batteryLevelReported?.value, telemetry?.batteryLevelEstimated?.value,
                    telemetry?.voltage?.value, telemetry?.batteryCurrent?.value,
                    latest.chargeEstimate?.voltageSag?.let { VoltageDelta(it.deltaMillivolts) }
                        ?: telemetry?.voltageSag?.value,
                ),
                MobileControllerBatteryReadingsDto(bms?.energyPercent?.value, null,
                    bms?.voltage?.value, bms?.current?.value, null),
            ),
        ) else null
        if (renderedStructure != structure()) rebuild() else bindings.forEach { it() }
    }

    fun handleBack(): Boolean {
        if (selectedGroup != null) { selectedGroup = null; showCells = true; rebuild(); return true }
        if (showCells) { showCells = false; rebuild(); return true }
        return false
    }

    /** Rebuild only when the rendered BMS tree changes shape, not for each new reading. */
    private fun structure(): List<Any?> {
        val snapshot = bms
        val groups = snapshot?.groups.orEmpty()
        val selected = selectedGroup?.let(::group)
        return listOf(
            verified,
            snapshot?.availability,
            snapshot?.topology?.confidence,
            snapshot?.topology?.seriesGroupCount,
            snapshot?.topology?.parallelCount,
            snapshot?.groups?.isEmpty(),
            snapshot?.let {
                it.groups.isNotEmpty() && it.topology.confidence == MobileBmsTopologyConfidenceDto.UNVERIFIED &&
                    it.topology.seriesGroupCount == null
            } == true,
            snapshot?.lowestGroupIndex,
            snapshot?.highestGroupIndex,
            snapshot?.cellDelta != null,
            snapshot?.energyPercent != null,
            snapshot?.voltage != null,
            snapshot?.current != null,
            snapshot?.bmsPackCurrent0 != null,
            snapshot?.bmsPackCurrent1 != null,
            snapshot?.cellDelta != null,
            snapshot?.balancingSummary?.isNotBlank() == true,
            snapshot?.faultSummary != null,
            groups.map { g -> listOf(
                g.index, g.packNumber, g.label, g.voltage != null,
                g.alertLevel == MobileBmsAlertLevelDto.WARNING || g.alertLevel == MobileBmsAlertLevelDto.CRITICAL,
                g.temperature != null, g.resistance != null, g.detail != null,
                g.recentVoltages.size, g.latestVoltage != null, g.isBalancing != null,
            ) },
            snapshot?.temperatures?.size,
            snapshot?.highestTemperature != null,
            snapshot?.faults?.map { it.label to it.alertLevel },
            selectedGroup,
            selected?.let { g -> listOf(
                g.alertLevel, g.isBalancing != null, g.detail != null, g.temperature != null,
                g.resistance != null, g.recentVoltages.size,
                g.latestVoltage != null && g.voltage?.value != g.latestVoltage,
            ) },
            showCells,
        )
    }

    private fun group(index: UShort) = allGroups.firstOrNull { it.index == index }

    private fun rebuild() {
        renderedStructure = structure(); bindings.clear(); page.removeAllViews()
        page.addView(ui.title(context.getString(R.string.pack))); page.addView(ui.space(14))
        val snapshot = bms
        val detail = selectedGroup?.let(::group)
        if (detail != null) renderDetail(snapshot!!, detail) else renderMain(snapshot)
    }

    private fun renderControllerBattery() {
        val warning = card().apply { tag = "bms.no-data.warning" }
        warning.addView(ui.text(ui.label("bms.no_data.title"), 20f, true, ui.orange))
        listOf("cell_balance_warning", "diagnostics_warning").forEach { key ->
            warning.addView(ui.text(ui.label("bms.no_data.row.$key"), 16f).apply { setPadding(0, ui.dp(8), 0, 0) })
        }
        val estimateCard = card().apply { tag = "bms.no-data.estimate" }
        val largeText = resources.configuration.fontScale >= 1.3f
        val hero = if (largeText) ui.column() else ui.row()
        estimateCard.addView(hero, LinearLayout.LayoutParams(-1, -2))
        val estimate = ui.column()
        val certainty = ui.column().apply {
            setPadding(ui.dp(10), ui.dp(14), ui.dp(10), ui.dp(14))
            background = ui.rounded(18)
        }
        fun heroParams() = if (largeText) LinearLayout.LayoutParams(-1, -2)
            else LinearLayout.LayoutParams(0, -2, 1f)
        hero.addView(estimate, heroParams())
        hero.addView(certainty, heroParams())
        estimate.addView(ui.text(ui.label("bms.no_data.pack_estimate"), 13f, true, ui.muted))
        val amount = ui.text("", 34f, true)
        bind { amount.text = controllerBattery?.level?.let { "${it.value} %" } ?: "— %" }
        estimate.addView(amount)
        val detail = ui.text("", 14f, color = ui.muted)
        bind { detail.text = ui.label("bms.no_data.estimate_detail." + when (controllerBattery?.detail) {
            MobileControllerBatteryEstimateDetailDto.RECENT_SAG -> "recent_sag"
            MobileControllerBatteryEstimateDetailDto.VOLTAGE_CURVE -> "voltage_curve"
            MobileControllerBatteryEstimateDetailDto.UNAVAILABLE, null -> "unavailable"
        }) }
        estimate.addView(detail)
        certainty.addView(ui.space(4))
        certainty.addView(ui.text(ui.label("bms.no_data.confidence"), 13f, true, ui.muted))
        val confidence = ui.text("", 23f, true).apply { tag = "bms.no-data.confidence" }
        val confidenceDetail = ui.text("", 14f, color = ui.muted)
        bind {
            val confidenceValue = controllerBattery?.confidence
            confidence.text = ui.label("bms.no_data.confidence." + when (confidenceValue) {
                MobileControllerBatteryConfidenceDto.MEDIUM -> "medium"
                MobileControllerBatteryConfidenceDto.LOW -> "low"
                MobileControllerBatteryConfidenceDto.UNKNOWN, null -> "unknown"
            })
            confidenceDetail.text = ui.label(if (confidenceValue == MobileControllerBatteryConfidenceDto.UNKNOWN || confidenceValue == null)
                "bms.no_data.confidence_detail.telemetry_unavailable" else "bms.no_data.confidence_detail.not_cell_safe")
        }
        certainty.addView(confidence)
        certainty.addView(confidenceDetail)
        val readings = card()
        readings.addView(ui.text(ui.label("bms.no_data.what_we_can_see"), 13f, true, ui.muted))
        metric(readings, ui.label("bms.no_data.pack_voltage")) {
            controllerBattery?.voltage?.let { number(it.value.toDouble() / 1000, 1) + " V" }
        }
        metric(readings, ui.label("bms.no_data.ride_sag")) {
            controllerBattery?.voltageSag?.let { number(kotlin.math.abs(it.value.toDouble()) / 1000, 1) + " V" }
        }
        metric(readings, ui.label("bms.no_data.load_now")) { controllerBattery?.current?.let(::currentText) }
        val unknown = card()
        unknown.addView(ui.text(ui.label("bms.no_data.what_is_unknown"), 13f, true, ui.muted))
        listOf("cell_voltages", "weak_groups", "diagnostics").forEach { key ->
            unknown.addView(ui.text(ui.label("bms.no_data.row.$key"), 15f, color = ui.muted).apply { setPadding(0, ui.dp(8), 0, 0) })
        }
    }

    private fun renderMain(snapshot: MobileBmsSnapshotDto?) {
        when {
            !verified -> emptyState(context.getString(R.string.pack_no_readback), context.getString(R.string.pack_no_readback_detail))
            snapshot == null || snapshot.availability == MobileReadbackAvailabilityDto.UNSUPPORTED -> renderControllerBattery()
            snapshot.groups.isEmpty() && snapshot.topology.confidence == MobileBmsTopologyConfidenceDto.UNVERIFIED && snapshot.topology.seriesGroupCount == null -> {
                heading(topologyLabel(snapshot))
                emptyState(context.getString(R.string.pack_unknown_topology), context.getString(R.string.pack_unknown_topology_detail))
                renderMeasurements(snapshot)
            }
            else -> {
                heading(topologyLabel(snapshot))
                val counts = ui.text("", 14f, color = ui.muted)
                bind { counts.text = context.getString(R.string.pack_observed_groups, bms?.observedGroupCount?.toInt() ?: 0) }
                page.addView(counts)
                page.addView(ui.space(12))
                page.addView(ui.row().apply {
                    addView(ui.action(context.getString(R.string.pack_overview), !showCells) { showCells = false; rebuild() }.apply {
                        tag = "pack.mode.overview"; isSelected = !showCells
                    }, LinearLayout.LayoutParams(0, -2, 1f))
                    addView(ui.action(context.getString(R.string.pack_cell_map), showCells) { showCells = true; rebuild() }.apply {
                        tag = "pack.mode.cell-map"; isSelected = showCells
                    }, LinearLayout.LayoutParams(0, -2, 1f))
                })
                page.addView(ui.space(10))
                if (showCells) renderCellMap(snapshot) else renderOverview(snapshot)
            }
        }
    }

    private fun renderOverview(snapshot: MobileBmsSnapshotDto) {
        if (snapshot.availability == MobileReadbackAvailabilityDto.UNAVAILABLE) {
            val status = card()
            status.addView(ui.text(context.getString(R.string.pack_unavailable), 17f, true, ui.orange))
            status.addView(ui.text(context.getString(R.string.pack_unavailable_detail), 14f, color = ui.muted))
        }
        val lowest = snapshot.lowestGroupIndex?.let(::group)
        val highest = snapshot.highestGroupIndex?.let(::group)
        if (lowest != null && highest != null && snapshot.cellDelta != null) {
            val card = card()
            card.addView(ui.text(context.getString(R.string.pack_difference), 15f, color = ui.muted))
            val delta = ui.text("", 29f, true)
            bind { delta.text = bms?.cellDelta?.value?.value?.let { "$it mV" } ?: "—" }
            card.addView(delta)
            val chart = PackVoltageBarChart(context, emptyList()).apply {
                tag = "pack.voltage-chart"
                contentDescription = context.getString(R.string.pack_chart)
                layoutParams = LinearLayout.LayoutParams(-1, ui.dp(110))
            }
            bind { chart.updateValues(bms?.groups.orEmpty().mapNotNull { g ->
                g.voltage?.value?.value?.let { g.index.toInt() to (it.toDouble() / 1000.0) }
            }) }
            card.addView(chart)
            card.addView(ui.row().apply {
                addView(groupExtreme(context.getString(R.string.pack_lowest), lowest.index), LinearLayout.LayoutParams(0, -2, 1f))
                addView(groupExtreme(context.getString(R.string.pack_highest), highest.index), LinearLayout.LayoutParams(0, -2, 1f))
            })
            val readings = ui.text("", 13f, color = ui.muted)
            bind { readings.text = context.getString(R.string.pack_readings, bms?.observedGroupCount?.toInt() ?: 0) }
            card.addView(readings)
        }
        renderMeasurements(snapshot)
        snapshot.groups.filter { it.alertLevel == MobileBmsAlertLevelDto.WARNING || it.alertLevel == MobileBmsAlertLevelDto.CRITICAL }.forEach { g ->
            val warning = ui.action(context.getString(R.string.pack_warning, groupName(g))) {
                selectedGroup = g.index; rebuild()
            }
            bind {
                val current = group(g.index) ?: g
                warning.text = context.getString(R.string.pack_warning, groupName(current))
                warning.setTextColor(if (current.alertLevel == MobileBmsAlertLevelDto.CRITICAL) ui.red else ui.orange)
            }
            page.addView(warning)
        }
        if (snapshot.groups.isNotEmpty()) {
            page.addView(ui.space(12)); sectionTitle(context.getString(R.string.pack_cells))
            renderGroups(snapshot.groups.take(24))
            if (snapshot.groups.size > 24) page.addView(ui.action(context.getString(R.string.pack_cell_map)) { showCells = true; rebuild() })
        }
        if (snapshot.temperatures.isNotEmpty()) {
            page.addView(ui.space(10)); sectionTitle(context.getString(R.string.pack_temperatures))
            val tempCard = card()
            snapshot.temperatures.indices.forEach { index ->
                metric(tempCard, context.getString(R.string.pack_sensor, index + 1)) {
                    bms?.temperatures?.getOrNull(index)?.let { number(it.value.value.toDouble() / 1000, 1) + " °C" }
                }
            }
        } else if (snapshot.highestTemperature != null) {
            val c = card(); metric(c, context.getString(R.string.temperature)) {
                bms?.highestTemperature?.let { number(it.value.value.toDouble() / 1000, 1) + " °C" }
            }
        }
        snapshot.balancingSummary?.takeIf(String::isNotBlank)?.let { summary ->
            val balancing = card()
            balancing.addView(ui.text(context.getString(R.string.pack_balancing), 17f, true))
            val detail = ui.text(summary, 14f, color = ui.muted).apply { tag = "pack.balancing.detail" }
            bind { detail.text = bms?.balancingSummary.orEmpty() }
            balancing.addView(detail)
        }
        snapshot.faults.forEach { fault -> page.addView(ui.text("⚠  ${fault.label}", 16f, true, alertColor(fault.alertLevel))) }
    }

    private fun renderMeasurements(snapshot: MobileBmsSnapshotDto) {
        if (snapshot.energyPercent == null && snapshot.voltage == null && snapshot.current == null && snapshot.bmsPackCurrent0 == null && snapshot.bmsPackCurrent1 == null) return
        val card = card()
        if (snapshot.energyPercent != null) metric(card, context.getString(R.string.battery)) {
            bms?.energyPercent?.let { (if (it.source == MobileValueSourceDto.ESTIMATED) "≈" else "") + "${it.value.value} %" }
        }
        if (snapshot.voltage != null) metric(card, context.getString(R.string.voltage)) {
            bms?.voltage?.value?.value?.let { number(it.toDouble() / 1000, 2) + " V" }
        }
        if (snapshot.current != null) metric(card, ui.label("bms.pack.current")) {
            bms?.current?.value?.let(::currentText)
        }
        if (snapshot.bmsPackCurrent0 != null) metric(card, context.getString(R.string.pack_current_1)) {
            bms?.bmsPackCurrent0?.value?.let(::currentText)
        }
        if (snapshot.bmsPackCurrent1 != null) metric(card, context.getString(R.string.pack_current_2)) {
            bms?.bmsPackCurrent1?.value?.let(::currentText)
        }
    }

    private fun renderCellMap(snapshot: MobileBmsSnapshotDto) {
        if (snapshot.groups.size > 24) {
            val search = EditText(context).apply {
                setSingleLine(true); hint = context.getString(R.string.pack_find_reading); setText(readingQuery)
                contentDescription = context.getString(R.string.pack_find_reading); tag = "pack.search"
                addTextChangedListener(object : android.text.TextWatcher {
                    override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
                    override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) { readingQuery = s?.toString().orEmpty(); renderGroupList() }
                    override fun afterTextChanged(s: android.text.Editable?) = Unit
                })
            }
            page.addView(search)
        }
        renderGroupList()
    }

    private fun renderGroupList() {
        val snapshot = bms ?: return
        page.findViewWithTag<View>("pack.group-list")?.let(page::removeView)
        val container = ui.column().apply { tag = "pack.group-list" }
        val grouped = visibleGroups.groupBy { it.packNumber }
        val usePacks = grouped.keys.any { it != null } && grouped.values.sumOf { it.size } == visibleGroups.size
        if (usePacks) grouped.entries.sortedBy { it.key?.toInt() ?: Int.MAX_VALUE }.forEach { (packNumber, groups) ->
            if (packNumber != null) container.addView(ui.text(context.getString(R.string.pack_number, packNumber.toInt()), 15f, true, ui.muted))
            groupGrid(container, groups)
        } else groupGrid(container, visibleGroups)
        page.addView(container)
    }

    private fun renderGroups(groups: List<MobileBmsGroupSnapshotDto>) = groupGrid(page, groups)

    private fun groupGrid(parent: LinearLayout, groups: List<MobileBmsGroupSnapshotDto>) {
        val grid = GridLayout(context).apply {
            tag = "pack.group-grid"
            columnCount = groupColumnCount()
            addOnLayoutChangeListener { view, left, _, right, _, oldLeft, _, oldRight, _ ->
                if (right - left != oldRight - oldLeft) {
                    val next = groupColumnCount(right - left)
                    if (columnCount != next) columnCount = next
                }
            }
        }
        groups.forEach { g ->
            val index = g.index
            val cellShape = ui.rounded(8, ui.card, true)
            val cell = ui.column().apply {
                tag = "pack.group.${g.index}"
                minimumHeight = maxOf(ui.dp(60), (60 * resources.displayMetrics.scaledDensity).toInt())
                setPadding(ui.dp(8), ui.dp(6), ui.dp(8), ui.dp(6))
                background = RippleDrawable(
                    ColorStateList.valueOf(if (ui.dark) 0x33FFFFFF else 0x22000000),
                    cellShape,
                    ui.rounded(8, Color.WHITE, false),
                )
                isClickable = true
                isFocusable = true
                descendantFocusability = ViewGroup.FOCUS_BLOCK_DESCENDANTS
                setOnClickListener { selectedGroup = index; rebuild() }
                accessibilityDelegate = object : View.AccessibilityDelegate() {
                    override fun onInitializeAccessibilityNodeInfo(
                        host: View,
                        info: android.view.accessibility.AccessibilityNodeInfo,
                    ) {
                        super.onInitializeAccessibilityNodeInfo(host, info)
                        info.className = android.widget.Button::class.java.name
                        info.isClickable = true
                        info.isFocusable = true
                        info.addAction(android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK)
                    }
                }
            }
            val top = ui.row()
            val title = ui.text("", 11f, color = ui.muted).apply {
                tag = "pack.group.title.$index"
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            }
            top.addView(title, LinearLayout.LayoutParams(0, -2, 1f))
            val alert = ui.text("", 13f, true).apply {
                tag = "pack.group.alert.$index"
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            }
            top.addView(alert)
            cell.addView(top, LinearLayout.LayoutParams(-1, -2))
            cell.addView(ui.space(2))
            val voltage = ui.text("", 16f).apply {
                tag = "pack.group.value.$index"
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
                typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
                fontFeatureSettings = "tnum"
            }
            cell.addView(voltage, LinearLayout.LayoutParams(-1, -2))
            bind {
                val current = group(index)
                val displayed = current ?: g
                cellShape.setStroke(ui.dp(1), groupCellStroke(displayed.alertLevel))
                title.text = groupReadingLabel(displayed)
                voltage.text = current?.let(::groupVoltage) ?: context.getString(R.string.unavailable)
                voltage.setTextColor(current?.let { alertColor(it.alertLevel) } ?: ui.muted)
                alert.text = when (displayed.alertLevel) {
                    MobileBmsAlertLevelDto.CRITICAL, MobileBmsAlertLevelDto.WARNING -> "!"
                    MobileBmsAlertLevelDto.UNKNOWN -> "?"
                    MobileBmsAlertLevelDto.NOMINAL -> ""
                }
                alert.setTextColor(alertColor(displayed.alertLevel))
                val accessibility = mutableListOf(
                    context.getString(R.string.pack_group_accessibility, groupName(displayed), voltage.text),
                )
                if (displayed.alertLevel == MobileBmsAlertLevelDto.WARNING || displayed.alertLevel == MobileBmsAlertLevelDto.CRITICAL) {
                    accessibility += context.getString(R.string.pack_cell_warning)
                } else if (displayed.alertLevel == MobileBmsAlertLevelDto.UNKNOWN) {
                    accessibility += context.getString(R.string.setting_unknown)
                }
                if (displayed.isBalancing == true) accessibility += context.getString(R.string.pack_balancing)
                displayed.detail?.takeIf(String::isNotBlank)?.let(accessibility::add)
                cell.contentDescription = accessibility.joinToString(", ")
            }
            grid.addView(cell, GridLayout.LayoutParams().apply {
                width = 0; height = -2; columnSpec = GridLayout.spec(GridLayout.UNDEFINED, 1f)
                setMargins(ui.dp(3), ui.dp(3), ui.dp(3), ui.dp(3))
            })
        }
        parent.addView(grid, LinearLayout.LayoutParams(-1, -2))
    }

    private fun groupColumnCount(width: Int = (resources.displayMetrics.widthPixels - ui.dp(46)).coerceAtLeast(0)): Int {
        val minWidth = if (resources.configuration.fontScale >= 1.3f) 240 else 80
        val spacing = ui.dp(8)
        val available = width.coerceAtLeast(ui.dp(minWidth))
        return ((available + spacing) / (ui.dp(minWidth) + spacing)).coerceAtLeast(1)
    }

    private fun groupReadingLabel(group: MobileBmsGroupSnapshotDto): String {
        val pack = group.packNumber
        val reading = group.packReadingIndex
        return if (pack != null && reading != null) {
            "P${pack.toInt()} · ${reading.toInt().toString().padStart(2, '0')}"
        } else group.index.toString().padStart(2, '0')
    }

    private fun groupCellStroke(level: MobileBmsAlertLevelDto): Int = when (level) {
        MobileBmsAlertLevelDto.CRITICAL -> Color.argb(140, Color.red(ui.orange), Color.green(ui.orange), Color.blue(ui.orange))
        MobileBmsAlertLevelDto.WARNING -> ui.orange
        MobileBmsAlertLevelDto.NOMINAL, MobileBmsAlertLevelDto.UNKNOWN -> Color.argb(51, Color.red(ui.muted), Color.green(ui.muted), Color.blue(ui.muted))
    }

    private fun renderDetail(snapshot: MobileBmsSnapshotDto, selected: MobileBmsGroupSnapshotDto) {
        val selectedIndex = selected.index
        fun current() = group(selectedIndex)
        // Swift's detail Back action explicitly returns to the cell map, even when
        // the detail was opened from an overview warning/extreme-value control.
        page.addView(ui.action(ui.label("bms.pack.back")) { selectedGroup = null; showCells = true; rebuild() })
        page.addView(ui.space(12))
        val card = card()
        val groupLabel = ui.text("", 18f, true, ui.muted)
        bind { groupLabel.text = current()?.let(::groupName).orEmpty() }
        card.addView(groupLabel)
        val voltage = ui.text("", 32f, true).apply { tag = "pack.detail.voltage" }
        bind { voltage.text = current()?.let(::groupVoltage) ?: "—" }
        card.addView(voltage)
        val warning = ui.text(context.getString(R.string.pack_cell_warning), 15f, true)
        bind {
            val level = current()?.alertLevel
            warning.visibility = if (level == MobileBmsAlertLevelDto.WARNING || level == MobileBmsAlertLevelDto.CRITICAL) View.VISIBLE else View.GONE
            warning.setTextColor(level?.let(::alertColor) ?: ui.muted)
        }
        card.addView(warning)
        val balancing = ui.text(context.getString(R.string.pack_balancing), 15f, true, ui.cyan)
        bind { balancing.visibility = if (current()?.isBalancing == true) View.VISIBLE else View.GONE }
        card.addView(balancing)
        val detail = ui.text("", 14f, color = ui.muted)
        bind { detail.text = current()?.detail.orEmpty(); detail.visibility = if (detail.text.isBlank()) View.GONE else View.VISIBLE }
        card.addView(detail)
        if (selected.temperature != null) metric(card, context.getString(R.string.temperature)) { current()?.temperature?.let { number(it.value.value.toDouble() / 1000, 1) + " °C" } }
        if (selected.resistance != null) metric(card, context.getString(R.string.pack_resistance)) { current()?.resistance?.let { number(it.value.toDouble(), 2) + " mΩ" } }
        val historySize = selected.recentVoltages.size
        val history = current()?.recentVoltages.orEmpty().map { it.value.toDouble() / 1000.0 }
        if (historySize > 1) {
            card.addView(ui.space(10)); card.addView(ui.text(context.getString(R.string.pack_recent_history), 17f, true))
            val chart = GroupVoltageChart(context, history).apply { contentDescription = context.getString(R.string.pack_recent_history_accessibility, historySize); layoutParams = LinearLayout.LayoutParams(-1, ui.dp(120)) }
            bind { chart.updateValues(current()?.recentVoltages.orEmpty().map { it.value.toDouble() / 1000.0 }) }
            card.addView(chart)
            metric(card, context.getString(R.string.pack_recent_range)) {
                val values = current()?.recentVoltages.orEmpty().map { it.value.toDouble() / 1000.0 }
                if (values.isEmpty()) null else "${number(values.minOrNull()!!, 3)}–${number(values.maxOrNull()!!, 3)} V"
            }
        if (selected.latestVoltage != null && selected.voltage?.value != selected.latestVoltage) metric(card, context.getString(R.string.pack_latest_raw)) {
                current()?.latestVoltage?.takeIf { current()?.voltage?.value != it }?.let { number(it.value.toDouble() / 1000, 3) + " V" }
            }
            card.addView(ui.text(context.getString(R.string.pack_stabilized_explanation), 13f, color = ui.muted))
        }
        val ordered = snapshot.groups.sortedBy { it.index }
        val position = ordered.indexOfFirst { it.index == selected.index }
        page.addView(ui.row().apply {
            if (position > 0) addView(ui.action(context.getString(R.string.pack_previous)) { selectedGroup = ordered[position - 1].index; rebuild() }, LinearLayout.LayoutParams(0, -2, 1f))
            if (position in 0 until ordered.lastIndex) addView(ui.action(context.getString(R.string.pack_next)) { selectedGroup = ordered[position + 1].index; rebuild() }, LinearLayout.LayoutParams(0, -2, 1f))
        })
    }

    private fun groupExtreme(title: String, index: UShort) = ui.action("", onClick = { selectedGroup = index; rebuild() }).apply {
        textSize = 14f
        bind { val g = group(index); text = "$title\n${g?.let(::groupVoltage) ?: "—"}" }
    }
    private fun emptyState(title: String, detail: String) { val c = card(); c.addView(ui.text(title, 19f, true, ui.orange)); c.addView(ui.space(8)); c.addView(ui.text(detail, 15f, color = ui.muted)) }
    private fun heading(value: String) {
        val label = ui.text(value, 17f, true)
        bind { label.text = bms?.let(::topologyLabel) ?: value }
        page.addView(label); page.addView(ui.space(4))
    }
    private fun sectionTitle(value: String) { page.addView(ui.text(value, 18f, true)) }
    private fun card() = ui.column().apply {
        setPadding(ui.dp(16), ui.dp(16), ui.dp(16), ui.dp(14)); background = ui.rounded(22)
        page.addView(this, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(12) })
    }
    private fun metric(parent: LinearLayout, label: String, value: () -> String?) {
        val row = ui.row(); row.addView(ui.text(label, 15f, color = ui.muted), LinearLayout.LayoutParams(0, -2, 1f))
        val reading = ui.text("", 20f, true).apply { gravity = Gravity.END; tag = "pack.metric.$label" }
        row.addView(reading); bind { reading.text = value() ?: "—" }
        parent.addView(row, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = ui.dp(10) })
    }
    private fun bind(update: () -> Unit) { bindings += update; update() }
    private fun groupName(g: MobileBmsGroupSnapshotDto) = g.label ?: context.getString(R.string.pack_group, g.index.toInt())
    private fun topologyLabel(snapshot: MobileBmsSnapshotDto): String = if (
        snapshot.topology.confidence == MobileBmsTopologyConfidenceDto.UNVERIFIED &&
        snapshot.topology.seriesGroupCount == null && snapshot.topology.parallelCount == null && snapshot.groups.isNotEmpty()
    ) context.getString(R.string.pack_unverified_topology, snapshot.observedGroupCount.toInt()) else snapshot.topology.layoutLabel
    private fun groupVoltage(g: MobileBmsGroupSnapshotDto) = g.voltage?.value?.value?.let { number(it.toDouble() / 1000, 3) + " V" }
    private fun alertColor(level: MobileBmsAlertLevelDto) = when (level) { MobileBmsAlertLevelDto.CRITICAL -> ui.red; MobileBmsAlertLevelDto.WARNING -> ui.orange; else -> ui.foreground }
    private fun number(value: Double, places: Int) = String.format(Locale.getDefault(), "%.${places}f", value)
    private fun currentText(value: BatteryCurrent): String = TelemetryValueFormatting.current(context, value)
}

/** Swift-equivalent cell overview bars with labeled voltage ticks and group indices. */
internal class PackVoltageBarChart(context: Context, private var groups: List<Pair<Int, Double>>) : View(context) {
    private val gridPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = CutoutUi(context).muted; strokeWidth = resources.displayMetrics.density; style = Paint.Style.STROKE
    }
    private val barPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = android.graphics.Color.rgb(10, 132, 255); style = Paint.Style.FILL
    }
    private val labelPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = CutoutUi(context).muted; textSize = 10f * resources.displayMetrics.scaledDensity
    }
    private val axisPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = CutoutUi(context).muted; textSize = 11f * resources.displayMetrics.scaledDensity; textAlign = Paint.Align.RIGHT
    }

    fun updateValues(next: List<Pair<Int, Double>>) { groups = next; invalidate() }

    /** Geometry is calculated from only the Rust-projected group observations. */
    internal fun barGeometry(width: Int, height: Int): List<RectF> {
        if (groups.isEmpty()) return emptyList()
        val density = resources.displayMetrics.density
        val left = 48f * density
        val right = width - 8f * density
        val top = 8f * density
        val bottom = height - 25f * density
        if (right <= left || bottom <= top) return emptyList()
        val low = (groups.minOf { it.second } - 0.005).coerceAtLeast(0.0)
        val high = groups.maxOf { it.second } + 0.005
        val span = (high - low).coerceAtLeast(0.001)
        val firstIndex = groups.minOf { it.first }
        val slotCount = groups.maxOf { it.first } - firstIndex + 1
        val slot = (right - left) / slotCount
        val barWidth = (slot * 0.58f).coerceAtMost(22f * density)
        val baseline = bottom
        return groups.map { group ->
            val center = left + slot * (group.first - firstIndex + 0.5f)
            val valueY = bottom - ((group.second - low) / span * (bottom - top)).toFloat()
            RectF(center - barWidth / 2f, valueY, center + barWidth / 2f, baseline)
        }
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        if (groups.isEmpty()) return
        val density = resources.displayMetrics.density
        val left = 48f * density
        val right = width - 8f * density
        val top = 8f * density
        val bottom = height - 25f * density
        if (right <= left || bottom <= top) return
        val low = (groups.minOf { it.second } - 0.005).coerceAtLeast(0.0)
        val high = groups.maxOf { it.second } + 0.005
        val span = (high - low).coerceAtLeast(0.001)
        val firstIndex = groups.minOf { it.first }
        val slotCount = groups.maxOf { it.first } - firstIndex + 1
        val slot = (right - left) / slotCount
        for (tick in 0..2) {
            val value = low + span * tick / 2.0
            val y = bottom - ((value - low) / span * (bottom - top)).toFloat()
            canvas.drawLine(left, y, right, y, gridPaint)
            canvas.drawText(String.format(Locale.getDefault(), "%.3f", value), left - 5f * density, y + 4f * density, axisPaint)
        }
        canvas.drawLine(left, top, left, bottom, gridPaint)
        canvas.drawLine(left, bottom, right, bottom, gridPaint)
        barGeometry(width, height).forEach { canvas.drawRoundRect(it, 2f * density, 2f * density, barPaint) }
        val labelIndexes = groups.indices.filter { index ->
            groups.size <= 4 || index == 0 || index == groups.lastIndex ||
                (groups[index].first - firstIndex) % ((slotCount + 2) / 3).coerceAtLeast(1) == 0
        }.distinct()
        labelIndexes.forEach { index ->
            val label = groups[index].first.toString()
            val center = left + slot * (groups[index].first - firstIndex + 0.5f)
            canvas.drawText(label, center - labelPaint.measureText(label) / 2f, height - 4f * density, labelPaint)
        }
    }
}

/** Detail history remains a line chart, matching Swift's per-reading trend. */
private class GroupVoltageChart(context: Context, private var values: List<Double>) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = android.graphics.Color.rgb(10, 132, 255); strokeWidth = resources.displayMetrics.density * 2f; style = Paint.Style.STROKE
    }
    fun updateValues(next: List<Double>) { values = next; invalidate() }
    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        if (values.size < 2) return
        val min = values.minOrNull()!!; val max = values.maxOrNull()!!
        val range = (max - min).takeIf { it > 0.000001 } ?: 0.01
        val left = paddingLeft + 4f; val right = width - paddingRight - 4f
        val top = paddingTop + 12f; val bottom = height - paddingBottom - 12f
        val path = android.graphics.Path()
        values.forEachIndexed { index, value ->
            val x = left + (right - left) * index / (values.lastIndex)
            val y = bottom - (value - min) / range * (bottom - top)
            if (index == 0) path.moveTo(x, y.toFloat()) else path.lineTo(x, y.toFloat())
        }
        canvas.drawPath(path, paint)
    }
}
