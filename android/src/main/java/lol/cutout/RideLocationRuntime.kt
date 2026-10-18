package lol.cutout

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.HandlerThread
import android.os.SystemClock
import uniffi.cutout_mobile_ffi.*

/** Applies Rust-selected location acquisition to Android's native provider. */
class RideLocationRuntime(
    context: Context,
    private val rideMap: RideMapRuntime,
    private val isForegroundServiceActive: () -> Boolean,
) : AutoCloseable {
    private val appContext = context.applicationContext
    private val locationManager = appContext.getSystemService(LocationManager::class.java)
    private val thread = HandlerThread("CutoutRideLocation").apply { start() }
    @Volatile private var nativeProviderUnavailable = false
    @Volatile private var lastAcquisition: MobileRideMapLocationAcquisitionDto? = null
    @Volatile private var nativeAuthorizationEvidence: MobileRideMapLocationAuthorizationDto? = null
    @Volatile private var rideOpen = false
    private var active = false
    private var fineGranted = false
    private var coarseGranted = false
    private var closed = false
    private val presentationObservation = rideMap.observe { value ->
        rideOpen = when (value.snapshot?.state) {
            MobileRideLifecycleStateDto.ACTIVE,
            MobileRideLifecycleStateDto.PAUSED,
            MobileRideLifecycleStateDto.INTERRUPTED -> true
            else -> false
        }
    }

    private val listener = object : LocationListener {
        override fun onLocationChanged(location: Location) {
            onLocationCallback(listOf(location))
        }

        override fun onProviderEnabled(provider: String) {
            nativeProviderUnavailable = false
            refreshEnvironment()
        }

        override fun onProviderDisabled(provider: String) {
            refreshEnvironment()
        }
    }

    /**
     * Supplies native permission history when Android reports no grant. The OS grant remains
     * authoritative whenever fine or coarse location access is actually present.
     */
    fun setAuthorizationEvidence(value: MobileRideMapLocationAuthorizationDto) {
        require(
            value == MobileRideMapLocationAuthorizationDto.NOT_DETERMINED ||
                value == MobileRideMapLocationAuthorizationDto.DENIED ||
                value == MobileRideMapLocationAuthorizationDto.RESTRICTED,
        ) { "Authorization evidence without an Android grant must be denied, restricted, or not determined" }
        nativeAuthorizationEvidence = value
        refreshEnvironment()
    }

    /** Read current OS evidence, ask Rust for the acquisition decision, then apply that decision. */
    fun refreshEnvironment(completed: ((Result<MobileRideMapLocationAcquisitionDto>) -> Unit)? = null) {
        val fine = hasPermission(Manifest.permission.ACCESS_FINE_LOCATION)
        val coarse = hasPermission(Manifest.permission.ACCESS_COARSE_LOCATION)
        fineGranted = fine
        coarseGranted = coarse
        val authorization = when {
            fine || coarse -> MobileRideMapLocationAuthorizationDto.WHEN_IN_USE
            else -> nativeAuthorizationEvidence ?: MobileRideMapLocationAuthorizationDto.NOT_DETERMINED
        }
        val servicesEnabled = try {
            locationManager?.isLocationEnabled == true
        } catch (_: SecurityException) {
            false
        }
        val serviceUnavailable = !isForegroundServiceActive() &&
            (rideOpen || lastAcquisition?.demand == MobileRideMapLocationDemandDto.RECORD)
        rideMap.refreshLocationEnvironment(
            MobileRideMapLocationEnvironmentDto(
                authorization, servicesEnabled, nativeProviderUnavailable || serviceUnavailable,
            ),
        ) { result ->
            result.onSuccess { acquisition ->
                lastAcquisition = acquisition
                applyAcquisition(acquisition)
            }
            completed?.invoke(result)
        }
    }

    /**
     * Platform callback adapter. No location is synthesized or dropped here; Rust receives the
     * original timestamp, coordinates, accuracy, speed and course evidence and owns admission.
     */
    fun onLocationCallback(
        locations: List<Location>,
        completed: ((Result<List<MobileRideMapCoreOutcomeDto>>) -> Unit)? = null,
    ) {
        val receiptMonotonic = elapsedRealtimeMillis()
        val receiptWallClock = System.currentTimeMillis().toULong()
        val samples = locations.map { location ->
            MobilePhoneLocationSampleDto(
                wallClockUnixMs = receiptWallClock,
                sourceTimestampUnixSeconds = location.time.takeIf { it > 0L }?.let { it / 1000.0 },
                latitudeDegrees = location.latitude,
                longitudeDegrees = location.longitude,
                altitudeMeters = location.altitude,
                horizontalAccuracyMeters = location.accuracy.toDouble().takeIf { location.hasAccuracy() },
                verticalAccuracyMeters = if (location.hasVerticalAccuracy()) location.verticalAccuracyMeters.toDouble() else null,
                speedMetersPerSecond = location.speed.toDouble().takeIf { location.hasSpeed() },
                speedAccuracyMetersPerSecond = if (location.hasSpeedAccuracy()) location.speedAccuracyMetersPerSecond.toDouble() else null,
                courseDegrees = location.bearing.toDouble().takeIf { location.hasBearing() },
                courseAccuracyDegrees = if (location.hasBearingAccuracy()) location.bearingAccuracyDegrees.toDouble() else null,
            )
        }
        rideMap.submitLocationCallback(samples, receiptMonotonic, receiptWallClock) { result ->
            if (result.isFailure) rideMap.reportLocationFailure(result.exceptionOrNull()?.message ?: "Location sample was not recorded")
            completed?.invoke(result)
        }
    }

    private fun applyAcquisition(acquisition: MobileRideMapLocationAcquisitionDto) {
        val shouldRun = acquisition.demand == MobileRideMapLocationDemandDto.RECORD &&
            isForegroundServiceActive() && (fineGranted || coarseGranted)
        if (!shouldRun) {
            stopProviders()
            return
        }
        if (active) return
        try {
            val manager = locationManager ?: throw IllegalArgumentException("Android location service is unavailable")
            var requested = fineGranted && locationManager?.isProviderEnabled(LocationManager.GPS_PROVIDER) == true
            if (requested) {
                manager.requestLocationUpdates(LocationManager.GPS_PROVIDER, LOCATION_INTERVAL_MS, 0f, listener, thread.looper)
                active = true
            }
            if (manager.isProviderEnabled(LocationManager.NETWORK_PROVIDER)) {
                manager.requestLocationUpdates(LocationManager.NETWORK_PROVIDER, LOCATION_INTERVAL_MS, 0f, listener, thread.looper)
                requested = true
                active = true
            }
            if (!requested) throw IllegalArgumentException("No Android location provider is enabled")
            nativeProviderUnavailable = false
            rideMap.reportLocationFailure(null)
        } catch (error: SecurityException) {
            stopProviders()
            nativeProviderUnavailable = true
            rideMap.reportLocationFailure(error.message ?: "Location permission is unavailable")
            forwardUnavailableEnvironment()
        } catch (error: IllegalArgumentException) {
            stopProviders()
            nativeProviderUnavailable = true
            rideMap.reportLocationFailure(error.message ?: "Android location provider is unavailable")
            forwardUnavailableEnvironment()
        }
    }

    private fun stopProviders() {
        if (!active) return
        try {
            locationManager?.removeUpdates(listener)
        } catch (error: SecurityException) {
            rideMap.reportLocationFailure(error.message ?: "Location updates could not be stopped")
        }
        active = false
    }

    private fun hasPermission(permission: String): Boolean =
        appContext.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

    private fun forwardUnavailableEnvironment() {
        val authorization = when {
            hasPermission(Manifest.permission.ACCESS_FINE_LOCATION) ||
                hasPermission(Manifest.permission.ACCESS_COARSE_LOCATION) ->
                MobileRideMapLocationAuthorizationDto.WHEN_IN_USE
            else -> nativeAuthorizationEvidence ?: MobileRideMapLocationAuthorizationDto.NOT_DETERMINED
        }
        val servicesEnabled = try {
            locationManager?.isLocationEnabled == true
        } catch (_: SecurityException) {
            false
        }
        rideMap.refreshLocationEnvironment(
            MobileRideMapLocationEnvironmentDto(authorization, servicesEnabled, true),
        ) { result -> result.onSuccess { lastAcquisition = it } }
    }

    private fun elapsedRealtimeMillis(): ULong =
        (SystemClock.elapsedRealtimeNanos() / 1_000_000L).toULong()

    override fun close() {
        if (closed) return
        closed = true
        stopProviders()
        presentationObservation.close()
        thread.quitSafely()
    }

    companion object {
        private const val LOCATION_INTERVAL_MS = 1_000L
    }
}
