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

/** Native editing and image effects; Rust owns limits, persistence and immutable Send. */
internal class CaptureEvidenceDialog(
    private val context: Context,
    private val charging: AndroidChargingClient,
    private val source: MobileCaptureSubmissionSourceDto,
    private val pickImages: (((List<Uri>) -> Unit) -> Unit)?,
) {
    private val ui = CutoutUi(context)
    private val content = ui.column()
    private val note = EditText(context).apply {
        hint = "Add a note"; minLines = 3; maxLines = 6
        tag = "captures.evidence.note"
    }
    private val count = ui.text("0 / 1024 bytes", 13f, color = ui.muted)
    private val images = ui.column()
    private val message = ui.text("", 14f, color = ui.muted)
    private var screenshots = listOf<ByteArray>()
    private var status: MobileCaptureSubmissionStatusDto? = null
    private var working = false
    private var dirty = false
    private var loaded = false
    private var dismissed = false
    private var noteLimit = 1024
    private var screenshotLimit = 4
    private val add = ui.action("Add screenshots") { pickImages?.invoke(::importImages) }
    private val save = ui.action("Save note and screenshots") { save() }
    private val send = ui.action("Review private Send", primary = true) { review() }

    fun show() {
        content.setPadding(ui.dp(20), ui.dp(12), ui.dp(20), ui.dp(20))
        content.addView(note); content.addView(count); content.addView(images)
        content.addView(add); content.addView(save); content.addView(send); content.addView(message)
        val dialog = AlertDialog.Builder(context).setTitle("Recording notes")
            .setView(ScrollView(context).apply { addView(content) })
            .setNegativeButton("Close", null).create()
        dialog.setOnDismissListener { dismissed = true }
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
        send.isEnabled = loaded && !working && !dirty && status?.phase != MobileCaptureSubmissionPhaseDto.SENT
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

    private fun <T> run(operation: suspend (RideDatabaseHandle) -> T, done: (T) -> Unit) {
        if (working || dismissed) return
        working = true; update()
        charging.withDatabase(operation) { result ->
            if (dismissed) return@withDatabase
            result.onSuccess(done).onFailure { message.text = it.message ?: "The recording could not be updated." }
            working = false; renderImages(); update()
        }
    }

    private fun refresh() = run({ database ->
        CaptureSubmissionClientHandle(database).use { client -> client.status(source) to client.evidence(source) }
    }) { (next, evidence) ->
        status = next; note.setText(evidence.note); screenshots = evidence.screenshots
        noteLimit = evidence.noteLimitBytes.toInt(); screenshotLimit = evidence.screenshotLimit.toInt()
        loaded = true; dirty = false
        message.text = if (next?.phase == MobileCaptureSubmissionPhaseDto.SENT) "Received privately · ${next.receipt?.operationId.orEmpty()}" else ""
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

    private fun review() = run({ database ->
        CaptureSubmissionClientHandle(database).use { it.prepare(source) }
    }) { prepared ->
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
        AlertDialog.Builder(context).setTitle("Send this capture privately?")
            .setMessage("Send ${categories.joinToString(", ")} to the CutOut maintainer for protocol support. Music is excluded. Your original recording stays on this device.")
            .setNegativeButton("Not now", null)
            .setPositiveButton("Send privately") { _, _ ->
                run({ database ->
                    val credential = charging.captureCredential()
                    CaptureSubmissionClientHandle(database).use { it.send(prepared.operationId, credential.identityId, credential.token) }
                }) { next ->
                    status = next
                    message.text = if (next.receipt != null) "Received privately · ${next.receipt!!.operationId}" else "Send has not completed. Retry to confirm its receipt."
                }
            }.show()
    }

    private fun importImages(uris: List<Uri>) {
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
