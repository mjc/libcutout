package lol.cutout

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.app.Activity
import android.app.Application
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadata
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import android.net.Uri
import uniffi.cutout_mobile_ffi.*
import java.io.Closeable
import java.io.ByteArrayOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicReference
import java.util.concurrent.atomic.AtomicLong

data class AndroidMusicPresentation(
    val selectedProvider: MobileMusicProviderDto = MobileMusicProviderDto.APPLE_MUSIC,
    val snapshot: MobileMusicSnapshotDto? = null,
    val listenerAccessGranted: Boolean = false,
    val error: String? = null,
    val artwork: Bitmap? = null,
    val historyPolicy: MobileMusicHistoryPolicyDto = MobileMusicHistoryPolicyDto.DISABLED,
    val history: MobileMusicHistoryDto? = null,
    val historyError: String? = null,
    val lastRecordedSequence: ULong? = null,
    val spotifyDisconnectPending: Boolean = false,
    val spotifyDisconnectError: String? = null,
    val spotifyConfigured: Boolean = BuildConfig.SPOTIFY_CLIENT_ID.isNotBlank(),
    val spotifyAuthorized: Boolean = false,
    val spotifyAuthorizing: Boolean = false,
    val spotifyNeedsAuthorization: Boolean = false,
    val spotifyCredentialRestoreRetryAvailable: Boolean = false,
)

/** Receives lifecycle notifications from the OS-managed notification-listener service. */
object AndroidMusicSessionBridge {
    private val listeners = CopyOnWriteArrayList<() -> Unit>()
    fun addListener(listener: () -> Unit): Closeable {
        listeners += listener
        return Closeable { listeners -= listener }
    }
    internal fun notifySessionsChanged() = listeners.forEach { it() }
}

/**
 * Platform media-session adapter. It consumes only sessions exposed by Android after the user
 * enables notification-listener access. Rust canonicalizes observations and admits commands.
 */
