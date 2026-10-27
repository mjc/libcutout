package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.text.InputType
import android.view.View
import android.view.ViewGroup
import android.widget.*
import uniffi.cutout_mobile_ffi.*
import java.io.File
import java.text.DateFormat
import java.util.Date

internal enum class CaptureHistorySendBadge { SENT, LOCAL_DELETED }

internal fun captureHistorySendBadge(status: MobileCaptureSubmissionStatusDto?): CaptureHistorySendBadge? {
    if (status?.receipt == null) return null
    return when (status.phase) {
        MobileCaptureSubmissionPhaseDto.SENT,
        MobileCaptureSubmissionPhaseDto.DELETING,
        MobileCaptureSubmissionPhaseDto.DELETED ->
            if (status.localPayloadState == MobileCaptureSubmissionLocalPayloadStateDto.REMOVED) {
                CaptureHistorySendBadge.LOCAL_DELETED
            } else {
                CaptureHistorySendBadge.SENT
            }
        else -> null
    }
}

/** Android presentation for Rust-owned capture admission, recording lifecycle, and history. */
class DiagnosticCapturePanel(
    context: Context,
    private val bluetooth: AndroidBleClient,
    private val onShare: (File) -> Unit,
    private val onRequestBluetooth: () -> Unit = {},
    private val charging: AndroidChargingClient? = null,
    private val pickScreenshots: (((List<android.net.Uri>) -> Unit) -> Unit)? = null,
) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val content = ui.column()
    private val renderHandler = Handler(Looper.getMainLooper())
    private val queuedRender = Runnable { render() }
    private var captureState = bluetooth.diagnostics.current()
    private var bleState = bluetooth.latestPresentation
    private var disposed = false
    private val captureObserver = bluetooth.diagnostics.observe { next ->
        if (!disposed) { captureState = next; scheduleRender() }
    }
    private val bleObserver = bluetooth.addObserver { next ->
        val relevantChange = next.phase != bleState.phase || next.session?.connection != bleState.session?.connection ||
            next.devices.map { it.id to (it.advertisedName ?: it.name) } !=
            bleState.devices.map { it.id to (it.advertisedName ?: it.name) }
        if (!disposed) { bleState = next; if (relevantChange) scheduleRender() }
    }

    init {
        setBackgroundColor(ui.background)
        isFillViewport = true
        addView(content, LayoutParams(-1, -2))
        content.tag = "captures.home"
        render()
        refresh()
    }

    fun refresh() {
        if (disposed) return
        bluetooth.diagnostics.refresh()
    }

    fun dispose() {
        disposed = true
        renderHandler.removeCallbacks(queuedRender)
        captureObserver.close()
        bleObserver.close()
    }

    private fun scheduleRender() {
        renderHandler.removeCallbacks(queuedRender)
        renderHandler.postDelayed(queuedRender, 150L)
    }

    private fun render() {
        if (disposed) return
        content.removeAllViews()
        content.setPadding(ui.dp(20), ui.dp(12), ui.dp(20), ui.dp(28))
        content.addView(ui.title(context.getString(R.string.capture_title)))
        content.addView(ui.space(18))

        val lifecycle = captureState.lifecycle
        val attempt = lifecycle.attempt
        if (attempt != null && attempt.stage !in setOf(MobileCaptureStageDto.SAVED, MobileCaptureStageDto.FAILED)) {
            section(context.getString(R.string.capture_active)) {
                val stage = stageLabel(attempt.stage)
                addView(ui.text(stage, 18f, true))
                captureState.deviceName?.takeIf(String::isNotBlank)?.let {
                    addView(ui.text(it, 15f, color = ui.muted))
                }
                captureState.fileName?.let { addView(ui.text(it, 13f, color = ui.muted)) }
                captureState.note?.takeIf(String::isNotBlank)?.let { addView(ui.text(it, 14f, color = ui.muted)) }
                captureState.writerStatus?.let { status ->
                    addView(ui.text(context.getString(
                        R.string.capture_writer_status, status.queuedMessages.toString(),
                        android.text.format.Formatter.formatShortFileSize(context, status.bytesWritten.toLong()),
                    ), 14f, color = ui.muted))
                    addView(ui.text(context.getString(R.string.capture_notification_count, captureState.notificationCount.toString()), 14f, color = ui.muted))
                    status.lastError?.let { addView(ui.text(it, 14f, color = ui.red)) }
                }
                if (attempt.origin == MobileCaptureOriginDto.MANUAL &&
                    attempt.stage in setOf(MobileCaptureStageDto.RECORDING, MobileCaptureStageDto.SAVE_FAILED)) {
                    addView(ui.space(12))
                    val actionLabel = if (attempt.stage == MobileCaptureStageDto.SAVE_FAILED)
                        R.string.capture_retry_save else R.string.capture_stop
                    addView(ui.action(context.getString(actionLabel), primary = true) {
                        bluetooth.finishManualCapture { admitted ->
                            if (!admitted) Toast.makeText(context, R.string.capture_save_failed, Toast.LENGTH_LONG).show()
                            refresh()
                        }
                    }.apply { tag = "captures.active.stop" })
                } else if (attempt.origin == MobileCaptureOriginDto.AUTOMATIC) {
                    addView(ui.space(6))
                    addView(ui.text(context.getString(R.string.capture_automatic_description), 13f, color = ui.muted))
                }
            }
            content.addView(ui.space(18))
        }

        section(context.getString(R.string.capture_manual_new)) {
            addView(ui.text(context.getString(R.string.capture_privacy), 14f, color = ui.muted))
            addView(ui.space(8))
            addView(ui.action(context.getString(R.string.capture_manual_new), primary = true) {
                if (!bluetooth.hasPermissions() || !bluetooth.isEnabled()) onRequestBluetooth()
                else chooseDevice(lifecycle)
            }.apply {
                tag = "captures.manual.start"
                isEnabled = canStartManual(lifecycle)
            })
            if (!lifecycle.canStart) addView(ui.text(stageLabel(attempt?.stage), 14f, color = ui.muted))
        }

        (captureState.error ?: captureState.historyError ?: captureState.managedCaptureCleanupError)?.let { message ->
            content.addView(ui.space(12))
            content.addView(ui.text(message, 14f, color = ui.red))
        }

        content.addView(ui.space(22))
        section(context.getString(R.string.capture_recent)) {
            charging?.let { community ->
                addView(ui.action(context.getString(R.string.capture_batch_review), primary = true) {
                    CaptureSubmissionBatchDialog(
                        context = context,
                        charging = community,
                        directories = listOf(
                            File(context.filesDir, "DiagnosticCaptures"),
                            File(context.cacheDir, "diagnostic-exports"),
                        ),
                        onFinished = { receiptConfirmed ->
                            if (receiptConfirmed) bluetooth.diagnostics.deduplicateManagedCaptureFiles()
                            bluetooth.diagnostics.refreshHistory()
                        },
                    ).show()
                }.apply { tag = "captures.batch.review" })
                addView(ui.space(8))
            }
            if (captureState.loading) addView(ProgressBar(context).apply { isIndeterminate = true })
            if (captureState.captures.isEmpty() && !captureState.loading) {
                addView(ui.text(context.getString(R.string.capture_empty), 15f, color = ui.muted))
            }
            captureState.captures.forEachIndexed { index, entry ->
                if (index > 0) { addView(ui.space(8)); addView(ui.divider()); addView(ui.space(8)) }
                addView(historyRow(entry))
            }
            captureState.nextCursor?.let {
                addView(ui.action(context.getString(R.string.capture_load_more)) {
                    bluetooth.diagnostics.refreshHistory(it)
                }.apply { isEnabled = !captureState.loading })
            }
            addView(ui.space(10))
            addView(ui.text(context.getString(R.string.capture_privacy), 13f, color = ui.muted))
        }
        if (captureState.sessionArtifacts.isNotEmpty()) {
            content.addView(ui.space(18))
            section(context.getString(R.string.capture_this_session)) {
                captureState.sessionArtifacts.asReversed().forEachIndexed { index, artifact ->
                    if (index > 0) { addView(ui.space(8)); addView(ui.divider()); addView(ui.space(8)) }
                    addView(ui.text(artifact.deviceName?.takeIf(String::isNotBlank)
                        ?: context.getString(R.string.capture_untitled), 17f, true))
                    artifact.description?.takeIf(String::isNotBlank)?.let { addView(ui.text(it, 14f, color = ui.muted)) }
                    addView(ui.text(context.getString(
                        R.string.capture_writer_status, artifact.status.queuedMessages.toString(),
                        android.text.format.Formatter.formatShortFileSize(context, artifact.status.bytesWritten.toLong()),
                    ), 14f, color = ui.muted))
                    addView(ui.text(context.getString(R.string.capture_notification_count, artifact.notificationCount.toString()), 14f, color = ui.muted))
                    artifact.error?.let { addView(ui.text(it, 14f, color = ui.orange)) }
                    addView(ui.action(context.getString(R.string.capture_share)) {
                        bluetooth.diagnostics.exportSessionArtifact(artifact) { result ->
                            result.onSuccess(onShare).onFailure {
                                Toast.makeText(context, it.message ?: context.getString(R.string.capture_error), Toast.LENGTH_LONG).show()
                            }
                        }
                    }.apply { tag = "captures.session.${artifact.id}.share" })
                }
            }
        }
        content.addView(ui.space(20))
    }

    private fun section(title: String, build: LinearLayout.() -> Unit) {
        val block = ui.column().apply {
            addView(ui.text(title, 20f, true).apply { isAccessibilityHeading = true })
            addView(ui.space(9))
            addView(ui.column().apply {
                setPadding(ui.dp(16), ui.dp(14), ui.dp(16), ui.dp(14))
                background = ui.rounded(22)
                build()
            })
        }
        content.addView(block)
    }

    private fun stageLabel(stage: MobileCaptureStageDto?): String = when (stage) {
        null, MobileCaptureStageDto.STARTING -> context.getString(R.string.capture_starting)
        MobileCaptureStageDto.RECORDING -> context.getString(R.string.capture_recording)
        MobileCaptureStageDto.SAVING, MobileCaptureStageDto.FINALIZING -> context.getString(R.string.capture_saving)
        MobileCaptureStageDto.SAVE_FAILED, MobileCaptureStageDto.FAILED -> context.getString(R.string.capture_failed)
        MobileCaptureStageDto.SAVED -> context.getString(R.string.capture_recent)
    }

    internal fun historyRow(entry: MobileLiveCaptureHistoryEntryDto): View {
        val row = ui.column()
        row.addView(ui.text(entry.recording?.advertisedName ?: entry.platformIdentifier, 17f, true))
        val started = Date(entry.startedAtMilliseconds.toLong())
        row.addView(ui.text(DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(started), 14f, color = ui.muted))
        row.addView(ui.text("${entry.eventCount} events · ${android.text.format.Formatter.formatShortFileSize(context, entry.storedBytes.toLong())}", 14f, color = ui.muted))
        if (entry.interrupted) row.addView(ui.text(context.getString(R.string.capture_interrupted), 14f, color = ui.orange))
        else row.addView(ui.text(when (entry.integrity) {
            MobileCaptureIntegrityDto.Complete -> context.getString(R.string.capture_complete)
            is MobileCaptureIntegrityDto.Incomplete -> context.getString(R.string.capture_incomplete)
            MobileCaptureIntegrityDto.Unknown -> context.getString(R.string.capture_integrity_unknown)
        }, 14f, color = ui.muted))
        val buttons = ui.row()
        buttons.addView(ui.action(context.getString(R.string.capture_share)) {
            bluetooth.diagnostics.export(entry.liveCaptureId) { result ->
                result.onSuccess(onShare).onFailure {
                    Toast.makeText(context, it.message ?: context.getString(R.string.capture_error), Toast.LENGTH_LONG).show()
                }
            }
        }.apply { tag = "captures.history.${entry.liveCaptureId}.share" })
        val badge = captureHistorySendBadge(entry.submission.status)
        if (badge != null) {
            val sentLabel = if (badge == CaptureHistorySendBadge.LOCAL_DELETED) {
                R.string.capture_sent_local_deleted
            } else {
                R.string.capture_sent_privately
            }
            row.addView(ui.icon(CutoutSymbol.CHECK, ui.green, 18).apply {
                tag = "captures.history.${entry.liveCaptureId}.sent.icon"
                contentDescription = context.getString(sentLabel)
            })
            row.addView(ui.text(context.getString(sentLabel), 14f, color = ui.green)
                .apply { tag = "captures.history.${entry.liveCaptureId}.sent" })
        }
        charging?.let { community ->
            row.addView(ui.action(context.getString(R.string.capture_review_private_send)) {
                CaptureEvidenceDialog(
                    context,
                    community,
                    entry.submission.source,
                    pickScreenshots,
                    onReceiptConfirmed = {
                        bluetooth.diagnostics.deduplicateManagedCaptureFiles()
                    },
                    onStatusChanged = {
                        bluetooth.diagnostics.refreshHistory()
                    },
                ).show()
            }.apply { tag = "captures.history.${entry.liveCaptureId}.notes" })
        }
        row.addView(buttons)
        return row
    }

    private fun chooseDevice(lifecycle: MobileCaptureLifecycleSnapshotDto) {
        if (!canStartManual(lifecycle)) return
        val connectedId = connectedCapturePlatformIdentifier()
        val connectedCandidate = connectedId?.let { id ->
            bleState.devices.firstOrNull { it.id == id } ?: NearbyPev(
                id = id,
                name = bleState.session?.identity?.model ?: "Connected device",
                rssi = 0,
                suggested = false,
                advertisedName = bleState.session?.identity?.model,
            )
        }
        val candidates = (bleState.devices + listOfNotNull(connectedCandidate)).distinctBy(NearbyPev::id).filter { item ->
            item.id.isNotBlank() && (connectedId == null || item.id == connectedId)
        }
        if (candidates.isEmpty()) {
            AlertDialog.Builder(context).setMessage(R.string.capture_no_devices)
                .setPositiveButton(R.string.capture_scan) { _, _ -> requestBluetoothOrScan() }
                .setNegativeButton(android.R.string.cancel, null).show()
            return
        }
        val labels = candidates.map { item ->
            val name = item.advertisedName?.takeIf(String::isNotBlank) ?: item.name
            "$name  ·  ${item.id.takeLast(8)}"
        }.toTypedArray()
        AlertDialog.Builder(context)
            .setTitle(R.string.capture_choose_device)
            .setItems(labels) { _, which -> descriptionDialog(candidates[which], lifecycle) }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun descriptionDialog(device: NearbyPev, lifecycle: MobileCaptureLifecycleSnapshotDto) {
        val input = EditText(context).apply {
            hint = context.getString(R.string.capture_description)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES or InputType.TYPE_TEXT_FLAG_MULTI_LINE
            minLines = 2
            maxLines = 4
        }
        val wrapper = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(ui.dp(24), ui.dp(8), ui.dp(24), 0)
            addView(input, LinearLayout.LayoutParams(-1, -2))
        }
        val name = device.advertisedName?.takeIf(String::isNotBlank) ?: device.name
        AlertDialog.Builder(context).setTitle(name).setView(wrapper)
            .setPositiveButton(R.string.capture_start, null)
            .setNegativeButton(android.R.string.cancel, null)
            .create().also { dialog ->
                dialog.setOnShowListener {
                    dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                        if (!canStartManual(lifecycle, device.id)) return@setOnClickListener
                        bluetooth.startManualCapture(device.id, null, input.text.toString()) { started ->
                            if (!started) Toast.makeText(context, R.string.capture_start_failed, Toast.LENGTH_LONG).show()
                            refresh()
                        }
                        dialog.dismiss()
                    }
                }
            }.show()
    }

    private fun requestBluetoothOrScan() {
        if (!bluetooth.hasPermissions() || !bluetooth.isEnabled()) onRequestBluetooth()
        else bluetooth.scan()
    }

    private fun connectedCapturePlatformIdentifier(): String? =
        bleState.session?.connection?.takeIf {
            it.transport == MobileConnectionTransportStateDto.CONNECTED &&
                it.readiness == MobileConnectionReadinessDto.VERIFIED
        }?.token?.platformIdentifier

    private fun canStartManual(
        lifecycle: MobileCaptureLifecycleSnapshotDto,
        platformId: String? = null,
    ): Boolean {
        if (!captureState.ready || !lifecycle.canStart) return false
        val connectedId = connectedCapturePlatformIdentifier()
        return when {
            connectedId != null -> platformId == null || platformId == connectedId
            bleState.phase == NativePhase.IDLE -> true
            else -> false
        }
    }
}

