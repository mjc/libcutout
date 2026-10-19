package lol.cutout.ffitest

import android.content.Context
import android.content.res.Configuration
import android.graphics.Rect
import androidx.test.platform.app.InstrumentationRegistry
import android.view.View
import android.view.Gravity
import android.graphics.drawable.RippleDrawable
import android.widget.FrameLayout
import android.widget.TextView
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import lol.cutout.RideHistoryController
import lol.cutout.restoreRideSelection
import lol.cutout.historyRouteMatchesRide
import lol.cutout.shouldFitHistoryRoute
import lol.cutout.historyVehicleFilterLabel
import lol.cutout.historyVehicleOptions
import lol.cutout.RideMapSurface
import lol.cutout.routeSegmentsForStyle
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import uniffi.cutout_mobile_ffi.MobileChargingSpotDto
import uniffi.cutout_mobile_ffi.MobileGeoBoundsDto
import uniffi.cutout_mobile_ffi.MobileRideSegmentStartReasonDto
import uniffi.cutout_mobile_ffi.MobileRideMapRouteProjectionDto
import lol.cutout.RideMapRuntime
import org.junit.Test

open class RideHistoryControllerTest {
    @Test
    fun selectionRestorationUsesStableRideIdentityAcrossFreshRows() {
        data class Row(val identity: ByteArray, val version: Int)
        val original = Row(byteArrayOf(4, 2), 1)
        val refreshed = Row(byteArrayOf(4, 2), 2)
        val other = Row(byteArrayOf(9), 2)

        check(restoreRideSelection(original.identity, listOf(other, refreshed)) { it.identity } === refreshed) {
            "The selected ride should resolve to its refreshed Rust row by identity"
        }
        check(restoreRideSelection(original.identity, listOf(other)) { it.identity } == null) {
            "A ride removed by the active query must no longer remain selected"
        }
    }

    @Test
    fun retainedRouteAndRetryAreFencedToTheSelectedRideIdentity() {
        val firstRide = byteArrayOf(1, 4, 7)
        val secondRide = byteArrayOf(2, 5, 8)
        check(historyRouteMatchesRide(firstRide.copyOf(), firstRide)) {
            "A retry for the same ride should retain its last valid route projection"
        }
        check(!historyRouteMatchesRide(firstRide.copyOf(), secondRide)) {
            "A route projection from another ride must not appear in the newly selected detail"
        }
        check(!historyRouteMatchesRide(null, secondRide)) {
            "A failed initial route projection should remain retryable"
        }
    }

    @Test
    fun historyCameraFitsOnlyForNewRideOrExplicitRoutePreview() {
        val ride = byteArrayOf(3, 1, 4)
        val nextRide = byteArrayOf(9, 2, 6)
        check(shouldFitHistoryRoute(null, ride, explicitFit = false)) {
            "The first visible projection should fit its ride"
        }
        check(!shouldFitHistoryRoute(ride.copyOf(), ride, explicitFit = false)) {
            "An unrelated detail refresh must preserve the user's map camera"
        }
        check(shouldFitHistoryRoute(ride.copyOf(), ride, explicitFit = true)) {
            "An explicit route-preview action should fit again"
        }
        check(shouldFitHistoryRoute(ride, nextRide, explicitFit = false)) {
            "Selecting a different ride should fit its route"
        }
    }

    @Test
    fun vehicleFilterKeepsAnUnresolvedSelectionAcrossMetadataRefresh() {
        val selection = "board:retired"
        val refreshed = historyVehicleOptions(
            listOf("board:z" to "Zeta", "board:a" to "Alpha", "board:z" to "Duplicate"),
            selection,
            "Vehicle name unavailable",
        )

        check(refreshed == listOf("board:a" to "Alpha", "board:retired" to "Vehicle name unavailable", "board:z" to "Zeta")) {
            "Vehicle options should be unique and stable, while retaining an active identity no longer in metadata"
        }
        check(historyVehicleFilterLabel(selection, refreshed, "All vehicles", "Vehicle name unavailable") ==
            "Vehicle name unavailable") {
            "An unresolved selected identity must not be relabeled as a GPS-only ride"
        }
        check(historyVehicleFilterLabel(null, refreshed, "All vehicles", "Vehicle name unavailable") ==
            "All vehicles")
    }

