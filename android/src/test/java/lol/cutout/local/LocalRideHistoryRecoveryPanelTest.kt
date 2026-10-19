package lol.cutout.local

import android.content.Context
import android.os.SystemClock
import android.view.View
import android.widget.FrameLayout
import androidx.test.core.app.ApplicationProvider
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.RideHistoryController
import lol.cutout.RideHistoryPanel
import lol.cutout.RideMapRuntime
import lol.cutout.RideMapSurface
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import uniffi.cutout_mobile_ffi.*
import java.nio.ByteBuffer
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalRideHistoryRecoveryPanelTest {
    @Test
    fun viewportEmptyOffersRustFullExtentProjectionAndExplicitCameraFit() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val runtime = RideMapRuntime(context)
        val controller = RideHistoryController(runtime)
        val surface = RecoveryMapSurface(context)
        var panel: RideHistoryPanel? = null
        var started = false
        try {
            val startedSnapshot = await { done -> runtime.startGpsRide(completed = done) }
            started = true
            val wallClock = System.currentTimeMillis().toULong()
            val receipt = SystemClock.elapsedRealtime().toULong()
            await { done ->
                runtime.submitLocationCallback(
                    samples = listOf(location(wallClock, 39.7394158, -104.9902510)),
                    receiptMonotonicMs = receipt,
                    receiptWallClockUnixMs = wallClock,
                    completed = done,
                )
            }
            val stopped = await { done -> runtime.stop(done) }
            check(stopped.summary.pointCount > 0UL) { "Fixture ride must retain a Rust route point" }
            val savedRide = await { done -> runtime.save(done) }
            check(savedRide.summary.pointCount > 0UL) { "Fixture ride must be durably saved before history queries" }
            started = false

            val listed = CountDownLatch(1)
            controller.onChanged = { if (!controller.state.loading) listed.countDown() }
            onMain { controller.refresh() }
            check(awaitWithMainLooper(listed, 10, TimeUnit.SECONDS)) { "Ride history did not load" }
            val fixtureIdentity = rideIdBytes(savedRide.rideId ?: startedSnapshot.rideId).toList()
            val ride = controller.state.rides.firstOrNull { it.id.bytes.toList() == fixtureIdentity }
                ?: error("The Rust-recorded fixture ride did not appear in history")

            val selected = CountDownLatch(1)
            controller.onChanged = {
                if (controller.state.selected?.id?.bytes?.toList() == fixtureIdentity && !controller.state.loading) {
                    selected.countDown()
                }
            }
            onMain { controller.select(ride) }
            check(awaitWithMainLooper(selected, 10, TimeUnit.SECONDS)) { "Selected ride projection did not complete" }
            check(controller.state.route?.points?.isNotEmpty() == true) { "Full-extent route projection is empty" }

            val viewport = MobileGeoBoundsDto(-89.0, -88.0, 0.0, 1.0)
            val outsideViewport = CountDownLatch(1)
            controller.onChanged = {
                if (!controller.state.loading &&
                    controller.state.route?.routePresence == MobileRideMapRoutePresenceDto.EMPTY_VIEWPORT
                ) outsideViewport.countDown()
            }
            onMain {
                controller.projectSelectedViewport(viewport)
            }
            check(awaitWithMainLooper(outsideViewport, 10, TimeUnit.SECONDS)) {
                "Rust did not report the selected route as outside the visible viewport"
            }

            val recovered = CountDownLatch(1)
            controller.onChanged = {
                panel?.render()
                if (!controller.state.loading &&
                    controller.state.route?.routePresence == MobileRideMapRoutePresenceDto.VISIBLE &&
                    surface.lastFitCamera
                ) recovered.countDown()
            }
            panel = RideHistoryPanel(context, controller, surface) {}
            onMain {
                panel!!.render()
                val preview = checkNotNull(panel!!.findViewWithTag<View>("ride_map_route_preview")) {
                    "Empty viewport must offer full-route recovery"
                }
                check(preview.performClick()) { "Route preview recovery control did not handle the click" }
                check(!surface.lastFitCamera) {
                    "The previously projected empty viewport must not consume the fit before Rust returns the full route"
                }
            }
            check(awaitWithMainLooper(recovered, 10, TimeUnit.SECONDS)) {
                "Route recovery did not replace the viewport projection and refit the camera"
            }
            check(surface.lastProjection?.points?.isNotEmpty() == true) {
                "Recovery should display the bounded full-extent Rust route projection"
            }
        } finally {
            panel?.dispose()
            if (started) runCatching { await { done -> runtime.discard(done) } }
            runtime.close()
        }
    }

    @Test
    fun retainedHistoryRefreshErrorExposesRetryWithoutDroppingRows() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val runtime = RideMapRuntime(context)
        val controller = RideHistoryController(runtime)
        var started = false
        var panel: RideHistoryPanel? = null
        try {
            await { done -> runtime.startGpsRide(completed = done) }
            started = true
            val wallClock = System.currentTimeMillis().toULong()
            val receipt = SystemClock.elapsedRealtime().toULong()
            await { done ->
                runtime.submitLocationCallback(
                    samples = listOf(location(wallClock, 39.7394158, -104.9902510)),
                    receiptMonotonicMs = receipt,
                    receiptWallClockUnixMs = wallClock,
                    completed = done,
                )
            }
            await { done -> runtime.stop(done) }
            val savedRide = await { done -> runtime.save(done) }
            check(savedRide.summary.pointCount > 0UL) { "Fixture ride must be durably saved before history queries" }
            started = false

            val loaded = CountDownLatch(1)
            controller.onChanged = { if (!controller.state.loading) loaded.countDown() }
            onMain { controller.refresh() }
            check(awaitWithMainLooper(loaded, 10, TimeUnit.SECONDS)) { "History query did not complete" }
            check(controller.state.rides.isNotEmpty()) { "Fixture must create a retained history row" }

            // Keep this isolated Robolectric fixture in the test database so the controller's
            // failed refresh can retain its previously loaded Rust-backed history row.
            runtime.close()

            panel = RideHistoryPanel(context, controller, RecoveryMapSurface(context)) {}
            val failed = CountDownLatch(1)
            controller.onChanged = {
                panel?.render()
                if (controller.state.error != null && !controller.state.loading) failed.countDown()
            }
            onMain {
                controller.refresh(preserveSelection = true)
            }
            check(awaitWithMainLooper(failed, 10, TimeUnit.SECONDS)) { "Closed-runtime refresh should report failure" }
            check(controller.state.rides.isNotEmpty()) { "Failed refresh dropped previously loaded rows" }
            val retry = checkNotNull(panel!!.findViewWithTag<View>("ride_map_history_refresh_retry")) {
                "A failed refresh with retained rows must offer retry"
            }

            val retried = CountDownLatch(1)
            controller.onChanged = {
                panel?.render()
                if (controller.state.error != null && !controller.state.loading) retried.countDown()
            }
            onMain {
                check(retry.performClick()) { "History retry action did not handle the click" }
            }
            check(awaitWithMainLooper(retried, 10, TimeUnit.SECONDS)) { "Retry did not issue another history request" }
            check(controller.state.rides.isNotEmpty()) { "Retry failure dropped the retained history rows" }
        } finally {
            panel?.dispose()
            if (started) runCatching { await { done -> runtime.discard(done) } }
            runtime.close()
        }
    }

    @Test
    fun emptyHistoryRefreshErrorAlsoExposesRetry() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val runtime = RideMapRuntime(context)
        val controller = RideHistoryController(runtime)
        val panel = RideHistoryPanel(context, controller, RecoveryMapSurface(context)) {}
        try {
            val filtered = CountDownLatch(1)
            controller.onChanged = { if (!controller.state.loading) filtered.countDown() }
            onMain { controller.setSearch("no-history-match-${UUID.randomUUID()}") }
            check(awaitWithMainLooper(filtered, 10, TimeUnit.SECONDS)) { "Unique empty history query did not complete" }
            check(controller.state.rides.isEmpty() && controller.state.error == null) {
                "The unique search should produce a successful empty state"
            }

            runtime.close()
            val failed = CountDownLatch(1)
            controller.onChanged = {
                panel.render()
                if (controller.state.error != null && !controller.state.loading) failed.countDown()
            }
            onMain { controller.refresh() }
            check(awaitWithMainLooper(failed, 10, TimeUnit.SECONDS)) { "Empty-history refresh should report failure" }
            check(controller.state.rides.isEmpty()) { "An empty failed result should remain empty" }
            val retry = checkNotNull(panel.findViewWithTag<View>("ride_map_history_refresh_retry")) {
                "An empty history failure must offer retry"
            }
            val retried = CountDownLatch(1)
            controller.onChanged = {
                panel.render()
                if (controller.state.error != null && !controller.state.loading) retried.countDown()
            }
            onMain { check(retry.performClick()) { "Empty-history retry action did not handle the click" } }
            check(awaitWithMainLooper(retried, 10, TimeUnit.SECONDS)) { "Empty-history retry did not issue another request" }
            check(controller.state.rides.isEmpty()) { "Retry changed the empty history results" }
        } finally {
            panel.dispose()
            runtime.close()
        }
    }

    private fun location(wallClock: ULong, latitude: Double, longitude: Double) = MobilePhoneLocationSampleDto(
        wallClockUnixMs = wallClock,
        sourceTimestampUnixSeconds = null,
        latitudeDegrees = latitude,
        longitudeDegrees = longitude,
        altitudeMeters = 1.0,
        horizontalAccuracyMeters = 3.0,
        verticalAccuracyMeters = null,
        speedMetersPerSecond = null,
        speedAccuracyMetersPerSecond = null,
        courseDegrees = null,
        courseAccuracyDegrees = null,
    )

    private fun rideIdBytes(value: String): ByteArray {
        val id = UUID.fromString(value)
        return ByteBuffer.allocate(16).putLong(id.mostSignificantBits).putLong(id.leastSignificantBits).array()
    }

    private fun <T> await(start: ((Result<T>) -> Unit) -> Unit): T {
        val latch = CountDownLatch(1)
        var result: Result<T>? = null
        start { result = it; latch.countDown() }
        check(awaitWithMainLooper(latch, 15, TimeUnit.SECONDS)) { "Ride-history fixture operation did not complete" }
        return checkNotNull(result).getOrThrow()
    }

    private fun onMain(block: () -> Unit) {
        InstrumentationRegistry.getInstrumentation().runOnMainSync { block() }
    }

    private class RecoveryMapSurface(context: Context) : FrameLayout(context), RideMapSurface {
        override val view: View get() = this
        override var onViewportChanged: ((MobileGeoBoundsDto) -> Unit)? = null
        override var onChargingSpotSelected: ((String) -> Unit)? = null
        var lastProjection: MobileRideMapRouteProjectionDto? = null
        var lastFitCamera = false
        override fun show(projection: MobileRideMapRouteProjectionDto) = showRoute(projection, true)
        override fun showRoute(projection: MobileRideMapRouteProjectionDto, fitCamera: Boolean) {
            lastProjection = projection
            lastFitCamera = fitCamera
        }
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
