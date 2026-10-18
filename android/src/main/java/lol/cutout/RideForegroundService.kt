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

/** Android foreground lifetime for the existing app-owned ride and accessory adapters. */
class RideForegroundService : Service() {
    private val app get() = application as CutoutApplication
    private val observers = mutableListOf<AutoCloseable>()
    private var started = false
    private var closing = false
    private var lightingObservation: AutoCloseable? = null

    override fun onCreate() {
        super.onCreate()
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.ride_notification_channel), NotificationManager.IMPORTANCE_LOW),
        )
        observers += app.bluetooth.addObserver { refresh() }
        observers += app.rides.observe { value -> app.latestRide = value; refresh() }
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
        val types = foregroundTypes(app)
        if (types == 0) {
            stopSelf()
            return
        }
        try {
            startForeground(NOTIFICATION, notification(), types)
            if (!started) {
                started = true
                app.foregroundServiceChanged(true)
            }
            app.foregroundServiceError = null
        } catch (error: RuntimeException) {
            app.foregroundServiceError = getString(R.string.ride_service_failed)
            Log.w("CutoutRide", "Could not maintain foreground ride ownership", error)
            app.foregroundServiceChanged(false)
            stopSelf()
        }
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
        observers.forEach { it.close() }; observers.clear()
        lightingObservation?.close(); lightingObservation = null
        app.foregroundServiceChanged(false)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        private const val CHANNEL = "cutout.ride"
        private const val NOTIFICATION = 1
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
            return types
        }

        fun startForCurrentWork(context: Context) {
            val app = context.applicationContext as CutoutApplication
            if (foregroundTypes(app) == 0) return
            try {
                context.startForegroundService(Intent(context, RideForegroundService::class.java))
            } catch (error: RuntimeException) {
                app.foregroundServiceError = context.getString(R.string.ride_service_failed)
                Log.w("CutoutRide", "Could not start foreground ride owner", error)
            }
        }
    }
}
