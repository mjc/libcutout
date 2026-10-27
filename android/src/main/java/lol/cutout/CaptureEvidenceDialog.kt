package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.text.Editable
import android.text.TextWatcher
import android.widget.*
import uniffi.cutout_mobile_ffi.*
import java.io.ByteArrayOutputStream

internal fun captureSubmissionEvidenceIsReviewable(loaded: Boolean, working: Boolean, dirty: Boolean): Boolean =
    loaded && !working && !dirty

internal fun captureSubmissionOriginLabel(context: Context, origin: MobileCaptureSubmissionOriginDto): String = when (origin) {
    MobileCaptureSubmissionOriginDto.AUTOMATIC -> context.getString(R.string.capture_send_origin_automatic)
    MobileCaptureSubmissionOriginDto.MANUAL -> context.getString(R.string.capture_send_origin_manual)
    MobileCaptureSubmissionOriginDto.UNKNOWN -> context.getString(R.string.capture_send_origin_unknown)
}

internal fun captureSubmissionIntegrityLabel(context: Context, integrity: MobileCaptureIntegrityDto): String = when (integrity) {
    MobileCaptureIntegrityDto.Complete -> context.getString(R.string.capture_complete)
    is MobileCaptureIntegrityDto.Incomplete -> context.getString(R.string.capture_incomplete)
    MobileCaptureIntegrityDto.Unknown -> context.getString(R.string.capture_integrity_unknown)
}