class AndroidMusicClient(
    context: Context,
    private val lifecycle: MobileMusicProviderLifecycle,
    private val rides: RideMapRuntime,
    private val onPresentation: (AndroidMusicPresentation) -> Unit = {},
) : Closeable, RideMapStopAdmissionFence {
    private val appContext = context.applicationContext
    private val main = Handler(Looper.getMainLooper())
    private val spotifyApi = AndroidSpotifyWebApi(appContext, BuildConfig.SPOTIFY_CLIENT_ID)
    private val spotifyHttpSession = MobileSpotifyHttpSession()
    private val spotifyPlayerStateRequests = AndroidSpotifyPlayerStateRequests(lifecycle)
    private val spotifyPlayerStateExpiryCheck: Runnable = Runnable {
        val ticket = spotifyPollTicket ?: return@Runnable
        val monitor = monitorGeneration ?: return@Runnable
        val provider = providerGeneration ?: return@Runnable
        if (!isCurrentSpotifyWork(monitor, provider) || !spotifyPlayerStateRequests.isCurrent(ticket)) return@Runnable
        when (spotifyPlayerStateRequests.expire(ticket, SystemClock.elapsedRealtime().toULong())) {
            MobileMusicRequestExpiration.PENDING -> main.postDelayed(
                spotifyPlayerStateExpiryCheck,
                PLAYER_STATE_EXPIRY_CHECK_INTERVAL_MS,
            )
            MobileMusicRequestExpiration.EXPIRED -> {
                clearSpotifyPollTicket(ticket)
                publishStaleSpotifySnapshotIfDue()
                publish(latestPresentation.copy(error = appContext.getString(R.string.music_session_unavailable)))
                scheduleSpotifyPoll(monitor, MobileMusicProviderWorkState.CREDENTIALS_AVAILABLE)
            }
            MobileMusicRequestExpiration.STALE -> {
                clearSpotifyPollTicket(ticket)
                scheduleSpotifyPoll(monitor, MobileMusicProviderWorkState.CREDENTIALS_AVAILABLE)
            }
        }
    }
    private val spotifyFreshnessCheck: Runnable = Runnable {
        val monitor = monitorGeneration ?: return@Runnable
        if (closed || selectedProvider != MobileMusicProviderDto.SPOTIFY ||
            lifecycle.classifyMonitor(monitor) != MobileMusicProviderCallbackMatch.CURRENT) return@Runnable
        publishStaleSpotifySnapshotIfDue()
        if (!lifecycle.isPlayerStateStale(SystemClock.elapsedRealtime().toULong())) {
            main.postDelayed(spotifyFreshnessCheck, PLAYER_STATE_EXPIRY_CHECK_INTERVAL_MS)
        }
    }
    private val artworkWorker = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(1),
        { task -> Thread(task, "CutoutMusicArtwork").apply { isDaemon = true } },
        ThreadPoolExecutor.DiscardOldestPolicy(),
    )
    private val artworkRequest = AtomicLong(0)
    // Native callbacks are admitted before entering this FIFO. Core work is serialized through
    // RideMapRuntime; this worker only waits for each lease and polls asynchronous receipts.
    private val observationWorker = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(32),
        { task -> Thread(task, "CutoutMusicObservation").apply { isDaemon = true } },
        ThreadPoolExecutor.AbortPolicy(),
    )
    private val historyWorker = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(8),
        { task -> Thread(task, "CutoutMusicHistory").apply { isDaemon = true } },
        ThreadPoolExecutor.AbortPolicy(),
    )
    private val queuedObservations = AtomicLong(0)
    private val manager = appContext.getSystemService(MediaSessionManager::class.java)
    private val notificationListener = ComponentName(appContext, AndroidMusicNotificationListenerService::class.java)
    private val preferences = appContext.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)
    private val observers = CopyOnWriteArrayList<(AndroidMusicPresentation) -> Unit>()
    private val bridgeObservation = AndroidMusicSessionBridge.addListener { main.post { refreshSessions() } }
    private var activeSessionListener: MediaSessionManager.OnActiveSessionsChangedListener? = null
    private var currentController: MediaController? = null
    private var currentControllerCallback: MediaController.Callback? = null
    private var providerGeneration: MobileMusicProviderSessionId? = null
    private var soundCloudCommand: ((MobileSoundCloudCommand, (Result<Unit>) -> Unit) -> Unit)? = null
    private var soundCloudStop: (() -> Unit)? = null
    private var soundCloudObservation: Closeable? = null
    private var soundCloudSnapshot: (() -> MobileMusicSnapshotDto)? = null
    private var soundCloudError: (() -> String?)? = null
    private var monitorGeneration: MobileMusicMonitorId? = null
    private var spotifyAuthorizationId: MobileMusicAuthorizationId? = null
    private var spotifyAuthorizationMonitor: MobileMusicMonitorId? = null
    private var spotifyCredentialRestoreRetryAttempted = false
    private var spotifyCredentialRestoreOperation = 0L
    private var spotifyPlayback: MobileSpotifyPlayback? = null
    private var spotifyPollTicket: SpotifyPlayerStateTicket? = null
    private var spotifyPollPending = false
    private var spotifyPollInFlight = false
    private var resumedActivities = 0
    private val activityLifecycleCallbacks = object : Application.ActivityLifecycleCallbacks {
        override fun onActivityCreated(activity: Activity, state: android.os.Bundle?) = Unit
        override fun onActivityStarted(activity: Activity) = Unit
        override fun onActivityPaused(activity: Activity) {
            if (resumedActivities > 0) resumedActivities--
            if (resumedActivities == 0 && selectedProvider == MobileMusicProviderDto.SPOTIFY) {
                retireSpotifyPoll()
                spotifyApi.retireProviderWork()
                lifecycle.suspend()
                activeSessionListener?.let { listener ->
                    runCatching { manager?.removeOnActiveSessionsChangedListener(listener) }
                }
                activeSessionListener = null
                detachSession()
                monitorGeneration = null
            }
        }
        override fun onActivityResumed(activity: Activity) {
            val wasBackground = resumedActivities == 0
            resumedActivities++
            if (wasBackground && selectedProvider == MobileMusicProviderDto.SPOTIFY) {
                lifecycle.resume()
                refreshSpotify()
            }
        }
        override fun onActivityStopped(activity: Activity) = Unit
        override fun onActivitySaveInstanceState(activity: Activity, outState: android.os.Bundle) = Unit
        override fun onActivityDestroyed(activity: Activity) = Unit
    }
    private val spotifyPollRunnable = Runnable { pollSpotifyPlayback() }
    private var selectedProvider = musicProviderFromStorage(preferences.getString(PROVIDER_KEY, null))
    private var defaultHistoryPolicy = historyPolicyFromStorage(preferences.getString(HISTORY_POLICY_KEY, null))
    private var spotifyHistoryReadbackRevision = 0L
    @Volatile var latestPresentation = AndroidMusicPresentation(
        selectedProvider = selectedProvider,
        historyPolicy = defaultHistoryPolicy,
    )
        private set
    private var closed = false
    @Volatile private var acceptingObservations = true
    @Volatile private var observationLifecycleReady = false

    init {
        rides.setFutureRideMusicHistoryPolicy(defaultHistoryPolicy)
        (appContext as? Application)?.registerActivityLifecycleCallbacks(activityLifecycleCallbacks)
    }

    fun observe(observer: (AndroidMusicPresentation) -> Unit): Closeable {
        observers += observer
        main.post { if (observer in observers) observer(latestPresentation) }
        return Closeable { observers -= observer }
    }

    /** Opens Android's system page only after an explicit user action. */
    fun notificationListenerSettingsIntent(): Intent =
        Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    fun hasNotificationListenerAccess(): Boolean {
        val enabled = Settings.Secure.getString(appContext.contentResolver, "enabled_notification_listeners")
            ?: return false
        return enabled.split(':').any { flattened ->
            val component = ComponentName.unflattenFromString(flattened)
            component == notificationListener
        }
    }

    /** Provider choice is an explicit app preference; it never launches that provider. */
    fun selectProvider(provider: MobileMusicProviderDto) {
        if (selectedProvider == provider) return
        val previousProvider = selectedProvider
        selectedProvider = provider
        if (previousProvider == MobileMusicProviderDto.SPOTIFY) {
            spotifyCredentialRestoreOperation++
            cancelSpotifyAuthorization()
        }
        preferences.edit().putString(PROVIDER_KEY, musicProviderProfile(provider).storageKey).apply()
        stopMonitoring()
        if (previousProvider == MobileMusicProviderDto.SOUND_CLOUD && provider != MobileMusicProviderDto.SOUND_CLOUD) {
            soundCloudStop?.invoke()
        }
        retireProvider()
        publish(latestPresentation.copy(selectedProvider = provider, snapshot = null, artwork = null, error = null,
            listenerAccessGranted = hasNotificationListenerAccess(),
            spotifyConfigured = spotifyApi.isConfigured,
            spotifyAuthorized = spotifyApi.isAuthorized && spotifyApi.hasPlaybackScopes,
            spotifyAuthorizing = false,
            spotifyCredentialRestoreRetryAvailable = false))
        if (provider == MobileMusicProviderDto.SPOTIFY || provider == MobileMusicProviderDto.SOUND_CLOUD || hasNotificationListenerAccess()) refreshSessions()
    }

    /** Explicit recovery action: reconnect an existing MediaSession or open the selected
     * provider when no session exists. Android provider authentication stays in that app. */
    fun connectOrRetrySelectedProvider(activity: Activity? = null) {
        main.post {
            if (closed) return@post
            if (selectedProvider == MobileMusicProviderDto.SPOTIFY) {
                if (spotifyApi.hasCredentialRestoreFailure && !spotifyCredentialRestoreRetryAttempted) {
                    publish(latestPresentation.copy(error = appContext.getString(R.string.spotify_saved_credentials_unavailable)))
                    val operation = ++spotifyCredentialRestoreOperation
                    spotifyApi.retryCredentialRestore { result -> main.post {
                        if (closed || selectedProvider != MobileMusicProviderDto.SPOTIFY ||
                            operation != spotifyCredentialRestoreOperation) return@post
                        spotifyCredentialRestoreRetryAttempted = true
                        if (result.isSuccess) {
                            spotifyCredentialRestoreRetryAttempted = false
                            refreshSpotify()
                        } else publish(latestPresentation.copy(
                            error = appContext.getString(R.string.spotify_saved_credentials_unavailable),
                            spotifyCredentialRestoreRetryAvailable = false,
                        ))
                    } }
                    return@post
                }
                if (!spotifyApi.isConfigured) {
                    publish(latestPresentation.copy(error = appContext.getString(R.string.spotify_not_configured)))
                    return@post
                }
                if (!spotifyApi.isAuthorized || !spotifyApi.hasPlaybackScopes || latestPresentation.spotifyNeedsAuthorization) {
                    if (activity == null) {
                        publish(latestPresentation.copy(error = appContext.getString(R.string.spotify_auth_required)))
                    } else authorizeSpotify(activity)
                    return@post
                }
                preferences.edit().putBoolean(SPOTIFY_MONITORING_OPT_OUT_KEY, false).apply()
                resumeObservations()
                publish(latestPresentation.copy(error = null, spotifyDisconnectError = null))
                refreshSessions()
                refreshHistory()
                return@post
            }
            if (!hasNotificationListenerAccess()) {
                // Media access is an OS permission. A recovery tap must never
                // launch a provider or try to grant it on the rider's behalf.
                refreshSessions()
                refreshHistory()
                return@post
            }
            publish(latestPresentation.copy(error = null, spotifyDisconnectError = null))
            // Refresh first so a stale controller is detached on this same
            // explicit retry. If no session remains, use the same Rust-authorized
            // launch effect as Open app; no provider account is authorized here.
            refreshSessions()
            if (currentController == null) openSelectedProvider()
            refreshHistory()
        }
    }

    /** Explicit Connect/Reauthorize action. Rust owns this authorization and monitor attempt. */
    fun authorizeSpotify(activity: Activity) {
        main.post {
            if (closed || selectedProvider != MobileMusicProviderDto.SPOTIFY) return@post
            spotifyCredentialRestoreOperation++
            if (!spotifyApi.isConfigured) {
                publish(latestPresentation.copy(error = appContext.getString(R.string.spotify_not_configured)))
                return@post
            }
            if (spotifyAuthorizationId != null) return@post
            stopMonitoring()
            lifecycle.requestMonitor(MobileMusicProviderMonitorRequest.AUTHORIZE)
            val monitor = lifecycle.beginMonitor()
            if (monitor == null || monitor.start != MobileMusicProviderMonitorStart.AUTHORIZE) {
                monitor?.let { lifecycle.finishMonitor(it.generation) }
                publish(latestPresentation.copy(error = appContext.getString(R.string.spotify_auth_failed)))
                return@post
            }
            val authorization = lifecycle.beginAuthorizationEffect(
                MobileMusicProviderAuthorizationKind.AUTHORIZING,
                SystemClock.elapsedRealtime().toULong(),
            )
            if (authorization == null) {
                lifecycle.finishMonitor(monitor.generation)
                publish(latestPresentation.copy(error = appContext.getString(R.string.spotify_auth_failed)))
                return@post
            }
            spotifyAuthorizationId = authorization.id
            spotifyAuthorizationMonitor = monitor.generation
            publish(latestPresentation.copy(spotifyAuthorizing = true, error = null))
            spotifyApi.authorize(activity, authorization.id.value) { result ->
                if (result.isFailure) main.post {
                    if (spotifyAuthorizationId == authorization.id) finishSpotifyAuthorization(
                        authorization.id, result.exceptionOrNull() ?: IllegalStateException(),
                    )
                }
            }
        }
    }

    /** Called only by the non-exported AppAuth PendingIntent result Activity. */
    fun completeSpotifyAuthorization(intent: Intent) {
        val rawId = intent.getLongExtra(AndroidSpotifyWebApi.EXTRA_TRANSACTION_ID, -1L)
        val id = spotifyAuthorizationId ?: return
        if (rawId < 0 || id.value != rawId.toULong() ||
            lifecycle.classifyAuthorization(id) != MobileMusicProviderAuthorizationMatch.AUTHORIZING
        ) return
        spotifyApi.completeAuthorization(intent, id.value) { result ->
            main.post {
                if (closed || spotifyAuthorizationId != id) return@post
                val error = result.exceptionOrNull()
                finishSpotifyAuthorization(id, error)
            }
        }
    }

    private fun finishSpotifyAuthorization(id: MobileMusicAuthorizationId, error: Throwable?) {
        val monitor = spotifyAuthorizationMonitor
        spotifyAuthorizationId = null
        spotifyAuthorizationMonitor = null
        val authorizationResult = lifecycle.finishAuthorization(id)
        monitor?.let(lifecycle::finishMonitor)
        val active = authorizationResult == MobileMusicProviderAuthorizationMatch.AUTHORIZING
        val canceled = error is AndroidSpotifyWebApi.SpotifyAuthorizationCancelled
        val authorized = active && error == null && spotifyApi.isAuthorized && spotifyApi.hasPlaybackScopes
        if (authorized) {
            preferences.edit().putBoolean(SPOTIFY_MONITORING_OPT_OUT_KEY, false).apply()
            resumeObservations()
        }
        publish(latestPresentation.copy(
            spotifyAuthorizing = false,
            spotifyAuthorized = authorized,
            spotifyCredentialRestoreRetryAvailable = false,
            error = when {
                !active -> appContext.getString(R.string.spotify_auth_stale)
                canceled -> null
                error != null -> appContext.getString(R.string.spotify_auth_failed)
                !authorized -> appContext.getString(R.string.spotify_auth_invalid)
                else -> null
            },
        ))
        if (authorized) refreshSessions()
    }

    private fun cancelSpotifyAuthorization() {
        val id = spotifyAuthorizationId ?: return
        spotifyApi.retireAuthorization(id.value)
        lifecycle.invalidateAuthorization()
        spotifyAuthorizationMonitor?.let(lifecycle::finishMonitor)
        spotifyAuthorizationId = null
        spotifyAuthorizationMonitor = null
    }

    /** Explicit Android equivalent of Swift's provider open action. */
    fun openProviderFromSettings(completed: (Result<Unit>) -> Unit = {}) = openSelectedProvider(completed)

    /** Publishes SoundCloud's Rust presentation projection to the shared compact player only. */
    fun presentSoundCloudSnapshot(snapshot: MobileMusicSnapshotDto, error: String? = null) {
        main.post {
            if (selectedProvider == MobileMusicProviderDto.SOUND_CLOUD && !closed) {
                publish(latestPresentation.copy(snapshot = snapshot, artwork = null, error = error))
            }
        }
    }

    /** Joins the app-owned native SoundCloud player to the existing compact presentation. */
    fun bindSoundCloudPlayer(player: AndroidSoundCloudPlayer) {
        soundCloudObservation?.close()
        soundCloudSnapshot = player::musicSnapshot
        soundCloudError = { player.latest.error }
        soundCloudStop = player::stop
        soundCloudCommand = { command, completed ->
            player.command(command, completed)
        }
        soundCloudObservation = player.observe { value ->
            presentSoundCloudSnapshot(player.musicSnapshot(), value.error)
        }
    }

    /** Opts out of Spotify observation, retires its callbacks, then deletes Spotify ride history
     * through the shared Rust database owner. A failed deletion remains visible and retryable. */
    fun disconnectSpotify(completed: (Result<Unit>) -> Unit = {}) {
        main.post {
            spotifyCredentialRestoreOperation++
            cancelSpotifyAuthorization()
            if (selectedProvider == MobileMusicProviderDto.SPOTIFY) {
                preferences.edit().putBoolean(SPOTIFY_MONITORING_OPT_OUT_KEY, true).apply()
                spotifyHistoryReadbackRevision++
                stopMonitoring()
                retireProvider()
                publish(latestPresentation.copy(
                    snapshot = null,
                    artwork = null,
                    error = null,
                    spotifyNeedsAuthorization = false,
                    spotifyDisconnectPending = true,
                    spotifyDisconnectError = null,
                    history = latestPresentation.history?.copy(events = latestPresentation.history?.events.orEmpty().filter {
                        it.provider != MobileMusicProviderDto.SPOTIFY
                    }),
                ))
            }
            acceptingObservations = false
            drainObservations { drained ->
                if (drained.isFailure) {
                    finishSpotifyDisconnect(drained, completed)
                    return@drainObservations
                }
                spotifyApi.clearCredentials { cleared ->
                    main.post {
                        if (cleared.isFailure) {
                            finishSpotifyDisconnect(cleared, completed)
                            return@post
                        }
                        lifecycle.invalidateAuthorization()
                        rides.withCore({ core -> core.deleteSpotifyMusicHistory() }) { result ->
                            finishSpotifyDisconnect(result, completed)
                        }
                    }
                }
            }
        }
    }

    private fun finishSpotifyDisconnect(result: Result<Unit>, completed: (Result<Unit>) -> Unit) {
        result.onSuccess {
            spotifyPlayback = null
            spotifyHistoryReadbackRevision++
            publish(latestPresentation.copy(
                spotifyDisconnectPending = false,
                spotifyDisconnectError = null,
                historyError = null,
                spotifyAuthorized = false,
                spotifyNeedsAuthorization = false,
                    spotifyCredentialRestoreRetryAvailable = false,
                history = latestPresentation.history?.copy(events = latestPresentation.history?.events.orEmpty().filter {
                    it.provider != MobileMusicProviderDto.SPOTIFY
                }),
            ))
            if (selectedProvider != MobileMusicProviderDto.SPOTIFY) {
                acceptingObservations = true
                refreshHistory()
            }
            completed(Result.success(Unit))
        }.onFailure { error ->
            if (selectedProvider != MobileMusicProviderDto.SPOTIFY) acceptingObservations = true
            publish(latestPresentation.copy(
                spotifyDisconnectPending = false,
                spotifyDisconnectError = appContext.getString(R.string.spotify_disconnect_failed),
                historyError = appContext.getString(R.string.spotify_disconnect_failed),
            ))
            completed(Result.failure(error))
        }
    }

    /** The persisted Swift-equivalent preference used for rides created in the future. */
    fun historyPolicyForNewRide(): MobileMusicHistoryPolicyDto = defaultHistoryPolicy

    /** Call only after RideMapRuntime.bindMusicObservationLifecycle has completed successfully. */
    fun markObservationLifecycleReady() {
        val ready = {
            observationLifecycleReady = true
            refreshSessions()
        }
        if (Looper.myLooper() == main.looper) ready() else main.post(ready)
    }

    /** Refreshes active-ride privacy policy and its authoritative timeline from Rust/SQLite. */
    fun refreshHistory() {
        val revision = ++spotifyHistoryReadbackRevision
        rides.withCore({ core ->
            val snapshot = core.currentSnapshot(SystemClock.elapsedRealtime().toULong())
            val active = snapshot?.state == MobileRideLifecycleStateDto.ACTIVE ||
                snapshot?.state == MobileRideLifecycleStateDto.PAUSED
            val policy = if (active) core.currentMusicHistoryPolicy() else defaultHistoryPolicy
            val command = if (active) core.beginCurrentMusicHistory() else null
            policy to command
        }) { result ->
            result.onSuccess { (policy, command) ->
                if (command == null) {
                    if (revision == spotifyHistoryReadbackRevision) publish(latestPresentation.copy(historyPolicy = policy, history = null, historyError = null))
                } else pollHistory(command, policy, revision)
            }.onFailure { error ->
                if (revision == spotifyHistoryReadbackRevision) publish(latestPresentation.copy(historyError = error.message ?: error.toString()))
            }
        }
    }

    /** Persists the default for future rides; if a ride is open, Rust commits its active policy. */
    fun setHistoryPolicy(policy: MobileMusicHistoryPolicyDto, completed: (Result<Unit>) -> Unit = {}) {
        main.post {
            if (closed) {
                completed(Result.failure(IllegalStateException("Music client is closed")))
                return@post
            }
            val previous = defaultHistoryPolicy
            defaultHistoryPolicy = policy
            preferences.edit().putString(HISTORY_POLICY_KEY, historyPolicyStorageKey(policy)).apply()
            rides.setFutureRideMusicHistoryPolicy(policy)
            rides.withCore({ core ->
                val snapshot = core.currentSnapshot(SystemClock.elapsedRealtime().toULong())
                if (snapshot == null || snapshot.state != MobileRideLifecycleStateDto.ACTIVE &&
                    snapshot.state != MobileRideLifecycleStateDto.PAUSED
                ) {
                    // A future-ride preference has no active Rust ride to mutate.
                    null
                } else {
                    core.beginSetMusicHistoryPolicy(policy)
                }
            }) { result ->
                result.onSuccess { command ->
                    if (command == null) {
                        publish(latestPresentation.copy(historyPolicy = policy, historyError = null))
                        completed(Result.success(Unit))
                    } else pollWrite(command) { write ->
                        write.onSuccess {
                            refreshHistory()
                            completed(Result.success(Unit))
                        }.onFailure { error ->
                            defaultHistoryPolicy = previous
                            preferences.edit().putString(HISTORY_POLICY_KEY, historyPolicyStorageKey(previous)).apply()
                            rides.setFutureRideMusicHistoryPolicy(previous)
                            publish(latestPresentation.copy(historyError = error.message ?: error.toString()))
                            completed(Result.failure(error))
                        }
                    }
                }.onFailure { error ->
                    defaultHistoryPolicy = previous
                    preferences.edit().putString(HISTORY_POLICY_KEY, historyPolicyStorageKey(previous)).apply()
                    rides.setFutureRideMusicHistoryPolicy(previous)
                    publish(latestPresentation.copy(historyError = error.message ?: error.toString()))
                    completed(Result.failure(error))
                }
            }
        }
    }

    /** Deletes active-ride music history only after Rust reports durable completion. */
    fun deleteCurrentHistory(completed: (Result<Unit>) -> Unit = {}) {
        rides.withCore({ it.beginDeleteCurrentMusicHistory() }) { result ->
            result.onSuccess { command ->
                pollWrite(command) { write ->
                    write.onSuccess {
                        refreshHistory()
                        completed(Result.success(Unit))
                    }.onFailure { error ->
                        publish(latestPresentation.copy(historyError = error.message ?: error.toString()))
                        completed(Result.failure(error))
                    }
                }
            }.onFailure { error ->
                publish(latestPresentation.copy(historyError = error.message ?: error.toString()))
                completed(Result.failure(error))
            }
        }
    }

    /** Runs [completed] after every callback admitted before this call has settled in Rust. */
    fun drainObservations(completed: () -> Unit) {
        drainObservations { result -> if (result.isSuccess) completed() }
    }

    fun drainObservations(completed: (Result<Unit>) -> Unit) {
        try {
            observationWorker.execute { main.post { completed(Result.success(Unit)) } }
        } catch (_: RuntimeException) {
            // A rejected barrier means the owner is closing; do not claim pending work settled.
            main.post {
                val error = IllegalStateException(appContext.getString(R.string.music_observation_failed))
                publish(latestPresentation.copy(error = error.message))
                completed(Result.failure(error))
            }
        }
    }

    /** Freezes callback admission and drains the accepted FIFO before a ride Stop is submitted. */
    fun prepareStopAfterObservations(completed: () -> Unit) {
        val prepare = {
            acceptingObservations = false
            drainObservations(completed)
        }
        if (Looper.myLooper() == main.looper) prepare() else main.post(prepare)
    }

    override fun beforeRideStop(proceed: () -> Unit) = prepareStopAfterObservations(proceed)

    override fun onRideStopFailed() = resumeObservations()

    override fun onRideRecordingActive() = resumeObservations()

    /** Reopens callback admission after a failed Stop or a new/resumed recording. */
    fun resumeObservations() {
        if (Looper.myLooper() == main.looper) acceptingObservations = true
        else main.post { acceptingObservations = true }
    }

    /** Called after the user enables access or returns from system Settings. */
    fun refreshSessions() {
        if (closed) return
        if (selectedProvider == MobileMusicProviderDto.SPOTIFY) {
            refreshSpotify()
            return
        }
        if (selectedProvider == MobileMusicProviderDto.SOUND_CLOUD) {
            if (monitorGeneration != null) stopMonitoring()
            detachSession()
            publish(newPresentation(null, hasNotificationListenerAccess()).copy(
                snapshot = soundCloudSnapshot?.invoke(),
                error = soundCloudError?.invoke(),
            ))
            return
        }
        if (!observationLifecycleReady) return
        val hasAccess = hasNotificationListenerAccess()
        if (!hasAccess) {
            detachSession()
            publish(newPresentation(null, false, appContext.getString(R.string.music_access_required)))
            return
        }
        if (selectedProvider == MobileMusicProviderDto.SPOTIFY && preferences.getBoolean(SPOTIFY_MONITORING_OPT_OUT_KEY, false)) {
            stopMonitoring()
            detachSession()
            publish(newPresentation(null, true))
            return
        }
        if (monitorGeneration == null) startMonitoring()
        try {
            val sessions = manager?.getActiveSessions(notificationListener).orEmpty()
            val matching = sessions.firstOrNull { providerForPackage(it.packageName) == selectedProvider }
            attachSession(matching)
        } catch (error: SecurityException) {
            detachSession()
            publish(newPresentation(null, false, error.message ?: appContext.getString(R.string.music_access_required)))
        } catch (error: RuntimeException) {
            detachSession()
            publish(newPresentation(null, true, error.message ?: appContext.getString(R.string.music_session_unavailable)))
        }
    }

    private fun startMonitoring() {
        if (!observationLifecycleReady || monitorGeneration != null || !hasNotificationListenerAccess()) return
        lifecycle.requestMonitor(MobileMusicProviderMonitorRequest.OBSERVE)
        val effect = lifecycle.beginMonitor() ?: return
        if (effect.start != MobileMusicProviderMonitorStart.OBSERVE) {
            lifecycle.finishMonitor(effect.generation)
            return
        }
        monitorGeneration = effect.generation
        val listener = MediaSessionManager.OnActiveSessionsChangedListener { main.post { refreshSessions() } }
        activeSessionListener = listener
        try {
            manager?.addOnActiveSessionsChangedListener(listener, notificationListener, Handler(main.looper))
            refreshSessionsWithoutStarting()
        } catch (_: SecurityException) {
            lifecycle.finishMonitor(effect.generation)
            monitorGeneration = null
            publish(newPresentation(null, false, appContext.getString(R.string.music_access_required)))
        }
    }

    private fun refreshSpotify() {
        if (!observationLifecycleReady) return
        if (spotifyAuthorizationId != null) {
            publish(latestPresentation.copy(
                spotifyConfigured = spotifyApi.isConfigured,
                spotifyAuthorized = spotifyApi.isAuthorized && spotifyApi.hasPlaybackScopes,
                spotifyAuthorizing = true,
            ))
            return
        }
        val authorized = spotifyApi.isAuthorized && spotifyApi.hasPlaybackScopes
        if (preferences.getBoolean(SPOTIFY_MONITORING_OPT_OUT_KEY, false)) {
            stopMonitoring()
            detachSession()
            publish(newPresentation(null, hasNotificationListenerAccess()).copy(
                spotifyConfigured = spotifyApi.isConfigured,
                spotifyAuthorized = authorized,
                spotifyAuthorizing = spotifyAuthorizationId != null,
            ))
            return
        }
        val localController = try {
            if (hasNotificationListenerAccess()) manager?.getActiveSessions(notificationListener)
                ?.firstOrNull { it.packageName == SPOTIFY_PACKAGE }
            else null
        } catch (_: SecurityException) { null }
        val localFirst = runCatching {
            spotifyPlaybackTransport(localController != null, spotifyApi.isAuthorized && spotifyApi.hasPlaybackScopes)
        }.getOrDefault(MobileMusicPlaybackTransport.UNAVAILABLE)
        if (localFirst == MobileMusicPlaybackTransport.APP_REMOTE && localController != null) {
            retireSpotifyPoll()
            spotifyApi.retireProviderWork()
            if (monitorGeneration == null) startMonitoring()
            if (currentController?.sessionToken != localController.sessionToken || providerGeneration == null) {
                attachSession(localController)
            }
            publish(latestPresentation.copy(
                selectedProvider = MobileMusicProviderDto.SPOTIFY,
                listenerAccessGranted = true,
                spotifyConfigured = spotifyApi.isConfigured,
                spotifyAuthorized = spotifyApi.isAuthorized && spotifyApi.hasPlaybackScopes,
                spotifyAuthorizing = false,
                spotifyNeedsAuthorization = false,
                error = null,
            ))
            return
        }
        if (currentController?.packageName == SPOTIFY_PACKAGE) detachSession()
        if (!spotifyApi.isConfigured) {
            stopMonitoring()
            publish(newPresentation(null, hasNotificationListenerAccess(), appContext.getString(R.string.spotify_not_configured)).copy(
                spotifyConfigured = false,
                spotifyAuthorized = false,
                spotifyAuthorizing = spotifyAuthorizationId != null,
            ))
            return
        }
        if (spotifyApi.hasCredentialRestoreFailure) {
            stopMonitoring()
            publish(newPresentation(null, hasNotificationListenerAccess(), appContext.getString(R.string.spotify_saved_credentials_unavailable)).copy(
                spotifyConfigured = true,
                spotifyAuthorized = false,
                spotifyAuthorizing = false,
                spotifyNeedsAuthorization = true,
                spotifyCredentialRestoreRetryAvailable = !spotifyCredentialRestoreRetryAttempted,
            ))
            return
        }
        if (!authorized) {
            stopMonitoring()
            publish(newPresentation(null, hasNotificationListenerAccess()).copy(
                spotifyConfigured = true,
                spotifyAuthorized = false,
                spotifyAuthorizing = spotifyAuthorizationId != null,
                error = if (spotifyAuthorizationId == null) appContext.getString(R.string.spotify_auth_required) else null,
            ))
            return
        }
        if (monitorGeneration == null) {
            lifecycle.requestMonitor(MobileMusicProviderMonitorRequest.OBSERVE)
            val effect = lifecycle.beginMonitor()
            if (effect == null || effect.start != MobileMusicProviderMonitorStart.OBSERVE) {
                effect?.let { lifecycle.finishMonitor(it.generation) }
                publish(latestPresentation.copy(
                    spotifyConfigured = true,
                    spotifyAuthorized = true,
                    error = appContext.getString(R.string.music_provider_unavailable),
                ))
                return
            }
            monitorGeneration = effect.generation
            providerGeneration = lifecycle.beginProviderSession()
            if (providerGeneration == null) {
                stopMonitoring()
                publish(latestPresentation.copy(error = appContext.getString(R.string.music_provider_unavailable)))
                return
            }
        }
        if (providerGeneration == null) {
            providerGeneration = lifecycle.beginProviderSession()
            if (providerGeneration == null) {
                stopMonitoring()
                publish(latestPresentation.copy(error = appContext.getString(R.string.music_provider_unavailable)))
                return
            }
        }
        publish(latestPresentation.copy(
            selectedProvider = MobileMusicProviderDto.SPOTIFY,
            listenerAccessGranted = hasNotificationListenerAccess(),
            spotifyConfigured = true,
            spotifyAuthorized = true,
            spotifyAuthorizing = false,
            error = null,
        ))
        if (!spotifyPollInFlight && !spotifyPollPending) pollSpotifyPlayback()
    }

    private fun pollSpotifyPlayback() {
        if (closed || selectedProvider != MobileMusicProviderDto.SPOTIFY || monitorGeneration == null ||
            providerGeneration == null || !spotifyApi.isAuthorized || !spotifyApi.hasPlaybackScopes ||
            preferences.getBoolean(SPOTIFY_MONITORING_OPT_OUT_KEY, false)
        ) return
        val monitor = monitorGeneration ?: return
        if (lifecycle.classifyMonitor(monitor) != MobileMusicProviderCallbackMatch.CURRENT) return
        spotifyPollPending = false
        if (spotifyPollInFlight) return
        val now = SystemClock.elapsedRealtime().toULong()
        val request = try { spotifyHttpSession.playbackRequest(now) } catch (error: Throwable) {
            publish(latestPresentation.copy(error = spotifyErrorMessage(error)))
            scheduleSpotifyPoll(monitor, MobileMusicProviderWorkState.CREDENTIALS_AVAILABLE, spotifyRetryDelay(error))
            return
        }
        val ticket = spotifyPlayerStateRequests.begin(now) ?: run {
            scheduleSpotifyPoll(monitor, MobileMusicProviderWorkState.CREDENTIALS_AVAILABLE, PLAYER_STATE_EXPIRY_CHECK_INTERVAL_MS)
            return
        }
        spotifyPollTicket = ticket
        main.removeCallbacks(spotifyPlayerStateExpiryCheck)
        main.postDelayed(spotifyPlayerStateExpiryCheck, PLAYER_STATE_EXPIRY_CHECK_INTERVAL_MS)
        spotifyPollInFlight = true
        val provider = providerGeneration ?: return
        spotifyApi.execute(request.url, request.method) { result ->
            if (result.isFailure) {
                main.post { completeSpotifyPoll(monitor, provider, ticket, result.exceptionOrNull()) }
                return@execute
            }
            val response = result.getOrThrow()
            if (response.status == 401) {
                runCatching {
                    spotifyHttpSession.receiveResponse(response.status.toUShort(), response.retryAfter,
                        response.body.size.toULong(), SystemClock.elapsedRealtime().toULong())
                }
                spotifyApi.execute(request.url, request.method, forceRefresh = true) { renewed ->
                    main.post {
                        if (renewed.isFailure) completeSpotifyPoll(monitor, provider, ticket, renewed.exceptionOrNull())
                        else consumeSpotifyPlaybackResponse(monitor, provider, ticket, renewed.getOrThrow())
                    }
                }
            } else {
                main.post { consumeSpotifyPlaybackResponse(monitor, provider, ticket, response) }
            }
        }
    }

    private fun consumeSpotifyPlaybackResponse(
        monitor: MobileMusicMonitorId,
        provider: MobileMusicProviderSessionId,
        ticket: SpotifyPlayerStateTicket,
        response: SpotifyHttpResponse,
    ) {
        if (!isCurrentSpotifyWork(monitor, provider) || !spotifyPlayerStateRequests.isCurrent(ticket)) {
            if (spotifyPollTicket == ticket) clearSpotifyPollTicket(ticket)
            return
        }
        val now = SystemClock.elapsedRealtime().toULong()
        try {
            val hasBody = spotifyHttpSession.receiveResponse(
                response.status.toUShort(), response.retryAfter, response.body.size.toULong(), now,
            )
            if (hasBody) {
                val playback = MobileSpotifyPlayback.parse(response.body)
                replaceSpotifyPlayback(playback)
                val state = if (playback.isPlaying()) MobileMusicPlaybackStateDto.PLAYING else MobileMusicPlaybackStateDto.PAUSED
                val snapshot = playback.snapshot(state, now)
                val artwork = playback.artworkUrl()?.let { spotifyArtwork(it, provider, playback.itemIdentifier()) }
                submitSpotifyObservation(snapshot, artwork)
            } else {
                spotifyPlayback = spotifyPlayback?.withoutActiveDevice()?.also { spotifyPlayback?.close() }
                val snapshot = spotifyPlayback?.snapshot(MobileMusicPlaybackStateDto.STOPPED, now) ?: unavailableSnapshot()
                submitSpotifyObservation(snapshot, null)
            }
            val accepted = spotifyPlayerStateRequests.complete(ticket, now)
            if (accepted == MobileMusicRequestCompletion.ACCEPTED) {
                spotifyPlayerStateRequests.markObserved(now)
                scheduleSpotifyFreshnessCheck()
                clearSpotifyPollTicket(ticket)
                publish(latestPresentation.copy(spotifyAuthorized = true, spotifyNeedsAuthorization = false, error = null))
                completeSpotifyPoll(monitor, provider, ticket, null, alreadyCompleted = true)
            } else {
                clearSpotifyPollTicket(ticket)
            }
        } catch (error: Throwable) {
            completeSpotifyPoll(monitor, provider, ticket, error)
        }
    }

    private fun completeSpotifyPoll(
        monitor: MobileMusicMonitorId,
        provider: MobileMusicProviderSessionId,
        ticket: SpotifyPlayerStateTicket,
        error: Throwable?,
        alreadyCompleted: Boolean = false,
    ) {
        if (monitorGeneration != monitor || providerGeneration != provider || closed ||
            (spotifyPollTicket != ticket && !alreadyCompleted)) return
        if (!alreadyCompleted) {
            val completion = spotifyPlayerStateRequests.complete(ticket, SystemClock.elapsedRealtime().toULong())
            clearSpotifyPollTicket(ticket)
            if (completion == MobileMusicRequestCompletion.STALE) return
        }
        val needsAuthorization = error is MobileSpotifyPlaybackException.Unauthorized ||
            error is AndroidSpotifyWebApi.SpotifyAuthenticationFailure
        publish(latestPresentation.copy(
            spotifyAuthorized = spotifyApi.isAuthorized && spotifyApi.hasPlaybackScopes,
            spotifyNeedsAuthorization = needsAuthorization,
            error = error?.let(::spotifyErrorMessage),
        ))
        if (error != null) publishStaleSpotifySnapshotIfDue()
        if (!needsAuthorization) {
            val state = when (error) {
                is MobileSpotifyPlaybackException.Forbidden -> MobileMusicProviderWorkState.REQUIRES_USER_ACTION
                is MobileSpotifyPlaybackException.RateLimited,
                null -> if (error == null) MobileMusicProviderWorkState.REMOTE_POLLING else MobileMusicProviderWorkState.CREDENTIALS_AVAILABLE
                else -> MobileMusicProviderWorkState.CREDENTIALS_AVAILABLE
            }
            scheduleSpotifyPoll(monitor, state, spotifyRetryDelay(error))
        }
    }

    private fun clearSpotifyPollTicket(ticket: SpotifyPlayerStateTicket) {
        if (spotifyPollTicket != ticket) return
        spotifyPollTicket = null
        spotifyPollInFlight = false
        main.removeCallbacks(spotifyPlayerStateExpiryCheck)
    }

    private fun retireSpotifyPoll() {
        main.removeCallbacks(spotifyPollRunnable)
        main.removeCallbacks(spotifyPlayerStateExpiryCheck)
        main.removeCallbacks(spotifyFreshnessCheck)
        spotifyPollPending = false
        spotifyPollTicket?.let { spotifyPlayerStateRequests.retire(it, SystemClock.elapsedRealtime().toULong()) }
        spotifyPollTicket = null
        spotifyPollInFlight = false
    }

    private fun publishStaleSpotifySnapshotIfDue() {
        val now = SystemClock.elapsedRealtime().toULong()
        if (lifecycle.isPlayerStateStale(now)) {
            val stale = spotifyPlayback?.snapshot(MobileMusicPlaybackStateDto.STALE, now)
                ?: latestPresentation.snapshot?.copy(
                    state = MobileMusicPlaybackStateDto.STALE,
                    capabilities = MobileMusicCapabilitiesDto(false, false, false, false, false),
                )
                ?: return
            publish(latestPresentation.copy(snapshot = stale))
        }
    }

    private fun scheduleSpotifyFreshnessCheck() {
        main.removeCallbacks(spotifyFreshnessCheck)
        main.postDelayed(spotifyFreshnessCheck, PLAYER_STATE_EXPIRY_CHECK_INTERVAL_MS)
    }

    private fun scheduleSpotifyPoll(monitor: MobileMusicMonitorId, work: MobileMusicProviderWorkState, minDelayMs: Long = 0L) {
        if (closed || monitorGeneration != monitor) return
        val effect = lifecycle.nextMonitorPoll(monitor, work, SystemClock.elapsedRealtime().toULong()) ?: return
        val delay = maxOf((effect.deadlineMs.toLong() - SystemClock.elapsedRealtime()).coerceAtLeast(0), minDelayMs)
        spotifyPollPending = true
        main.removeCallbacks(spotifyPollRunnable)
        main.postDelayed(spotifyPollRunnable, delay)
    }

    private fun isCurrentSpotifyWork(monitor: MobileMusicMonitorId, provider: MobileMusicProviderSessionId): Boolean =
        !closed && selectedProvider == MobileMusicProviderDto.SPOTIFY && monitorGeneration == monitor &&
            providerGeneration == provider && lifecycle.classifyMonitor(monitor) == MobileMusicProviderCallbackMatch.CURRENT &&
            lifecycle.classifyProviderSession(provider) == MobileMusicProviderCallbackMatch.CURRENT

    private fun spotifyErrorMessage(error: Throwable): String = when {
        error is AndroidSpotifyWebApi.SpotifyAuthorizationCancelled -> appContext.getString(R.string.spotify_auth_cancelled)
        error is AndroidSpotifyWebApi.SpotifyAuthenticationFailure || error is MobileSpotifyPlaybackException.Unauthorized -> appContext.getString(R.string.spotify_auth_required)
        error is AndroidSpotifyWebApi.SpotifyCredentialPersistenceFailure -> appContext.getString(R.string.spotify_saved_credentials_unavailable)
        error is MobileSpotifyPlaybackException.Forbidden -> appContext.getString(R.string.spotify_playback_forbidden)
        error is MobileSpotifyPlaybackException.RateLimited -> appContext.getString(R.string.spotify_rate_limited)
        error is MobileSpotifyPlaybackException.InvalidPayload -> appContext.getString(R.string.music_session_unavailable)
        error.message == appContext.getString(R.string.spotify_auth_required) -> appContext.getString(R.string.spotify_auth_required)
        else -> appContext.getString(R.string.music_session_unavailable)
    }

    private fun spotifyRetryDelay(error: Throwable?): Long =
        (error as? MobileSpotifyPlaybackException.RateLimited)?.retryAfterMilliseconds?.toLong()?.coerceAtMost(86_400_000L) ?: 0L

    private fun replaceSpotifyPlayback(value: MobileSpotifyPlayback) {
        if (spotifyPlayback !== value) spotifyPlayback?.close()
        spotifyPlayback = value
    }

    private fun spotifyArtwork(url: String, provider: MobileMusicProviderSessionId, item: String?): Bitmap? {
        val parsed = runCatching { Uri.parse(url) }.getOrNull() ?: return null
        if (parsed.scheme != "https" || parsed.host == null) return null
        val id = artworkRequest.incrementAndGet()
        return try {
            artworkWorker.execute {
                val bitmap = runCatching { loadBoundedArtwork(parsed) }.getOrNull() ?: return@execute
                main.post {
                    if (!closed && providerGeneration == provider && artworkRequest.get() == id &&
                        spotifyPlayback?.itemIdentifier() == item
                    ) publish(latestPresentation.copy(artwork = bitmap))
                }
            }
            null
        } catch (_: RuntimeException) { null }
    }

    private fun submitSpotifyObservation(snapshot: MobileMusicSnapshotDto, artwork: Bitmap?) {
        if (!acceptingObservations) return
        val request = lifecycle.beginMusicObservation()
        when (val admission = request.admission()) {
            is MobileMusicObservationAdmission.Admitted -> {
                queuedObservations.incrementAndGet()
                try {
                    observationWorker.execute {
                        try {
                            processObservation(request, admission.lease, snapshot, artwork, System.currentTimeMillis().toULong())
                        } catch (_: Exception) {
                            request.failRequiredEffects()
                        } finally {
                            request.release(); request.close(); queuedObservations.decrementAndGet()
                        }
                    }
                } catch (_: RuntimeException) {
                    request.failRequiredEffects(); request.release(); request.close(); queuedObservations.decrementAndGet()
                }
            }
            MobileMusicObservationAdmission.Full, MobileMusicObservationAdmission.Exhausted -> request.close()
        }
    }

    private fun commandSpotify(command: MobileMusicCommandDto, completed: (Result<Unit>) -> Unit) {
        if (command == MobileMusicCommandDto.OPEN_PROVIDER) {
            completeAction(openSelectedSystemProvider(), completed)
            return
        }
        retireSpotifyPoll()
        val provider = providerGeneration
        val snapshot = latestPresentation.snapshot
        if (provider == null || snapshot == null || !spotifyApi.isAuthorized || !spotifyApi.hasPlaybackScopes) {
            completeAction(Result.failure(IllegalStateException(appContext.getString(R.string.spotify_auth_required))), completed)
            return
        }
        val now = SystemClock.elapsedRealtime().toULong()
        val effect = lifecycle.beginSupportedTransportEffect(
            MobileMusicTransportOwner.Provider(provider), command, snapshot.capabilities, now,
        ) ?: run {
            completeAction(Result.failure(IllegalStateException(appContext.getString(R.string.music_command_unavailable))), completed)
            return
        }
        val request = try {
            val device = spotifyPlayback?.deviceId()
                ?: throw IllegalStateException(appContext.getString(R.string.music_command_unavailable))
            spotifyHttpSession.commandRequest(command, device, now)
        } catch (error: Throwable) {
            lifecycle.finishTransport(provider, effect.id, MobileMusicTransportOutcome.FAILED)
            completeAction(Result.failure(error), completed)
            return
        }
        spotifyApi.execute(request.url, request.method) { result ->
            if (result.isFailure) {
                finishSpotifyCommand(
                    provider, effect.id,
                    Result.failure(result.exceptionOrNull() ?: IllegalStateException(appContext.getString(R.string.music_provider_unavailable))),
                    completed,
                )
                return@execute
            }
            val response = result.getOrThrow()
            if (response.status == 401) {
                runCatching { spotifyHttpSession.receiveResponse(response.status.toUShort(), response.retryAfter,
                    response.body.size.toULong(), SystemClock.elapsedRealtime().toULong()) }
                spotifyApi.execute(request.url, request.method, forceRefresh = true) { renewed ->
                    if (renewed.isFailure) finishSpotifyCommand(
                        provider, effect.id,
                        Result.failure(renewed.exceptionOrNull() ?: IllegalStateException(appContext.getString(R.string.music_provider_unavailable))),
                        completed,
                    )
                    else finishSpotifyHttpCommand(provider, effect.id, renewed.getOrThrow(), completed)
                }
            } else finishSpotifyHttpCommand(provider, effect.id, response, completed)
        }
    }

    private fun finishSpotifyHttpCommand(
        provider: MobileMusicProviderSessionId,
        requestId: MobileMusicTransportRequestId,
        response: SpotifyHttpResponse,
        completed: (Result<Unit>) -> Unit,
    ) {
        val result = runCatching {
            spotifyHttpSession.receiveResponse(
                response.status.toUShort(), response.retryAfter, response.body.size.toULong(),
                SystemClock.elapsedRealtime().toULong(),
            )
            Unit
        }
        finishSpotifyCommand(provider, requestId, result, completed)
        if (result.isSuccess) main.post {
            if (!closed && providerGeneration == provider && monitorGeneration != null) pollSpotifyPlayback()
        }
    }

    private fun finishSpotifyCommand(
        provider: MobileMusicProviderSessionId,
        requestId: MobileMusicTransportRequestId,
        result: Result<Unit>,
        completed: (Result<Unit>) -> Unit,
    ) {
        val current = providerGeneration == provider && !closed && selectedProvider == MobileMusicProviderDto.SPOTIFY
        lifecycle.finishTransport(
            provider, requestId,
            if (current && result.isSuccess) MobileMusicTransportOutcome.ACCEPTED else MobileMusicTransportOutcome.FAILED,
        )
        main.post {
            if (current) completeAction(result, completed)
            else completed(Result.failure(IllegalStateException(appContext.getString(R.string.music_provider_unavailable))))
        }
    }

    private fun refreshSessionsWithoutStarting() {
        try {
            val sessions = manager?.getActiveSessions(notificationListener).orEmpty()
            attachSession(sessions.firstOrNull { providerForPackage(it.packageName) == selectedProvider })
        } catch (_: SecurityException) {
            publish(newPresentation(null, false, appContext.getString(R.string.music_access_required)))
        }
    }

    private fun attachSession(controller: MediaController?) {
        if (currentController?.sessionToken == controller?.sessionToken) {
            controller?.let(::observeController)
            return
        }
        detachSession()
        if (controller == null) {
            publish(newPresentation(unavailableSnapshot(), true, appContext.getString(R.string.music_no_active_session)))
            return
        }
        currentController = controller
        providerGeneration = lifecycle.beginProviderSession()
        val callback = object : MediaController.Callback() {
            override fun onMetadataChanged(metadata: MediaMetadata?) = observeController(controller)
            override fun onPlaybackStateChanged(state: PlaybackState?) = observeController(controller)
            override fun onSessionDestroyed() {
                main.post { if (currentController === controller) refreshSessions() }
            }
        }
        currentControllerCallback = callback
        try {
            controller.registerCallback(callback, Handler(main.looper))
            observeController(controller)
        } catch (error: RuntimeException) {
            detachSession()
            publish(newPresentation(null, true, error.message ?: appContext.getString(R.string.music_session_unavailable)))
        }
    }

    private fun observeController(controller: MediaController) {
        if (!acceptingObservations || currentController !== controller || providerForPackage(controller.packageName) != selectedProvider) return
        val snapshot = project(controller)
        val metadata = controller.metadata
        val immediateArtwork = boundedArtwork(metadata)
        val wallClockAtMs = System.currentTimeMillis().toULong()
        val request = lifecycle.beginMusicObservation()
        when (val admission = request.admission()) {
            is MobileMusicObservationAdmission.Admitted -> {
                if (selectedProvider == MobileMusicProviderDto.SPOTIFY) {
                    spotifyPlayerStateRequests.markObserved(SystemClock.elapsedRealtime().toULong())
                    retireSpotifyPoll()
                    scheduleSpotifyFreshnessCheck()
                }
                queuedObservations.incrementAndGet()
                try {
                    observationWorker.execute {
                        try {
                            processObservation(request, admission.lease, snapshot, immediateArtwork, wallClockAtMs)
                        } catch (error: Exception) {
                            request.failRequiredEffects()
                            main.post {
                                publish(latestPresentation.copy(
                                    error = error.message ?: appContext.getString(R.string.music_observation_failed),
                                ))
                            }
                        } finally {
                            request.release()
                            request.close()
                            queuedObservations.decrementAndGet()
                        }
                    }
                } catch (error: RuntimeException) {
                    request.failRequiredEffects()
                    request.release()
                    request.close()
                    queuedObservations.decrementAndGet()
                    publish(latestPresentation.copy(error = appContext.getString(R.string.music_observation_failed)))
                }
                if (immediateArtwork == null) loadArtworkUri(controller, metadata)
            }
            MobileMusicObservationAdmission.Full -> {
                request.close()
                publish(latestPresentation.copy(error = appContext.getString(R.string.music_observation_queue_full)))
            }
            MobileMusicObservationAdmission.Exhausted -> {
                request.close()
                publish(latestPresentation.copy(error = appContext.getString(R.string.music_observation_failed)))
            }
        }
    }

    /** Performs Swift's admitted-player, ride-context, classified-history and receipt sequence. */
    private fun processObservation(
        request: MobileMusicObservationRequest,
        lease: MobileMusicObservationLease,
        source: MobileMusicSnapshotDto,
        artwork: Bitmap?,
        wallClockAtMs: ULong,
    ) {
        while (request.isCurrent() && lease.state() == MobileMusicObservationLeaseState.PENDING) {
            Thread.sleep(OBSERVATION_POLL_INTERVAL_MS)
        }
        if (!request.isCurrent() || lease.state() != MobileMusicObservationLeaseState.READY) return
        val canonical = lifecycle.observeAdmittedPlayer(lease, source)
        if (canonical != null) {
            main.post {
                if (!closed && request.isCurrent()) {
                    publish(latestPresentation.copy(
                        snapshot = canonical,
                        listenerAccessGranted = true,
                        error = null,
                        artwork = artwork,
                    ))
                }
            }
        }

        val observationContext = withCoreBlocking { core ->
            val previousFailure = core.retireClosedMusicHistory(lifecycle, lease)
            val current = core.currentSnapshot(SystemClock.elapsedRealtime().toULong())
            val context = current?.rideId?.let { rideId ->
                core.musicObservationContextForObservation(lease, rideId, source.observedAtMs)
            }
            previousFailure to context
        }
        val (previousFailure, context) = observationContext
        if (previousFailure != null) {
            main.post {
                if (!closed && request.isCurrent()) publish(latestPresentation.copy(
                    historyError = appContext.getString(R.string.music_previous_history_incomplete),
                ))
            }
        }
        val recording = when (context) {
            is MobileMusicObservationContext.Ready -> context.recording
            MobileMusicObservationContext.RideNotOpen, MobileMusicObservationContext.OutOfOrder, null -> null
        }
        if (context == MobileMusicObservationContext.OutOfOrder) {
            request.settle()
            return
        }

        val decision = lifecycle.observeAdmittedMusic(
            lease,
            recording,
            source,
            wallClockAtMs,
            0uL,
        ) ?: run {
            request.settle()
            return
        }
        val transition = decision.historyTransition
        if (transition == null) {
            request.settle()
            return
        }
        if (recording == null) {
            lifecycle.acknowledgeHistoryTransition(transition.id)
            request.settle()
            return
        }

        val command = withCoreBlocking { core -> core.beginRecordMusicObservation(lease) }
        val result = pollMusicCommand(command)
        when (result.outcome) {
            MobileMusicTimelineOutcomeDto.RECORDED,
            MobileMusicTimelineOutcomeDto.DUPLICATE,
            MobileMusicTimelineOutcomeDto.DISABLED -> lifecycle.acknowledgeHistoryTransition(transition.id)
            MobileMusicTimelineOutcomeDto.FULL,
            MobileMusicTimelineOutcomeDto.OUT_OF_ORDER,
            MobileMusicTimelineOutcomeDto.RIDE_NOT_OPEN -> Unit
        }
        request.settle()
        main.post {
            if (!closed && request.isCurrent()) {
                publish(latestPresentation.copy(
                    lastRecordedSequence = result.sequence ?: latestPresentation.lastRecordedSequence,
                    historyPolicy = result.effectivePolicy ?: latestPresentation.historyPolicy,
                    error = null,
                ))
            }
        }
    }

    private fun <T> withCoreBlocking(block: (MobileRideMapCore) -> T): T {
        val latch = CountDownLatch(1)
        val result = AtomicReference<Result<T>>()
        rides.withCore({ core -> block(core) }) { value ->
            result.set(value)
            latch.countDown()
        }
        latch.await()
        return checkNotNull(result.get()).getOrThrow()
    }

    private fun pollMusicCommand(command: MobileRideMapMusicCommand): MobileMusicTimelineRecordResultDto {
        try {
            while (true) {
                when (val result = command.poll()) {
                    MobileRideMapMusicPollDto.Pending -> Thread.sleep(OBSERVATION_POLL_INTERVAL_MS)
                    is MobileRideMapMusicPollDto.Completed -> return result.result
                }
            }
        } finally {
            command.close()
        }
    }

    private fun pollHistory(command: MobileRideMapMusicHistoryCommand, policy: MobileMusicHistoryPolicyDto, revision: Long) {
        try {
            historyWorker.execute {
                try {
                    var history: MobileMusicHistoryDto? = null
                    while (true) {
                        when (val state = command.poll()) {
                            MobileRideMapMusicHistoryPollDto.Pending -> Thread.sleep(OBSERVATION_POLL_INTERVAL_MS)
                            is MobileRideMapMusicHistoryPollDto.Completed -> {
                                history = state.history
                                break
                            }
                        }
                    }
                    main.post {
                        if (revision == spotifyHistoryReadbackRevision) publish(latestPresentation.copy(
                            historyPolicy = policy, history = history, historyError = null,
                        ))
                    }
                } catch (error: Exception) {
                    main.post { publish(latestPresentation.copy(historyError = error.message ?: error.toString())) }
                } finally {
                    command.close()
                }
            }
        } catch (error: RuntimeException) {
            command.close()
            publish(latestPresentation.copy(historyError = error.message ?: error.toString()))
        }
    }

    private fun pollWrite(command: MobileRideMapMusicWriteCommand, completed: (Result<Unit>) -> Unit) {
        try {
            historyWorker.execute {
                val result = runCatching {
                    while (true) {
                        when (command.poll()) {
                            MobileRideMapMusicWritePollDto.PENDING -> Thread.sleep(OBSERVATION_POLL_INTERVAL_MS)
                            MobileRideMapMusicWritePollDto.COMPLETED -> return@runCatching Unit
                        }
                    }
                }
                command.close()
                main.post { completed(result) }
            }
        } catch (error: RuntimeException) {
            command.close()
            completed(Result.failure(error))
        }
    }

    private fun project(controller: MediaController): MobileMusicSnapshotDto {
        val metadata = controller.metadata
        val playback = controller.playbackState
        val actions = playback?.actions ?: 0L
        val launchAvailable = appContext.packageManager.getLaunchIntentForPackage(controller.packageName) != null
        val provider = selectedProvider
        val state = when (playback?.state) {
            PlaybackState.STATE_PLAYING, PlaybackState.STATE_FAST_FORWARDING, PlaybackState.STATE_REWINDING -> MobileMusicPlaybackStateDto.PLAYING
            PlaybackState.STATE_PAUSED -> MobileMusicPlaybackStateDto.PAUSED
            PlaybackState.STATE_STOPPED, PlaybackState.STATE_NONE -> MobileMusicPlaybackStateDto.STOPPED
            PlaybackState.STATE_BUFFERING, PlaybackState.STATE_CONNECTING -> MobileMusicPlaybackStateDto.BUFFERING
            else -> MobileMusicPlaybackStateDto.UNAVAILABLE
        }
        val itemId = metadata?.getString(MediaMetadata.METADATA_KEY_MEDIA_ID)
            ?: metadata?.description?.mediaUri?.toString()
        val item = itemId?.takeIf(String::isNotBlank)?.let { id ->
            MobileMusicItemDto(
                identifier = id.take(MAX_FIELD_LENGTH),
                title = metadata?.getString(MediaMetadata.METADATA_KEY_TITLE)?.take(MAX_FIELD_LENGTH),
                artist = metadata?.getString(MediaMetadata.METADATA_KEY_ARTIST)?.take(MAX_FIELD_LENGTH),
            )
        }
        return MobileMusicSnapshotDto(
            provider = provider,
            sessionId = "${controller.packageName}:${controller.sessionToken.hashCode()}",
            state = state,
            item = item,
            positionMilliseconds = playback?.position?.takeIf { it >= 0L }?.toULong(),
            durationMilliseconds = metadata?.getLong(MediaMetadata.METADATA_KEY_DURATION)?.takeIf { it > 0L }?.toULong(),
            observedAtMs = SystemClock.elapsedRealtime().toULong(),
            capabilities = MobileMusicCapabilitiesDto(
                previous = has(actions, PlaybackState.ACTION_SKIP_TO_PREVIOUS),
                play = has(actions, PlaybackState.ACTION_PLAY) || has(actions, PlaybackState.ACTION_PLAY_PAUSE),
                pause = has(actions, PlaybackState.ACTION_PAUSE) || has(actions, PlaybackState.ACTION_PLAY_PAUSE),
                next = has(actions, PlaybackState.ACTION_SKIP_TO_NEXT),
                openProvider = launchAvailable,
            ),
        )
    }

    private fun boundedArtwork(metadata: MediaMetadata?): Bitmap? {
        val original = metadata?.getBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART)
            ?: metadata?.getBitmap(MediaMetadata.METADATA_KEY_ART)
            ?: return null
        val longest = maxOf(original.width, original.height)
        if (longest <= MAX_ARTWORK_EDGE) return original
        val scale = MAX_ARTWORK_EDGE.toFloat() / longest
        return Bitmap.createScaledBitmap(
            original,
            (original.width * scale).toInt().coerceAtLeast(1),
            (original.height * scale).toInt().coerceAtLeast(1),
            true,
        )
    }

    private fun loadArtworkUri(controller: MediaController, metadata: MediaMetadata?) {
        val uri = metadata?.getString(MediaMetadata.METADATA_KEY_ALBUM_ART_URI)
            ?: metadata?.getString(MediaMetadata.METADATA_KEY_ART_URI)
            ?: return
        val parsed = runCatching { Uri.parse(uri) }.getOrNull() ?: return
        if (parsed.scheme != "https" && parsed.scheme != "content") return
        val requestId = artworkRequest.incrementAndGet()
        val sessionToken = controller.sessionToken
        val generation = providerGeneration
        try {
            artworkWorker.execute {
                val bitmap = runCatching { loadBoundedArtwork(parsed) }.getOrNull() ?: return@execute
                main.post {
                    if (requestId != artworkRequest.get() || closed || currentController?.sessionToken != sessionToken ||
                        providerGeneration != generation
                    ) return@post
                    val current = latestPresentation
                    if (current.selectedProvider == selectedProvider) publish(current.copy(artwork = bitmap))
                }
            }
        } catch (_: RuntimeException) {
            // Artwork is optional; a full queue leaves the current player presentation intact.
        }
    }

    private fun loadBoundedArtwork(uri: Uri): Bitmap? {
        val bytes = if (uri.scheme == "content") {
            appContext.contentResolver.openInputStream(uri)?.use { readAtMost(it) }
        } else {
            val connection = (URL(uri.toString()).openConnection() as HttpURLConnection).apply {
                connectTimeout = ARTWORK_CONNECT_TIMEOUT_MS
                readTimeout = ARTWORK_READ_TIMEOUT_MS
                instanceFollowRedirects = false
                requestMethod = "GET"
            }
            try {
                if (connection.responseCode != HttpURLConnection.HTTP_OK) return null
                if (connection.contentLengthLong > MAX_ARTWORK_BYTES) return null
                connection.inputStream.use { readAtMost(it) }
            } finally {
                connection.disconnect()
            }
        } ?: return null
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        val sample = (maxOf(bounds.outWidth, bounds.outHeight) / MAX_ARTWORK_EDGE).coerceAtLeast(1)
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply {
            inSampleSize = sample
            inPreferredConfig = Bitmap.Config.RGB_565
        })
    }

    private fun readAtMost(input: java.io.InputStream): ByteArray? {
        val output = ByteArrayOutputStream()
        val chunk = ByteArray(8 * 1024)
        var total = 0
        while (true) {
            val count = input.read(chunk)
            if (count < 0) break
            total += count
            if (total > MAX_ARTWORK_BYTES) return null
            output.write(chunk, 0, count)
        }
        return output.toByteArray()
    }

    private fun observeController(controller: MediaController, command: MobileMusicCommandDto): Result<Unit> {
        if (currentController !== controller) return Result.failure(IllegalStateException("Music session changed"))
        val generation = providerGeneration ?: return Result.failure(IllegalStateException("Music session is unavailable"))
        val snapshot = project(controller)
        val effect = lifecycle.beginSupportedTransportEffect(
            MobileMusicTransportOwner.Provider(generation), command, snapshot.capabilities,
            SystemClock.elapsedRealtime().toULong(),
        ) ?: return Result.failure(IllegalStateException(appContext.getString(R.string.music_command_unavailable)))
        val result = runCatching {
            when (command) {
                MobileMusicCommandDto.PLAY -> controller.transportControls.play()
                MobileMusicCommandDto.PAUSE -> controller.transportControls.pause()
                MobileMusicCommandDto.PREVIOUS -> controller.transportControls.skipToPrevious()
                MobileMusicCommandDto.NEXT -> controller.transportControls.skipToNext()
                MobileMusicCommandDto.OPEN_PROVIDER -> {
                    val intent = appContext.packageManager.getLaunchIntentForPackage(controller.packageName)
                        ?: error(appContext.getString(R.string.music_provider_unavailable))
                    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    appContext.startActivity(intent)
                }
            }
        }
        val outcome = if (result.isSuccess) MobileMusicTransportOutcome.ACCEPTED else MobileMusicTransportOutcome.FAILED
        lifecycle.finishTransport(generation, effect.id, outcome)
        return result
    }

    fun command(command: MobileMusicCommandDto, completed: (Result<Unit>) -> Unit = {}) {
        if (selectedProvider == MobileMusicProviderDto.SPOTIFY) {
            main.post {
                val local = currentController?.takeIf { it.packageName == SPOTIFY_PACKAGE }
                if (local != null) {
                    if (command == MobileMusicCommandDto.OPEN_PROVIDER) completeAction(openSelectedSystemProvider(), completed)
                    else completeAction(observeController(local, command), completed)
                } else commandSpotify(command, completed)
            }
            return
        }
        if (selectedProvider == MobileMusicProviderDto.SOUND_CLOUD) {
            if (command == MobileMusicCommandDto.OPEN_PROVIDER) {
                completeAction(openSoundCloud(), completed)
                return
            }
            val mapped = when (command) {
                MobileMusicCommandDto.PLAY -> MobileSoundCloudCommand.PLAY
                MobileMusicCommandDto.PAUSE -> MobileSoundCloudCommand.PAUSE
                MobileMusicCommandDto.PREVIOUS -> MobileSoundCloudCommand.PREVIOUS
                MobileMusicCommandDto.NEXT -> MobileSoundCloudCommand.NEXT
                MobileMusicCommandDto.OPEN_PROVIDER -> error("handled above")
            }
            val effect = soundCloudCommand
            if (effect == null) completeAction(Result.failure(IllegalStateException(appContext.getString(R.string.music_command_unavailable))), completed)
            else effect(mapped) { result -> completeAction(result, completed) }
            return
        }
        val controller = currentController
        if (controller == null) {
            completeAction(
                Result.failure(IllegalStateException(appContext.getString(R.string.music_no_active_session))),
                completed,
            )
            return
        }
        main.post { completeAction(observeController(controller, command), completed) }
    }

    private fun openSoundCloud(): Result<Unit> {
        val generation = providerGeneration ?: lifecycle.beginProviderSession()
            ?: return Result.failure(IllegalStateException(appContext.getString(R.string.music_provider_unavailable)))
        providerGeneration = generation
        val admission = lifecycle.beginSoundcloudCommand(
            generation, MobileMusicCommandDto.OPEN_PROVIDER, SystemClock.elapsedRealtime().toULong(),
        )
        val handoff = admission as? MobileSoundCloudCommandAdmission.Handoff
            ?: return Result.failure(IllegalStateException(appContext.getString(R.string.music_command_unavailable)))
        return runCatching {
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(handoff.url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            appContext.startActivity(intent)
        }.also { result ->
            lifecycle.finishTransport(
                generation,
                handoff.effect.id,
                if (result.isSuccess) MobileMusicTransportOutcome.ACCEPTED else MobileMusicTransportOutcome.FAILED,
            )
        }
    }

    /** Opens the selected provider only in response to an explicit UI command. */
    fun openSelectedProvider(completed: (Result<Unit>) -> Unit = {}) {
        val result = if (selectedProvider == MobileMusicProviderDto.SOUND_CLOUD) {
            openSoundCloud()
        } else {
            openSelectedSystemProvider()
        }
        completeAction(result, completed)
    }

    private fun completeAction(result: Result<Unit>, completed: (Result<Unit>) -> Unit) {
        val error = result.exceptionOrNull()?.message
        publish(latestPresentation.copy(error = error))
        completed(result)
    }

    private fun openSelectedSystemProvider(): Result<Unit> {
        val packageName = when (selectedProvider) {
            MobileMusicProviderDto.APPLE_MUSIC -> APPLE_MUSIC_PACKAGE
            MobileMusicProviderDto.SPOTIFY -> SPOTIFY_PACKAGE
            MobileMusicProviderDto.SOUND_CLOUD -> return Result.failure(IllegalStateException("SoundCloud uses its Rust-admitted handoff"))
        }
        val generation = providerGeneration ?: lifecycle.beginProviderSession()
            ?: return Result.failure(IllegalStateException(appContext.getString(R.string.music_provider_unavailable)))
        providerGeneration = generation
        val intent = appContext.packageManager.getLaunchIntentForPackage(packageName)
            ?: return Result.failure(IllegalStateException(appContext.getString(R.string.music_provider_unavailable)))
        val capabilities = MobileMusicCapabilitiesDto(false, false, false, false, true)
        val effect = lifecycle.beginSupportedTransportEffect(
            MobileMusicTransportOwner.Provider(generation), MobileMusicCommandDto.OPEN_PROVIDER,
            capabilities, SystemClock.elapsedRealtime().toULong(),
        ) ?: return Result.failure(IllegalStateException(appContext.getString(R.string.music_command_unavailable)))
        return runCatching {
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            appContext.startActivity(intent)
        }.also { result ->
            lifecycle.finishTransport(
                generation, effect.id,
                if (result.isSuccess) MobileMusicTransportOutcome.ACCEPTED else MobileMusicTransportOutcome.FAILED,
            )
        }
    }

    private fun unavailableSnapshot() = MobileMusicSnapshotDto(
        provider = selectedProvider,
        sessionId = "android-no-active-session",
        state = MobileMusicPlaybackStateDto.UNAVAILABLE,
        item = null,
        positionMilliseconds = null,
        durationMilliseconds = null,
        observedAtMs = SystemClock.elapsedRealtime().toULong(),
        capabilities = MobileMusicCapabilitiesDto(false, false, false, false, false),
    )

    private fun detachSession() {
        artworkRequest.incrementAndGet()
        val controller = currentController
        val callback = currentControllerCallback
        if (controller != null && callback != null) runCatching { controller.unregisterCallback(callback) }
        providerGeneration?.let { generation ->
            lifecycle.retireProviderSession(generation)
            lifecycle.invalidateCommandFeedback()
        }
        currentController = null
        currentControllerCallback = null
        providerGeneration = null
    }

    private fun retireProvider() {
        detachSession()
    }

    private fun publish(value: AndroidMusicPresentation) {
        latestPresentation = value
        onPresentation(value)
        observers.forEach { observer -> main.post { if (observer in observers) observer(value) } }
    }

    private fun newPresentation(
        snapshot: MobileMusicSnapshotDto?,
        listenerAccessGranted: Boolean,
        error: String? = null,
    ): AndroidMusicPresentation = latestPresentation.copy(
        selectedProvider = selectedProvider,
        snapshot = snapshot,
        listenerAccessGranted = listenerAccessGranted,
        error = error,
        artwork = null,
        historyPolicy = latestPresentation.historyPolicy,
    )

    override fun close() {
        if (closed) return
        closed = true
        cancelSpotifyAuthorization()
        (appContext as? Application)?.unregisterActivityLifecycleCallbacks(activityLifecycleCallbacks)
        activeSessionListener?.let { listener -> runCatching { manager?.removeOnActiveSessionsChangedListener(listener) } }
        activeSessionListener = null
        detachSession()
        monitorGeneration?.let(lifecycle::finishMonitor)
        monitorGeneration = null
        bridgeObservation.close()
        soundCloudObservation?.close()
        soundCloudObservation = null
        soundCloudSnapshot = null
        soundCloudError = null
        soundCloudStop = null
        soundCloudCommand = null
        lifecycle.cancelMonitor()
        retireSpotifyPoll()
        spotifyPlayback?.close()
        spotifyPlayback = null
        spotifyHttpSession.close()
        spotifyApi.close()
        artworkWorker.shutdownNow()
        observationWorker.shutdownNow()
        historyWorker.shutdownNow()
    }

    private fun has(actions: Long, action: Long) = actions and action != 0L

    private fun providerForPackage(packageName: String): MobileMusicProviderDto? = when (packageName) {
        appContext.packageName -> null // SoundCloud's native MediaSession is presentation only, never ride history.
        SPOTIFY_PACKAGE -> MobileMusicProviderDto.SPOTIFY
        APPLE_MUSIC_PACKAGE -> MobileMusicProviderDto.APPLE_MUSIC
        else -> null
    }

    companion object {
        const val SPOTIFY_PACKAGE = "com.spotify.music"
        const val APPLE_MUSIC_PACKAGE = "com.apple.android.music"
        const val SOUNDCLOUD_PACKAGE = "com.soundcloud.android"
        private const val PREFERENCES_NAME = "music_provider_preferences"
        private const val PROVIDER_KEY = "selected_provider"
        private const val HISTORY_POLICY_KEY = "music_history_policy_default"
        private const val SPOTIFY_MONITORING_OPT_OUT_KEY = "spotify_monitoring_opt_out"
        private const val MAX_ARTWORK_EDGE = 512
        private const val MAX_ARTWORK_BYTES = 2 * 1024 * 1024
        private const val ARTWORK_CONNECT_TIMEOUT_MS = 2_000
        private const val ARTWORK_READ_TIMEOUT_MS = 3_000
        private const val MAX_FIELD_LENGTH = 256
        private const val OBSERVATION_POLL_INTERVAL_MS = 10L
        private const val PLAYER_STATE_EXPIRY_CHECK_INTERVAL_MS = 5_000L
    }

    private fun stopMonitoring() {
        retireSpotifyPoll()
        spotifyApi.retireProviderWork()
        activeSessionListener?.let { listener -> runCatching { manager?.removeOnActiveSessionsChangedListener(listener) } }
        activeSessionListener = null
        monitorGeneration?.let(lifecycle::finishMonitor)
        monitorGeneration = null
        detachSession()
        lifecycle.cancelMonitor()
    }
}

