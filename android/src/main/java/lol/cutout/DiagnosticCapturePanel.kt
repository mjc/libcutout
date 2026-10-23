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

    private fun historyRow(entry: MobileLiveCaptureHistoryEntryDto): View {
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
        charging?.let { community ->
            row.addView(ui.action("Notes and private Send") {
                CaptureEvidenceDialog(
                    context,
                    community,
                    MobileCaptureSubmissionSourceDto.LiveCapture(entry.liveCaptureId),
                    pickScreenshots,
                    onReceiptConfirmed = { bluetooth.diagnostics.deduplicateManagedCaptureFiles() },
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
