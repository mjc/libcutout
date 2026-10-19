package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.widget.LinearLayout
import android.widget.ScrollView
import uniffi.cutout_mobile_ffi.*
import java.math.BigDecimal
import java.text.NumberFormat
import java.util.Locale

/** Independent reports and packet readback remain distinct; Rust compares their meaning. */
internal class CaptureSettingsReview(
    private val context: Context,
    private val load: ((Result<List<MobileCaptureSettingsObservationDto>>) -> Unit) -> Unit,
    private val readback: (List<MobileCaptureResearchReadbackRequestDto>, (Result<List<MobileCaptureResearchReadbackDto>>) -> Unit) -> Unit,
) {
    private val ui = CutoutUi(context)
    private val content = ui.column().apply { setPadding(ui.dp(20), ui.dp(12), ui.dp(20), ui.dp(20)) }
    private var dialog: AlertDialog? = null
    private var request = 0
    private var observations = emptyList<MobileCaptureSettingsObservationDto>()
    private var decoded = emptyList<MobileCaptureResearchReadbackDto>()
    private var loading = false
    private var error: String? = null
    private var loadFailed = false

    fun show(): AlertDialog {
        val scroll = ScrollView(context).apply { addView(content); tag = "capture.settings.review" }
        return AlertDialog.Builder(context).setTitle(ui.label("capture.settings.review.title"))
            .setView(scroll).setPositiveButton(android.R.string.ok, null).create().also { shown ->
                dialog = shown
                shown.setOnDismissListener { request++; dialog = null }
                shown.show()
                shown.window?.setBackgroundDrawable(ui.rounded(24, ui.background, outlined = false))
                reload()
            }
    }

    private fun reload() {
        val token = ++request
        loading = true; error = null; loadFailed = false; observations = emptyList(); decoded = emptyList()
        render()
        load { result ->
            if (dialog?.isShowing != true || request != token) return@load
            result.fold({ saved ->
                observations = saved
                if (saved.isEmpty()) { loading = false; render() }
                else decode(token)
            }, {
                loading = false; loadFailed = true; error = ui.label("capture.settings.load_error"); render()
            })
        }
    }

    private fun decode(token: Int = ++request) {
        loading = true; error = null; decoded = emptyList(); render()
        val requests = observations.map { MobileCaptureResearchReadbackRequestDto(it.profile.profileId, it.observedAtMs) }
        readback(requests) { result ->
            if (dialog?.isShowing != true || token != request) return@readback
            loading = false
            result.onSuccess { decoded = it }.onFailure { error = ui.label("capture.settings.read_error") }
            render()
        }
    }

    private fun render() {
        content.removeAllViews()
        if (loading) content.addView(ui.text(ui.label("capture.settings.review.loading"), color = ui.muted))
        error?.let {
            content.addView(ui.text(it, color = ui.orange).apply { tag = "capture.settings.review.error" })
            content.addView(ui.action(ui.label("capture.settings.retry_load")) {
                if (loadFailed) reload() else decode()
            }.apply { tag = "capture.settings.review.retry" })
        }
        if (!loading && error == null && observations.isEmpty()) {
            content.addView(ui.text(ui.label("capture.settings.review.empty"), color = ui.muted))
        }
        observations.forEachIndexed { index, observation ->
            val catalog = captureResearchCatalog(observation.profile.profileId)
            val results = decoded.getOrNull(index)?.takeIf {
                it.profileId == observation.profile.profileId && it.atMs == observation.observedAtMs
            }?.values.orEmpty()
            val unusual = captureResearchUnusualFields(observation)
            val title = if (index == 0) ui.label("capture.settings.baseline") else
                String.format(Locale.getDefault(), ui.label("capture.settings.observation_number").replace("%lld", "%d"), index + 1)
            content.addView(ui.space(16))
            content.addView(ui.text("$title · ${time(observation.observedAtMs)}", 18f, true).apply { isAccessibilityHeading = true })
            content.addView(ui.text(observation.profile.model ?: observation.profile.profileId, color = ui.muted))
            content.addView(ui.text("${source(observation.source)} · ${time(observation.observedAtMs)}", 14f, color = ui.muted))
            row("capture.settings.review.entered_at", time(observation.recordedAtMs))
            observation.values.forEach { reported ->
                val field = catalog?.fields?.firstOrNull { it.id == reported.settingId }
                val packet = results.firstOrNull { it.settingId == reported.settingId }
                content.addView(ui.space(12))
                content.addView(ui.text(field?.let { ui.label(it.labelKey) } ?: reported.settingId, 17f, true))
                row("capture.settings.review.recorded", reportedValue(reported, field))
                if (!loading && error == null) {
                    row("capture.settings.review.decoded", packet?.value?.let { decodedValue(it, field) }
                        ?: ui.label(if (packet == null) "capture.settings.not_available" else "settings.empty"))
                    packet?.let {
                        content.addView(ui.text("${ui.label("capture.settings.review.packet")} · ${time(it.observedAtMs)}", 13f, color = ui.muted))
                        it.value?.let { value ->
                            if (captureResearchValuesDiffer(observation.profile.profileId, reported, value) == true)
                                content.addView(ui.text(ui.label("capture.settings.review.differ"), 14f, color = ui.orange))
                        }
                    }
                }
                if (reported.settingId in unusual) content.addView(ui.text(ui.label("capture.settings.review.unusual"), 14f, color = ui.orange))
            }
            observation.note?.takeIf { it.isNotEmpty() }?.let { content.addView(ui.text(it)) }
        }
    }

    private fun row(key: String, value: String) {
        content.addView(ui.text("${ui.label(key)}: $value", 15f))
    }

    private fun source(value: MobileCaptureObservationSourceDto) = ui.label("capture.settings.source." + when (value) {
        MobileCaptureObservationSourceDto.MANUFACTURER_APP -> "manufacturer_app"
        MobileCaptureObservationSourceDto.WHEEL_DISPLAY -> "wheel_display"
        MobileCaptureObservationSourceDto.OTHER -> "other"
    })

    private fun reportedValue(reported: MobileCaptureSettingObservationValueDto, field: MobileCaptureResearchFieldDto?): String =
        when (val value = reported.value) {
            MobileCaptureObservedValueDto.Unobserved -> ui.label("capture.settings.not_observed")
            MobileCaptureObservedValueDto.Off -> ui.label("settings.choice.off")
            is MobileCaptureObservedValueDto.Boolean -> ui.label(if (value.value) "controls.on" else "settings.choice.off")
            is MobileCaptureObservedValueDto.Entered -> {
                val choice = (field?.control as? MobileSettingControlDto.Choices)?.choices?.firstOrNull { it.id.toString() == value.value }
                val text = choice?.let { ui.label(it.labelKey) } ?: value.value
                reported.unit?.let { "$text $it" } ?: text
            }
        }

    private fun decodedValue(value: MobileSettingValueDto, field: MobileCaptureResearchFieldDto?): String = when (value) {
        MobileSettingValueDto.Disabled -> ui.label("settings.choice.off")
        is MobileSettingValueDto.Boolean -> ui.label(if (value.value) "controls.on" else "settings.choice.off")
        is MobileSettingValueDto.Choice -> (field?.control as? MobileSettingControlDto.Choices)?.choices
            ?.firstOrNull { it.id == value.id }?.let { ui.label(it.labelKey) } ?: value.id.toString()
        is MobileSettingValueDto.Number -> {
            val number = field?.control as? MobileSettingControlDto.Number
            if (number == null) value.value.toString() else {
                val text = NumberFormat.getNumberInstance().apply {
                    minimumFractionDigits = number.precision.toInt(); maximumFractionDigits = number.precision.toInt()
                }.format(BigDecimal.valueOf(value.value.toLong(), number.precision.toInt()))
                val unit = CaptureResearchPresentation.defaultUnit(field)
                if (unit.isEmpty()) text else "$text $unit"
            }
        }
    }

    private fun time(ms: ULong): String = String.format(Locale.getDefault(), "%02d:%02d", (ms / 60_000UL).toLong(), (ms / 1000UL % 60UL).toLong())
}