internal fun spotifyPlaybackTransport(localSessionAvailable: Boolean, webApiAuthorized: Boolean): MobileMusicPlaybackTransport =
    musicPlaybackTransport(localSessionAvailable, webApiAuthorized)

private fun historyPolicyFromStorage(value: String?): MobileMusicHistoryPolicyDto = when (value) {
    "opaque_item" -> MobileMusicHistoryPolicyDto.OPAQUE_ITEM
    "human_readable" -> MobileMusicHistoryPolicyDto.HUMAN_READABLE
    else -> MobileMusicHistoryPolicyDto.DISABLED
}

private fun historyPolicyStorageKey(policy: MobileMusicHistoryPolicyDto): String = when (policy) {
    MobileMusicHistoryPolicyDto.DISABLED -> "disabled"
    MobileMusicHistoryPolicyDto.OPAQUE_ITEM -> "opaque_item"
    MobileMusicHistoryPolicyDto.HUMAN_READABLE -> "human_readable"
}

/** Manifest registration is owned by the app integration; this service never reads notification text. */
class AndroidMusicNotificationListenerService : android.service.notification.NotificationListenerService() {
    override fun onListenerConnected() { AndroidMusicSessionBridge.notifySessionsChanged() }
    override fun onListenerDisconnected() { AndroidMusicSessionBridge.notifySessionsChanged() }
    override fun onNotificationPosted(sbn: android.service.notification.StatusBarNotification?) {
        AndroidMusicSessionBridge.notifySessionsChanged()
    }
    override fun onNotificationRemoved(sbn: android.service.notification.StatusBarNotification?) {
        AndroidMusicSessionBridge.notifySessionsChanged()
    }
}
