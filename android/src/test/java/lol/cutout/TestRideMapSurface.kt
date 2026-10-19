package lol.cutout

import android.content.Context
import android.view.View
import android.widget.FrameLayout
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import uniffi.cutout_mobile_ffi.MobileChargingSpotDto
import uniffi.cutout_mobile_ffi.MobileGeoBoundsDto
import uniffi.cutout_mobile_ffi.MobileRideMapRouteProjectionDto

/** Reusable test renderer for map panel and Activity workflow tests without MapLibre JNI. */
class TestRideMapSurface(context: Context) : FrameLayout(context), RideMapSurface {
    override val view: View get() = this
    override var onViewportChanged: ((MobileGeoBoundsDto) -> Unit)? = null
    override var onChargingSpotSelected: ((String) -> Unit)? = null
    override fun show(projection: MobileRideMapRouteProjectionDto) = Unit
    override fun showCurrentLocation(latitude: Double, longitude: Double) = Unit
    override fun clearRoute() = Unit
    override fun currentMapCenter(): LatLng? = null
    override fun currentCameraPosition(): CameraPosition? = null
    override fun restoreCameraPosition(position: CameraPosition?) = Unit
    override fun showChargingSpots(spots: List<MobileChargingSpotDto>, selectedId: String?) = Unit
    override fun recenter() = Unit
    override fun setActive(value: Boolean) = Unit
    override fun onHostStart() = Unit
    override fun onHostResume() = Unit
    override fun onHostPause() = Unit
    override fun onHostStop() = Unit
    override fun onHostDestroy() = Unit
    override fun onHostLowMemory() = Unit
}
