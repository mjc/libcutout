package lol.cutout

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.media.MediaCodec
import android.media.MediaFormat
import uniffi.cutout_mobile_ffi.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.SocketTimeoutException
import java.net.URL
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference

data class CameraCaptureIdentity(
    val generation: MobileCaptureGenerationDto,
    val fileName: String,
)

data class CameraPanelState(
    val wifiReady: Boolean = false,
    val isReading: Boolean = false,
    val connection: CameraConnectionState = CameraConnectionState.NOT_CONFIGURED,
    val evidence: MobileNovatekReadOnlySnapshotDto? = null,
    val preview: MobileCameraSnapshotDto? = null,
    val error: String? = null,
    val recordingRequest: CameraRequestState? = null,
    val stillRequest: CameraRequestState? = null,
    val downloadingPath: String? = null,
    val downloaded: Map<String, File> = emptyMap(),
    val thumbnailPath: String? = null,
    val thumbnails: Map<String, ByteArray> = emptyMap(),
    val operationError: String? = null,
    val savedPreviewFile: File? = null,
)

enum class CameraConnectionState { NOT_CONFIGURED, WIFI_REQUIRED, DISCOVERING, CONNECTED, UNSUPPORTED }
enum class CameraRequestState { IN_FLIGHT, ACKNOWLEDGED, REFUSED, UNKNOWN, TIMED_OUT, FAILED }

