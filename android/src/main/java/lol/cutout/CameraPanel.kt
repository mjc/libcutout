package lol.cutout

import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.core.content.FileProvider
import uniffi.cutout_mobile_ffi.*
import java.io.File

/** Native camera route. The Rust session owns camera identity, read evidence, and command authorization. */
class CameraPanel(
    context: Context,
    private val client: AndroidCameraClient,
) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val content = ui.column()
    private val statusCard = ui.column()
    private val captureCard = ui.column()
    private val statusTitle = ui.text("", 20f, true)
    private val statusDetail = ui.text("", 14f, color = ui.muted)
    private val error = ui.text("", 14f, color = ui.red)
    private val connect = ui.action("", true) { client.loadEvidence() }
    private val captureTitle = ui.text(ui.label("camera.truth.title"), 17f, true)
    private val truthRows = ui.column()
    private val recordingControls = ui.row()
    private val stillButton = ui.action(ui.label("camera.still.request")) { client.requestStillCapture() }
    private val stillStatus = ui.text("", 13f, color = ui.muted)
    private val previewSurface = SurfaceView(context)
    private val previewStatus = ui.text("", 13f, color = ui.muted)
    private val previewControls = ui.row()
    private val mediaTitle = ui.text(ui.label("camera.evidence.media_title"), 17f, true)
    private val mediaRows = ui.column()
    private val loadMedia = ui.action(ui.label("camera.evidence.media_title")) { client.loadMedia() }
    private val loadMoreMedia = ui.action(ui.label("camera.evidence.media_load_more")) { mediaPage++ ; renderMedia(currentState.evidence!!) }
    private val filesCard = ui.column()
    private var active = false
    private var observation: AutoCloseable? = null
    private var previewRequested = false
    private var saveRequested = false
    private var previewStartIssued = false
    private var currentState = client.current()
    private var lastConnected = false
    private var mediaPage = 1
    private val holderCallback = object : SurfaceHolder.Callback {
        override fun surfaceCreated(holder: SurfaceHolder) {
            client.setPreviewSurface(holder.surface)
            launchPreviewIfReady()
        }
        override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
            client.setPreviewSurface(holder.surface)
        }
        override fun surfaceDestroyed(holder: SurfaceHolder) { client.setPreviewSurface(null) }
    }

    init {
        setBackgroundColor(ui.background)
        clipToPadding = false
        content.setPadding(ui.dp(24), ui.dp(24), ui.dp(24), ui.dp(32))
        content.addView(ui.title(ui.label("navigation.section.camera")))
        content.addView(ui.space(20))
        statusCard.background = ui.rounded(24)
        statusCard.tag = "camera.setup.card"
        statusCard.setPadding(ui.dp(20), ui.dp(18), ui.dp(20), ui.dp(18))
        statusCard.addView(statusTitle)
        statusCard.addView(ui.space(8))
        statusCard.addView(statusDetail)
        statusCard.addView(connect)
        statusCard.addView(error)
        content.addView(statusCard)
        content.addView(ui.space(16))

        captureCard.background = ui.rounded(24)
        captureCard.tag = "camera.truth.card"
        captureCard.setPadding(ui.dp(20), ui.dp(18), ui.dp(20), ui.dp(18))
        captureCard.addView(captureTitle)
        captureCard.addView(ui.space(12))
        captureCard.addView(truthRows)
        captureCard.addView(ui.space(10))
        captureCard.addView(recordingControls)
        captureCard.addView(stillButton)
        captureCard.addView(stillStatus)
        previewSurface.setBackgroundColor(android.graphics.Color.BLACK)
        previewSurface.visibility = View.GONE
        previewSurface.setOnClickListener { /* Keep the preview surface passive, like SwiftUI. */ }
        previewSurface.holder.addCallback(holderCallback)
        captureCard.addView(previewSurface, LinearLayout.LayoutParams(-1, ui.dp(190)).apply {
            topMargin = ui.dp(14)
        })
        captureCard.addView(previewStatus)
        captureCard.addView(previewControls)
        content.addView(captureCard)
        content.addView(ui.space(16))

        filesCard.apply {
            background = ui.rounded(24); setPadding(ui.dp(20), ui.dp(18), ui.dp(20), ui.dp(18))
        }
        filesCard.addView(mediaTitle)
        loadMedia.tag = "camera.files.load"
        filesCard.addView(loadMedia)
        filesCard.addView(mediaRows)
        content.addView(filesCard)
        addView(content, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
        render(currentState)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (observation == null) observation = client.observe { render(it) }
    }

    override fun onDetachedFromWindow() {
        observation?.close()
        observation = null
        super.onDetachedFromWindow()
    }

    fun setActive(value: Boolean) {
        if (active == value) return
        active = value
        client.setActive(value)
        if (!value && previewRequested) {
            previewRequested = false
            previewStartIssued = false
            client.stopPreview()
        }
    }

    fun render(state: CameraPanelState) {
        currentState = state
        statusTitle.text = when (state.connection) {
            CameraConnectionState.NOT_CONFIGURED -> ui.label("camera.connection.not_configured")
            CameraConnectionState.WIFI_REQUIRED -> ui.label("camera.connection.wifi_required")
            CameraConnectionState.DISCOVERING -> ui.label("camera.connection.discovering")
            CameraConnectionState.CONNECTED -> ui.label("camera.connection.connected")
            CameraConnectionState.UNSUPPORTED -> ui.label("camera.connection.unsupported")
        }
        statusDetail.text = when (state.connection) {
            CameraConnectionState.NOT_CONFIGURED -> ui.label("camera.connection.detail.not_configured")
            CameraConnectionState.WIFI_REQUIRED -> ui.label("camera.connection.detail.wifi_required")
            CameraConnectionState.DISCOVERING -> ui.label("camera.connection.detail.discovering")
            CameraConnectionState.CONNECTED -> ui.label("camera.connection.detail.connected")
            CameraConnectionState.UNSUPPORTED -> ui.label("camera.connection.detail.unsupported")
        }
        error.text = state.error.orEmpty()
        connect.text = ui.label(if (state.isReading) "camera.origin.reading" else "camera.origin.read")
        connect.tag = "camera.connect"
        connect.isEnabled = active && state.wifiReady && !state.isReading
        statusDetail.visibility = if (state.connection == CameraConnectionState.CONNECTED) View.GONE else View.VISIBLE
        error.visibility = if (state.error == null) View.GONE else View.VISIBLE
        val evidence = state.evidence
        val isConnected = state.connection == CameraConnectionState.CONNECTED && evidence != null
        captureCard.visibility = if (isConnected) View.VISIBLE else View.GONE
        filesCard.visibility = if (isConnected) View.VISIBLE else View.GONE
        if (isConnected && !lastConnected) {
            previewSurface.visibility = View.VISIBLE
            previewRequested = true
            previewSurface.post(::launchPreviewIfReady)
        }
        lastConnected = isConnected
        if (evidence == null) return

        truthRows.removeAllViews()
        truthRows.addView(truthRow(ui.label("camera.truth.preview"), previewLabel(state.preview?.preview)))
        truthRows.addView(truthRow(ui.label("camera.truth.recording"), recordingLabel(state.preview?.onboardRecording)))
        truthRows.addView(truthRow(ui.label("camera.truth.storage"),
            ui.label(if (evidence.storagePresent) "camera.storage.present" else "camera.storage.missing")))
        captureTitle.contentDescription = "${ui.label("camera.truth.title")}, ${ui.label("camera.evidence.firmware")}: ${evidence.firmwareVersion}"

        val supportsRecording = evidence.configuration.any { it.commandId.toInt() == 2001 && it.status.toInt() == 0 }
        recordingControls.removeAllViews()
        if (supportsRecording) {
            recordingControls.addView(ui.action(ui.label(if (state.recordingRequest == CameraRequestState.IN_FLIGHT)
                "camera.recording.requesting" else "camera.recording.request_start"), true) { client.requestRecording(true) },
                LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply { marginEnd = ui.dp(8) })
            recordingControls.addView(ui.action(ui.label("camera.recording.request_stop")) { client.requestRecording(false) },
                LinearLayout.LayoutParams(0, ui.dp(48), 1f))
        } else {
            recordingControls.addView(ui.text(ui.label("camera.recording.unavailable"), 14f, color = ui.muted))
        }
        val supportsStill = evidence.configuration.any { it.commandId.toInt() == 1001 && it.status.toInt() == 0 }
        stillButton.visibility = if (supportsStill) View.VISIBLE else View.GONE
        stillStatus.text = requestLabel(state.stillRequest, "camera.still.request_sent")
        stillStatus.visibility = if (state.stillRequest == null) View.GONE else View.VISIBLE

        val preview = state.preview?.preview ?: MobileCameraPreviewStateDto.STOPPED
        previewStatus.text = previewLabel(preview)
        if (preview !in listOf(MobileCameraPreviewStateDto.STOPPED, MobileCameraPreviewStateDto.UNAVAILABLE)) {
            previewSurface.visibility = View.VISIBLE
        } else if (!previewRequested) {
            previewSurface.visibility = View.GONE
        }
        previewControls.removeAllViews()
        val previewStarted = preview !in listOf(MobileCameraPreviewStateDto.STOPPED, MobileCameraPreviewStateDto.INTERRUPTED)
        if (previewStarted) {
            previewControls.addView(ui.action(ui.label("camera.preview.stop")) {
                previewRequested = false; previewStartIssued = false; client.stopPreview()
            }.apply { tag = "camera.preview.stop" })
        } else {
            previewControls.addView(ui.action(ui.label("camera.preview.start")) {
                previewRequested = true; previewStartIssued = false; saveRequested = false
                previewSurface.visibility = View.VISIBLE; launchPreviewIfReady()
            }.apply { tag = "camera.preview.start" })
        }
        previewControls.addView(ui.action(ui.label("camera.preview.save"), true) {
            previewRequested = true; previewStartIssued = false; saveRequested = true
            previewSurface.visibility = View.VISIBLE; launchPreviewIfReady()
        }, LinearLayout.LayoutParams(-2, ui.dp(48)).apply { marginStart = ui.dp(8) })

        renderMedia(evidence)
        state.savedPreviewFile?.let { file ->
            previewControls.addView(ui.action(ui.label("camera.preview.export")) { share(file) },
                LinearLayout.LayoutParams(-2, ui.dp(48)).apply { marginStart = ui.dp(8) })
        }
        state.operationError?.let { error.text = it; error.visibility = View.VISIBLE }
    }

    private fun renderMedia(evidence: MobileNovatekReadOnlySnapshotDto) {
        val media = evidence.media.orEmpty()
        mediaRows.removeAllViews()
        mediaPage = mediaPage.coerceAtLeast(1)
        loadMedia.visibility = if (evidence.media != null) View.GONE else View.VISIBLE
        if (evidence.media == null) return
        if (media.isEmpty()) {
            mediaRows.addView(ui.text(ui.label("camera.evidence.metadata_only"), 14f, color = ui.muted))
            return
        }
        mediaRows.addView(ui.space(8))
        media.take(mediaPage * 100).forEachIndexed { index, entry ->
            if (index > 0) mediaRows.addView(ui.divider())
            val row = ui.column()
            row.setPadding(0, ui.dp(12), 0, ui.dp(12))
            val top = ui.row()
            val details = ui.column().apply { layoutParams = LinearLayout.LayoutParams(0, -2, 1f) }
            details.addView(ui.text(entry.name, 14f, true).apply { maxLines = 1; ellipsize = android.text.TextUtils.TruncateAt.END })
            details.addView(ui.text(entry.time, 12f, color = ui.muted))
            top.addView(details)
            top.addView(ui.action(ui.label(if (stateIsDownloading(entry.path)) "camera.evidence.media_cancel" else "camera.evidence.media_download")) {
                if (stateIsDownloading(entry.path)) client.cancelDownload() else client.download(entry)
            })
            row.addView(top)
            stateThumb(entry.path)?.let { bytes ->
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.let { bitmap ->
                    row.addView(ImageView(context).apply { setImageBitmap(bitmap); adjustViewBounds = true; maxHeight = ui.dp(110) })
                }
            }
            val buttons = ui.row()
            stateDownloaded(entry.path)?.let { file ->
                buttons.addView(ui.text(ui.label("camera.evidence.media_saved"), 12f, color = ui.muted))
                buttons.addView(ui.action(ui.label("camera.evidence.media_export")) { share(file) },
                    LinearLayout.LayoutParams(-2, ui.dp(44)).apply { marginStart = ui.dp(8) })
            }
            if (mobileNovatekMediaThumbnailsSupported(evidence.firmwareVersion, evidence.configuration)) {
                val thumbnailButton = ui.action(ui.label("camera.evidence.thumbnail_request")) { client.loadThumbnail(entry) }
                thumbnailButton.isEnabled = currentState.thumbnailPath == null || currentState.thumbnailPath == entry.path
                buttons.addView(thumbnailButton, LinearLayout.LayoutParams(-2, ui.dp(44)).apply { marginStart = ui.dp(8) })
            }
            if (buttons.childCount > 0) row.addView(buttons)
            mediaRows.addView(row)
        }
        loadMoreMedia.visibility = if (mediaPage * 100 < media.size) View.VISIBLE else View.GONE
        if (loadMoreMedia.parent == null) mediaRows.addView(loadMoreMedia)
        mediaRows.addView(ui.space(8))
        mediaRows.addView(ui.text(ui.label("camera.evidence.media_local_only"), 12f, color = ui.muted))
    }

    private fun stateIsDownloading(path: String) = currentState.downloadingPath == path
    private fun stateDownloaded(path: String) = currentState.downloaded[path]
    private fun stateThumb(path: String) = currentState.thumbnails[path]

    private fun launchPreviewIfReady() {
        if (!active || !previewRequested || previewStartIssued || !previewSurface.holder.surface.isValid) return
        previewStartIssued = true
        client.startPreview(saveRequested)
    }

    private fun truthRow(title: String, value: String): View = ui.row().apply {
        setPadding(0, ui.dp(7), 0, ui.dp(7))
        addView(ui.text(title, 14f), LinearLayout.LayoutParams(0, -2, 1f))
        addView(ui.text(value, 14f, color = ui.muted), LinearLayout.LayoutParams(-2, -2))
        contentDescription = "$title, $value"
    }

    private fun previewLabel(state: MobileCameraPreviewStateDto?): String = ui.label(when (state) {
        MobileCameraPreviewStateDto.BUFFERING -> "camera.preview.buffering"
        MobileCameraPreviewStateDto.LIVE -> "camera.preview.live"
        MobileCameraPreviewStateDto.STALE -> "camera.preview.stale"
        MobileCameraPreviewStateDto.INTERRUPTED -> "camera.preview.interrupted"
        MobileCameraPreviewStateDto.UNAVAILABLE -> "camera.preview.unavailable"
        else -> "camera.preview.stopped"
    })

    private fun recordingLabel(state: MobileCameraOnboardRecordingStateDto?): String = ui.label(when (state) {
        MobileCameraOnboardRecordingStateDto.STOPPED -> "camera.recording.stopped"
        MobileCameraOnboardRecordingStateDto.RECORDING -> "camera.recording.active"
        else -> "camera.recording.unknown"
    })

    private fun requestLabel(state: CameraRequestState?, acknowledged: String): String = ui.label(when (state) {
        CameraRequestState.IN_FLIGHT -> "camera.recording.requesting"
        CameraRequestState.ACKNOWLEDGED -> acknowledged
        CameraRequestState.REFUSED -> "camera.command.refused"
        CameraRequestState.UNKNOWN -> "camera.command.unknown"
        CameraRequestState.TIMED_OUT -> "camera.command.timed_out"
        CameraRequestState.FAILED -> "camera.command.failed"
        null -> "camera.command.unknown"
    })

    private fun share(file: File) {
        runCatching {
            val uri: Uri = FileProvider.getUriForFile(context, "${context.packageName}.exports", file)
            context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
                type = "application/octet-stream"; putExtra(Intent.EXTRA_STREAM, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }, ui.label("camera.evidence.media_export")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }.onFailure { currentState = currentState.copy(operationError = it.localizedMessage) }
    }
}
