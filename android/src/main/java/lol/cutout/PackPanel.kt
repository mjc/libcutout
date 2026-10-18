package lol.cutout

import android.content.Context
import android.view.Gravity
import android.widget.GridLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import uniffi.cutout_mobile_ffi.*
import java.util.Locale

/** Observed pack fields only. Native rows retain identity as telemetry changes. */
class PackPanel(context: Context) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val page = ui.column()
    private val bindings = mutableListOf<() -> Unit>()
    private var latest = BlePresentation()
    private var selectedGroup: UShort? = null
    private var showCells = false
    private var renderedStructure: List<Any?>? = null
    private val verified get() = latest.session?.connection?.readiness == MobileConnectionReadinessDto.VERIFIED
    private val bms get() = latest.bms.takeIf { verified }
    private val telemetry get() = latest.telemetry.takeIf { verified }

    init {
        setBackgroundColor(ui.background)
        page.setPadding(ui.dp(24), ui.dp(20), ui.dp(24), ui.dp(28))
        addView(page)
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
    private fun structure(): List<Any?> = listOf(verified, selectedGroup, showCells, bms?.topology,
        bms?.groups?.map { it.index to it.label }, bms?.faults?.map { it.label }, bms?.groups?.isEmpty())
    private fun group(index: UShort) = bms?.groups?.firstOrNull { it.index == index }
    private fun battery(): BatteryLevelReading? = bms?.energyPercent ?: telemetry?.let { snapshot ->
        mobileRiderDashboardProjection(snapshot).dashboardMetrics
            .filterIsInstance<MobileRiderDashboardMetricDescriptorDto.BatteryLevel>().firstOrNull()?.reading
    }

    private fun rebuild() {
        renderedStructure = structure(); bindings.clear(); page.removeAllViews()
        page.addView(ui.title(context.getString(R.string.pack))); page.addView(ui.space(20))
        val selected = selectedGroup?.let(::group)
        if (selected != null) {
            val index = selected.index
            page.addView(ui.action(context.getString(R.string.back)) { selectedGroup = null; rebuild() })
            page.addView(ui.space(12))
            page.addView(ui.text(selected.label ?: context.getString(R.string.pack_group, index.toInt()), 24f, true))
            val card = card()
            metric(card, context.getString(R.string.voltage)) { group(index)?.voltage?.value?.value?.let { number(it.toDouble() / 1000, 3) + " V" } }
            metric(card, context.getString(R.string.temperature)) { group(index)?.temperature?.value?.value?.let { number(it.toDouble() / 1000, 1) + " °C" } }
            metric(card, context.getString(R.string.pack_balancing)) { group(index)?.isBalancing?.let { context.getString(if (it) R.string.on else R.string.off) } }
            val detail = ui.text("", 14f, color = ui.muted)
            card.addView(detail)
            bind { detail.text = group(index)?.detail.orEmpty() }
            card.addView(ui.space(12)); card.addView(ui.text(context.getString(R.string.pack_recent_voltage), 15f, true))
            val recent = ui.text("", 14f); card.addView(recent)
            bind { recent.text = group(index)?.recentVoltages.orEmpty().joinToString("   ") { number(it.value.toDouble() / 1000, 3) + " V" } }
            return
        }
        if (bms?.groups.isNullOrEmpty()) {
            card().apply {
                addView(ui.text(context.getString(R.string.pack_no_cells), 20f, true, ui.orange)); addView(ui.space(8))
                addView(ui.text(context.getString(R.string.pack_no_cells_detail), 15f, color = ui.muted))
            }
        } else {
            page.addView(ui.text(bms!!.topology.layoutLabel, 17f, true))
            page.addView(ui.text(context.getString(R.string.pack_observed_groups, bms!!.observedGroupCount.toInt()), 14f, color = ui.muted))
            page.addView(ui.space(12))
            page.addView(ui.row().apply {
                addView(ui.action(context.getString(R.string.pack_overview), !showCells) { showCells = false; rebuild() }, LinearLayout.LayoutParams(0, -2, 1f))
                addView(ui.action(context.getString(R.string.pack_cell_map), showCells) { showCells = true; rebuild() }, LinearLayout.LayoutParams(0, -2, 1f))
            })
        }
        if (!showCells || bms == null) {
            val pack = card()
            metric(pack, context.getString(R.string.battery)) { battery()?.let { (if (it.source == MobileValueSourceDto.ESTIMATED) "≈" else "") + "${it.value.value} %" } }
            metric(pack, context.getString(R.string.voltage)) { (bms?.voltage ?: telemetry?.voltage)?.value?.value?.let { number(it.toDouble() / 1000, 1) + " V" } }
            metric(pack, ui.label("bms.pack.current")) { (bms?.current ?: telemetry?.batteryCurrent)?.value?.let(::currentText) }
            metric(pack, context.getString(R.string.pack_delta)) { bms?.cellDelta?.value?.value?.let { "$it mV" } }
            metric(pack, context.getString(R.string.temperature)) { bms?.highestTemperature?.value?.value?.let { number(it.toDouble() / 1000, 1) + " °C" } }
            metric(pack, context.getString(R.string.pack_balancing)) { bms?.balancingSummary }
            metric(pack, context.getString(R.string.pack_faults)) { bms?.faultSummary }
            bms?.faults.orEmpty().forEachIndexed { index, fault ->
                val text = ui.text(fault.label, 15f); pack.addView(text)
                bind { text.setTextColor(bms?.faults?.getOrNull(index)?.alertLevel?.let(::alertColor) ?: ui.foreground) }
            }
        }
        bms?.takeIf { it.groups.isNotEmpty() }?.let { snapshot ->
            page.addView(ui.space(16)); page.addView(ui.text(context.getString(R.string.pack_cells), 17f, true))
            val grid = GridLayout(context).apply { columnCount = if (showCells) 4 else 2 }
            snapshot.groups.forEach { observedGroup ->
                val index = observedGroup.index
                val button = ui.action("") { selectedGroup = index; rebuild() }.apply { tag = "pack.group.$index" }
                bind {
                    val current = group(index)
                    val value = current?.voltage?.value?.value?.let { number(it.toDouble() / 1000, 3) + " V" } ?: "—"
                    button.text = "${current?.label ?: context.getString(R.string.pack_group, index.toInt())}\n$value"
                    button.setTextColor(current?.alertLevel?.let(::alertColor) ?: ui.foreground)
                }
                grid.addView(button, GridLayout.LayoutParams().apply {
                    width = 0; height = ui.dp(84); columnSpec = GridLayout.spec(GridLayout.UNDEFINED, 1f)
                    setMargins(ui.dp(3), ui.dp(3), ui.dp(3), ui.dp(3))
                })
            }
            page.addView(grid, LinearLayout.LayoutParams(-1, -2))
        }
    }
    private fun card() = ui.column().apply {
        background = ui.rounded(22); setPadding(ui.dp(16), ui.dp(16), ui.dp(16), ui.dp(16))
        page.addView(this, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(12) })
    }
    private fun metric(parent: LinearLayout, label: String, value: () -> String?) {
        val row = ui.row()
        row.addView(ui.text(label, 15f, color = ui.muted), LinearLayout.LayoutParams(0, -2, 1f))
        val reading = ui.text("", 22f, true).apply { gravity = Gravity.END; tag = "pack.metric.$label" }
        row.addView(reading)
        bind { reading.text = value() ?: "—" }
        parent.addView(row, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = ui.dp(12) })
    }
    private fun bind(update: () -> Unit) { bindings += update; update() }
    private fun alertColor(level: MobileBmsAlertLevelDto) = when (level) {
        MobileBmsAlertLevelDto.CRITICAL -> ui.red
        MobileBmsAlertLevelDto.WARNING -> ui.orange
        else -> ui.foreground
    }
    private fun number(value: Double, places: Int) = String.format(Locale.getDefault(), "%.${places}f", value)
    private fun currentText(value: BatteryCurrent): String = when (value) {
        BatteryCurrent.Zero -> "0.0 A"
        is BatteryCurrent.CurrentIn -> context.getString(R.string.pack_current_in, number(value.milliamps.toDouble() / 1000, 1))
        is BatteryCurrent.CurrentOut -> context.getString(R.string.pack_current_out, number(value.milliamps.toDouble() / 1000, 1))
        is BatteryCurrent.Unknown -> number(value.milliamps.toDouble() / 1000, 1) + " A ?"
    }
}
