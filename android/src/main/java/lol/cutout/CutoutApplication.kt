package lol.cutout

import android.app.Application
import android.os.Looper
import uniffi.cutout_mobile_ffi.*

/** Process-owned native adapters; every screen uses the same Rust recording and storage owners. */
open class CutoutApplication : Application() {
    /** Keep the native renderer replaceable without replacing Activity navigation or ride state. */
    open fun createRideMapSurface(context: android.content.Context): RideMapSurface = RideRouteMapView(context)

    private val rideOwner = lazy { RideMapRuntime(this) }
    val rides: RideMapRuntime get() = rideOwner.value
    private val locationOwner = lazy { RideLocationRuntime(this, rides) { rideServiceActive } }
    val locations: RideLocationRuntime get() = locationOwner.value
    private val sessionStateOwner = lazy {
        CutoutSessionStateHandle.withDatabase(rides.databaseForNativeOwner())
    }
    fun sessionStateForNativeOwner(): CutoutSessionStateHandle {
        check(Looper.myLooper() != Looper.getMainLooper()) { "Session storage must open off the main thread" }
        return sessionStateOwner.value
    }
    private val musicLifecycleOwner = lazy { MobileMusicProviderLifecycle() }
    private val musicOwner = lazy {
        AndroidMusicClient(this, musicLifecycleOwner.value, rides).also { client ->
            client.bindSoundCloudPlayer(soundCloudMusic)
            rides.setStopAdmissionFence(client)
            rides.bindMusicObservationLifecycle(musicLifecycleOwner.value) { result ->
                result.onSuccess { client.markObservationLifecycleReady() }
            }
        }
    }
    val music: AndroidMusicClient get() = musicOwner.value
    private val mediaAdmissionOwner = lazy { MediaPlaybackForegroundAdmission(this) }
    val mediaPlaybackAdmission: MediaPlaybackForegroundAdmission get() = mediaAdmissionOwner.value
    val existingMediaPlaybackAdmission: MediaPlaybackForegroundAdmission? get() = if (mediaAdmissionOwner.isInitialized()) mediaPlaybackAdmission else null
    private val soundCloudOwner = lazy {
        AndroidSoundCloudPlayer(this, beforePlayback = mediaPlaybackAdmission::request)
    }
    val soundCloudMusic: AndroidSoundCloudPlayer get() = soundCloudOwner.value
    val existingSoundCloudMusic: AndroidSoundCloudPlayer? get() = if (soundCloudOwner.isInitialized()) soundCloudMusic else null
    private val cameraOwner = lazy {
        AndroidCameraClient(this, ::sessionStateForNativeOwner,
            currentCaptureIdentity = { bluetooth.diagnostics.activeCameraCaptureIdentity() })
    }
    val camera: AndroidCameraClient get() = cameraOwner.value
    private val chargingOwner = lazy { AndroidChargingClient(this, rides) }
    val charging: AndroidChargingClient get() = chargingOwner.value
    private val bluetoothOwner = lazy {
        AndroidBleClient(this) { presentation, state ->
            phoneAlarms.onSessionObservation(presentation, state)
        }
    }
    val bluetooth: AndroidBleClient get() = bluetoothOwner.value
    private val phoneAlarmOwner = lazy { AndroidPhoneAlarmClient(this) }
    val phoneAlarms: AndroidPhoneAlarmClient get() = phoneAlarmOwner.value
    fun refreshPhoneAlarmAuthorization() {
        if (!phoneAlarmOwner.isInitialized()) return
        rides.withDatabase({ phoneAlarms.refreshAuthorization(sessionStateForNativeOwner()) }) { }
    }
    private val lightingOwner = lazy { AndroidMelkLightingClient(this) }
    val lighting: AndroidMelkLightingClient get() = lightingOwner.value
    val existingLighting: AndroidMelkLightingClient? get() = if (lightingOwner.isInitialized()) lighting else null

    @Volatile var rideServiceActive = false
        private set
    @Volatile var latestRide = RideMapPresentation()
    @Volatile var foregroundServiceError: String? = null

    fun foregroundServiceChanged(active: Boolean) {
        rideServiceActive = active
        if (locationOwner.isInitialized()) locations.refreshEnvironment()
    }

    override fun onTerminate() {
        if (locationOwner.isInitialized()) locations.close()
        if (cameraOwner.isInitialized()) camera.close()
        if (musicOwner.isInitialized()) music.close()
        if (soundCloudOwner.isInitialized()) soundCloudMusic.close()
        if (mediaAdmissionOwner.isInitialized()) mediaPlaybackAdmission.close()
        if (phoneAlarmOwner.isInitialized()) phoneAlarms.close()
        if (chargingOwner.isInitialized()) charging.close()
        if (bluetoothOwner.isInitialized()) bluetooth.close {
            if (rideOwner.isInitialized()) rides.close()
        } else if (rideOwner.isInitialized()) rides.close()
        if (lightingOwner.isInitialized()) lighting.stop()
        super.onTerminate()
    }
}
