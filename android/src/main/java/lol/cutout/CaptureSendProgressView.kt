package lol.cutout

import android.content.Context
import android.content.res.ColorStateList
import android.os.Handler
import android.os.Looper
import android.view.View
import android.widget.LinearLayout
import android.widget.ProgressBar
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionProgressDto
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionProgressListener
import uniffi.cutout_mobile_ffi.MobileCaptureSubmissionProgressPhaseDto

/** Displays transport byte counts; only the operation's receipt can complete Send. */
internal class CaptureSendProgressView(context: Context) : LinearLayout(context) {
    private val ui = CutoutUi(context)
    private val main = Handler(Looper.getMainLooper())
    private var generation = 0L
    private val label = ui.text("", 14f, color = ui.muted).apply {
        tag = "captures.send.progress.label"
        accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE
    }
    private val bar = ProgressBar(context, null, android.R.attr.progressBarStyleHorizontal).apply {
        tag = "captures.send.progress.bar"
        max = 10_000
        progressTintList = ColorStateList.valueOf(ui.brand)
        indeterminateTintList = ColorStateList.valueOf(ui.brand)
    }
    private val bytes = ui.text("", 13f, color = ui.muted).apply { tag = "captures.send.progress.bytes" }
    private val receiptSpinner = ProgressBar(context).apply {
        tag = "captures.send.progress.receipt"
        isIndeterminate = true
        indeterminateTintList = ColorStateList.valueOf(ui.brand)
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        visibility = View.GONE
    }

    init {
        orientation = VERTICAL
        tag = "captures.send.progress"
        addView(ui.row().apply {
            addView(label, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            addView(receiptSpinner, LayoutParams(ui.dp(18), ui.dp(18)))
        })
        addView(bar, LayoutParams(LayoutParams.MATCH_PARENT, ui.dp(16)))
        addView(bytes)
        visibility = View.GONE
    }

    fun begin(operationId: String): MobileCaptureSubmissionProgressListener {
        val action = ++generation
        var boundOperationId = operationId
        var sawProgress = false
        var attemptId: ULong? = null
        var lastBytes = 0uL
        visibility = View.VISIBLE
        label.text = "Starting Send"
        bytes.text = ""
        receiptSpinner.visibility = View.GONE
        bar.isIndeterminate = true
        bar.contentDescription = label.text
        return object : MobileCaptureSubmissionProgressListener {
            override fun onProgress(progress: MobileCaptureSubmissionProgressDto) {
                val snapshot = progress.copy()
                main.post {
                    if (generation != action) return@post
                    if (!sawProgress) {
                        boundOperationId = snapshot.operationId
                        sawProgress = true
                    }
                    if (snapshot.operationId != boundOperationId) return@post
                    if (attemptId == null) attemptId = snapshot.attemptId
                    if (snapshot.attemptId != attemptId || snapshot.bytesSent < lastBytes) return@post
                    lastBytes = snapshot.bytesSent
                    render(snapshot)
                }
            }
        }
    }

    private fun render(value: MobileCaptureSubmissionProgressDto) {
        visibility = View.VISIBLE
        val fraction = if (value.totalBytes == 0uL) 0.0 else
            value.bytesSent.toDouble() / value.totalBytes.toDouble()
        val percent = (fraction * 100).toInt()
        val verifying = value.phase == MobileCaptureSubmissionProgressPhaseDto.VERIFYING_RECEIPT
        bar.isIndeterminate = false
        bar.progress = (fraction * bar.max).toInt()
        label.text = if (verifying) "Verifying receipt…" else "Sending · $percent%"
        bytes.text = "${value.bytesSent} / ${value.totalBytes} bytes"
        receiptSpinner.visibility = if (verifying) View.VISIBLE else View.GONE
        bar.contentDescription = "${label.text} · ${bytes.text}"
    }

    fun clear() {
        generation++
        visibility = View.GONE
        label.text = ""
        bytes.text = ""
        receiptSpinner.visibility = View.GONE
        bar.isIndeterminate = false
        bar.progress = 0
        bar.contentDescription = null
    }
}
