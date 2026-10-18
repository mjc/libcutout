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
    private val bluetoothOwner = lazy {
        AndroidBleClient(this)
    }
    val bluetooth: AndroidBleClient get() = bluetoothOwner.value
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
        if (rideOwner.isInitialized()) rides.close()
        super.onTerminate()
    }
}