internal data class CaptureBatchCandidate(
    val sourceArtifactId: String,
    val sourceKind: String,
    val displayName: String?,
    val eligible: Boolean,
    val skipReason: String?,
    val origin: String,
    val integrity: String,
    val sourceBytes: ULong,
    val sourceSha256: String?,
    val transferBytes: ULong?,
    val transferSha256: String?,
    val categories: List<String>,
    val noteBytes: ULong,
    val screenshotCount: UInt,
    val receipt: String?,
)

internal data class CaptureBatchProgress(
    val sourceArtifactId: String,
    val phase: String,
    val message: String,
    val total: ULong,
    val sent: ULong,
    val cleaned: ULong,
    val failed: ULong,
    val skipped: ULong,
    val cancelled: ULong,
    val current: ULong,
)

internal data class CaptureBatchSummary(
    val sent: ULong,
    val cleaned: ULong,
    val failed: ULong,
    val skipped: ULong,
    val cancelled: ULong,
    val failedSourceArtifactIds: List<String>,
    val pendingCleanupSourceArtifactIds: List<String>,
)

/** The Rust adapter owns inventory, send ordering, receipt validation and cleanup. */
internal interface CaptureSubmissionBatchOperations {
    fun review(completed: (Result<List<CaptureBatchCandidate>>) -> Unit)
    fun sendSelected(
        sourceArtifactIds: List<String>,
        progress: (CaptureBatchProgress) -> Unit,
        completed: (Result<CaptureBatchSummary>) -> Unit,
    )
    fun requestCancel()
}

