package lol.cutout

import android.content.Context
import android.content.BroadcastReceiver
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import uniffi.cutout_mobile_ffi.MobileSoundCloudCommand
import uniffi.cutout_mobile_ffi.MobileSoundCloudEffect
import uniffi.cutout_mobile_ffi.MobileSoundCloudPlayer
import uniffi.cutout_mobile_ffi.MobileSoundCloudSnapshot
import uniffi.cutout_mobile_ffi.MobileSoundCloudState
import uniffi.cutout_mobile_ffi.MobileSoundCloudTrack
import java.io.ByteArrayOutputStream
import java.io.Closeable
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Future
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

data class AndroidSoundCloudPresentation(
    val tracks: List<MobileSoundCloudTrack> = emptyList(),
    val snapshot: MobileSoundCloudSnapshot,
    val isSearching: Boolean = false,
    val elapsedSeconds: Int = 0,
    val error: String? = null,
)

/** Executes Rust-admitted SoundCloud requests and HLS playback. No history or audio is retained. */
class AndroidSoundCloudPlayer(
    context: Context,
    private val core: MobileSoundCloudPlayer = MobileSoundCloudPlayer(),
    private val onPresentation: (AndroidSoundCloudPresentation) -> Unit = {},
    /** Completes after the app has a foreground media-playback owner for this request. */
    private val beforePlayback: ((completed: (Result<Unit>) -> Unit) -> Unit) = { it(Result.success(Unit)) },
) : Closeable {
    private data class HttpResult(val body: ByteArray, val status: Int, val location: String?)
    private val appContext = context.applicationContext
    private val main = Handler(Looper.getMainLooper())
    private val positionTick = object : Runnable {
        override fun run() {
            val media = player
            if (closed || media == null) return
            val elapsed = runCatching { (media.currentPosition / 1_000).coerceAtLeast(0) }.getOrDefault(0)
            publish(latest.copy(snapshot = core.snapshot(), elapsedSeconds = elapsed))
            if (media.isPlaying) main.postDelayed(this, 1_000)
        }
    }
    private val network = ThreadPoolExecutor(
        2, 2, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(8),
        { task -> Thread(task, "CutoutSoundCloudHttp").apply { isDaemon = true } },
        ThreadPoolExecutor.AbortPolicy(),
    )
    private val searchRevision = AtomicLong(0)
    private val playbackRequestRevision = AtomicLong(0)
    private val catalogueLock = Any()
    private val observers = CopyOnWriteArrayList<(AndroidSoundCloudPresentation) -> Unit>()
    private var searchTask: Future<*>? = null
    private var streamTask: Future<*>? = null
    private var player: MediaPlayer? = null
    private var activePlaybackId: ULong? = null
    private var focusRequest: AudioFocusRequest? = null
    private var foregroundRequestPlaybackId: ULong? = null
    private var foregroundReceiptPlaybackId: ULong? = null
    private var playRequestedPlaybackId: ULong? = null
    private val audioManager = appContext.getSystemService(AudioManager::class.java)
    private val mediaSession = MediaSession(appContext, "CutOutSoundCloud").apply {
        setFlags(MediaSession.FLAG_HANDLES_MEDIA_BUTTONS or MediaSession.FLAG_HANDLES_TRANSPORT_CONTROLS)
        setCallback(object : MediaSession.Callback() {
            override fun onPlay() = command(MobileSoundCloudCommand.PLAY)
            override fun onPause() = command(MobileSoundCloudCommand.PAUSE)
            override fun onSkipToPrevious() = command(MobileSoundCloudCommand.PREVIOUS)
            override fun onSkipToNext() = command(MobileSoundCloudCommand.NEXT)
            override fun onStop() = command(MobileSoundCloudCommand.PAUSE)
        }, main)
    }
    val mediaSessionToken: MediaSession.Token get() = mediaSession.sessionToken
    @Volatile var latest = AndroidSoundCloudPresentation(snapshot = core.snapshot())
        private set
    @Volatile private var closed = false
    @Volatile var nativePlaybackActive: Boolean = false
        private set
    private val focusListener = AudioManager.OnAudioFocusChangeListener { change ->
        when (change) {
            AudioManager.AUDIOFOCUS_GAIN -> if (core.snapshot().state == MobileSoundCloudState.PLAYING) {
                runCatching { player?.setVolume(1f, 1f) }
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> runCatching { player?.setVolume(0.2f, 0.2f) }
            AudioManager.AUDIOFOCUS_LOSS, AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> {
                runCatching { player?.pause() }
                nativePlaybackActive = false
                command(MobileSoundCloudCommand.PAUSE)
            }
        }
    }
    private val noisyRouteReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) {
                runCatching { player?.pause() }
                nativePlaybackActive = false
                command(MobileSoundCloudCommand.PAUSE)
            }
        }
    }

    init {
        appContext.registerReceiver(
            noisyRouteReceiver,
            IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY),
            Context.RECEIVER_NOT_EXPORTED,
        )
    }

    fun observe(observer: (AndroidSoundCloudPresentation) -> Unit): Closeable {
        observers += observer
        main.post { if (observer in observers) observer(latest) }
        return Closeable { observers -= observer }
    }

    fun search(query: String) {
        main.post {
            if (closed) return@post
            val revision = synchronized(catalogueLock) { searchRevision.incrementAndGet() }
            searchTask?.cancel(true)
            publish(latest.copy(isSearching = true, error = null))
            try {
                searchTask = network.submit {
                    runCatching {
                        val endpoint = core.searchEndpoint(query)
                        val response = authorizedRequest(endpoint)
                        check(response.status == HttpURLConnection.HTTP_OK) { appContext.getString(R.string.music_soundcloud_request_failed) }
                        synchronized(catalogueLock) {
                            if (closed || revision != searchRevision.get()) emptyList()
                            else core.acceptCatalogue(response.body)
                        }
                    }.onSuccess { tracks ->
                        main.post {
                            if (!closed && revision == searchRevision.get()) publish(latest.copy(tracks = tracks, isSearching = false, error = null))
                        }
                    }.onFailure { error ->
                        main.post {
                            if (!closed && revision == searchRevision.get()) publish(latest.copy(isSearching = false, error = appContext.getString(R.string.music_soundcloud_request_failed)))
                        }
                    }
                }
            } catch (error: RuntimeException) {
                publish(latest.copy(isSearching = false, error = appContext.getString(R.string.music_soundcloud_request_failed)))
            }
        }
    }

    fun select(track: MobileSoundCloudTrack) {
        main.post {
            runCatching { core.select(track.urn) }
                .onSuccess { effect ->
                    publish(latest.copy(error = null))
                    apply(effect)
                }
                .onFailure { publish(latest.copy(error = appContext.getString(R.string.music_soundcloud_play_failed))) }
        }
    }

    fun command(command: MobileSoundCloudCommand, completed: (Result<Unit>) -> Unit = {}) {
        main.post {
            val result = runCatching { core.command(command) }
            result.onSuccess { effect ->
                publish(latest.copy(error = null))
                apply(effect)
            }.onFailure { error ->
                publish(latest.copy(error = appContext.getString(R.string.music_soundcloud_play_failed)))
            }
            completed(result.map { Unit })
        }
    }

    /** Used only for compact now-playing presentation. This never enters ride history. */
    fun musicSnapshot(): uniffi.cutout_mobile_ffi.MobileMusicSnapshotDto =
        core.musicSnapshot(SystemClock.elapsedRealtime().toULong())

    /** True only while Android's native player reports active audio output. */
    fun isNativePlaybackActive(): Boolean = nativePlaybackActive

    private fun apply(effect: MobileSoundCloudEffect) {
        if (closed) return
        when (effect) {
            MobileSoundCloudEffect.None -> {
                if (core.snapshot().state in setOf(
                        MobileSoundCloudState.IDLE,
                        MobileSoundCloudState.PAUSED,
                        MobileSoundCloudState.FAILED,
                    )
                ) invalidateForegroundPlayback()
            }
            MobileSoundCloudEffect.Pause -> {
                playbackRequestRevision.incrementAndGet()
                foregroundRequestPlaybackId = null
                foregroundReceiptPlaybackId = null
                playRequestedPlaybackId = null
                runCatching { player?.pause() }
                nativePlaybackActive = false
                abandonAudioFocus()
            }
            MobileSoundCloudEffect.Play -> {
                val id = core.snapshot().playbackId
                if (nativePlaybackActive && runCatching { player?.isPlaying == true }.getOrDefault(false)) return
                playRequestedPlaybackId = id
                ensureForegroundPlayback(id)
            }
            is MobileSoundCloudEffect.Fetch -> {
                val id = effect.id
                val requestRevision = playbackRequestRevision.incrementAndGet()
                foregroundRequestPlaybackId = null
                foregroundReceiptPlaybackId = null
                playRequestedPlaybackId = id
                streamTask?.cancel(true)
                releasePlayer()
                activePlaybackId = id
                publish(latest.copy(snapshot = core.snapshot(), elapsedSeconds = 0, error = null))
                // Claim foreground playback while the explicit selection is still in the
                // user-visible Activity. HLS I/O and decoder preparation can take several
                // seconds, by which time Android may reject a background service start.
                ensureForegroundPlayback(id, requestRevision)
                try {
                    streamTask = network.submit {
                        runCatching {
                            val streamResponse = authorizedRequest(effect.endpoint)
                            check(streamResponse.status == HttpURLConnection.HTTP_OK) { appContext.getString(R.string.music_soundcloud_request_failed) }
                            val streamEndpoint = core.streamEndpoint(streamResponse.body)
                            val redirect = authorizedRequest(streamEndpoint)
                            val mediaUrl = redirect.location
                                ?.takeIf { redirect.status == HttpURLConnection.HTTP_MOVED_TEMP }
                                ?: error(appContext.getString(R.string.music_soundcloud_invalid_response))
                            core.streamResolved(id, mediaUrl)
                        }.onSuccess { next -> main.post { if (isCurrent(id)) apply(next) } }
                            .onFailure { error ->
                                main.post {
                                    if (isCurrent(id)) {
                                        apply(core.failed(id))
                                        publish(latest.copy(error = appContext.getString(R.string.music_soundcloud_request_failed)))
                                    }
                                }
                            }
                    }
                } catch (error: RuntimeException) {
                    apply(core.failed(id))
                    publish(latest.copy(error = appContext.getString(R.string.music_soundcloud_request_failed)))
                }
            }
            is MobileSoundCloudEffect.Prepare -> prepare(effect.id, effect.url)
        }
        publish(latest.copy(snapshot = core.snapshot()))
    }

    /** Stops native audio and invalidates Rust playback when the foreground owner is lost. */
    fun onForegroundPlaybackServiceLost() {
        main.post {
            if (closed) return@post
            val snapshot = core.snapshot()
            if (!nativePlaybackActive && snapshot.state !in setOf(
                    MobileSoundCloudState.LOADING,
                    MobileSoundCloudState.PLAYING,
                )
            ) return@post

            runCatching { core.command(MobileSoundCloudCommand.PAUSE) }
                .onSuccess(::apply)
                .onFailure { error -> apply(core.failed(snapshot.playbackId)) }
            runCatching { player?.pause() }
            nativePlaybackActive = false
            abandonAudioFocus()
            publish(latest.copy(snapshot = core.snapshot(), error = appContext.getString(R.string.music_soundcloud_play_failed)))
        }
    }

    private fun ensureForegroundPlayback(id: ULong, revision: Long = playbackRequestRevision.get()) {
        if (!isCurrent(id) || revision != playbackRequestRevision.get()) return
        if (foregroundReceiptPlaybackId == id) {
            if (playRequestedPlaybackId == id && core.snapshot().state == MobileSoundCloudState.PLAYING) {
                startNativePlayback(id, revision)
            }
            return
        }
        if (foregroundRequestPlaybackId == id) return
        foregroundRequestPlaybackId = id
        try {
            beforePlayback { foreground ->
                main.post {
                    if (!isCurrent(id) || revision != playbackRequestRevision.get() || foregroundRequestPlaybackId != id) return@post
                    foregroundRequestPlaybackId = null
                    if (foreground.isFailure) {
                        failPlayback(id)
                        return@post
                    }
                    foregroundReceiptPlaybackId = id
                    if (playRequestedPlaybackId == id && core.snapshot().state == MobileSoundCloudState.PLAYING) {
                        startNativePlayback(id, revision)
                    }
                }
            }
        } catch (_: RuntimeException) {
            if (isCurrent(id) && revision == playbackRequestRevision.get()) failPlayback(id)
        }
    }

    private fun startNativePlayback(id: ULong, revision: Long) {
        if (!isCurrent(id) || revision != playbackRequestRevision.get() ||
            foregroundReceiptPlaybackId != id || playRequestedPlaybackId != id ||
            core.snapshot().state != MobileSoundCloudState.PLAYING
        ) return
        if (nativePlaybackActive && runCatching { player?.isPlaying == true }.getOrDefault(false)) return
        if (!runCatching { requestAudioFocus() }.getOrDefault(false)) {
            failPlayback(id)
            return
        }
        runCatching {
            val native = checkNotNull(player) { appContext.getString(R.string.music_soundcloud_play_failed) }
            native.start()
            check(native.isPlaying) { appContext.getString(R.string.music_soundcloud_play_failed) }
        }.onSuccess {
            nativePlaybackActive = true
            main.removeCallbacks(positionTick)
            main.post(positionTick)
        }.onFailure { failPlayback(id) }
    }

    private fun failPlayback(id: ULong) {
        if (!isCurrent(id)) return
        apply(core.failed(id))
        publish(latest.copy(error = appContext.getString(R.string.music_soundcloud_play_failed)))
    }

    private fun invalidateForegroundPlayback() {
        playbackRequestRevision.incrementAndGet()
        foregroundRequestPlaybackId = null
        foregroundReceiptPlaybackId = null
        playRequestedPlaybackId = null
    }

    private fun prepare(id: ULong, streamUrl: String) {
        val uri = runCatching { Uri.parse(streamUrl) }.getOrNull()
        if (uri?.scheme != "https" || uri.host.isNullOrBlank()) {
            apply(core.failed(id))
            publish(latest.copy(error = appContext.getString(R.string.music_soundcloud_invalid_response)))
            return
        }
        releasePlayer()
        val media = MediaPlayer()
        player = media
        activePlaybackId = id
        media.setAudioAttributes(AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
            .build())
        media.setOnPreparedListener {
            if (isCurrent(id)) apply(core.playerReady(id))
        }
        media.setOnCompletionListener {
            if (isCurrent(id)) {
                nativePlaybackActive = false
                runCatching { core.ended(id) }
                    .onSuccess(::apply)
                    .onFailure { publish(latest.copy(error = appContext.getString(R.string.music_soundcloud_play_failed))) }
            }
        }
        media.setOnErrorListener { _, _, _ ->
            if (isCurrent(id)) {
                nativePlaybackActive = false
                apply(core.failed(id))
                publish(latest.copy(error = appContext.getString(R.string.music_soundcloud_play_failed)))
            }
            true
        }
        runCatching {
            media.setDataSource(appContext, uri)
            media.prepareAsync()
        }.onFailure {
            apply(core.failed(id))
            publish(latest.copy(error = appContext.getString(R.string.music_soundcloud_play_failed)))
        }
    }

    private fun requestAudioFocus(): Boolean {
        val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                .build())
            .setOnAudioFocusChangeListener(focusListener)
            .build()
        focusRequest = request
        return audioManager.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
    }

    private fun abandonAudioFocus() {
        focusRequest?.let { request -> runCatching { audioManager.abandonAudioFocusRequest(request) } }
        focusRequest = null
    }

    private fun authorizedRequest(endpoint: String): HttpResult {
        val parsed = URL(endpoint)
        require(parsed.protocol == "https" && parsed.host == "api.soundcloud.com") {
            appContext.getString(R.string.music_soundcloud_invalid_response)
        }
        for (attempt in 0..1) {
            val connection = request(parsed, authorizationHeader())
            try {
                val status = connection.responseCode
                if (status == HttpURLConnection.HTTP_UNAUTHORIZED && attempt == 0) {
                    core.invalidateAuthorization()
                    continue
                }
                val bytes = if (status == HttpURLConnection.HTTP_MOVED_TEMP) byteArrayOf() else readBounded(connection, MAX_RESPONSE_BYTES)
                return HttpResult(bytes, status, connection.getHeaderField("Location"))
            } finally {
                connection.disconnect()
            }
        }
        error(appContext.getString(R.string.music_soundcloud_authorization_failed))
    }

    private fun authorizationHeader(): String {
        val now = SystemClock.elapsedRealtime().toULong()
        return core.authorization(now) ?: run {
            val tokenUrl = URL(core.tokenEndpoint())
            require(tokenUrl.protocol == "https" && tokenUrl.host == "music.cutout.lol") {
                appContext.getString(R.string.music_soundcloud_invalid_response)
            }
            val connection = request(tokenUrl, null)
            try {
                val status = connection.responseCode
                val bytes = readBounded(connection, MAX_RESPONSE_BYTES)
                if (status != HttpURLConnection.HTTP_OK) error(appContext.getString(R.string.music_soundcloud_authorization_failed))
                core.acceptAuthorization(bytes, SystemClock.elapsedRealtime().toULong())
            } finally { connection.disconnect() }
            core.authorization(SystemClock.elapsedRealtime().toULong())
                ?: error(appContext.getString(R.string.music_soundcloud_authorization_failed))
        }
    }

    private fun request(url: URL, authorization: String?): HttpURLConnection =
        (url.openConnection() as HttpURLConnection).apply {
            connectTimeout = NETWORK_TIMEOUT_MS
            readTimeout = NETWORK_TIMEOUT_MS
            instanceFollowRedirects = false
            requestMethod = "GET"
            authorization?.let { setRequestProperty("Authorization", it) }
        }

    private fun readBounded(connection: HttpURLConnection, maxBytes: Int): ByteArray {
        val status = connection.responseCode
        val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            ?: error(appContext.getString(R.string.music_soundcloud_invalid_response))
        if (connection.contentLengthLong > maxBytes) error(appContext.getString(R.string.music_soundcloud_invalid_response))
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(8 * 1024)
        stream.use {
            while (true) {
                val count = it.read(buffer)
                if (count < 0) break
                if (output.size() + count > maxBytes) error(appContext.getString(R.string.music_soundcloud_invalid_response))
                output.write(buffer, 0, count)
            }
        }
        return output.toByteArray()
    }

    private fun isCurrent(id: ULong) = !closed && core.snapshot().playbackId == id && activePlaybackId == id

    fun stop() {
        main.post {
            synchronized(catalogueLock) { searchRevision.incrementAndGet() }
            searchTask?.cancel(true)
            streamTask?.cancel(true)
            releasePlayer()
            apply(core.stop())
            publish(AndroidSoundCloudPresentation(snapshot = core.snapshot()))
        }
    }

    private fun releasePlayer() {
        main.removeCallbacks(positionTick)
        runCatching { player?.stop() }
        player?.release()
        player = null
        nativePlaybackActive = false
        activePlaybackId = null
        abandonAudioFocus()
    }

    private fun publish(value: AndroidSoundCloudPresentation) {
        latest = value
        publishMediaSession(value)
        onPresentation(value)
        observers.forEach { observer -> main.post { if (observer in observers) observer(value) } }
    }

    private fun publishMediaSession(value: AndroidSoundCloudPresentation) {
        val snapshot = value.snapshot
        val track = snapshot.track
        mediaSession.isActive = track != null
        mediaSession.setMetadata(track?.let {
            MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_MEDIA_ID, it.urn)
                .putString(MediaMetadata.METADATA_KEY_TITLE, it.title)
                .putString(MediaMetadata.METADATA_KEY_ARTIST, it.uploader)
                .putLong(MediaMetadata.METADATA_KEY_DURATION, it.durationMs.coerceAtMost(Long.MAX_VALUE.toULong()).toLong())
                .putString(MediaMetadata.METADATA_KEY_MEDIA_URI, it.permalink)
                .build()
        })
        var actions = PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE
        if (snapshot.hasPrevious) actions = actions or PlaybackState.ACTION_SKIP_TO_PREVIOUS
        if (snapshot.hasNext) actions = actions or PlaybackState.ACTION_SKIP_TO_NEXT
        val status = when (snapshot.state) {
            MobileSoundCloudState.IDLE -> PlaybackState.STATE_NONE
            MobileSoundCloudState.LOADING -> PlaybackState.STATE_BUFFERING
            MobileSoundCloudState.PLAYING -> PlaybackState.STATE_PLAYING
            MobileSoundCloudState.PAUSED -> PlaybackState.STATE_PAUSED
            MobileSoundCloudState.FAILED -> PlaybackState.STATE_ERROR
        }
        val position = runCatching { player?.currentPosition?.coerceAtLeast(0)?.toLong() ?: 0L }.getOrDefault(0L)
        mediaSession.setPlaybackState(PlaybackState.Builder()
            .setActions(actions)
            .setState(status, position, if (status == PlaybackState.STATE_PLAYING && nativePlaybackActive) 1f else 0f)
            .build())
    }

    override fun close() {
        if (closed) return
        closed = true
        synchronized(catalogueLock) {
            searchRevision.incrementAndGet()
            searchTask?.cancel(true)
            streamTask?.cancel(true)
            runCatching { core.stop() }
            core.close()
        }
        runCatching { appContext.unregisterReceiver(noisyRouteReceiver) }
        releasePlayer()
        mediaSession.isActive = false
        mediaSession.release()
        network.shutdownNow()
    }

    companion object {
        private const val MAX_RESPONSE_BYTES = 512 * 1024
        private const val NETWORK_TIMEOUT_MS = 25_000
    }
}