    @Test
    fun historyFiltersFitNarrowWidthsAtNormalAndAccessibilityTextSizes() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val baseContext = instrumentation.targetContext
        val runtime = RideMapRuntime(baseContext)
        try {
            instrumentation.runOnMainSync {
                val base = baseContext.resources.configuration
                listOf(1f, 2f).forEach { fontScale ->
                    val configuration = Configuration(base).apply { this.fontScale = fontScale }
                    val context = baseContext.createConfigurationContext(configuration)
                    val panel = lol.cutout.RideHistoryPanel(
                        context,
                        RideHistoryController(runtime),
                        HistoryTestRideMapSurface(context),
                    ) {}
                    val density = context.resources.displayMetrics.density
                    val width = (320 * density).toInt()
                    panel.measure(
                        View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                        View.MeasureSpec.makeMeasureSpec((1000 * density).toInt(), View.MeasureSpec.AT_MOST),
                    )
                    panel.layout(0, 0, width, panel.measuredHeight)

                    val filters = checkNotNull(panel.findViewWithTag<View>("ride_map_filters"))
                    val date = checkNotNull(panel.findViewWithTag<TextView>("ride_map_date_filter"))
                    val vehicle = checkNotNull(panel.findViewWithTag<TextView>("ride_map_vehicle_filter"))
                    val clear = checkNotNull(panel.findViewWithTag<View>("ride_map_clear_filters"))
                    val search = checkNotNull(panel.findViewWithTag<TextView>("ride_map_search"))
                    val minTargetHeight = (44 * density).toInt()
                    val filtersBounds = Rect(0, 0, filters.width, filters.height)
                    val dateBounds = Rect(0, 0, date.width, date.height)
                    val vehicleBounds = Rect(0, 0, vehicle.width, vehicle.height)
                    val clearBounds = Rect(0, 0, clear.width, clear.height)
                    panel.offsetDescendantRectToMyCoords(filters, filtersBounds)
                    panel.offsetDescendantRectToMyCoords(date, dateBounds)
                    panel.offsetDescendantRectToMyCoords(vehicle, vehicleBounds)
                    panel.offsetDescendantRectToMyCoords(clear, clearBounds)

                    check(filters.width <= width && filters.left >= 0 && filters.right <= width) {
                        "History filter group escaped the 320dp viewport at font scale $fontScale"
                    }
                    check(search.width >= filters.width - (8 * density).toInt()) {
                        "History search field did not use the available narrow width at font scale $fontScale"
                    }
                    listOf(date, vehicle).forEach { label ->
                        check(label.text.isNotBlank() && label.layout != null &&
                            (0 until label.layout.lineCount).all { label.layout.getEllipsisCount(it) == 0 }) {
                            "History filter label was blank, unlaid out, or ellipsized at font scale $fontScale: ${label.text}"
                        }
                        check((label.gravity and Gravity.RELATIVE_HORIZONTAL_GRAVITY_MASK) == Gravity.START) {
                            "History filter labels should align with the leading edge at font scale $fontScale"
                        }
                        check(label.compoundDrawablesRelative[2] != null) {
                            "History filter labels should expose a trailing menu chevron at font scale $fontScale"
                        }
                    }
                    check(listOf(date, vehicle, clear).all { control ->
                        (control.background as? RippleDrawable)?.getDrawable(0) == null
                    }) {
                        "Filter controls should keep the parent filter surface visible instead of drawing nested card fills"
                    }
                    check(date.height >= minTargetHeight && vehicle.height >= minTargetHeight && clear.height >= minTargetHeight) {
                        "History filter target fell below 44dp at font scale $fontScale: ${date.height}/${vehicle.height}/${clear.height}px"
                    }
                    check(dateBounds.left >= filtersBounds.left && dateBounds.right <= filtersBounds.right &&
                        vehicleBounds.left >= filtersBounds.left && vehicleBounds.right <= filtersBounds.right &&
                        clearBounds.left >= filtersBounds.left && clearBounds.right <= filtersBounds.right) {
                        "A history filter control extended beyond its container at font scale $fontScale"
                    }
                    if (fontScale >= 1.3f) {
                        check(vehicleBounds.top >= maxOf(dateBounds.bottom, clearBounds.bottom) &&
                            kotlin.math.abs(clearBounds.exactCenterY() - dateBounds.exactCenterY()) <= 1f &&
                            dateBounds.right <= clearBounds.left) {
                            "Accessibility filters should stack vehicle below date and vertically center clear beside date"
                        }
                    } else {
                        check(dateBounds.top == vehicleBounds.top && dateBounds.right <= vehicleBounds.left &&
                            vehicleBounds.right <= clearBounds.left) {
                            "Normal-size history filters should remain in one non-overlapping row"
                        }
                    }
                    check(clear.contentDescription?.isNotBlank() == true) {
                        "Clear-filters target must retain its accessible name"
                    }
                }
            }
        } finally {
            runtime.close()
        }
    }

    @Test
    fun mapKeepsRustBackgroundGapSegmentsSeparatelyStyled() {
        data class Point(val segmentId: ULong, val sequence: Int)
        val points = listOf(Point(1UL, 1), Point(1UL, 2), Point(2UL, 3), Point(2UL, 4))
        val reasons = mapOf(
            1UL to MobileRideSegmentStartReasonDto.INITIAL,
            2UL to MobileRideSegmentStartReasonDto.BACKGROUND_GAP,
        )
        val continuous = routeSegmentsForStyle(points, Point::segmentId, reasons, backgroundGap = false)
        val gaps = routeSegmentsForStyle(points, Point::segmentId, reasons, backgroundGap = true)
        check(continuous == listOf(points.take(2))) { "The Rust initial segment should use the normal route layer" }
        check(gaps == listOf(points.drop(2))) { "The Rust background-gap segment should use the dashed gap layer" }
    }

    @Test
    fun rustHistoryQueryAndFilterReloadUseTheProcessOwnedDatabase() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val runtime = RideMapRuntime(context)
        val controller = RideHistoryController(runtime)
        val loaded = CountDownLatch(1)
        controller.onChanged = {
            if (!controller.state.loading) loaded.countDown()
        }

        try {
            InstrumentationRegistry.getInstrumentation().runOnMainSync { controller.refresh() }
            check(awaitCallback(loaded, 10, TimeUnit.SECONDS)) { "Rust ride history query did not complete" }
            check(controller.state.error == null) { "Rust ride history query failed: ${controller.state.error}" }
            check(controller.state.rides.map { it.id }.toSet().size == controller.state.rides.size) {
                "A Rust history page must not repeat ride identities"
            }

            val reloaded = CountDownLatch(1)
            controller.onChanged = {
                if (!controller.state.loading) reloaded.countDown()
            }
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                controller.setIncludeShortRides(true)
            }
            check(awaitCallback(reloaded, 10, TimeUnit.SECONDS)) { "Filtered Rust history query did not complete" }
            check(controller.state.error == null) { "Filtered Rust history query failed: ${controller.state.error}" }
            check(controller.state.rides.map { it.id }.toSet().size == controller.state.rides.size) {
                "A filtered Rust history page must not repeat ride identities"
            }
            check(controller.includeShortRides) { "The selected short-ride filter was lost" }

            val allTime = CountDownLatch(1)
            controller.onChanged = { if (!controller.state.loading) allTime.countDown() }
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                controller.setLast30Days(false)
                controller.setIncludeShortRides(true)
                controller.setSearch("")
            }
            check(awaitCallback(allTime, 10, TimeUnit.SECONDS)) { "All-time history query did not complete" }
            val selectedRide = controller.state.rides.firstOrNull()
            if (selectedRide != null) {
                val rideKey = selectedRide.id.bytes.toList()
                val opened = CountDownLatch(1)
                controller.onChanged = { if (controller.state.selected != null && !controller.state.loading) opened.countDown() }
                InstrumentationRegistry.getInstrumentation().runOnMainSync { controller.select(selectedRide) }
                check(awaitCallback(opened, 10, TimeUnit.SECONDS)) { "Selected ride details did not complete" }

                val refreshed = CountDownLatch(1)
                controller.onChanged = {
                    if (controller.state.selected?.id?.bytes?.toList() == rideKey && !controller.state.loading) {
                        refreshed.countDown()
                    }
                }
                InstrumentationRegistry.getInstrumentation().runOnMainSync {
                    controller.refresh(preserveSelection = true)
                }
                check(awaitCallback(refreshed, 10, TimeUnit.SECONDS)) { "History refresh did not restore selected ride details" }
                check(controller.searchText.isEmpty() && !controller.last30Days && controller.includeShortRides) {
                    "Refreshing history changed its active filters"
                }
            } else {
                check(controller.searchText.isEmpty() && !controller.last30Days && controller.includeShortRides) {
                    "Refreshing empty history changed its active filters"
                }
            }
        } finally {
            runtime.close()
        }
    }

    /** Local Robolectric wrappers override this to drain their paused main looper. */
    protected open fun awaitCallback(latch: CountDownLatch, timeout: Long, unit: TimeUnit): Boolean =
        latch.await(timeout, unit)

    private class HistoryTestRideMapSurface(context: Context) : FrameLayout(context), RideMapSurface {
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