/** Android transport and decoder effects around the Rust-owned Novatek and RTSP boundaries. */
class AndroidCameraClient(
    private val context: Context,
    private val sessionState: () -> CutoutSessionStateHandle,
    private val currentCaptureIdentity: (() -> CameraCaptureIdentity?)? = null,
    private val onMediaReference: ((CameraCaptureIdentity, MobileCameraMediaProvenanceDto, File) -> Unit)? = null,
) {
    private val worker = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(32),
        { Thread(it, "CutoutCameraIO") }, ThreadPoolExecutor.AbortPolicy(),
    )
    private val main = Handler(Looper.getMainLooper())
    private val callbacks = CopyOnWriteArrayList<(CameraPanelState) -> Unit>()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO.limitedParallelism(1))
    private val networkManager = context.getSystemService(ConnectivityManager::class.java)
    private var networkCallback: ConnectivityManager.NetworkCallback? = null
    private var wifiNetwork: Network? = null
    @Volatile private var connectedNetwork: Network? = null
    private var active = false
    private var readingGeneration = 0L
    private var commandInFlight = false
    private var previewJob: Job? = null
    @Volatile private var previewSession: MobileCameraPreviewSession? = null
    @Volatile private var previewSink: MobileCameraPreviewFileSink? = null
    @Volatile private var previewSurface: Surface? = null
    private var codec: MediaCodec? = null
    private var codecConfiguration: MobileCameraVideoConfigurationDto? = null
    private var previewFileDestination: File? = null
    @Volatile private var view = CameraPanelState()
    private val previewGeneration = AtomicLong()
    private val activeHttp = AtomicReference<HttpURLConnection?>(null)
    @Volatile private var closed = false

    fun current(): CameraPanelState = view

    fun observe(callback: (CameraPanelState) -> Unit): AutoCloseable {
        callbacks += callback
        main.post { if (callback in callbacks) callback(view) }
        return AutoCloseable { callbacks.remove(callback) }
    }

    fun setActive(value: Boolean) {
        if (!value) activeHttp.getAndSet(null)?.disconnect()
        enqueue {
            if (active == value) return@enqueue
            active = value
            if (value) startNetworkObservation() else stopOnWorker()
        }
    }

    fun loadEvidence(includeMedia: Boolean = false) = enqueue {
        if (!active || !view.wifiReady || view.isReading) return@enqueue
        val generation = ++readingGeneration
        val state = sessionState()
        state.advanceCameraDiscoveryGeneration()
        val sessionToken = state.cameraSessionToken()
        val discoveryToken = state.cameraDiscoveryToken()
        val preserveConnection = includeMedia && view.evidence != null
        update(view.copy(isReading = true, error = null,
            connection = if (preserveConnection) CameraConnectionState.CONNECTED else CameraConnectionState.DISCOVERING))
        try {
            val origin = mobileNovatekR3ProOrigin()
            val firmware = request(origin, MobileNovatekReadCommandDto.FIRMWARE_VERSION, 4 * 1024)
            val liveView = request(origin, MobileNovatekReadCommandDto.LIVE_VIEW_FORMAT, 4 * 1024)
            val configuration = request(origin, MobileNovatekReadCommandDto.CONFIGURATION, 4 * 1024)
            val storage = request(origin, MobileNovatekReadCommandDto.STORAGE_PRESENT, 4 * 1024)
            val media = if (includeMedia) request(origin, MobileNovatekReadCommandDto.MEDIA_LIST, 512 * 1024) else null
            if (generation != readingGeneration || !state.cameraSessionTokenIsCurrent(sessionToken) ||
                !state.cameraDiscoveryTokenIsCurrent(discoveryToken)) return@enqueue
            val evidence = if (media != null) {
                state.configureNovatekReadOnlySession(origin, firmware, liveView, configuration, storage, media)
            } else {
                state.configureNovatekConnection(origin, firmware, liveView, configuration, storage)
            }
            if (!mobileNovatekFirmwareIsR3Pro(evidence.firmwareVersion)) {
                state.invalidateCameraLifecycle()
                state.clearNovatekSession()
                update(view.copy(isReading = false, connection = CameraConnectionState.UNSUPPORTED,
                    evidence = null, preview = state.cameraSnapshot(), error = context.getString(R.string.camera_unsupported)))
                return@enqueue
            }
            update(view.copy(isReading = false, connection = CameraConnectionState.CONNECTED,
                evidence = evidence, preview = state.cameraSnapshot(), error = null, operationError = null))
        } catch (error: Exception) {
            if (generation == readingGeneration) {
                Log.w(TAG, "Camera evidence request failed", error)
                update(view.copy(isReading = false,
                    connection = if (preserveConnection) CameraConnectionState.CONNECTED else if (view.wifiReady) CameraConnectionState.NOT_CONFIGURED else CameraConnectionState.WIFI_REQUIRED,
                    evidence = if (preserveConnection) view.evidence else null,
                    error = context.getString(R.string.camera_read_failed)))
            }
        }
    }

    fun requestRecording(start: Boolean) = command(if (start) MobileNovatekCommandDto.START_RECORDING else MobileNovatekCommandDto.STOP_RECORDING, true)
    fun requestStillCapture() = command(MobileNovatekCommandDto.STILL_CAPTURE, false)

    private fun command(command: MobileNovatekCommandDto, recording: Boolean) = enqueue {
        if (!active || !view.wifiReady || view.evidence == null || commandInFlight) {
            update(view.copy(operationError = context.getString(R.string.camera_command_unavailable)))
            return@enqueue
        }
        commandInFlight = true
        val state = sessionState()
        val origin = state.novatekSessionOrigin()
        val token = state.cameraSessionToken()
        if (origin == null) {
            commandInFlight = false
            update(view.copy(operationError = context.getString(R.string.camera_command_unavailable)))
            return@enqueue
        }
        val request = try { state.authorizeNovatekCommand(origin, command) } catch (_: Exception) {
            commandInFlight = false
            update(view.copy(operationError = context.getString(R.string.camera_command_unavailable)))
            return@enqueue
        }
        update(if (recording) view.copy(recordingRequest = CameraRequestState.IN_FLIGHT, operationError = null)
            else view.copy(stillRequest = CameraRequestState.IN_FLIGHT, operationError = null))
        try {
            val body = fetch(origin, request.target, 4 * 1024)
            if (!state.cameraSessionTokenIsCurrent(token)) throw StaleCameraWork()
            val outcome = try { state.completeNovatekCommand(request, body) } catch (_: Exception) { MobileNovatekCommandOutcomeDto.UNKNOWN }
            val result = when (outcome) {
                MobileNovatekCommandOutcomeDto.ACKNOWLEDGED -> CameraRequestState.ACKNOWLEDGED
                MobileNovatekCommandOutcomeDto.REFUSED -> CameraRequestState.REFUSED
                MobileNovatekCommandOutcomeDto.UNKNOWN -> CameraRequestState.UNKNOWN
            }
            update(if (recording) view.copy(recordingRequest = result, operationError = null)
                else view.copy(stillRequest = result, operationError = null))
        } catch (error: SocketTimeoutException) {
            update(if (recording) view.copy(recordingRequest = CameraRequestState.TIMED_OUT)
                else view.copy(stillRequest = CameraRequestState.TIMED_OUT))
        } catch (_: StaleCameraWork) {
            update(view.copy(operationError = context.getString(R.string.camera_wifi_required)))
        } catch (error: Exception) {
            Log.w(TAG, "Camera command failed", error)
            update(if (recording) view.copy(recordingRequest = CameraRequestState.FAILED)
                else view.copy(stillRequest = CameraRequestState.FAILED))
        } finally { commandInFlight = false }
    }

    fun setPreviewSurface(surface: Surface?) = enqueue {
        val previous = previewSurface
        if (previous === surface) return@enqueue
        val wasPreviewing = previewJob?.isActive == true
        val shouldSave = previewFileDestination != null
        previewSurface = surface
        if (wasPreviewing) {
            stopPreviewOnWorker()
            if (surface?.isValid == true && active && view.evidence != null) startPreview(shouldSave)
        }
    }

    fun startPreview(save: Boolean = false) = enqueue {
        val evidence = view.evidence ?: return@enqueue
        if (!active || !view.wifiReady || connectedNetwork == null ||
            connectedNetwork != networkManager.activeNetwork || previewSurface?.isValid != true) return@enqueue
        stopPreviewOnWorker()
        val generation = previewGeneration.incrementAndGet()
        val state = sessionState()
        state.advanceCameraPreviewGeneration()
        val token = state.cameraPreviewToken()
        update(view.copy(preview = state.cameraSnapshot(), operationError = null, savedPreviewFile = if (save) null else view.savedPreviewFile))
        val destination = if (save) cameraDirectory().resolve("camera-preview-${System.currentTimeMillis()}.h264") else null
        previewFileDestination = destination
        previewJob = scope.launch {
            var session: MobileCameraPreviewSession? = null
            var sink: MobileCameraPreviewFileSink? = null
            try {
                session = mobileCameraPreviewConnectForOrigin(evidence.movieRtspUri, mobileNovatekR3ProOrigin().address)
                if (generation != previewGeneration.get() || !state.cameraPreviewTokenIsCurrent(token) || previewSurface?.isValid != true) {
                    session.stop(); return@launch
                }
                val configuration = session.videoConfiguration()
                if (configuration != null) configureCodec(configuration)
                sink = destination?.let { MobileCameraPreviewFileSink.create(it.absolutePath) }
                previewSession = session
                previewSink = sink
                state.reduceCameraPreview(MobileCameraPreviewEventDto.STARTED)
                publishSnapshot(state)
                while (generation == previewGeneration.get()) {
                    val frame = session.nextVideoFrame() ?: break
                    if (generation != previewGeneration.get() || !state.cameraPreviewTokenIsCurrent(token)) break
                    sink?.writeFrame(frame)
                    val accepted = renderFrame(frame)
                    if (accepted) {
                        state.reduceCameraPreview(MobileCameraPreviewEventDto.FRAME_RECEIVED)
                        publishSnapshot(state)
                        if (destination != null && destination.length() > 0L) update(view.copy(savedPreviewFile = destination))
                    }
                }
                if (generation == previewGeneration.get()) state.reduceCameraPreview(MobileCameraPreviewEventDto.INTERRUPTED)
            } catch (error: CancellationException) {
                session?.stop()
            } catch (error: Exception) {
                Log.w(TAG, "Camera preview failed", error)
                if (generation == previewGeneration.get()) {
                    state.reduceCameraPreview(MobileCameraPreviewEventDto.INTERRUPTED)
                    update(view.copy(operationError = context.getString(R.string.camera_preview_failed)))
                }
            } finally {
                runCatching { sink?.finish() }
                if (previewSink === sink) previewSink = null
                if (previewSession === session) previewSession = null
                session?.stop()
            }
        }
    }

    fun stopPreview() = enqueue { stopPreviewOnWorker() }

    private fun stopPreviewOnWorker() {
        previewGeneration.incrementAndGet()
        val state = sessionState()
        state.advanceCameraPreviewGeneration()
        previewJob?.cancel(); previewJob = null
        runCatching { previewSession?.stop() }; previewSession = null
        runCatching { previewSink?.finish() }; previewSink = null
        stopCodec()
        state.reduceCameraPreview(MobileCameraPreviewEventDto.STOPPED)
        publishSnapshot(state)
    }

    fun loadMedia() = loadEvidence(includeMedia = true)

    fun download(media: MobileNovatekMediaEntryDto) = enqueue {
        if (!active || !view.wifiReady || view.evidence?.media?.none { it.path == media.path && it.sizeBytes == media.sizeBytes } != false) {
            update(view.copy(operationError = context.getString(R.string.camera_media_unavailable))); return@enqueue
        }
        val state = sessionState()
        val token = state.cameraSessionToken()
        val origin = state.novatekSessionOrigin() ?: return@enqueue
        val request = try { state.authorizeNovatekMediaDownload(media.path) } catch (_: Exception) {
            update(view.copy(operationError = context.getString(R.string.camera_media_unavailable))); return@enqueue
        }
        val capture = currentCaptureIdentity?.invoke()
        val capturedAtMonotonic = android.os.SystemClock.elapsedRealtime().toULong()
        val capturedAtWallClock = System.currentTimeMillis().coerceAtLeast(0).toULong()
        val destination = cameraDirectory().resolve("camera-media-${System.currentTimeMillis()}-${safeName(media.name)}")
        val temporary = File(context.cacheDir, "camera-download-${request.operationId}.tmp")
        update(view.copy(downloadingPath = media.path, operationError = null))
        try {
            val actualSize = download(origin, request.target, request.sizeBytes.toLong(), temporary)
            if (!state.cameraSessionTokenIsCurrent(token)) throw StaleCameraWork()
            if (actualSize.toULong() != request.sizeBytes) throw IllegalStateException("download size mismatch")
            if (!temporary.renameTo(destination)) throw IllegalStateException("could not save camera media")
            val association = capture?.let {
                MobileNovatekMediaDownloadAssociationInput(
                    it.generation, it.fileName, capturedAtMonotonic, capturedAtWallClock,
                    MobileCameraClockUncertaintyDto.Unknown)
            }
            val result = state.completeNovatekMediaDownload(MobileNovatekMediaDownloadCompletionInput(request.operationId, actualSize.toULong(), association))
            val downloaded = view.downloaded.toMutableMap().apply { put(media.path, destination) }
            update(view.copy(downloadingPath = null, downloaded = downloaded, operationError = null))
            if (capture != null && result.provenance != null) onMediaReference?.invoke(capture, result.provenance!!, destination)
        } catch (error: Exception) {
            temporary.delete(); destination.delete()
            runCatching { state.cancelNovatekMediaDownload(request.operationId) }
            Log.w(TAG, "Camera media download failed", error)
            update(view.copy(downloadingPath = null, operationError = context.getString(R.string.camera_media_download_failed)))
        }
    }

    fun cancelDownload() {
        activeHttp.getAndSet(null)?.disconnect()
        enqueue { update(view.copy(downloadingPath = null)) }
    }

    fun loadThumbnail(media: MobileNovatekMediaEntryDto) = enqueue {
        val state = sessionState()
        val token = state.cameraSessionToken()
        val origin = state.novatekSessionOrigin() ?: return@enqueue
        val target = try { state.novatekMediaThumbnailTarget(media.path, media.sizeBytes) } catch (_: Exception) {
            update(view.copy(operationError = context.getString(R.string.camera_thumbnail_unavailable))); return@enqueue
        }
        update(view.copy(thumbnailPath = media.path, operationError = null))
        try {
            val bytes = fetch(origin, target, MAX_THUMBNAIL_BYTES)
            if (!state.cameraSessionTokenIsCurrent(token)) throw StaleCameraWork()
            val images = view.thumbnails.toMutableMap().apply { put(media.path, bytes) }
            update(view.copy(thumbnailPath = null, thumbnails = images))
        } catch (error: Exception) {
            update(view.copy(thumbnailPath = null, operationError = context.getString(R.string.camera_thumbnail_failed)))
        }
    }

    private fun startNetworkObservation() {
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) { refreshNetwork(network) }
            override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                enqueue { applyNetwork(network, capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) }
            }
            override fun onLost(network: Network) {
                if (connectedNetwork == network) activeHttp.getAndSet(null)?.disconnect()
                enqueue { if (connectedNetwork == network) applyNetwork(null, false) }
            }
        }
        networkCallback = callback
        try { networkManager.registerDefaultNetworkCallback(callback) }
        catch (error: Exception) { update(view.copy(error = context.getString(R.string.camera_wifi_required))) }
        refreshNetwork(networkManager.activeNetwork)
    }

    private fun refreshNetwork(network: Network?) {
        enqueue {
            val caps = network?.let { runCatching { networkManager.getNetworkCapabilities(it) }.getOrNull() }
            applyNetwork(network, caps?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true)
        }
    }

    private fun applyNetwork(network: Network?, wifi: Boolean) {
        val changed = connectedNetwork != null && connectedNetwork != network || view.evidence != null && !wifi
        if (changed) {
            readingGeneration++
            stopPreviewOnWorker()
            val state = sessionState()
            state.invalidateCameraLifecycle()
            state.clearNovatekSession()
        } else {
            sessionState().advanceCameraDiscoveryGeneration()
        }
        connectedNetwork = network.takeIf { wifi }
        wifiNetwork = connectedNetwork
        update(view.copy(wifiReady = wifi, connection = if (wifi) CameraConnectionState.NOT_CONFIGURED else CameraConnectionState.WIFI_REQUIRED,
            evidence = if (changed) null else view.evidence, preview = sessionState().cameraSnapshot(),
            isReading = false, error = null))
    }

    private fun stopOnWorker() {
        readingGeneration++
        stopPreviewOnWorker()
        networkCallback?.let { runCatching { networkManager.unregisterNetworkCallback(it) } }
        networkCallback = null
        connectedNetwork = null
        wifiNetwork = null
        val state = sessionState()
        state.invalidateCameraLifecycle()
        state.clearNovatekSession()
        update(CameraPanelState(connection = CameraConnectionState.NOT_CONFIGURED, preview = state.cameraSnapshot()))
    }

    private fun request(origin: MobileNovatekHttpOriginDto, command: MobileNovatekReadCommandDto, max: Int): ByteArray =
        fetch(origin, mobileNovatekReadCommandTarget(command), max)

    private fun fetch(origin: MobileNovatekHttpOriginDto, target: String, maxBytes: Int): ByteArray {
        val url = URL("http://${origin.address}:${origin.port}$target")
        val network = wifiNetwork ?: throw IllegalStateException("camera Wi-Fi network is unavailable")
        val connection = (network.openConnection(url) as HttpURLConnection)
        activeHttp.set(connection)
        connection.instanceFollowRedirects = false
        connection.requestMethod = "GET"
        connection.connectTimeout = READ_TIMEOUT_MS
        connection.readTimeout = READ_TIMEOUT_MS
        connection.useCaches = false
        try {
            val status = connection.responseCode
            requireResponseOrigin(connection.url, origin)
            if (status !in 200..299) throw IllegalStateException("HTTP $status")
            if (connection.contentLengthLong > maxBytes) throw IllegalStateException("response too large")
            return connection.inputStream.use { readBounded(it, maxBytes) }
        } finally { activeHttp.compareAndSet(connection, null); connection.disconnect() }
    }

    private fun download(origin: MobileNovatekHttpOriginDto, target: String, maxBytes: Long, file: File): Long {
        val url = URL("http://${origin.address}:${origin.port}$target")
        val network = wifiNetwork ?: throw IllegalStateException("camera Wi-Fi network is unavailable")
        val connection = (network.openConnection(url) as HttpURLConnection)
        activeHttp.set(connection)
        connection.instanceFollowRedirects = false
        connection.requestMethod = "GET"; connection.connectTimeout = READ_TIMEOUT_MS
        connection.readTimeout = READ_TIMEOUT_MS; connection.useCaches = false
        try {
            val status = connection.responseCode
            requireResponseOrigin(connection.url, origin)
            if (status !in 200..299) throw IllegalStateException("HTTP $status")
            if (connection.contentLengthLong > maxBytes) throw IllegalStateException("file exceeds retained size")
            var count = 0L
            connection.inputStream.use { input -> FileOutputStream(file).use { output ->
                val buffer = ByteArray(32 * 1024)
                while (true) {
                    val read = input.read(buffer); if (read < 0) break
                    count += read
                    if (count > maxBytes) throw IllegalStateException("file exceeds retained size")
                    output.write(buffer, 0, read)
                }
            } }
            return count
        } finally { activeHttp.compareAndSet(connection, null); connection.disconnect() }
    }

    private fun requireResponseOrigin(url: URL, origin: MobileNovatekHttpOriginDto) {
        if (url.protocol != "http" || url.host != origin.address || (url.port.takeIf { it >= 0 } ?: 80) != origin.port.toInt())
            throw IllegalStateException("camera response changed origin")
    }

    private fun readBounded(input: java.io.InputStream, max: Int): ByteArray {
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(4096)
        while (true) {
            val count = input.read(buffer); if (count < 0) break
            if (output.size() + count > max) throw IllegalStateException("camera response exceeded bound")
            output.write(buffer, 0, count)
        }
        return output.toByteArray()
    }

    @Synchronized private fun configureCodec(configuration: MobileCameraVideoConfigurationDto) {
        val surface = previewSurface?.takeIf { it.isValid } ?: throw IllegalStateException("preview surface unavailable")
        if (!supportsCodec(configuration.codec) || configuration.parameterSets.size != 2)
            throw IllegalStateException("unsupported camera preview codec configuration")
        stopCodec()
        val format = MediaFormat.createVideoFormat("video/avc", configuration.width.toInt(), configuration.height.toInt())
        format.setByteBuffer("csd-0", annexB(configuration.parameterSets[0]))
        format.setByteBuffer("csd-1", annexB(configuration.parameterSets[1]))
        codec = MediaCodec.createDecoderByType("video/avc").apply { configure(format, surface, null, 0); start() }
        codecConfiguration = configuration
    }

    @Synchronized private fun renderFrame(frame: MobileCameraVideoFrameDto): Boolean {
        if (frame.parameterSets.size == 2) {
            val config = codecConfiguration
            if (config == null || !sameParameterSets(config.parameterSets, frame.parameterSets)) {
                val previous = codecConfiguration
                if (previous != null && previewSurface?.isValid == true) {
                    configureCodec(MobileCameraVideoConfigurationDto(previous.codec, previous.width,
                        previous.height, frame.parameterSets))
                } else return false
            }
        }
        val decoder = codec ?: return false
        return try {
            val inputIndex = decoder.dequeueInputBuffer(10_000)
            if (inputIndex < 0) return false
            val input = decoder.getInputBuffer(inputIndex) ?: return false
            input.clear()
            val annexBFrame = annexBAccessUnit(frame.data)
            if (annexBFrame.size > input.capacity()) return false
            input.put(annexBFrame)
            val presentationTimeUs = frame.timestamp.toDouble() * 1_000_000.0 / frame.clockRateHz.toDouble()
            if (!presentationTimeUs.isFinite() || presentationTimeUs < 0) return false
            decoder.queueInputBuffer(inputIndex, 0, annexBFrame.size, presentationTimeUs.toLong(),
                if (frame.isRandomAccessPoint) MediaCodec.BUFFER_FLAG_KEY_FRAME else 0)
            val info = MediaCodec.BufferInfo()
            var outputIndex = decoder.dequeueOutputBuffer(info, 0)
            while (outputIndex >= 0) {
                decoder.releaseOutputBuffer(outputIndex, true)
                outputIndex = decoder.dequeueOutputBuffer(info, 0)
            }
            true
        } catch (error: Exception) {
            Log.w(TAG, "Native H.264 decoder rejected frame", error)
            false
        }
    }

    @Synchronized private fun stopCodec() {
        codec?.let { runCatching { it.stop() }; runCatching { it.release() } }
        codec = null; codecConfiguration = null
    }

    fun close() {
        activeHttp.getAndSet(null)?.disconnect()
        setActive(false)
        closed = true
        scope.cancel()
        worker.shutdown()
    }

    private fun enqueue(action: () -> Unit) {
        if (closed) return
        try {
            worker.execute { action() }
        } catch (_: RejectedExecutionException) {
            update(view.copy(isReading = false, operationError = context.getString(R.string.camera_worker_busy)))
        }
    }

    private fun update(next: CameraPanelState) {
        view = next
        main.post { callbacks.forEach { it(next) } }
    }

    private fun publishSnapshot(state: CutoutSessionStateHandle) = update(view.copy(preview = state.cameraSnapshot()))

    private fun safeName(value: String): String = value.map { if (it.isLetterOrDigit() || it in "-_.") it else '_' }.joinToString("").take(120).ifBlank { "media" }
    private fun cameraDirectory(): File = File(context.filesDir, "camera").apply { mkdirs() }
    private fun sameParameterSets(left: List<ByteArray>, right: List<ByteArray>): Boolean =
        left.size == right.size && left.indices.all { left[it].contentEquals(right[it]) }
    private fun Boolean?.orFalse() = this == true
    private class StaleCameraWork : Exception()

    companion object {
        private const val TAG = "CutoutCamera"
        private const val READ_TIMEOUT_MS = 10_000
        private const val MAX_THUMBNAIL_BYTES = 2 * 1024 * 1024

        /** Rust retains the RFC 6381 codec identifier (for example, `avc1.4D401E`). */
        internal fun supportsCodec(codec: String): Boolean = codec.startsWith("avc1.", ignoreCase = true)

        /** Rust supplies AVCC access units with four-byte NAL length prefixes; Android codecs use Annex-B. */
        internal fun annexBAccessUnit(data: ByteArray): ByteArray {
            val out = ByteArrayOutputStream(data.size + 16)
            var offset = 0
            while (offset < data.size) {
                require(data.size - offset >= 4) { "truncated AVCC length" }
                val length = ((data[offset].toInt() and 255) shl 24) or ((data[offset + 1].toInt() and 255) shl 16) or
                    ((data[offset + 2].toInt() and 255) shl 8) or (data[offset + 3].toInt() and 255)
                require(length > 0 && length <= data.size - offset - 4) { "invalid AVCC NAL length" }
                out.write(byteArrayOf(0, 0, 0, 1)); out.write(data, offset + 4, length); offset += 4 + length
            }
            return out.toByteArray()
        }

        private fun annexB(parameterSet: ByteArray): java.nio.ByteBuffer =
            java.nio.ByteBuffer.allocate(parameterSet.size + 4).put(byteArrayOf(0, 0, 0, 1)).put(parameterSet).apply { flip() }
    }
}