internal class CaptureSubmissionBatchDialog(
    private val context: Context,
    private val charging: AndroidChargingClient,
    private val directories: List<File>,
    private val onFinished: (receiptConfirmed: Boolean) -> Unit,
    private val testOperations: CaptureSubmissionBatchOperations? = null,
) {
    private val main = Handler(Looper.getMainLooper())
    private val ui = CutoutUi(context)
    private var candidates = emptyList<CaptureBatchCandidate>()
    private val selected = linkedSetOf<String>()
    private val itemResults = linkedMapOf<String, String>()
    private var failedIds = emptyList<String>()
    private var reviewError: String? = null
    private var isReviewing = false
    private var isSending = false
    private var stopRequested = false
    private var reviewContent: LinearLayout? = null
    internal var reviewDialog: AlertDialog? = null
        private set
    internal var confirmationDialog: AlertDialog? = null
        private set
    private var progressText: String? = null
    private var summary: CaptureBatchSummary? = null
    private var receiptConfirmed = false
    private var sendAttempt = 0L
    private var requiresFreshReview = false
    private var sendCompleted = false
    private var retrySelection: Set<String>? = null
    private var reviewHeader: LinearLayout? = null
    private var candidateAdapter: CaptureBatchCandidateAdapter? = null
    private val batchOperations by lazy {
        testOperations ?: RustCaptureSubmissionBatchOperations(
            context = context, charging = charging, directories = directories,
        )
    }

    fun show() {
        val body = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(ui.dp(20), ui.dp(8), ui.dp(20), ui.dp(12))
        }
        val header = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
        }
        reviewHeader = header
        reviewContent = header
        candidateAdapter = CaptureBatchCandidateAdapter()
        val list = ListView(context).apply {
            adapter = candidateAdapter
            dividerHeight = ui.dp(1).coerceAtLeast(1)
            isFastScrollEnabled = true
            layoutParams = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, ui.dp(360),
            )
            tag = "captures.batch.candidates"
        }
        body.addView(header)
        body.addView(list)
        reviewDialog = AlertDialog.Builder(context)
            .setTitle(R.string.capture_batch_title)
            .setView(body)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(R.string.capture_batch_send, null)
            .create()
        reviewDialog?.setOnShowListener {
            reviewDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.setOnClickListener {
                if (selected.isNotEmpty() && !isReviewing && !isSending && !sendCompleted && !requiresFreshReview && reviewError == null) confirm()
            }
            render(header)
        }
        reviewDialog?.show()
        beginReview()
    }

    private fun beginReview() {
        if (isReviewing || isSending) return
        isReviewing = true
        reviewError = null
        candidates = emptyList()
        selected.clear()
        progressText = null
        itemResults.clear()
        summary = null
        renderCurrent()
        operations().review { result -> main.post {
            isReviewing = false
            result.onSuccess { rows ->
                candidates = rows
                selected.clear()
                val failedBeforeReview = retrySelection
                rows.filter { it.eligible && (failedBeforeReview == null || it.sourceArtifactId in failedBeforeReview) }
                    .forEach { selected += it.sourceArtifactId }
                requiresFreshReview = false
                stopRequested = false
                summary = null
                progressText = null
                itemResults.clear()
                failedIds = emptyList()
                retrySelection = null
                sendCompleted = false
            }.onFailure { reviewError = it.message ?: context.getString(R.string.capture_batch_review_failed) }
            renderCurrent()
        } }
    }

    private fun confirm() {
        if (sendCompleted || requiresFreshReview || selected.isEmpty()) return
        val count = selected.size
        confirmationDialog = AlertDialog.Builder(context)
            .setTitle(R.string.capture_batch_confirm_title)
            .setMessage(context.getString(R.string.capture_batch_confirm_message, count))
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(
                if (count == candidates.count { it.eligible }) R.string.capture_batch_send_local
                else R.string.capture_batch_send_selected_local,
                null,
            )
            .create()
        confirmationDialog?.setOnShowListener {
            confirmationDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.setOnClickListener {
                confirmationDialog?.dismiss()
                send(selected.toList())
            }
        }
        confirmationDialog?.show()
    }

    private fun send(ids: List<String>) {
        if (ids.isEmpty() || isSending || sendCompleted || requiresFreshReview) return
        val attempt = ++sendAttempt
        isSending = true
        reviewError = null
        progressText = context.getString(R.string.capture_batch_starting)
        failedIds = emptyList()
        renderCurrent()
        operations().sendSelected(ids, { event -> main.post {
            if (sendAttempt != attempt || !isSending) return@post
            receiptConfirmed = receiptConfirmed || event.phase == "Sent" || event.phase == "Cleaned"
            if (event.phase == "Failed") failedIds = (failedIds + event.sourceArtifactId).distinct()
            else if (event.phase == "Sent" || event.phase == "Cleaned") failedIds = failedIds - event.sourceArtifactId
            progressText = context.getString(
                R.string.capture_batch_progress,
                event.current.toLong(), event.total.toLong(), event.sent.toLong(), event.cleaned.toLong(),
                event.failed.toLong(), event.skipped.toLong(), event.cancelled.toLong(),
            )
            itemResults[event.sourceArtifactId] = event.message
            renderCurrent()
        } }) { result -> main.post {
            if (sendAttempt != attempt) return@post
            isSending = false
            sendCompleted = true
            result.onSuccess { next ->
                summary = next
                failedIds = next.failedSourceArtifactIds
                requiresFreshReview = requiresFreshReview || next.cancelled > 0uL
                receiptConfirmed = receiptConfirmed || next.sent > 0uL || next.cleaned > 0uL
                progressText = context.getString(
                    R.string.capture_batch_result,
                    next.sent.toLong(), next.cleaned.toLong(), next.failed.toLong(), next.skipped.toLong(), next.cancelled.toLong(),
                )
                if (next.pendingCleanupSourceArtifactIds.isNotEmpty()) {
                    progressText += " · " + context.getString(
                        R.string.capture_batch_pending_cleanup,
                        next.pendingCleanupSourceArtifactIds.size,
                    )
                }
                onFinished(receiptConfirmed)
            }.onFailure {
                reviewError = it.message ?: context.getString(R.string.capture_batch_failed)
                failedIds = (failedIds + ids).distinct()
                progressText = context.getString(R.string.capture_batch_retained)
                onFinished(receiptConfirmed)
            }
            renderCurrent()
        } }
    }

    private fun renderCurrent() {
        reviewContent?.let(::render)
    }

    private fun render(content: LinearLayout) {
        content.removeAllViews()
        candidateAdapter?.notifyDataSetChanged()
        reviewDialog?.getButton(AlertDialog.BUTTON_NEGATIVE)?.isEnabled = !isSending
        reviewDialog?.getButton(AlertDialog.BUTTON_POSITIVE)?.apply {
            isEnabled = !isSending && !isReviewing && !sendCompleted && !requiresFreshReview && reviewError == null && selected.isNotEmpty()
            setText(if (selected.size == candidates.count { it.eligible }) R.string.capture_batch_send
            else R.string.capture_batch_send_selected)
        }
        content.addView(ui.text(context.getString(R.string.capture_batch_explanation), 14f, color = ui.muted))
        content.addView(ui.space(10))
        if (isReviewing) content.addView(ProgressBar(context).apply { isIndeterminate = true })
        reviewError?.let { error ->
            content.addView(ui.text(error, 14f, color = ui.red))
            if (!isSending) content.addView(ui.action(context.getString(
                if (failedIds.isEmpty()) R.string.capture_batch_retry_review else R.string.capture_batch_retry_failed,
            )) {
                if (failedIds.isEmpty()) beginReview() else retryFailedReview()
            }.apply { tag = if (failedIds.isEmpty()) "captures.batch.retryReview" else "captures.batch.reviewFailedAgain" })
        }
        content.addView(ui.text(context.getString(R.string.capture_batch_counts, selected.size, candidates.count { it.eligible }, candidates.count { !it.eligible }), 14f, true))
        progressText?.let { content.addView(ui.text(it, 14f, color = ui.muted)) }
        if (isSending) {
            content.addView(ui.space(8))
            content.addView(ui.action(context.getString(R.string.capture_batch_stop)) {
                if (!stopRequested) {
                    stopRequested = true
                    requiresFreshReview = true
                    operations().requestCancel()
                    renderCurrent()
                }
            }.apply { isEnabled = !stopRequested; tag = "captures.batch.stop" })
        } else if (summary != null && requiresFreshReview) {
            content.addView(ui.action(context.getString(R.string.capture_batch_review_again)) { beginReview() }
                .apply { isEnabled = !isReviewing; tag = "captures.batch.reviewAgain" })
        } else if (summary != null && failedIds.isNotEmpty()) {
            content.addView(ui.action(context.getString(R.string.capture_batch_retry_failed)) { retryFailedReview() }
                .apply { tag = "captures.batch.reviewFailedAgain" })
        }
    }

    private fun retryFailedReview() {
        if (!sendCompleted || failedIds.isEmpty()) return
        retrySelection = failedIds.toSet()
        beginReview()
    }

    private fun operations(): CaptureSubmissionBatchOperations = batchOperations

    private inner class CaptureBatchCandidateAdapter : BaseAdapter() {
        override fun getCount(): Int = candidates.size
        override fun getItem(position: Int): CaptureBatchCandidate = candidates[position]
        override fun getItemId(position: Int): Long = position.toLong()

        override fun getView(position: Int, recycled: View?, parent: ViewGroup): View {
            val candidate = getItem(position)
            val row = (recycled as? LinearLayout) ?: LinearLayout(context).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(0, ui.dp(8), 0, ui.dp(8))
            }
            row.removeAllViews()
            val title = candidate.displayName ?: candidate.sourceKind
            if (candidate.eligible) {
                row.addView(CheckBox(context).apply {
                    text = title
                    tag = "captures.batch.candidate.${candidate.sourceArtifactId}"
                    setOnCheckedChangeListener(null)
                    isChecked = candidate.sourceArtifactId in selected
                    isEnabled = !isSending && !isReviewing && !sendCompleted && !requiresFreshReview
                    setOnCheckedChangeListener { _, checked ->
                        if (checked) selected += candidate.sourceArtifactId else selected -= candidate.sourceArtifactId
                        reviewHeader?.let(::render)
                    }
                })
            } else {
                row.addView(ui.text(title, 15f, true))
                row.addView(ui.text(candidate.skipReason ?: context.getString(R.string.capture_batch_skipped), 13f, color = ui.orange))
            }
            row.addView(ui.text(context.getString(R.string.capture_batch_origin_integrity, candidate.origin, candidate.integrity), 13f, color = ui.muted))
            row.addView(ui.text(context.getString(R.string.capture_batch_source, candidate.sourceBytes.toLong(), candidate.sourceSha256 ?: context.getString(R.string.capture_batch_unavailable)), 12f, color = ui.muted))
            val transfer = candidate.transferBytes?.let { bytes -> candidate.transferSha256?.let { sha ->
                context.getString(R.string.capture_batch_transfer, bytes.toLong(), sha)
            } } ?: context.getString(R.string.capture_batch_transfer_unavailable)
            row.addView(ui.text(transfer, 12f, color = ui.muted))
            if (candidate.categories.isNotEmpty()) row.addView(ui.text(context.getString(R.string.capture_batch_categories, candidate.categories.joinToString()), 12f, color = ui.muted))
            row.addView(ui.text(context.getString(R.string.capture_batch_evidence, candidate.noteBytes.toLong(), candidate.screenshotCount.toLong()), 12f, color = ui.muted))
            candidate.receipt?.let { row.addView(ui.text(context.getString(R.string.capture_sent_privately), 13f, color = ui.green)) }
            itemResults[candidate.sourceArtifactId]?.let { row.addView(ui.text(it, 13f, color = if (candidate.sourceArtifactId in failedIds) ui.orange else ui.green)) }
            return row
        }
    }
}

