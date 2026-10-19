package lol.cutout

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.view.Gravity
import android.view.View
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
        if (renderedStructure != structure()) rebuild() else bindings.forEach { it() }
    }

    fun handleBack(): Boolean {
        if (selectedGroup != null) { selectedGroup = null; rebuild(); return true }
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
                g.index, g.packNumber, g.label, g.voltage != null, g.alertLevel,
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
        if (snapshot == null && verified) renderControllerMeasurements()
    }

    private fun renderControllerMeasurements() {
        val card = card()
        card.addView(ui.text(context.getString(R.string.pack_controller_readings), 17f, true))
        metric(card, context.getString(R.string.battery)) {
            telemetry?.let(::mobileRiderDashboardProjection)?.dashboardMetrics
                ?.filterIsInstance<MobileRiderDashboardMetricDescriptorDto.BatteryLevel>()?.firstOrNull()?.reading
                ?.let { (if (it.source == MobileValueSourceDto.ESTIMATED) "≈" else "") + "${it.value.value} %" }
        }
        metric(card, context.getString(R.string.voltage)) { telemetry?.voltage?.value?.value?.let { number(it.toDouble() / 1000, 2) + " V" } }
        metric(card, ui.label("bms.pack.current")) { telemetry?.batteryCurrent?.value?.let(::currentText) }
    }

    private fun renderMain(snapshot: MobileBmsSnapshotDto?) {
        when {
            !verified -> emptyState(context.getString(R.string.pack_no_readback), context.getString(R.string.pack_no_readback_detail))
            snapshot == null -> emptyState(context.getString(R.string.pack_no_readback), context.getString(R.string.pack_waiting_detail))
            snapshot.availability == MobileReadbackAvailabilityDto.UNSUPPORTED -> emptyState(context.getString(R.string.pack_unsupported), context.getString(R.string.pack_unsupported_detail))
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
                    addView(ui.action(context.getString(R.string.pack_overview), !showCells) { showCells = false; rebuild() }, LinearLayout.LayoutParams(0, -2, 1f))
                    addView(ui.action(context.getString(R.string.pack_cell_map), showCells) { showCells = true; rebuild() }, LinearLayout.LayoutParams(0, -2, 1f))
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
            page.addView(ui.action(context.getString(R.string.pack_warning, groupName(g))) {
                selectedGroup = g.index; rebuild()
            }.apply { setTextColor(if (g.alertLevel == MobileBmsAlertLevelDto.CRITICAL) ui.red else ui.orange) })
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
        val grid = GridLayout(context).apply { columnCount = 3 }
        groups.forEach { g ->
            val index = g.index
            val button = ui.action("", onClick = { selectedGroup = index; rebuild() }).apply {
                tag = "pack.group.${g.index}"; textSize = 14f; gravity = Gravity.CENTER; minHeight = ui.dp(64)
            }
            bind {
                val current = group(index)
                button.text = if (current == null) "${groupName(g)}\n—" else "${groupName(current)}\n${groupVoltage(current) ?: "—"}"
                button.setTextColor(current?.let { alertColor(it.alertLevel) } ?: ui.muted)
                button.contentDescription = context.getString(R.string.pack_group_accessibility,
                    current?.let(::groupName) ?: groupName(g), current?.let(::groupVoltage) ?: context.getString(R.string.unavailable))
            }
            grid.addView(button, GridLayout.LayoutParams().apply {
                width = 0; height = ui.dp(74); columnSpec = GridLayout.spec(GridLayout.UNDEFINED, 1f)
                setMargins(ui.dp(3), ui.dp(3), ui.dp(3), ui.dp(3))
            })
        }
        parent.addView(grid, LinearLayout.LayoutParams(-1, -2))
    }

    private fun renderDetail(snapshot: MobileBmsSnapshotDto, selected: MobileBmsGroupSnapshotDto) {
        val selectedIndex = selected.index
        fun current() = group(selectedIndex)
        page.addView(ui.action(context.getString(R.string.back)) { selectedGroup = null; showCells = true; rebuild() })
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
