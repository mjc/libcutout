package lol.cutout

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.IBinder
import android.util.Log
import uniffi.cutout_mobile_ffi.MobileMelkLightingSessionStateDto
import uniffi.cutout_mobile_ffi.MobileRideEventDto
import uniffi.cutout_mobile_ffi.MobileSoundCloudCommand

/** Android foreground lifetime for the existing app-owned ride and accessory adapters. */
class RideForegroundService : Service() {
    private val app get() = application as CutoutApplication
    private val observers = mutableListOf<AutoCloseable>()
    private var started = false
    private var closing = false
    private var lightingObservation: AutoCloseable? = null
    private var musicObservation: AutoCloseable? = null
    private var mediaAdmissionObservation: AutoCloseable? = null

    override fun onCreate() {
        super.onCreate()
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.ride_notification_channel), NotificationManager.IMPORTANCE_LOW),
        )
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(MEDIA_CHANNEL, getString(R.string.music), NotificationManager.IMPORTANCE_LOW),
        )
        observers += app.bluetooth.addObserver { refresh() }
        observers += app.rides.observe { value -> app.latestRide = value; refresh() }
        mediaAdmissionObservation = app.mediaPlaybackAdmission.observe(::refresh)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            PAUSE -> app.rides.pause { refresh() }
            RESUME -> app.rides.resume { refresh() }
            STOP -> app.rides.stop { refresh() }
        }
        refresh()
        // Rust restores durable rides when the app next opens; do not restart native
        // location work from a background process without a current OS admission.
        return START_NOT_STICKY
    }

    private fun refresh() {
        if (closing) return
        if (lightingObservation == null) app.existingLighting?.let { lighting ->
            lightingObservation = lighting.addObserver { refresh() }
        }
        if (musicObservation == null) app.existingSoundCloudMusic?.let { player ->
            musicObservation = player.observe { value ->
                app.mediaPlaybackAdmission.playbackChanged(value, player.nativePlaybackActive)
                refresh()
            }
        }
        val types = foregroundTypes(app)
        if (types == 0) {
            stopSelf()
            return
        }
        try {
            val media = app.existingSoundCloudMusic?.takeIf {
                types and ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK != 0 && it.latest.snapshot.track != null
            }
            startForeground(NOTIFICATION, media?.let(::mediaNotification) ?: notification(), types)
            val notifications = getSystemService(NotificationManager::class.java)
            if (media != null && app.latestRide.snapshot?.liveActivityStatistics != null) {
                notifications.notify(RIDE_NOTIFICATION, notification())
            } else notifications.cancel(RIDE_NOTIFICATION)
            if (!started) {
                started = true
                app.foregroundServiceChanged(true)
            }
            app.foregroundServiceError = null
            if (types and ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK != 0) app.mediaPlaybackAdmission.foregroundReady()
        } catch (error: RuntimeException) {
            app.mediaPlaybackAdmission.foregroundFailed()
            app.existingSoundCloudMusic?.onForegroundPlaybackServiceLost()
            app.foregroundServiceError = getString(R.string.ride_service_failed)
            Log.w("CutoutRide", "Could not maintain foreground ride ownership", error)
            app.foregroundServiceChanged(false)
            stopSelf()
        }
    }

    private fun mediaNotification(player: AndroidSoundCloudPlayer): Notification {
        val snapshot = player.latest.snapshot
        val open = PendingIntent.getActivity(this, R.string.music,
            Intent(this, MainActivity::class.java).putExtra("page", R.string.music)
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val builder = Notification.Builder(this, MEDIA_CHANNEL)
            .setSmallIcon(R.drawable.ic_cutout).setContentTitle(snapshot.track?.title)
            .setContentText(snapshot.track?.uploader).setContentIntent(open).setOngoing(true)
            .setCategory(Notification.CATEGORY_TRANSPORT).setOnlyAlertOnce(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
        var actions = 0
        if (snapshot.hasPrevious) {
            builder.addAction(Notification.Action.Builder(android.R.drawable.ic_media_previous,
                getString(R.string.music_previous), SoundCloudNotificationReceiver.pendingIntent(this, MobileSoundCloudCommand.PREVIOUS)).build())
            actions++
        }
        val playing = snapshot.state == uniffi.cutout_mobile_ffi.MobileSoundCloudState.PLAYING
        builder.addAction(Notification.Action.Builder(
            if (playing) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
            getString(if (playing) R.string.music_pause else R.string.music_play),
            SoundCloudNotificationReceiver.pendingIntent(this, if (playing) MobileSoundCloudCommand.PAUSE else MobileSoundCloudCommand.PLAY)).build())
        actions++
        if (snapshot.hasNext) {
            builder.addAction(Notification.Action.Builder(android.R.drawable.ic_media_next,
                getString(R.string.music_next), SoundCloudNotificationReceiver.pendingIntent(this, MobileSoundCloudCommand.NEXT)).build())
            actions++
        }
        return builder.setStyle(Notification.MediaStyle().setMediaSession(player.mediaSessionToken)
            .setShowActionsInCompactView(*IntArray(actions) { it })).build()
    }

    private fun notification(): Notification {
        val open = PendingIntent.getActivity(this, 0,
            Intent(this, MainActivity::class.java).putExtra("page", R.string.map)
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val snapshot = app.latestRide.snapshot
        val stateText = snapshot?.takeIf { it.liveActivityStatistics != null }?.let { getString(when (it.state) {
            uniffi.cutout_mobile_ffi.MobileRideLifecycleStateDto.PAUSED -> R.string.ride_notification_paused
            uniffi.cutout_mobile_ffi.MobileRideLifecycleStateDto.INTERRUPTED -> R.string.ride_notification_interrupted
            else -> R.string.ride_recording_active
        }) }
            ?: app.bluetooth.latestPresentation.session?.identity?.model
            ?: getString(R.string.app_name)
        val builder = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_cutout).setContentTitle(getString(R.string.app_name))
            .setContentText(stateText).setContentIntent(open).setOngoing(true)
            .setCategory(Notification.CATEGORY_SERVICE).setOnlyAlertOnce(true)
        snapshot?.liveActivityStatistics?.let { statistics ->
            val metrics = getString(R.string.ride_notification_metrics,
                RideMetricFormatting.distance(this, statistics.distanceMillimetres.toDouble() / 1000.0),
                RideMetricFormatting.duration(statistics.durationMilliseconds))
            builder.setContentTitle(stateText).setContentText(metrics)
                .setStyle(Notification.BigTextStyle().bigText(metrics))
        }
        val actions = snapshot?.allowedActions.orEmpty()
        if (MobileRideEventDto.PAUSE in actions) builder.addAction(Notification.Action.Builder(
            null, getString(R.string.ride_map_pause), command(PAUSE)).build())
        if (MobileRideEventDto.RESUME in actions) builder.addAction(Notification.Action.Builder(
            null, getString(R.string.ride_map_resume), command(RESUME)).build())
        if (MobileRideEventDto.STOP in actions) builder.addAction(Notification.Action.Builder(
            null, getString(R.string.ride_map_stop), command(STOP)).build())
        return builder.build()
    }

    private fun command(action: String) = PendingIntent.getService(this, action.hashCode(),
        Intent(this, RideForegroundService::class.java).setAction(action),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)

    override fun onDestroy() {
        closing = true
        app.mediaPlaybackAdmission.foregroundFailed()
        app.existingSoundCloudMusic?.onForegroundPlaybackServiceLost()
        observers.forEach { it.close() }; observers.clear()
        lightingObservation?.close(); lightingObservation = null
        musicObservation?.close(); musicObservation = null
        mediaAdmissionObservation?.close(); mediaAdmissionObservation = null
        getSystemService(NotificationManager::class.java).cancel(RIDE_NOTIFICATION)
        app.foregroundServiceChanged(false)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        private const val CHANNEL = "cutout.ride"
        private const val MEDIA_CHANNEL = "cutout.music"
        private const val NOTIFICATION = 1
        private const val RIDE_NOTIFICATION = 2
        private const val PAUSE = "lol.cutout.ride.PAUSE"
        private const val RESUME = "lol.cutout.ride.RESUME"
        private const val STOP = "lol.cutout.ride.STOP"

        internal fun foregroundTypes(app: CutoutApplication): Int {
            var types = 0
            val token = app.bluetooth.latestPresentation.session?.connection?.token
            val accessory = app.existingLighting?.currentPresentation()?.snapshot?.state
            if ((token != null || accessory is MobileMelkLightingSessionStateDto.Ready) &&
                app.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED) {
                types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE
            }
            val locationGranted = app.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED ||
                app.checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
            if (app.latestRide.snapshot?.liveActivityStatistics != null && locationGranted) {
                types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION
            }
            if (app.existingSoundCloudMusic?.nativePlaybackActive == true || app.existingMediaPlaybackAdmission?.hasPendingNativePlayback == true) {
                types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
            }
            return types
        }

        fun startForCurrentWork(context: Context): Boolean {
            val app = context.applicationContext as CutoutApplication
            if (foregroundTypes(app) == 0) return false
            return try {
                context.startForegroundService(Intent(context, RideForegroundService::class.java))
                true
            } catch (error: RuntimeException) {
                app.foregroundServiceError = context.getString(R.string.ride_service_failed)
                Log.w("CutoutRide", "Could not start foreground ride owner", error)
                false
            }
        }
    }
}
