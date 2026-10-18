package lol.cutout

import android.app.Application
import android.os.Looper
import uniffi.cutout_mobile_ffi.*

/** Process-owned native adapters; every screen uses the same Rust recording and storage owners. */
open class CutoutApplication : Application() {
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
            rides.setStopAdmissionFence(client)
            rides.bindMusicObservationLifecycle(musicLifecycleOwner.value) { result ->
                result.onSuccess { client.markObservationLifecycleReady() }
            }
        }
    }
    val music: AndroidMusicClient get() = musicOwner.value
    private val cameraOwner = lazy { AndroidCameraClient(this, ::sessionStateForNativeOwner) }
    val camera: AndroidCameraClient get() = cameraOwner.value
    private val chargingOwner = lazy { AndroidChargingClient(this, rides) }
    val charging: AndroidChargingClient get() = chargingOwner.value
    private val bluetoothOwner = lazy {
        AndroidBleClient(this)
    }
    val bluetooth: AndroidBleClient get() = bluetoothOwner.value
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
        if (chargingOwner.isInitialized()) charging.close()
        if (rideOwner.isInitialized()) rides.close()
        if (lightingOwner.isInitialized()) lighting.stop()
        super.onTerminate()
    }
}
