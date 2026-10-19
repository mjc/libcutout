package lol.cutout

import android.content.Context
import android.view.Gravity
import android.view.View
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import uniffi.cutout_mobile_ffi.*
import java.util.Locale

/** Read-only VESC diagnostics rendered from the same admitted Rust snapshot as Ride. */
class VescDebugPanel(context: Context) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val page = ui.column()
    private val title = ui.title(ui.label("vesc.debug.title"))
    private val subtitle = ui.text("", 15f, color = ui.muted).apply { tag = "vesc.debug.subtitle" }
    private val metrics = ui.column()
    private val metricValues = linkedMapOf<String, TextView>()
    private val metricDetails = linkedMapOf<String, TextView>()
    private val rows = linkedMapOf<String, Pair<LinearLayout, TextView>>()

    init {
        setBackgroundColor(ui.background)
        page.setPadding(ui.dp(24), ui.dp(20), ui.dp(24), ui.dp(24))
        addView(page)
        page.addView(title)
        page.addView(subtitle, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
        listOf(listOf("duty", "headroom"), listOf("board", "controller")).forEach { keys ->
            val row = ui.row()
            keys.forEachIndexed { index, key ->
                val tile = ui.column().apply {
                    background = ui.rounded(22)
                    setPadding(ui.dp(16), ui.dp(14), ui.dp(16), ui.dp(14))
                    addView(ui.text(ui.label("vesc.debug.metric.$key"), 14f, color = ui.muted))
                }
                metricValues[key] = ui.text("—", 26f, true).apply {
                    tag = "vesc.debug.metric.$key"
                    tile.addView(this, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
                }
                metricDetails[key] = ui.text("", 12f, color = ui.muted).apply {
                    tile.addView(this, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
                }
                row.addView(tile, LinearLayout.LayoutParams(0, -2, 1f).apply { if (index > 0) marginStart = ui.dp(12) })
            }
            metrics.addView(row, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(12) })
        }
        page.addView(metrics, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        page.addView(ui.space(20))
        listOf("session", "protocol", "state", "notifications", "pack_voltage", "battery_current", "motor_current", "footpad").forEach { key ->
            val row = ui.row().apply { minimumHeight = ui.dp(48); setPadding(0, ui.dp(10), 0, ui.dp(10)) }
            row.addView(ui.text(ui.label("vesc.debug.row.$key"), 15f, color = ui.muted), LinearLayout.LayoutParams(0, -2, 1f))
            val value = ui.text("—", 16f, true).apply {
                tag = "vesc.debug.row.$key"
                gravity = Gravity.END
            }
            row.addView(value, LinearLayout.LayoutParams(0, -2, 1f))
            rows[key] = row to value
            page.addView(row)
        }
        render(BlePresentation())
    }

    /** Capture lifecycle is optional so this panel remains usable during connection-only renders. */
    fun render(presentation: BlePresentation, capture: DiagnosticCapturePresentation? = null) {
        val session = presentation.session
        val snapshot = presentation.telemetry.takeIf {
            session?.connection?.readiness == MobileConnectionReadinessDto.VERIFIED &&
                session.identity?.protocol == MobileProtocolFamilyDto.VESC
        }
        val phase = context.getString(when {
            presentation.problem -> R.string.connection_failed
            session?.connection?.readiness == MobileConnectionReadinessDto.VERIFIED -> R.string.connected
            presentation.phase == NativePhase.CONNECTING -> R.string.connecting
            presentation.phase == NativePhase.DISCOVERING -> R.string.discovering
            presentation.phase == NativePhase.IDENTIFYING -> R.string.identifying
            else -> R.string.disconnected
        })
        title.text = session?.identity?.model?.takeIf(String::isNotBlank) ?: ui.label("vesc.debug.title")
        subtitle.text = captureStatus(capture) ?: phase
        metrics.visibility = if (snapshot == null) View.GONE else View.VISIBLE
        rows.forEach { (key, row) -> row.first.visibility = if (key == "session" || snapshot != null) View.VISIBLE else View.GONE }
        row("session", phase)
        if (snapshot == null) return

        metric("duty", snapshot.pwm?.permille?.let { number(it.toDouble() / 10, 1) + " %" }, ui.label("vesc.debug.detail.motor_duty"))
        val headroom = mobileRiderDashboardProjection(snapshot).safetyMetrics
            .filterIsInstance<MobileRiderSafetyMetricDescriptorDto.PwmHeadroom>().firstOrNull()?.value as? MobileRiderHeadroomValueDto.Available
        metric("headroom", headroom?.let { number(it.permille.toDouble() / 10, 1) + " %" }, ui.label("vesc.debug.detail.remaining_duty"))
        metric("board", snapshot.pitch?.value?.value?.let { number(it.toDouble() / 1000, 1) + "°" },
            detail("vesc.debug.detail.balance", snapshot.balanceAngle?.value?.value?.let { number(it.toDouble() / 1000, 1) + "°" } ?: "—"))
        metric("controller", snapshot.controllerTemperature?.value?.value?.let { number(it.toDouble() / 1000, 0) + " °C" },
            detail("vesc.debug.detail.motor_temperature", snapshot.motorTemperature?.value?.value?.let { number(it.toDouble() / 1000, 0) + " °C" } ?: "—"))
        row("protocol", "VESC")
        row("state", ui.label("vesc.debug.state." + snapshot.operatingState.name.lowercase(Locale.ROOT)))
        row("notifications", presentation.notificationCount.toString())
        row("pack_voltage", snapshot.voltage?.value?.value?.let { number(it.toDouble() / 1000, 1) + " V" })
        row("battery_current", snapshot.batteryCurrent?.value?.let { TelemetryValueFormatting.current(context, it) })
        row("motor_current", snapshot.motorCurrent?.value?.value?.let { number(it.toDouble() / 1000, 1) + " A" })
        row("footpad", snapshot.footpad?.let { TelemetryValueFormatting.footpad(ui, it) })
    }

    private fun metric(key: String, value: String?, detail: String) {
        metricValues[key]?.text = value ?: "—"
        metricDetails[key]?.text = detail
    }
    private fun row(key: String, value: String?) { rows[key]?.second?.text = value ?: "—" }
    private fun number(value: Double, digits: Int) = String.format(Locale.getDefault(), "%.${digits}f", value)
    private fun detail(key: String, value: String) = String.format(Locale.getDefault(),
        ui.label(key).replace("%1\$@", "%1\$s").replace("%@", "%s"), value)

    /** Mirrors the capture-priority subtitle passed to Swift's VESC debug screen. */
    private fun captureStatus(capture: DiagnosticCapturePresentation?): String? {
        val attempt = capture?.lifecycle?.attempt ?: return null
        if (capture.writerStatus?.failed == true || attempt.stage in setOf(
                MobileCaptureStageDto.SAVE_FAILED, MobileCaptureStageDto.FAILED,
            )) return ui.label("captures.save_failed")
        return when (attempt.stage) {
            MobileCaptureStageDto.STARTING -> ui.label("captures.starting")
            MobileCaptureStageDto.SAVING, MobileCaptureStageDto.FINALIZING -> ui.label("captures.saving")
            MobileCaptureStageDto.RECORDING -> if (capture.notificationCount > 0UL)
                ui.label("captures.receiving") else ui.label("captures.waiting")
            MobileCaptureStageDto.SAVE_FAILED, MobileCaptureStageDto.FAILED -> ui.label("captures.save_failed")
            MobileCaptureStageDto.SAVED -> {
                val fileName = capture.fileName ?: capture.sessionArtifacts.lastOrNull()?.file?.name
                if (fileName == null) ui.label("capture.status.stored")
                else String.format(Locale.getDefault(), ui.label("capture.status.saved").replace("%@", "%s"), fileName)
            }
        }
    }
}
