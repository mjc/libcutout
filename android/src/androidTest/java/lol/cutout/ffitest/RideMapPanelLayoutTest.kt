package lol.cutout.ffitest

import android.content.Context
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.FrameLayout
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidChargingClient
import lol.cutout.ChargingMapPanel
import lol.cutout.RideMapPanel
import lol.cutout.RideMapRuntime
import lol.cutout.RideMapSurface
import lol.cutout.RideMapViewportSizing
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import uniffi.cutout_mobile_ffi.MobileChargingSpotDto
import uniffi.cutout_mobile_ffi.MobileGeoBoundsDto
import uniffi.cutout_mobile_ffi.MobileRideMapRouteProjectionDto
import kotlin.math.roundToInt

/** Layout regression coverage for the production map panel with only the renderer injected. */
open class RideMapPanelLayoutTest {
    @org.junit.Test
    fun liveAndHistoryMapHeightUsesSwiftViewportClamp() {
        check(RideMapViewportSizing.mapHeightPx(500, 1f) == 230) // 46% of available height.
        check(RideMapViewportSizing.mapHeightPx(300, 1f) == 160) // Minimum, but not taller than viewport.
        check(RideMapViewportSizing.mapHeightPx(1_000, 1f) == 360) // Maximum.
        val expectedAt2x = 460 // 500dp viewport × 46%, rounded to 230dp at 2x density.
        val actualAt2x = RideMapViewportSizing.mapHeightPx(1_000, 2f)
        check(actualAt2x == expectedAt2x) {
            "Expected Swift-clamped map height $expectedAt2x px at 2x density, got $actualAt2x px"
        }
        check(RideMapViewportSizing.mapHeightPx(0, 1f) == 0)
    }

    @org.junit.Test
    fun chargingControlsPrecedeMapAndListModeHidesSharedMap() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val runtime = RideMapRuntime(context)
        val charging = AndroidChargingClient(context, runtime)
        var panel: RideMapPanel? = null
        try {
            instrumentation.runOnMainSync {
                panel = RideMapPanel(
                    context = context,
                    runtime = runtime,
                    chargingClient = charging,
                    onRequestLocation = {},
                    onExport = {},
                    mapSurfaceFactory = { TestRideMapSurface(it) },
                )
                val root = checkNotNull(panel)
                val width = (393 * context.resources.displayMetrics.density).toInt()
                val height = (852 * context.resources.displayMetrics.density).toInt()
                fun layout() {
                    root.measure(
                        View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                        View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
                    )
                    root.layout(0, 0, width, height)
                }
                layout()
                val surface = checkNotNull(root.findViewWithTag<View>("ride_map_surface_slot"))
                val density = context.resources.displayMetrics.density
                val availableDp = height / density
                val expectedHeight = (minOf(availableDp, minOf(maxOf(availableDp * 0.46f, 160f), 360f)) * density)
                    .roundToInt()
                check(surface.height == expectedHeight) {
                    "Expected initial map surface height $expectedHeight px from $height px viewport at $density density, got ${surface.height} px"
                }
                root.findViewWithTag<Button>("ride_map_mode_charging").performClick()
                layout()
                val slot = checkNotNull(root.findViewWithTag<View>("ride_map_surface_slot"))
                val chargingPage = checkNotNull(root.findViewWithTag<ChargingMapPanel>("ride_map_charging_page"))
                val content = root.getChildAt(0) as ViewGroup
                check(content.indexOfChild(chargingPage) < content.indexOfChild(slot)) {
                    "Charging mode controls must precede the map surface"
                }
                check(slot.visibility == View.VISIBLE)
                check(slot.height > 0 && chargingPage.height > 0)
                chargingPage.findViewWithTag<Button>("charging_view_list").performClick()
                layout()
                check(slot.visibility == View.GONE) { "Charging list mode must remove the shared map from layout" }
                check(chargingPage.visibility == View.VISIBLE && chargingPage.height > 0)
                chargingPage.findViewWithTag<Button>("charging_view_map").performClick()
                layout()
                check(slot.visibility == View.VISIBLE) { "Returning to charging map mode must restore the map" }
            }
        } finally {
            instrumentation.runOnMainSync { panel?.onHostDestroy() }
            charging.close()
            runtime.close()
        }
    }

    private class TestRideMapSurface(context: Context) : FrameLayout(context), RideMapSurface {
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
}
