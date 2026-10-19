package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.text.format.Formatter
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import uniffi.cutout_mobile_ffi.*
import java.util.Locale

/** Native capture chrome. Rust lifecycle stages remain authoritative for state and completion. */
internal class ActiveCapturePanel(
    context: Context,
    private val finish: (MobileCaptureGenerationDto, (Boolean) -> Unit) -> Unit,
    private val onFinishRejected: () -> Unit,
) {
    private val ui = CutoutUi(context)
    private val main = Handler(Looper.getMainLooper())
    val header = ui.column().apply { tag = "captures.active.identity" }
    val technical = ui.column().apply { tag = "captures.technical.content" }
    val footer = ui.column().apply {
        tag = "captures.active.footer"
        setPadding(ui.dp(18), ui.dp(10), ui.dp(18), ui.dp(12))
        setBackgroundColor(ui.background)
    }

    private val identity = ui.text(ui.label("capture.session"), 22f, true).apply {
        isAccessibilityHeading = true
        tag = "captures.active.kind"
    }
    private val advertisedName = ui.text("", 15f, color = ui.muted).apply { tag = "captures.active.device" }
    private val elapsed = ui.text("—", 34f, true).apply {
        typeface = android.graphics.Typeface.create("sans-serif-monospace", android.graphics.Typeface.NORMAL)
        tag = "captures.active.elapsed"
    }
    private val status = ui.text("", 15f, true).apply {
        gravity = Gravity.CENTER_VERTICAL
        setPadding(ui.dp(12), ui.dp(8), ui.dp(12), ui.dp(8))
        tag = "captures.active.status"
    }
    private lateinit var technicalButton: Button
    private val technicalContent = ui.column().apply {
        visibility = View.GONE
        addView(metric(ui.label("capture.detail.file_size"), "—", "captures.active.bytes"))
        addView(metric(ui.label("capture.detail.pending_writes"), "—", "captures.active.queued"))
        addView(metric(ui.label("capture.detail.packets"), "—", "captures.active.packets"))
        addView(metric(ui.label("capture.detail.writer"), "—", "captures.active.writer"))
        addView(ui.text("", 14f, color = ui.red).apply {
            tag = "captures.active.writer.error"
            visibility = View.GONE
        })
    }
    private val metricValues = mapOf(
        "captures.active.bytes" to technicalContent.getChildAt(0).findViewWithTag<TextView>("captures.active.bytes.value"),
        "captures.active.queued" to technicalContent.getChildAt(1).findViewWithTag<TextView>("captures.active.queued.value"),
        "captures.active.packets" to technicalContent.getChildAt(2).findViewWithTag<TextView>("captures.active.packets.value"),
        "captures.active.writer" to technicalContent.getChildAt(3).findViewWithTag<TextView>("captures.active.writer.value"),
    )
    private val writerError = technicalContent.findViewWithTag<TextView>("captures.active.writer.error")
    private val saveButton = ui.action(ui.label("capture.stop"), primary = true) { requestFinish() }.apply {
        tag = "capture.stop"
    }
    private val savingProgress = ProgressBar(context).apply {
        isIndeterminate = true
        indeterminateTintList = android.content.res.ColorStateList.valueOf(android.graphics.Color.BLACK)
        visibility = View.GONE
        tag = "capture.saving.progress"
    }
    private val finishRow = FrameLayout(context).apply {
        addView(saveButton, FrameLayout.LayoutParams(-1, -2))
        addView(savingProgress, FrameLayout.LayoutParams(ui.dp(22), ui.dp(22), Gravity.START or Gravity.CENTER_VERTICAL).apply {
            marginStart = ui.dp(16)
        })
    }
    private val automaticHint = ui.text(ui.label("captures.automatic_hint"), 14f, color = ui.muted).apply {
        tag = "captures.automatic.hint"
    }
    private val leaveHint = ui.text(ui.label("captures.leave_hint"), 13f, color = ui.muted).apply {
        tag = "captures.leave_hint"
        gravity = Gravity.CENTER
        setPadding(0, ui.dp(8), 0, 0)
    }

    private var current: DiagnosticCapturePresentation? = null
    private var currentGeneration: MobileCaptureGenerationDto? = null
    private var finishingGeneration: MobileCaptureGenerationDto? = null
    private var finishAccepted = false
    private var technicalExpanded = false
    private var disposed = false
    private var attached = false

    init {
        header.addView(identity)
        header.addView(advertisedName.apply { visibility = View.GONE })
        header.addView(elapsed.apply {
            contentDescription = ui.label("capture.detail.elapsed")
            setPadding(0, ui.dp(5), 0, ui.dp(5))
        })
        header.addView(status)
        header.addView(ui.space(8))

        technicalButton = ui.action(ui.label("captures.technical")) {
            if (!disposed) {
                technicalExpanded = !technicalExpanded
                technicalContent.visibility = if (technicalExpanded) View.VISIBLE else View.GONE
                technicalButton.isSelected = technicalExpanded
            }
        }.apply { tag = "captures.technical" }
        technical.addView(technicalButton)
        technical.addView(technicalContent)

        footer.addView(ui.divider())
        footer.addView(finishRow, LinearLayout.LayoutParams(-1, -2).apply {
            topMargin = ui.dp(8)
        })
        footer.addView(automaticHint)
        footer.addView(leaveHint)
    }

    fun render(value: DiagnosticCapturePresentation, showFooter: Boolean) {
        if (disposed) return
        val oldGeneration = currentGeneration
        current = value
        val attempt = value.lifecycle.attempt
        currentGeneration = attempt?.generation?.copy()
        if (oldGeneration != currentGeneration) {
            finishingGeneration = null
            finishAccepted = false
        }
        val generation = currentGeneration
        if (generation != null && finishingGeneration == generation && finishAccepted &&
            attempt?.stage in setOf(MobileCaptureStageDto.SAVE_FAILED, MobileCaptureStageDto.SAVED, MobileCaptureStageDto.FAILED)
        ) {
            finishingGeneration = null
            finishAccepted = false
        }

        identity.text = value.deviceKind?.takeIf(String::isNotBlank) ?: ui.label("capture.session")
        val name = value.deviceName?.takeIf(String::isNotBlank)
        advertisedName.text = name.orEmpty()
        advertisedName.visibility = if (name == null || name == identity.text) View.GONE else View.VISIBLE
        updateElapsed(android.os.SystemClock.elapsedRealtime().toULong())
        val saving = attempt?.stage in setOf(MobileCaptureStageDto.SAVING, MobileCaptureStageDto.FINALIZING)
        val writerFailed = value.writerStatus?.failed == true || attempt?.stage in setOf(
            MobileCaptureStageDto.SAVE_FAILED, MobileCaptureStageDto.FAILED,
        )
        val statusText = when {
            writerFailed -> ui.label("captures.save_failed")
            saving -> ui.label("captures.saving")
            attempt?.stage == MobileCaptureStageDto.STARTING -> ui.label("captures.starting")
            value.notificationCount > 0UL -> ui.label("captures.receiving")
            else -> ui.label("captures.waiting")
        }
        status.text = statusText
        status.setTextColor(if (writerFailed) ui.red else if (saving) ui.brand else ui.green)
        status.background = ui.rounded(12, ui.card, outlined = false)
        status.contentDescription = statusText
        updateTechnical(value)

        val manual = attempt?.origin == MobileCaptureOriginDto.MANUAL
        val canFinish = manual && attempt?.stage in setOf(MobileCaptureStageDto.RECORDING, MobileCaptureStageDto.SAVE_FAILED)
        val finishPending = generation != null && finishingGeneration == generation
        val isSaving = saving || finishPending
        saveButton.text = if (isSaving) ui.label("captures.saving") else ui.label("capture.stop")
        saveButton.isEnabled = canFinish && !isSaving
        savingProgress.visibility = if (isSaving) View.VISIBLE else View.GONE
        finishRow.visibility = if (manual && attempt?.stage != MobileCaptureStageDto.STARTING) View.VISIBLE else View.GONE
        automaticHint.visibility = if (!manual) View.VISIBLE else View.GONE
        footer.visibility = if (showFooter) View.VISIBLE else View.GONE
        attached = showFooter
    }

    fun updateElapsed(nowElapsedMillis: ULong) {
        if (disposed) return
        val value = current ?: return
        val start = value.startedElapsedMillis
        if (start == null) {
            elapsed.text = "—"
            elapsed.contentDescription = "${ui.label("capture.detail.elapsed")}: —"
            return
        }
        val end = value.finishedElapsedMillis ?: nowElapsedMillis
        val elapsedMillis = if (end >= start) end - start else 0UL
        val totalSeconds = (elapsedMillis / 1000UL).coerceAtMost(Long.MAX_VALUE.toULong()).toLong()
        val hours = totalSeconds / 3600
        val minutes = totalSeconds % 3600 / 60
        val seconds = totalSeconds % 60
        val formatted = if (hours > 0) String.format(Locale.getDefault(), "%d:%02d:%02d", hours, minutes, seconds)
            else String.format(Locale.getDefault(), "%d:%02d", minutes, seconds)
        elapsed.text = formatted
        elapsed.contentDescription = "${ui.label("capture.detail.elapsed")}: $formatted"
    }

    fun isAttachedToActiveRoute(): Boolean = attached && !disposed

    fun dispose() {
        disposed = true
        attached = false
        current = null
        finishingGeneration = null
    }

    private fun requestFinish() {
        val value = current ?: return
        val attempt = value.lifecycle.attempt ?: return
        val expected = attempt.generation.copy()
        if (disposed || !attached || finishingGeneration == expected || attempt.origin != MobileCaptureOriginDto.MANUAL ||
            attempt.stage !in setOf(MobileCaptureStageDto.RECORDING, MobileCaptureStageDto.SAVE_FAILED)
        ) return
        // Stale view events are rejected before forwarding the request to the Rust-owned client.
        if (currentGeneration != expected) return
        finishingGeneration = expected
        finishAccepted = false
        saveButton.isEnabled = false
        saveButton.text = ui.label("captures.saving")
        savingProgress.visibility = View.VISIBLE
        finish(expected) { admitted ->
            main.post {
                if (disposed || currentGeneration != expected || finishingGeneration != expected) return@post
                if (!admitted) {
                    finishingGeneration = null
                    finishAccepted = false
                    current?.let { render(it, attached) }
                    onFinishRejected()
                } else {
                    finishAccepted = true
                    current?.let { render(it, attached) }
                }
            }
        }
    }

    private fun updateTechnical(value: DiagnosticCapturePresentation) {
        val writerStatus = value.writerStatus
        val values = mapOf(
            "captures.active.bytes" to (writerStatus?.let {
                Formatter.formatFileSize(technical.context, it.bytesWritten.coerceAtMost(Long.MAX_VALUE.toULong()).toLong())
            } ?: "—"),
            "captures.active.queued" to (writerStatus?.queuedMessages?.toString() ?: "—"),
            "captures.active.packets" to value.notificationCount.toString(),
            "captures.active.writer" to ui.label(if (writerStatus?.failed == true) "capture.detail.writer.failed" else "capture.detail.writer.healthy"),
        )
        values.forEach { (tag, text) -> metricValues[tag]?.text = text }
        writerError?.apply {
            text = writerStatus?.lastError.orEmpty()
            visibility = if (text.isNullOrBlank()) View.GONE else View.VISIBLE
        }
    }

    private fun metric(label: String, value: String, tag: String): LinearLayout = ui.row().apply {
        setPadding(0, ui.dp(4), 0, ui.dp(4))
        addView(ui.text(label, 14f, color = ui.muted), LinearLayout.LayoutParams(0, -2, 1f))
        addView(ui.text(value, 14f, true).apply {
            this.tag = "$tag.value"
            gravity = Gravity.END or Gravity.CENTER_VERTICAL
            setTextIsSelectable(true)
        }, LinearLayout.LayoutParams(0, -2, 1f))
    }
}