private class RustCaptureSubmissionBatchOperations(
    private val context: Context,
    private val charging: AndroidChargingClient,
    private val directories: List<File>,
) : CaptureSubmissionBatchOperations {
    @Volatile private var client: CaptureSubmissionClientHandle? = null
    @Volatile private var plan: CaptureSubmissionBatchPlanHandle? = null

    override fun review(completed: (Result<List<CaptureBatchCandidate>>) -> Unit) {
        charging.withDatabase({ database ->
            val client = CaptureSubmissionClientHandle(database)
            val plan = client.reviewAll(directories.map { it.absolutePath })
            this.client = client
            this.plan = plan
            plan.candidates().map(::projectCandidate)
        }, completed)
    }

    override fun sendSelected(
        sourceArtifactIds: List<String>,
        progress: (CaptureBatchProgress) -> Unit,
        completed: (Result<CaptureBatchSummary>) -> Unit,
    ) {
        charging.withDatabase({ database ->
            val reviewedPlan = checkNotNull(plan) { "Review the captures before sending." }
            val client = this.client ?: CaptureSubmissionClientHandle(database)
            val credential = charging.captureCredential()
            Triple(client, reviewedPlan, credential)
        }) { prepared ->
            prepared.onFailure { completed(Result.failure(it)) }
            prepared.onSuccess { (client, reviewedPlan, credential) ->
                val publishProgress = progress
                charging.runCaptureBatch({
                    val listener = object : MobileCaptureSubmissionBatchProgressListener {
                        override fun onProgress(progress: MobileCaptureSubmissionBatchProgressDto) {
                            publishProgress(projectProgress(progress))
                        }
                    }
                    val summary = client.sendSelected(
                        plan = reviewedPlan,
                        selectedSourceArtifactIds = sourceArtifactIds,
                        accountId = credential.identityId,
                        token = credential.token,
                        listener = listener,
                    )
                    CaptureBatchSummary(
                        sent = summary.sent,
                        cleaned = summary.cleaned,
                        failed = summary.failed,
                        skipped = summary.skipped,
                        cancelled = summary.cancelled,
                        failedSourceArtifactIds = summary.failedSourceArtifactIds,
                        pendingCleanupSourceArtifactIds = summary.pendingCleanupSourceArtifactIds,
                    )
                }, completed)
            }
        }
    }

    override fun requestCancel() {
        plan?.requestCancel()
    }

    private fun projectCandidate(value: MobileCaptureSubmissionBatchCandidateDto): CaptureBatchCandidate {
        if (value.eligibility == MobileCaptureSubmissionBatchEligibilityDto.ELIGIBLE &&
            (value.transferBytes == null || value.transferSha256 == null)
        ) {
            throw IllegalStateException(context.getString(R.string.capture_batch_invalid_review))
        }
        val categories = value.categories?.let { categories -> buildList {
            if (categories.deviceIdentifiers) add(context.getString(R.string.capture_batch_category_identifiers))
            if (categories.location) add(context.getString(R.string.capture_batch_category_location))
            if (categories.notes) add(context.getString(R.string.capture_batch_category_notes))
            if (categories.annotations) add(context.getString(R.string.capture_batch_category_annotations))
            if (categories.settingsObservations) add(context.getString(R.string.capture_batch_category_settings))
        } } ?: emptyList()
        val eligibility = value.skipReason ?: value.eligibility
        return CaptureBatchCandidate(
            sourceArtifactId = value.sourceArtifactId,
            sourceKind = sourceKindLabel(value.sourceKind),
            displayName = value.displayName.takeIf(String::isNotBlank),
            eligible = value.eligibility == MobileCaptureSubmissionBatchEligibilityDto.ELIGIBLE,
            skipReason = if (value.eligibility == MobileCaptureSubmissionBatchEligibilityDto.ELIGIBLE) null else eligibilityLabel(eligibility),
            origin = captureSubmissionOriginLabel(context, value.origin),
            integrity = captureSubmissionIntegrityLabel(context, value.integrity),
            sourceBytes = value.sourceBytes,
            sourceSha256 = value.sourceSha256,
            transferBytes = value.transferBytes,
            transferSha256 = value.transferSha256,
            categories = categories,
            noteBytes = value.evidence?.noteBytes?.toULong() ?: 0uL,
            screenshotCount = value.evidence?.screenshots ?: 0u,
            receipt = value.status?.receipt?.operationId,
        )
    }

    private fun projectProgress(value: MobileCaptureSubmissionBatchProgressDto): CaptureBatchProgress {
        val phase = when (value.phase) {
            MobileCaptureSubmissionBatchItemPhaseDto.CURRENT -> "Current"
            MobileCaptureSubmissionBatchItemPhaseDto.SENT -> "Sent"
            MobileCaptureSubmissionBatchItemPhaseDto.CLEANED -> "Cleaned"
            MobileCaptureSubmissionBatchItemPhaseDto.FAILED -> "Failed"
            MobileCaptureSubmissionBatchItemPhaseDto.SKIPPED -> "Skipped"
            MobileCaptureSubmissionBatchItemPhaseDto.CANCELLED -> "Cancelled"
        }
        val reason = value.reason?.let(::eligibilityLabel)
        val message = when (value.phase) {
            MobileCaptureSubmissionBatchItemPhaseDto.CURRENT -> {
                val sent = value.bytesSent
                val total = value.totalBytes
                if (sent != null && total != null) context.getString(
                    R.string.capture_batch_bytes_progress,
                    android.text.format.Formatter.formatShortFileSize(context, sent.toLong()),
                    android.text.format.Formatter.formatShortFileSize(context, total.toLong()),
                ) else context.getString(R.string.capture_batch_preparing)
            }
            MobileCaptureSubmissionBatchItemPhaseDto.SENT -> context.getString(R.string.capture_batch_receipt_saved)
            MobileCaptureSubmissionBatchItemPhaseDto.CLEANED -> context.getString(R.string.capture_batch_cleaned)
            MobileCaptureSubmissionBatchItemPhaseDto.FAILED -> reason?.let {
                context.getString(R.string.capture_batch_failed_reason, it, context.getString(R.string.capture_batch_failed_retained))
            } ?: context.getString(R.string.capture_batch_failed_retained)
            MobileCaptureSubmissionBatchItemPhaseDto.SKIPPED -> reason ?: context.getString(R.string.capture_batch_skipped)
            MobileCaptureSubmissionBatchItemPhaseDto.CANCELLED -> context.getString(R.string.capture_batch_cancelled_item)
        }
        return CaptureBatchProgress(
            sourceArtifactId = value.sourceArtifactId,
            phase = phase,
            message = message,
            total = value.total,
            sent = value.sent,
            cleaned = value.cleaned,
            failed = value.failed,
            skipped = value.skipped,
            cancelled = value.cancelled,
            current = value.current,
        )
    }

    private fun sourceKindLabel(kind: MobileCaptureSubmissionBatchSourceKindDto): String = when (kind) {
        MobileCaptureSubmissionBatchSourceKindDto.LIVE_CAPTURE -> context.getString(R.string.capture_batch_kind_live)
        MobileCaptureSubmissionBatchSourceKindDto.RECORDED_ARTIFACT -> context.getString(R.string.capture_batch_kind_imported)
        MobileCaptureSubmissionBatchSourceKindDto.MANAGED_FILE -> context.getString(R.string.capture_batch_kind_file)
    }

    private fun eligibilityLabel(eligibility: MobileCaptureSubmissionBatchEligibilityDto): String = when (eligibility) {
        MobileCaptureSubmissionBatchEligibilityDto.ELIGIBLE -> context.getString(R.string.capture_batch_select)
        MobileCaptureSubmissionBatchEligibilityDto.ALREADY_SUBMITTED -> context.getString(R.string.capture_batch_reason_already_sent)
        MobileCaptureSubmissionBatchEligibilityDto.ACTIVE -> context.getString(R.string.capture_batch_reason_active)
        MobileCaptureSubmissionBatchEligibilityDto.EMPTY -> context.getString(R.string.capture_batch_reason_empty)
        MobileCaptureSubmissionBatchEligibilityDto.TOO_LARGE -> context.getString(R.string.capture_batch_reason_too_large)
        MobileCaptureSubmissionBatchEligibilityDto.CORRUPT -> context.getString(R.string.capture_batch_reason_corrupt)
        MobileCaptureSubmissionBatchEligibilityDto.UNAVAILABLE -> context.getString(R.string.capture_batch_reason_unavailable)
    }
}