/** Native editing and image effects; Rust owns limits, persistence and immutable Send. */
internal class CaptureEvidenceDialog(
    private val context: Context,
    private val charging: AndroidChargingClient,
    private val source: MobileCaptureSubmissionSourceDto,
    private val pickImages: (((List<Uri>) -> Unit) -> Unit)?,
    private val onReceiptConfirmed: () -> Unit = {},
    private val onStatusChanged: () -> Unit = {},
    private val testOperations: TestOperations? = null,
) {
    internal class TestOperations(
        val load: () -> Pair<MobileCaptureSubmissionStatusDto?, MobileCaptureEvidenceDto>,
        val prepare: () -> MobileCaptureSubmissionStatusDto,
        val send: (String, MobileCaptureSubmissionProgressListener) -> MobileCaptureSubmissionStatusDto,
        val remove: ((String, Boolean) -> MobileCaptureSubmissionStatusDto)? = null,
    )

    private val ui = CutoutUi(context)
    private val content = ui.column()
    private val note = EditText(context).apply {
        hint = "Add a note"; minLines = 3; maxLines = 6
        inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE
        tag = "captures.evidence.note"
    }
    private val count = ui.text("0 / 1024 bytes", 13f, color = ui.muted)
    private val images = ui.column()
    private val message = ui.text("", 14f, color = ui.muted).apply { tag = "captures.send.error" }
    private val sendProgress = CaptureSendProgressView(context)
    private var screenshots = listOf<ByteArray>()
    private var status: MobileCaptureSubmissionStatusDto? = null
    private var working = false
    private var dirty = false
    private var loaded = false
    private var dismissed = false
    private var noteLimit = 1024
    private var screenshotLimit = 4
    internal var currentDialog: AlertDialog? = null
        private set
    internal var confirmationDialog: AlertDialog? = null
        private set
    internal var removalConfirmationDialog: AlertDialog? = null
        private set
    private val add = ui.action("Add screenshots") { pickImages?.invoke(::importImages) }
    private val save = ui.action("Save note and screenshots") { save() }
    private val retryLoad = ui.action(context.getString(R.string.capture_retry_notes)) { refresh() }
        .apply { tag = "captures.evidence.retry" }
    private val send = ui.action(context.getString(R.string.capture_review_private_send), primary = true) { review() }
        .apply { tag = "captures.send.review" }
    private val remove = ui.action("Cancel Send") { removeSent(false) }.apply { tag = "captures.send.remove" }

    fun show() {
        content.setPadding(ui.dp(20), ui.dp(12), ui.dp(20), ui.dp(20))
        content.addView(note); content.addView(count); content.addView(images)
        content.addView(add); content.addView(save); content.addView(retryLoad); content.addView(send); content.addView(sendProgress)
        content.addView(remove); content.addView(message)
        val dialog = AlertDialog.Builder(context).setTitle("Recording notes")
            .setView(ScrollView(context).apply { addView(content) })
            .setNegativeButton("Close", null).create()
        currentDialog = dialog
        dialog.setOnDismissListener {
            dismissed = true
            sendProgress.clear()
        }
        dialog.show()
        note.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                if (!working && loaded) dirty = true
                update()
            }
            override fun afterTextChanged(s: Editable?) {}
        })
        refresh()
    }

    private fun editable(): Boolean = when (status?.phase) {
        null, MobileCaptureSubmissionPhaseDto.PREPARED, MobileCaptureSubmissionPhaseDto.CANCELLED,
        MobileCaptureSubmissionPhaseDto.DELETED -> true
        else -> false
    }

    private fun update() {
        val bytes = note.text.toString().toByteArray(Charsets.UTF_8).size
        count.text = "$bytes / $noteLimit bytes"
        note.isEnabled = loaded && !working && editable()
        add.text = "Add screenshots (${screenshots.size}/$screenshotLimit)"
        add.isEnabled = loaded && !working && editable() && screenshots.size < screenshotLimit && pickImages != null
        save.isEnabled = loaded && !working && editable() && dirty && bytes <= noteLimit
        retryLoad.visibility = if (!loaded && !working) android.view.View.VISIBLE else android.view.View.GONE
        retryLoad.isEnabled = !working
        send.isEnabled = captureSubmissionEvidenceIsReviewable(loaded, working, dirty) && status?.phase !in setOf(
            MobileCaptureSubmissionPhaseDto.SENT, MobileCaptureSubmissionPhaseDto.DELETING,
            MobileCaptureSubmissionPhaseDto.REJECTED, MobileCaptureSubmissionPhaseDto.CANCELLING,
            MobileCaptureSubmissionPhaseDto.DELETED)
        remove.visibility = if (status == null || status?.phase in setOf(
            MobileCaptureSubmissionPhaseDto.SENT,
            MobileCaptureSubmissionPhaseDto.DELETING,
            MobileCaptureSubmissionPhaseDto.CANCELLED,
            MobileCaptureSubmissionPhaseDto.DELETED,
        )) android.view.View.GONE else android.view.View.VISIBLE
        remove.text = "Cancel Send"
        remove.isEnabled = !working
    }

    private fun renderImages() {
        images.removeAllViews()
        screenshots.forEachIndexed { index, data ->
            images.addView(ImageView(context).apply {
                setImageBitmap(BitmapFactory.decodeByteArray(data, 0, data.size,
                    BitmapFactory.Options().apply { inSampleSize = 4 }))
                adjustViewBounds = true; maxHeight = ui.dp(160); contentDescription = "Screenshot ${index + 1}"
            })
            if (editable()) images.addView(ui.action("Remove screenshot ${index + 1}") {
                screenshots = screenshots.filterIndexed { i, _ -> i != index }
                dirty = true; renderImages(); update()
            }.apply { isEnabled = !working })
        }
    }

    private fun <T> run(
        operation: suspend (RideDatabaseHandle) -> T,
        onFinished: () -> Unit = {},
        done: (T) -> Unit,
    ) {
        if (working || dismissed) return
        working = true; update()
        charging.withDatabase(operation) { result ->
            if (dismissed) return@withDatabase
            onFinished()
            result.onSuccess(done).onFailure { message.text = it.message ?: "The recording could not be updated." }
            working = false; renderImages(); update()
        }
    }

    private fun refresh() {
        val test = testOperations
        if (test != null) {
            runTestOperation(test.load) { (next, evidence) -> applyEvidence(next, evidence) }
        } else {
            run({ database ->
                CaptureSubmissionClientHandle(database).use { client -> client.status(source) to client.evidence(source) }
            }) { (next, evidence) -> applyEvidence(next, evidence) }
        }
    }

    private fun applyEvidence(next: MobileCaptureSubmissionStatusDto?, evidence: MobileCaptureEvidenceDto) {
        status = next; note.setText(evidence.note); screenshots = evidence.screenshots
        noteLimit = evidence.noteLimitBytes.toInt(); screenshotLimit = evidence.screenshotLimit.toInt()
        loaded = true; dirty = false
        message.text = when (next?.localPayloadState) {
            MobileCaptureSubmissionLocalPayloadStateDto.REMOVAL_PENDING -> "Received privately · ${next.receipt?.operationId.orEmpty()} · Removing local capture payload"
            MobileCaptureSubmissionLocalPayloadStateDto.REMOVED -> context.getString(R.string.capture_sent_local_deleted)
            else -> when (next?.phase) {
                MobileCaptureSubmissionPhaseDto.SENT -> "Received privately · ${next.receipt?.operationId.orEmpty()}"
                MobileCaptureSubmissionPhaseDto.CANCELLED -> "Send cancelled"
                else -> ""
            }
        }
    }

    private fun save() {
        val text = note.text.toString(); val selected = screenshots
        run({ database -> CaptureSubmissionClientHandle(database).use { client ->
            val evidence = client.saveEvidence(source, text, selected)
            client.status(source) to evidence
        } }) { (next, evidence) ->
            status = next; note.setText(evidence.note); screenshots = evidence.screenshots
            dirty = false; message.text = "Saved"
        }
    }

    private fun review() {
        val test = testOperations
        if (test != null) {
            runTestOperation(test.prepare) { showSendConfirmation(it) }
        } else {
            run({ database -> CaptureSubmissionClientHandle(database).use { it.prepare(source) } }) {
                showSendConfirmation(it)
            }
        }
    }

    private fun showSendConfirmation(prepared: MobileCaptureSubmissionStatusDto) {
        status = prepared
        val categories = buildList {
            add("raw Bluetooth evidence, event order and timing")
            if (prepared.categories.location) add("location")
            if (prepared.categories.deviceIdentifiers) add("device and platform identifiers")
            if (prepared.categories.notes) add("descriptions and notes")
            if (prepared.categories.annotations) add("annotations and label boundaries")
            if (prepared.categories.settingsObservations) add("user-entered settings values")
            if (screenshots.isNotEmpty()) add("${screenshots.size} selected screenshots")
        }
        val confirmation = AlertDialog.Builder(context).setTitle("Send this capture privately?")
            .setMessage("Origin: ${captureSubmissionOriginLabel(context, prepared.origin)}. Integrity: ${captureSubmissionIntegrityLabel(context, prepared.integrity)}. Send ${categories.joinToString(", ")} to the CutOut maintainer for protocol support. Music is excluded. A matching receipt starts removal of the local capture payload.")
            .setNegativeButton("Not now", null)
            .setPositiveButton("Send privately", null)
            .create()
        confirmationDialog = confirmation
        confirmation.setOnShowListener {
            confirmation.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                confirmation.dismiss()
                message.text = ""
                val listener = sendProgress.begin(prepared.operationId)
                val test = testOperations
                if (test != null) {
                    runTestOperation({ test.send(prepared.operationId, listener) }, onFinished = {
                        sendProgress.clear()
                    }) { publishSendResult(it) }
                } else run({ database ->
                    val credential = charging.captureCredential()
                    CaptureSubmissionClientHandle(database).use { client ->
                        client.sendWithProgress(prepared.operationId, credential.identityId, credential.token, listener)
                    }
                }, onFinished = {
                    sendProgress.clear()
                }) { publishSendResult(it) }
            }
        }
        confirmation.show()
    }

    private fun publishSendResult(next: MobileCaptureSubmissionStatusDto) {
        status = next
        if (next.phase == MobileCaptureSubmissionPhaseDto.SENT && next.receipt != null) {
            onReceiptConfirmed()
            message.text = "Received privately · ${next.receipt!!.operationId} · Removing local capture payload"
        } else {
            message.text = "Send has not completed. Retry to confirm its receipt."
        }
        onStatusChanged()
    }

    private fun <T> runTestOperation(operation: () -> T, done: (T) -> Unit) =
        runTestOperation(operation, onFinished = {}, done = done)

    private fun <T> runTestOperation(operation: () -> T, onFinished: () -> Unit, done: (T) -> Unit) {
        if (working || dismissed) return
        working = true; update()
        runCatching(operation).onSuccess(done).onFailure { message.text = it.message ?: "The recording could not be updated." }
        onFinished()
        working = false; renderImages(); update()
    }

    private fun removeSent(deleting: Boolean) {
        val operation = status?.operationId ?: return
        val testRemove = testOperations?.remove
        if (testRemove != null) {
            runTestOperation({ testRemove(operation, deleting) }) { next ->
                status = next
                onStatusChanged()
                message.text = if (next.phase == MobileCaptureSubmissionPhaseDto.DELETED) "Sent copy deleted" else if (next.phase == MobileCaptureSubmissionPhaseDto.CANCELLED) "Send cancelled" else "Removal is pending. Retry to confirm it."
            }
            return
        }
        run({ database ->
            val credential = charging.existingCaptureCredential()
            CaptureSubmissionClientHandle(database).use { client ->
                if (deleting) {
                    check(credential != null) { "The existing account is required to delete this sent copy." }
                    client.delete(operation, credential.identityId, credential.token)
                } else client.cancel(operation, credential?.identityId, credential?.token)
            }
        }) { next ->
            status = next
            onStatusChanged()
            message.text = if (next.phase == MobileCaptureSubmissionPhaseDto.DELETED) "Sent copy deleted" else if (next.phase == MobileCaptureSubmissionPhaseDto.CANCELLED) "Send cancelled" else "Removal is pending. Retry to confirm it."
        }
    }

    private fun importImages(uris: List<Uri>) {
        if (uris.size > screenshotLimit - screenshots.size) {
            message.text = "Choose up to four screenshots."
            return
        }
        run({ _ -> uris.map { uri ->
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            context.contentResolver.openInputStream(uri).use { BitmapFactory.decodeStream(it, null, bounds) }
            require(bounds.outWidth > 0 && bounds.outHeight > 0) { "The screenshot could not be read." }
            val options = BitmapFactory.Options().apply {
                inSampleSize = 1
                while (maxOf(bounds.outWidth, bounds.outHeight) / inSampleSize > 2048) inSampleSize *= 2
            }
            val bitmap = context.contentResolver.openInputStream(uri).use { BitmapFactory.decodeStream(it, null, options) }
                ?: error("The screenshot could not be read.")
            try {
                ByteArrayOutputStream().use { output ->
                    check(bitmap.compress(Bitmap.CompressFormat.JPEG, 85, output)) { "The screenshot could not be read." }
                    output.toByteArray()
                }
            } finally { bitmap.recycle() }
        } }) { imported ->
            if (screenshots.size + imported.size > screenshotLimit) message.text = "Choose up to four screenshots."
            else { screenshots += imported; dirty = true; message.text = "" }
        }
    }
}
