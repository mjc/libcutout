package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import android.text.format.Formatter
import uniffi.cutout_mobile_ffi.*
import java.util.Locale

/** Private submission disclosure and confirmations for a Rust-retained capture source. */
class CaptureSubmissionPanel(
    context: Context,
    private val controller: CaptureSubmissionController,
) : LinearLayout(context) {
    private val ui = CutoutUi(context)
    private val body = ui.column()
    private var observation: AutoCloseable? = null
    private var presentation = CaptureSubmissionPresentation()
    private var activeDialog: AlertDialog? = null
    private var pendingConfirmation: PendingConfirmation? = null
    private var confirmationGeneration = 0L
    private var disposed = false

    private data class PendingConfirmation(
        val generation: Long,
        val operationId: String,
        val phase: MobileCaptureSubmissionPhaseDto,
        val action: ConfirmationAction,
    )

    private enum class ConfirmationAction { SEND, DELETE }

    init {
        orientation = VERTICAL
        tag = "capture.submission.panel"
        addView(ui.text(ui.label("capture.submission.heading"), 20f, true).apply { isAccessibilityHeading = true })
        addView(ui.space(8))
        addView(ui.text(ui.label("capture.submission.intro"), 15f, color = ui.muted))
        addView(ui.space(8))
        addView(body, LayoutParams(-1, -2))
        addView(ui.text(ui.label("capture.submission.privacy"), 13f, color = ui.muted).apply {
            setPadding(0, ui.dp(8), 0, ui.dp(4))
        })
        render(controller.current())
        observation = controller.observe { render(it) }
        controller.refresh()
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        observation?.close()
        observation = null
        confirmationGeneration++
        pendingConfirmation = null
        activeDialog?.dismiss()
        activeDialog = null
    }

    fun render(next: CaptureSubmissionPresentation) {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            post { if (!disposed) render(next) }
            return
        }
        if (disposed) return
        val status = next.status
        pendingConfirmation?.let { pending ->
            if (status == null || status.operationId != pending.operationId || status.phase != pending.phase) {
                dismissConfirmation(pending)
            }
        }
        presentation = next
        body.removeAllViews()
        if (status == null) {
            if (next.working) body.addView(ui.text(ui.label("capture.submission.loading"), color = ui.muted))
            else body.addView(ui.action(ui.label("capture.submission.review"), primary = true) {
                controller.prepare()
            }.apply { tag = "capture.submission.prepare" })
        } else {
            disclosure(status)
            body.addView(ui.space(8))
            actions(status, next)
        }
        next.error?.let { body.addView(ui.text(it, 14f, color = ui.orange).apply { tag = "capture.submission.error" }) }
        status?.failure?.let { failure -> body.addView(ui.text(failureText(failure), 14f, color = ui.orange).apply {
            tag = "capture.submission.failure"
        }) }
        if (next.working) body.addView(ui.text(
            ui.label(if (next.sending) "capture.submission.sending" else "capture.submission.working"),
            14f, color = ui.muted,
        ).apply { tag = "capture.submission.progress" })
    }

    private fun disclosure(status: MobileCaptureSubmissionStatusDto) {
        body.addView(row("capture.submission.phase", phaseText(status.phase)))
        body.addView(row("capture.submission.size", Formatter.formatFileSize(context, status.byteCount.coerceAtMost(Long.MAX_VALUE.toULong()).toLong())))
        body.addView(ui.text(ui.label("capture.submission.evidence"), 14f, color = ui.muted))
        val included = includedCategories(status.categories)
        if (included.isNotEmpty()) body.addView(row("capture.submission.included", included.joinToString(", ")))
        body.addView(row("capture.submission.excluded", ui.label("capture.submission.music_excluded")))
        body.addView(row("capture.submission.dropped", status.droppedMessages.toString()))
        status.receipt?.let { receipt ->
            body.addView(row("capture.submission.receipt", receipt.operationId, "capture.submission.receipt.id"))
            body.addView(row("capture.submission.sha256", receipt.sha256, "capture.submission.receipt.sha256"))
            body.addView(row("capture.submission.received_bytes", Formatter.formatFileSize(context, receipt.bytes.coerceAtMost(Long.MAX_VALUE.toULong()).toLong())))
        }
    }

    private fun actions(status: MobileCaptureSubmissionStatusDto, state: CaptureSubmissionPresentation) {
        when (status.phase) {
            MobileCaptureSubmissionPhaseDto.PREPARED -> {
                sendButton(ui.label("capture.submission.send"), state)
                cancelButton(state)
            }
            MobileCaptureSubmissionPhaseDto.SENDING -> {
                body.addView(ui.text(ui.label("capture.submission.receipt_pending"), 14f, color = ui.muted))
                if (!state.sending) sendButton(ui.label("capture.submission.retry_send"), state)
                cancelButton(state)
            }
            MobileCaptureSubmissionPhaseDto.FAILED -> {
                sendButton(ui.label("capture.submission.retry_send"), state)
                cancelButton(state)
            }
            MobileCaptureSubmissionPhaseDto.REJECTED -> {
                body.addView(ui.text(ui.label("capture.submission.rejected"), 14f, color = ui.orange))
                cancelButton(state)
            }
            MobileCaptureSubmissionPhaseDto.SENT -> {
                if (status.receipt != null) body.addView(ui.text(ui.label("capture.submission.received"), 16f, true))
                body.addView(ui.action(ui.label("capture.submission.delete")) { confirmDelete() }.apply {
                    tag = "capture.submission.delete"; isEnabled = !state.working
                })
            }
            MobileCaptureSubmissionPhaseDto.CANCELLING -> {
                body.addView(ui.text(ui.label("capture.submission.cancel_pending"), 14f, color = ui.muted))
                cancelButton(state)
            }
            MobileCaptureSubmissionPhaseDto.CANCELLED -> body.addView(ui.text(ui.label("capture.submission.cancelled"), 16f, true))
            MobileCaptureSubmissionPhaseDto.DELETING -> {
                body.addView(ui.text(ui.label("capture.submission.delete_pending"), 14f, color = ui.orange))
                body.addView(ui.action(ui.label("capture.submission.retry_delete")) { confirmDelete() }.apply {
                    tag = "capture.submission.delete.retry"; isEnabled = !state.working
                })
            }
            MobileCaptureSubmissionPhaseDto.DELETED -> body.addView(ui.text(ui.label("capture.submission.deleted"), 16f, true))
        }
    }

    private fun sendButton(title: String, state: CaptureSubmissionPresentation) {
        body.addView(ui.action(title, primary = true) { confirmSend(presentation.status ?: return@action) }.apply {
            tag = "capture.submission.send"
            isEnabled = !state.working && !state.sending
        })
    }

    private fun cancelButton(state: CaptureSubmissionPresentation) {
        body.addView(ui.action(ui.label("capture.submission.cancel")) {
            controller.cancel()
        }.apply {
            tag = "capture.submission.cancel"
            isEnabled = !state.cancelling && (!state.working || state.sending)
        })
    }

    private fun confirmSend(status: MobileCaptureSubmissionStatusDto) {
        if (disposed || status.phase !in setOf(
                MobileCaptureSubmissionPhaseDto.PREPARED,
                MobileCaptureSubmissionPhaseDto.SENDING,
                MobileCaptureSubmissionPhaseDto.FAILED,
            )) return
        val categories = (listOf(ui.label("capture.submission.raw_evidence"), ui.label("capture.submission.event_timing")) +
            includedCategories(status.categories)).joinToString(", ")
        dismissActiveConfirmation()
        val pending = newConfirmation(status, ConfirmationAction.SEND)
        pendingConfirmation = pending
        activeDialog = AlertDialog.Builder(context)
            .setTitle(ui.label("capture.submission.confirm_title"))
            .setMessage(String.format(Locale.getDefault(), ui.label("capture.submission.confirm_message"), categories))
            .setPositiveButton(ui.label("capture.submission.confirm_send")) { _, _ ->
                if (isCurrent(pending)) controller.send()
            }
            .setNegativeButton(ui.label("capture.submission.not_now"), null)
            .show()
        activeDialog?.setOnDismissListener { clearConfirmation(pending) }
    }

    private fun confirmDelete() {
        val status = presentation.status ?: return
        if (disposed || status.phase !in setOf(
                MobileCaptureSubmissionPhaseDto.SENT,
                MobileCaptureSubmissionPhaseDto.DELETING,
            )) return
        dismissActiveConfirmation()
        val pending = newConfirmation(status, ConfirmationAction.DELETE)
        pendingConfirmation = pending
        activeDialog = AlertDialog.Builder(context)
            .setTitle(ui.label("capture.submission.delete_title"))
            .setMessage(ui.label("capture.submission.delete_message"))
            .setPositiveButton(ui.label("capture.submission.delete_confirm")) { _, _ ->
                if (isCurrent(pending)) controller.delete()
            }
            .setNegativeButton(ui.label("capture.submission.keep"), null)
            .show()
        activeDialog?.setOnDismissListener { clearConfirmation(pending) }
    }

    private fun newConfirmation(
        status: MobileCaptureSubmissionStatusDto,
        action: ConfirmationAction,
    ) = PendingConfirmation(++confirmationGeneration, status.operationId, status.phase, action)

    private fun isCurrent(pending: PendingConfirmation): Boolean {
        if (disposed || pendingConfirmation != pending) return false
        val status = controller.current().status ?: return false
        return status.operationId == pending.operationId && status.phase == pending.phase
    }

    private fun dismissConfirmation(pending: PendingConfirmation) {
        if (pendingConfirmation != pending) return
        confirmationGeneration++
        pendingConfirmation = null
        val dialog = activeDialog
        activeDialog = null
        dialog?.dismiss()
    }

    private fun dismissActiveConfirmation() {
        confirmationGeneration++
        pendingConfirmation = null
        val dialog = activeDialog
        activeDialog = null
        dialog?.dismiss()
    }

    private fun clearConfirmation(pending: PendingConfirmation) {
        if (pendingConfirmation == pending) {
            pendingConfirmation = null
            activeDialog = null
        }
    }

    private fun row(key: String, value: String, valueTag: String? = null): TextView =
        ui.text("${ui.label(key)}: $value", 14f).apply { valueTag?.let { tag = it }; setPadding(0, ui.dp(3), 0, ui.dp(3)) }

    private fun includedCategories(categories: MobileCaptureSubmissionCategoriesDto): List<String> = buildList {
        if (categories.deviceIdentifiers) add(ui.label("capture.submission.category.identifiers"))
        if (categories.location) add(ui.label("capture.submission.category.location"))
        if (categories.notes) add(ui.label("capture.submission.category.notes"))
        if (categories.annotations) add(ui.label("capture.submission.category.annotations"))
        if (categories.settingsObservations) add(ui.label("capture.submission.category.settings"))
    }

    private fun phaseText(phase: MobileCaptureSubmissionPhaseDto) = ui.label("capture.submission.phase." + when (phase) {
        MobileCaptureSubmissionPhaseDto.PREPARED -> "prepared"
        MobileCaptureSubmissionPhaseDto.SENDING -> "sending"
        MobileCaptureSubmissionPhaseDto.FAILED -> "failed"
        MobileCaptureSubmissionPhaseDto.REJECTED -> "rejected"
        MobileCaptureSubmissionPhaseDto.SENT -> "sent"
        MobileCaptureSubmissionPhaseDto.CANCELLING -> "cancelling"
        MobileCaptureSubmissionPhaseDto.CANCELLED -> "cancelled"
        MobileCaptureSubmissionPhaseDto.DELETING -> "deleting"
        MobileCaptureSubmissionPhaseDto.DELETED -> "deleted"
    })

    private fun failureText(failure: MobileCaptureSubmissionFailureDto) = ui.label("capture.submission.failure." + when (failure) {
        MobileCaptureSubmissionFailureDto.NETWORK -> "network"
        MobileCaptureSubmissionFailureDto.UNAUTHORIZED -> "unauthorized"
        MobileCaptureSubmissionFailureDto.FORBIDDEN -> "forbidden"
        MobileCaptureSubmissionFailureDto.CONFLICT -> "conflict"
        MobileCaptureSubmissionFailureDto.REJECTED -> "rejected"
        MobileCaptureSubmissionFailureDto.THROTTLED -> "throttled"
        MobileCaptureSubmissionFailureDto.UNAVAILABLE -> "unavailable"
        MobileCaptureSubmissionFailureDto.TOO_LARGE -> "too_large"
        MobileCaptureSubmissionFailureDto.INVALID_CAPTURE -> "invalid_capture"
        MobileCaptureSubmissionFailureDto.MUSIC_PRESENT -> "music_present"
        MobileCaptureSubmissionFailureDto.QUOTA_EXCEEDED -> "quota_exceeded"
        MobileCaptureSubmissionFailureDto.CANCELLED -> "cancelled"
    })
}
