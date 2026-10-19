package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.text.format.Formatter
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Button
import java.io.File
import java.text.DateFormat
import java.util.Date
import java.util.Locale
import uniffi.cutout_mobile_ffi.MobileCaptureOriginDto

/** Detail for a file-only artifact retained by the native capture owner for this app session. */
class ArtifactCaptureDetailPanel(
    context: Context,
    private val artifact: DiagnosticCaptureArtifact,
    private val onExport: (DiagnosticCaptureArtifact, (Result<File>) -> Unit) -> Unit,
    private val onShare: (File) -> Unit,
    submissionController: CaptureSubmissionController,
    private val onSettings: () -> Unit,
) : ScrollView(context) {
    private val ui = CutoutUi(context)
    val navigationTitle: String
        get() = ui.label(if (artifact.status.failed || artifact.status.droppedMessages > 0UL)
            "captures.incomplete" else "captures.saved")
    private val content = ui.column()
    private val main = Handler(Looper.getMainLooper())
    private val submission = CaptureSubmissionPanel(context, submissionController)
    private lateinit var exportCard: LinearLayout
    private lateinit var technicalContent: LinearLayout
    private var built = false
    private var disposed = false
    private var technicalExpanded = false
    private var exporting = false
    private var exportError = false
    private var sharedFile: File? = null
    private var exportRequest = 0L

    init {
        tag = "captures.artifact.detail"
        isFillViewport = true
        setBackgroundColor(ui.background)
        content.setPadding(ui.dp(20), ui.dp(16), ui.dp(20), ui.dp(24))
        addView(content, LayoutParams(-1, -2))
        buildContent()
        // Enter at the capture identity rather than jumping to a later child control.
        isFocusableInTouchMode = true
        descendantFocusability = ViewGroup.FOCUS_BEFORE_DESCENDANTS
        requestFocus()
        if (artifact.file.isFile) export()
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        exportRequest++
        submission.dispose()
    }

    private fun buildContent() {
        if (built) return
        built = true
        section(null) {
            addView(ui.text(artifact.deviceName?.takeIf(String::isNotBlank)
                ?: context.getString(R.string.capture_untitled), 20f, true).apply {
                isAccessibilityHeading = true
                tag = "captures.artifact.device"
            })
            addView(ui.text(DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, Locale.getDefault())
                .format(Date(artifact.startedAtUnixMillis.toLong())), 14f, color = ui.muted))
            val source = if (artifact.origin == MobileCaptureOriginDto.MANUAL)
                ui.label("captures.manual") else ui.label("captures.automatic")
            addView(detailRow(ui.label("captures.source"), source, "captures.artifact.source"))
            artifact.description?.takeIf(String::isNotBlank)?.let {
                addView(ui.text(it, 14f, color = ui.muted).apply { tag = "captures.artifact.description" })
            }
            artifact.error?.let {
                addView(ui.text(it, 14f, color = ui.orange).apply { tag = "captures.artifact.error" })
            }
        }
        section(null) {
            lateinit var disclosure: Button
            disclosure = ui.action(ui.label("captures.technical")) {
                if (!disposed) {
                    technicalExpanded = !technicalExpanded
                    technicalContent.visibility = if (technicalExpanded) View.VISIBLE else View.GONE
                    disclosure.isSelected = technicalExpanded
                }
            }.apply {
                tag = "captures.artifact.technical.toggle"
                isSelected = technicalExpanded
            }
            addView(disclosure)
            technicalContent = ui.column().apply {
                visibility = View.GONE
                tag = "captures.artifact.technical"
                addView(detailRow(ui.label("capture.detail.elapsed"), duration(artifact.elapsedMillis), "captures.artifact.elapsed"))
                addView(detailRow(ui.label("capture.detail.packets"), artifact.notificationCount.toString(), "captures.artifact.packets"))
                addView(detailRow(ui.label("capture.detail.file_size"),
                    Formatter.formatFileSize(context, artifact.status.bytesWritten.toLongSaturated()), "captures.artifact.bytes"))
                addView(detailRow(ui.label("capture.detail.pending_writes"), artifact.status.queuedMessages.toString(), "captures.artifact.pending"))
                val writerHealth = ui.label(if (artifact.status.failed)
                    "capture.detail.writer.failed" else "capture.detail.writer.healthy")
                addView(detailRow(ui.label("capture.detail.writer"), writerHealth, "captures.artifact.writer"))
                addView(detailRow(context.getString(R.string.capture_platform_identity), artifact.platformIdentifier,
                    "captures.artifact.platform_id"))
                addView(ui.text(artifact.file.name, 13f, color = ui.muted).apply {
                    tag = "captures.artifact.filename"
                    typeface = android.graphics.Typeface.MONOSPACE
                    setTextIsSelectable(true)
                })
                artifact.status.lastError?.let { addView(ui.text(it, 14f, color = ui.orange)) }
            }
            addView(technicalContent)
        }
        section(null) { exportCard = this }
        renderExportCard()
        section(null) {
            addView(ui.action(ui.label("capture.settings.review.title")) {
                if (!disposed) onSettings()
            }.apply { tag = "captures.artifact.settings" })
        }
        section(null) {
            addView(submission, LinearLayout.LayoutParams(-1, -2))
        }
    }

    private fun export() {
        if (disposed || exporting || sharedFile != null || !artifact.file.isFile) return
        exporting = true
        exportError = false
        val request = ++exportRequest
        renderExportCard()
        try {
            onExport(artifact) { result ->
                main.post {
                    if (disposed || request != exportRequest) return@post
                    exportRequest++
                    exporting = false
                    result.fold(
                        onSuccess = { file ->
                            if (file.isFile) sharedFile = file else exportError = true
                        },
                        onFailure = { exportError = true },
                    )
                    renderExportCard()
                }
            }
        } catch (_: Exception) {
            if (!disposed && request == exportRequest) {
                exportRequest++
                exporting = false
                exportError = true
                renderExportCard()
            }
        }
    }

    private fun renderExportCard() {
        if (!::exportCard.isInitialized) return
        exportCard.removeAllViews()
        if (!artifact.file.isFile) {
            exportCard.addView(ui.text(ui.label("captures.file_unavailable"), 15f, color = ui.muted)
                .apply { tag = "captures.artifact.file_unavailable" })
            addPrivacy()
            return
        }
        val file = sharedFile
        if (file != null) {
            exportCard.addView(ui.action(ui.label("captures.share"), primary = true) {
                if (!disposed) onShare(file)
            }.apply { tag = "captures.artifact.share" })
        } else {
            exportCard.addView(ui.action(ui.label(if (exporting) "captures.preparing" else "captures.retry"), primary = true) {
                export()
            }.apply {
                tag = if (exporting) "captures.artifact.preparing" else "captures.artifact.export.retry"
                isEnabled = !exporting
            })
        }
        if (exportError) exportCard.addView(ui.text(context.getString(R.string.capture_error), 14f, color = ui.orange)
            .apply { tag = "captures.artifact.export.error" })
        addPrivacy()
    }

    private fun addPrivacy() {
        exportCard.addView(ui.text(context.getString(R.string.capture_privacy), 13f, color = ui.muted).apply {
            setPadding(0, ui.dp(8), 0, 0)
        })
    }

    private fun section(titleKey: String?, build: LinearLayout.() -> Unit) {
        val block = ui.column().apply {
            if (titleKey != null) {
                addView(ui.text(ui.label(titleKey), 20f, true).apply { isAccessibilityHeading = true })
                addView(ui.space(8))
            }
            addView(ui.column().apply {
                setPadding(ui.dp(16), ui.dp(14), ui.dp(16), ui.dp(14))
                background = ui.rounded(22)
                build()
            })
        }
        content.addView(block, LayoutParams(-1, -2))
        content.addView(ui.space(18))
    }

    private fun detailRow(label: String, value: String, rowTag: String): View = ui.row().apply {
        tag = rowTag
        setPadding(0, ui.dp(4), 0, ui.dp(4))
        addView(ui.text(label, 14f, color = ui.muted), LinearLayout.LayoutParams(0, -2, 1f))
        addView(ui.text(value, 14f, true).apply {
            gravity = android.view.Gravity.END or android.view.Gravity.CENTER_VERTICAL
            setTextIsSelectable(true)
        }, LinearLayout.LayoutParams(0, -2, 1.3f))
    }

    private fun duration(milliseconds: ULong): String {
        val totalSeconds = (milliseconds / 1000UL).coerceAtMost(Long.MAX_VALUE.toULong()).toLong()
        val hours = totalSeconds / 3600
        val minutes = (totalSeconds % 3600) / 60
        val seconds = totalSeconds % 60
        return if (hours > 0) String.format(Locale.getDefault(), "%d:%02d:%02d", hours, minutes, seconds)
        else String.format(Locale.getDefault(), "%d:%02d", minutes, seconds)
    }

    private fun ULong.toLongSaturated(): Long = coerceAtMost(Long.MAX_VALUE.toULong()).toLong()
}
