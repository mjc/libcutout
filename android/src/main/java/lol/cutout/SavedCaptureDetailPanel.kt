package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.text.format.Formatter
import android.view.View
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.io.File
import java.text.DateFormat
import java.util.Date
import java.util.Locale
import uniffi.cutout_mobile_ffi.MobileCaptureIntegrityDto
import uniffi.cutout_mobile_ffi.MobileLiveCaptureHistoryEntryDto

/** Detail for one Rust-retained database capture. Export and submission remain owner operations. */
class SavedCaptureDetailPanel(
    context: Context,
    private val entry: MobileLiveCaptureHistoryEntryDto,
    private val onExport: (String, (Result<File>) -> Unit) -> Unit,
    private val onShare: (File) -> Unit,
    submissionController: CaptureSubmissionController,
    private val onSettings: () -> Unit,
) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val content = ui.column()
    private val main = Handler(Looper.getMainLooper())
    private var disposed = false
    private var exporting = false
    private var exportError = false
    private var exportedFile: File? = null
    private var exportRequest = 0L
    private var built = false
    private lateinit var exportCard: LinearLayout
    private val submission = CaptureSubmissionPanel(context, submissionController)

    init {
        tag = "captures.detail"
        isFillViewport = true
        setBackgroundColor(ui.background)
        content.setPadding(ui.dp(20), ui.dp(16), ui.dp(20), ui.dp(24))
        addView(content, LayoutParams(-1, -2))
        render()
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        exportRequest++
        submission.dispose()
    }

    private fun render() {
        if (built) {
            renderExportCard()
            return
        }
        built = true
        section(null, "captures.database.header") {
            addView(ui.text(entry.recording?.advertisedName ?: entry.platformIdentifier, 20f, true).apply {
                isAccessibilityHeading = true
                tag = "captures.detail.device"
            })
            addView(ui.text(DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, Locale.getDefault())
                .format(Date(entry.startedAtMilliseconds.toLong())), 14f, color = ui.muted))
            if (entry.interrupted) {
                addView(ui.text(context.getString(R.string.capture_interrupted), 14f, color = ui.orange)
                    .apply { tag = "captures.detail.integrity" })
            } else when (entry.integrity) {
                MobileCaptureIntegrityDto.Complete -> Unit
                is MobileCaptureIntegrityDto.Incomplete -> addView(ui.text(ui.label("captures.incomplete"), 14f, color = ui.orange)
                    .apply { tag = "captures.detail.integrity" })
                MobileCaptureIntegrityDto.Unknown -> addView(ui.text(ui.label("captures.integrity_unknown"), 14f, color = ui.muted)
                    .apply { tag = "captures.detail.integrity" })
            }
        }
        section(null, "captures.database.metrics") {
            metric(this, ui.label("captures.events"), entry.eventCount.toString(), "captures.detail.events")
            addView(ui.divider())
            metric(this, ui.label("captures.stored_bytes"), Formatter.formatFileSize(context, entry.storedBytes.toLongSaturated()), "captures.detail.bytes")
            entry.recording?.model?.let { model ->
                addView(ui.divider())
                metric(this, ui.label("captures.model"), model.value, "captures.detail.model")
            }
        }
        section(null, "captures.database.export_card") {
            exportCard = this
        }
        renderExportCard()
        section("capture.settings.review.title", "captures.database.settings") {
            addView(ui.action(ui.label("capture.settings.review.title")) {
                if (!disposed) onSettings()
            }.apply { tag = "captures.detail.settings" })
        }
        section("capture.submission.review", "captures.database.submission") {
            addView(submission, LinearLayout.LayoutParams(-1, -2))
        }
    }

    private fun export() {
        if (disposed || exporting || exportedFile != null) return
        exporting = true
        exportError = false
        val request = ++exportRequest
        render()
        try {
            onExport(entry.liveCaptureId) { result ->
                main.post {
                    if (disposed || request != exportRequest) return@post
                    exportRequest++
                    exporting = false
                    result.fold(
                        onSuccess = { file ->
                            if (file.isFile) exportedFile = file else exportError = true
                        },
                        onFailure = { exportError = true },
                    )
                    render()
                }
            }
        } catch (_: Exception) {
            if (!disposed && request == exportRequest) {
                exportRequest++
                exporting = false
                exportError = true
                render()
            }
        }
    }

    private fun renderExportCard() {
        exportCard.removeAllViews()
        exportedFile?.let { file ->
            exportCard.addView(ui.action(ui.label("captures.share"), primary = true) {
                if (!disposed) onShare(file)
            }.apply { tag = "captures.detail.share" })
        } ?: run {
            val title = if (exporting) ui.label("captures.preparing") else ui.label("captures.export_recording")
            exportCard.addView(ui.action(title, primary = true, onClick = ::export).apply {
                tag = "captures.detail.export"
                isEnabled = !exporting
            })
        }
        if (exportError) exportCard.addView(ui.text(context.getString(R.string.capture_error), 14f, color = ui.orange)
            .apply { tag = "captures.detail.export_error" })
        exportCard.addView(ui.text(context.getString(R.string.capture_privacy), 13f, color = ui.muted).apply {
            setPadding(0, ui.dp(8), 0, 0)
        })
    }

    private fun section(titleKey: String?, tagValue: String, build: LinearLayout.() -> Unit) {
        val block = ui.column().apply {
            if (titleKey != null) {
                addView(ui.text(ui.label(titleKey), 20f, true).apply { isAccessibilityHeading = true })
                addView(ui.space(8))
            }
            addView(ui.column().apply {
                setPadding(ui.dp(16), ui.dp(14), ui.dp(16), ui.dp(14))
                background = ui.rounded(22)
                tag = tagValue
                build()
            })
        }
        content.addView(block, LayoutParams(-1, -2))
        content.addView(ui.space(18))
    }

    private fun metric(parent: LinearLayout, label: String, value: String, viewTag: String) {
        val row = ui.row().apply { tag = viewTag }
        row.addView(ui.text(label, 15f, color = ui.muted), LinearLayout.LayoutParams(0, -2, 1f))
        row.addView(ui.text(value, 16f, true).apply { gravity = android.view.Gravity.END })
        parent.addView(row, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(5); bottomMargin = ui.dp(5) })
    }

    private fun ULong.toLongSaturated(): Long = coerceAtMost(Long.MAX_VALUE.toULong()).toLong()
}
