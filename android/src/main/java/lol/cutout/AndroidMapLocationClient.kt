package lol.cutout

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import uniffi.cutout_mobile_ffi.MobileChargingLocationFixDto
import uniffi.cutout_mobile_ffi.mobileChargingLocationCandidateIsAccepted
import uniffi.cutout_mobile_ffi.mobileChargingLocationFixIsFresh
import java.io.Closeable

/** Short-lived native map-location acquisition, separate from ride recording admission. */
class AndroidMapLocationClient(
    context: Context,
    private val onFix: (MobileChargingLocationFixDto) -> Unit,
    private val onUnavailable: (String) -> Unit = {},
) : Closeable {
    private val appContext = context.applicationContext
    private val manager = appContext.getSystemService(LocationManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private val thread = HandlerThread("CutoutMapLocation").apply { start() }
    private val handler = Handler(thread.looper)
    @Volatile private var bestFix: MobileChargingLocationFixDto? = null
    @Volatile private var closed = false
    @Volatile private var generation = 0L
    private var activeListener: LocationListener? = null
    private var timeout: Runnable? = null

    private val listener = object : LocationListener {
        override fun onLocationChanged(location: Location) { accept(location) }
        override fun onProviderEnabled(provider: String) = Unit
        override fun onProviderDisabled(provider: String) = Unit
    }

    /** Requests a bounded native fix only when the Activity has already obtained permission. */
    fun refresh(): Boolean {
        if (closed) return false
        val hasFine = appContext.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
        val hasCoarse = appContext.checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
        if (!hasFine && !hasCoarse) {
            main.post { if (!closed) onUnavailable("Location permission is unavailable") }
            return false
        }
        val request = synchronized(this) { ++generation }
        handler.post {
            if (closed || request != generation) return@post
            stopOnOwner()
            try {
                val locationManager = manager ?: throw IllegalStateException("Location services are unavailable")
                val providers = locationManager.getProviders(true).filter { provider ->
                    provider == LocationManager.NETWORK_PROVIDER || (hasFine && provider == LocationManager.GPS_PROVIDER)
                }
                if (providers.isEmpty()) throw IllegalStateException("No location provider is enabled")
                providers.forEach { provider ->
                    runCatching { locationManager.getLastKnownLocation(provider) }.getOrNull()?.let(::accept)
                }
                activeListener = listener
                providers.forEach { provider ->
                    locationManager.requestLocationUpdates(provider, 0L, 0f, listener, thread.looper)
                }
                timeout = Runnable {
                    if (request == generation) stopOnOwner()
                }.also { handler.postDelayed(it, REQUEST_WINDOW_MS) }
            } catch (error: SecurityException) {
                stopOnOwner()
                main.post { onUnavailable(error.message ?: "Location permission is unavailable") }
            } catch (error: IllegalArgumentException) {
                stopOnOwner()
                main.post { onUnavailable(error.message ?: "No location provider is enabled") }
            } catch (error: IllegalStateException) {
                stopOnOwner()
                main.post { onUnavailable(error.message ?: "Location services are unavailable") }
            }
        }
        return true
    }

    /** Reuses actual ride samples without forwarding them into the recording callback path. */
    fun observe(sample: uniffi.cutout_mobile_ffi.MobilePhoneLocationSampleDto) {
        val timestamp = sample.sourceTimestampUnixSeconds ?: sample.wallClockUnixMs.toDouble() / 1_000.0
        val accuracy = sample.horizontalAccuracyMeters ?: return
        accept(MobileChargingLocationFixDto(sample.latitudeDegrees, sample.longitudeDegrees, timestamp, accuracy))
    }

    fun freshFix(nowUnixSeconds: Double = System.currentTimeMillis() / 1_000.0): MobileChargingLocationFixDto? =
        bestFix?.takeIf { mobileChargingLocationFixIsFresh(it, nowUnixSeconds) }

    fun cancel() {
        if (closed) return
        synchronized(this) { generation++ }
        handler.post { stopOnOwner() }
    }

    private fun accept(location: Location) {
        if (!location.hasAccuracy() || location.time <= 0L) return
        accept(MobileChargingLocationFixDto(
            location.latitude,
            location.longitude,
            location.time / 1_000.0,
            location.accuracy.toDouble(),
        ))
    }

    private fun accept(candidate: MobileChargingLocationFixDto) {
        val now = System.currentTimeMillis() / 1_000.0
        val old = bestFix
        if (!mobileChargingLocationCandidateIsAccepted(old, candidate, now)) return
        bestFix = candidate
        main.post { if (!closed) onFix(candidate) }
    }

    private fun stopOnOwner() {
        timeout?.let(handler::removeCallbacks)
        timeout = null
        activeListener?.let { current -> runCatching { manager?.removeUpdates(current) } }
        activeListener = null
    }

    override fun close() {
        if (closed) return
        closed = true
        synchronized(this) { generation++ }
        handler.post {
            stopOnOwner()
            thread.quitSafely()
        }
    }

    companion object { private const val REQUEST_WINDOW_MS = 8_000L }
}
