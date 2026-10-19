package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.view.View
import android.widget.LinearLayout
import uniffi.cutout_mobile_ffi.MobileCaptureSettingsObservationDto
import java.text.NumberFormat

/** View-scoped loading and navigation for observations admitted by the capture owner. */
internal class CaptureSettingsObservationSummary(
    context: Context,
    private val onLoad: ((Result<List<MobileCaptureSettingsObservationDto>>) -> Unit) -> Unit,
    private val onReview: () -> Unit,
) : LinearLayout(context) {
    private val ui = CutoutUi(context)
    private val main = Handler(Looper.getMainLooper())
    private val loadingText = ui.text(ui.label("capture.settings.review.loading"), 14f, color = ui.muted)
    private val errorText = ui.text(ui.label("capture.settings.load_error"), 14f, color = ui.orange)
    private val retry = ui.action(ui.label("capture.settings.retry_load")) { refresh() }
    private val count = ui.text("", 17f, color = ui.muted)
    private val review = ui.navigationRow(ui.label("capture.settings.review.title"), CutoutSymbol.SETTINGS) {
        if (!disposed && observations.isNotEmpty()) onReview()
    }
    private var observations = emptyList<MobileCaptureSettingsObservationDto>()
    private var loading = false
    private var failed = false
    private var disposed = false
    private var request = 0L

    init {
        orientation = VERTICAL
        tag = "capture.settings.summary"
        loadingText.tag = "capture.settings.summary.loading"
        errorText.tag = "capture.settings.load-error"
        retry.tag = "capture.settings.retry-load"
        review.tag = "capture.settings.observations"
        count.setPadding(0, 0, ui.dp(8), 0)
        count.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        review.addView(count, 2, LayoutParams(-2, -2))
        addView(loadingText)
        addView(errorText)
        addView(retry)
        addView(review)
        updateViews()
    }

    fun refresh() {
        if (disposed || loading) return
        val token = ++request
        loading = true
        failed = false
        updateViews()
        try {
            onLoad { result ->
                onMain {
                    if (disposed || token != request) return@onMain
                    request++
                    loading = false
                    result.fold(
                        onSuccess = { observations = it.toList(); failed = false },
                        onFailure = { failed = true },
                    )
                    updateViews()
                }
            }
        } catch (_: Exception) {
            if (!disposed && token == request) {
                request++
                loading = false
                failed = true
                updateViews()
            }
        }
    }

    /** A newer owner receipt supersedes an outstanding older read. */
    fun render(values: List<MobileCaptureSettingsObservationDto>) {
        if (disposed || values == observations) return
        request++
        loading = false
        failed = false
        observations = values.toList()
        updateViews()
    }

    /** A new route or capture must not consume the previous route's asynchronous read. */
    fun reset(values: List<MobileCaptureSettingsObservationDto> = emptyList()) {
        if (disposed) return
        request++
        loading = false
        failed = false
        observations = values.toList()
        updateViews()
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        request++
        loading = false
        review.isEnabled = false
        retry.isEnabled = false
    }

    private fun updateViews() {
        loadingText.visibility = if (loading) View.VISIBLE else View.GONE
        errorText.visibility = if (failed) View.VISIBLE else View.GONE
        retry.visibility = if (failed) View.VISIBLE else View.GONE
        retry.isEnabled = !disposed && !loading
        review.visibility = if (observations.isNotEmpty()) View.VISIBLE else View.GONE
        review.isEnabled = !disposed && observations.isNotEmpty()
        val formatted = NumberFormat.getIntegerInstance().format(observations.size)
        count.text = formatted
        review.contentDescription = "${ui.label("capture.settings.review.title")}, $formatted"
    }

    private fun onMain(block: () -> Unit) {
        if (Looper.myLooper() == Looper.getMainLooper()) block() else main.post(block)
    }
}
