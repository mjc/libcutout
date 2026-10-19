package lol.cutout.ffitest

import android.content.Context
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.ImageButton
import android.widget.FrameLayout
import android.os.Bundle
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
    fun activityBundleRestoresHistoryModeAndIndependentLiveAndHistoryCameras() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val runtime = RideMapRuntime(context)
        val charging = AndroidChargingClient(context, runtime)
        val panels = mutableListOf<RideMapPanel>()
        val surfaces = mutableListOf<TestRideMapSurface>()
        var saved: Bundle? = null
        try {
            instrumentation.runOnMainSync {
                fun newPanel() = RideMapPanel(
                    context = context,
                    runtime = runtime,
                    chargingClient = charging,
                    onRequestLocation = {},
                    onExport = {},
                    mapSurfaceFactory = { TestRideMapSurface(it).also(surfaces::add) },
                ).also(panels::add)

                val first = newPanel()
                val liveCamera = camera(39.74, -104.99, 12.0)
                val historyCamera = camera(39.78, -105.02, 15.0)
                surfaces.last().cameraPosition = liveCamera
                first.findViewWithTag<Button>("ride_map_mode_history").performClick()
                surfaces.last().cameraPosition = historyCamera
                saved = first.saveUiState()
                first.onHostDestroy()
                panels.remove(first)

                val restored = newPanel()
                restored.restoreUiState(saved)
                check(closeCamera(surfaces.last().cameraPosition, historyCamera)) {
                    "Activity restoration did not restore the history camera"
                }
                restored.findViewWithTag<Button>("ride_map_mode_live").performClick()
                check(closeCamera(surfaces.last().cameraPosition, liveCamera)) {
                    "Switching back to Live did not restore its independent camera"
                }
            }
        } finally {
            instrumentation.runOnMainSync { panels.forEach(RideMapPanel::onHostDestroy) }
            charging.close()
            runtime.close()
        }
    }

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
    fun historyDetailUsesItsLargerSwiftViewportClamp() {
        check(RideMapViewportSizing.historyDetailHeightPx(500, 1f) == 290) {
            "A 500dp history detail viewport should use 58% (290dp)"
        }
        check(RideMapViewportSizing.historyDetailHeightPx(300, 1f) == 240) {
            "History detail should clamp up to 240dp when space permits"
        }
        check(RideMapViewportSizing.historyDetailHeightPx(200, 1f) == 200) {
            "History detail must never exceed the available viewport"
        }
        check(RideMapViewportSizing.historyDetailHeightPx(1_000, 1f) == 520) {
            "History detail should clamp at 520dp"
        }
    }

    @org.junit.Test
    fun chargingControlsPrecedeMapAndListModeHidesSharedMap() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val runtime = RideMapRuntime(context)
        val charging = AndroidChargingClient(context, runtime)
        var panel: RideMapPanel? = null
        var surface: TestRideMapSurface? = null
        var locationRequests = 0
        try {
            instrumentation.runOnMainSync {
                panel = RideMapPanel(
                    context = context,
                    runtime = runtime,
                    chargingClient = charging,
                    onRequestLocation = { locationRequests++ },
                    onExport = {},
                    mapSurfaceFactory = { TestRideMapSurface(it).also { created -> surface = created } },
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
                val idleSlot = checkNotNull(root.findViewWithTag<View>("ride_map_surface_slot"))
                check(idleSlot.visibility == View.GONE) {
                    "Idle Live must not reserve an empty map above the recording controls"
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
                val requestsBeforeRecenter = locationRequests
                val recentersBefore = surface?.recenterCount ?: 0
                root.findViewWithTag<ImageButton>("ride_map_recenter").performClick()
                check(locationRequests > requestsBeforeRecenter || (surface?.recenterCount ?: 0) > recentersBefore) {
                    "Charging recenter must request an actual location fix or center on the fresh fix"
                }
                chargingPage.findViewWithTag<Button>("charging_view_list").performClick()
                layout()
                check(slot.visibility == View.GONE) { "Charging list mode must remove the shared map from layout" }
                check(chargingPage.visibility == View.VISIBLE && chargingPage.height > 0)
                chargingPage.findViewWithTag<Button>("charging_view_map").performClick()
                layout()
                check(slot.visibility == View.VISIBLE) { "Returning to charging map mode must restore the map" }
                surface?.showChargingPopup(View(context))
                chargingPage.deactivate()
                check(surface?.chargingPopup == null) { "Leaving charging mode must remove its selected-spot popup" }
                check(surface?.chargingSpots.isNullOrEmpty()) { "Leaving charging mode must clear charging pins" }
            }
        } finally {
            instrumentation.runOnMainSync { panel?.onHostDestroy() }
            charging.close()
            runtime.close()
        }
    }

    private class TestRideMapSurface(context: Context) : FrameLayout(context), RideMapSurface {
        var chargingPopup: View? = null
        var chargingSpots: List<MobileChargingSpotDto> = emptyList()
        var recenterCount = 0
        var cameraPosition: CameraPosition? = null
        override val view: View get() = this
        override var onViewportChanged: ((MobileGeoBoundsDto) -> Unit)? = null
        override var onChargingSpotSelected: ((String) -> Unit)? = null
        override fun show(projection: MobileRideMapRouteProjectionDto) = Unit
        override fun showCurrentLocation(latitude: Double, longitude: Double) = Unit
        override fun clearRoute() = Unit
        override fun currentMapCenter(): LatLng? = null
        override fun currentCameraPosition(): CameraPosition? = cameraPosition
        override fun restoreCameraPosition(position: CameraPosition?) { cameraPosition = position }
        override fun showChargingSpots(spots: List<MobileChargingSpotDto>, selectedId: String?) { chargingSpots = spots }
        override fun showChargingPopup(view: View?) { chargingPopup = view }
        override fun recenter() { recenterCount++ }
        override fun setActive(value: Boolean) = Unit
        override fun onHostStart() = Unit
        override fun onHostResume() = Unit
        override fun onHostPause() = Unit
        override fun onHostStop() = Unit
        override fun onHostDestroy() = Unit
        override fun onHostLowMemory() = Unit
    }

    private fun camera(latitude: Double, longitude: Double, zoom: Double) = CameraPosition.Builder()
        .target(LatLng(latitude, longitude)).zoom(zoom).tilt(0.0).bearing(0.0).build()

    private fun closeCamera(actual: CameraPosition?, expected: CameraPosition): Boolean {
        val camera = actual ?: return false
        val target = camera.target ?: return false
        val expectedTarget = expected.target ?: return false
        return kotlin.math.abs(target.latitude - expectedTarget.latitude) < 1e-7 &&
            kotlin.math.abs(target.longitude - expectedTarget.longitude) < 1e-7 &&
            kotlin.math.abs(camera.zoom - expected.zoom) < 1e-7
    }
}
