package lol.cutout

import android.content.Context
import android.app.AlertDialog
import android.graphics.Color
import android.view.MotionEvent
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.ImageButton
import uniffi.cutout_mobile_ffi.*
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapLibreMapOptions
import org.maplibre.android.maps.MapView
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.sources.GeoJsonSource
import org.json.JSONArray
import org.json.JSONObject
import uniffi.cutout_mobile_ffi.MobileRideMapCameraRegionDto
import uniffi.cutout_mobile_ffi.MobileGeoBoundsDto
import uniffi.cutout_mobile_ffi.MobileChargingSpotDto
import uniffi.cutout_mobile_ffi.MobileRideMapRouteDisplayPointDto
import uniffi.cutout_mobile_ffi.MobileRideMapRouteProjectionDto
import uniffi.cutout_mobile_ffi.mobileRideMapLimits
import kotlin.math.roundToInt

internal fun isBackgroundGap(reason: MobileRideSegmentStartReasonDto?): Boolean =
    reason == MobileRideSegmentStartReasonDto.BACKGROUND_GAP

internal fun <T> routeSegmentsForStyle(
    points: List<T>,
    segmentId: (T) -> ULong,
    reasons: Map<ULong, MobileRideSegmentStartReasonDto>,
    backgroundGap: Boolean,
): List<List<T>> = points.groupBy(segmentId).values.filter { segment ->
    isBackgroundGap(reasons[segmentId(segment.first())]) == backgroundGap
}

private fun segmentReasonString(reason: MobileRideSegmentStartReasonDto): Int = when (reason) {
    MobileRideSegmentStartReasonDto.INITIAL -> R.string.ride_map_segment_reason_initial
    MobileRideSegmentStartReasonDto.RESUME -> R.string.ride_map_segment_reason_resume
    MobileRideSegmentStartReasonDto.BACKGROUND_GAP -> R.string.ride_map_segment_reason_gap
    MobileRideSegmentStartReasonDto.IMPORT_BOUNDARY -> R.string.ride_map_segment_reason_import
    MobileRideSegmentStartReasonDto.STATIONARY -> R.string.ride_map_segment_reason_stationary
}

/** Small renderer contract so layout and mode behavior can be tested without MapLibre JNI. */
interface RideMapSurface {
    val view: View
    var onViewportChanged: ((MobileGeoBoundsDto) -> Unit)?
    var onChargingSpotSelected: ((String) -> Unit)?
    fun show(projection: MobileRideMapRouteProjectionDto)
    fun showRoute(projection: MobileRideMapRouteProjectionDto, fitCamera: Boolean) = show(projection)
    fun followLatestPoint() = recenter()
    var onCameraGesture: (() -> Unit)?
        get() = null
        set(value) {}
    fun showCurrentLocation(latitude: Double, longitude: Double)
    fun clearRoute()
    fun currentMapCenter(): LatLng?
    fun currentCameraPosition(): CameraPosition?
    fun restoreCameraPosition(position: CameraPosition?)
    fun showChargingSpots(spots: List<MobileChargingSpotDto>, selectedId: String? = null)
    /** Places a compact, interactive selection card over the map while charging mode is active. */
    fun showChargingPopup(view: View?) = Unit
    fun recenter()
    fun setActive(value: Boolean)
    fun onHostStart()
    fun onHostResume()
    fun onHostPause()
    fun onHostStop()
    fun onHostDestroy()
    fun onHostLowMemory()
}

object RideMapViewportSizing {
    fun mapHeightPx(availableHeightPx: Int, density: Float): Int {
        return scaledMapHeightPx(availableHeightPx, density, .46f, 160f, 360f)
    }

    fun historyDetailHeightPx(availableHeightPx: Int, density: Float): Int {
        return scaledMapHeightPx(availableHeightPx, density, .58f, 240f, 520f)
    }

    private fun scaledMapHeightPx(availableHeightPx: Int, density: Float, fraction: Float, minDp: Float, maxDp: Float): Int {
        if (availableHeightPx <= 0 || !density.isFinite() || density <= 0f) return 0
        val availableDp = availableHeightPx / density
        val heightDp = minOf(availableDp, minOf(maxOf(availableDp * fraction, minDp), maxDp))
        return (heightDp * density).roundToInt()
    }
}

/** Native MapLibre renderer. Route geometry and camera bounds are Rust projections. */
class RideRouteMapView(context: Context) : FrameLayout(context), RideMapSurface {
    override val view: View get() = this
    private val ui = CutoutUi(context)
    private val mapView: MapView
    private var map: MapLibreMap? = null
    private var latestProjection: MobileRideMapRouteProjectionDto? = null
    private var actualLocation: LatLng? = null
    private var styleLoaded = false
    private var hostStarted = false
    private var hostResumed = false
    private var active = false
    private var started = false
    private var resumed = false
    private var destroyed = false
    override var onViewportChanged: ((MobileGeoBoundsDto) -> Unit)? = null
    override var onChargingSpotSelected: ((String) -> Unit)? = null
    private var chargingSpots: List<MobileChargingSpotDto> = emptyList()
    private var selectedChargingSpotId: String? = null
    private var chargingPopup: View? = null
    private var pendingCameraPosition: CameraPosition? = null
    override var onCameraGesture: (() -> Unit)? = null
    private var pendingFollow = false
    private var pendingFit = false
    private val retry = ui.action(context.getString(R.string.ride_map_retry)) { retryStyle() }.apply {
        tag = "ride_map_retry"
        visibility = View.GONE
        contentDescription = context.getString(R.string.ride_map_retry)
    }
    private val failure = ui.text(context.getString(R.string.ride_map_failed), 14f, false, ui.muted).apply {
        gravity = Gravity.CENTER
        visibility = View.GONE
    }

    init {
        MapLibre.getInstance(context.applicationContext)
        mapView = MapView(context, MapLibreMapOptions().attributionEnabled(true).compassEnabled(true))
        mapView.contentDescription = context.getString(R.string.ride_map_title)
        addView(mapView, LayoutParams(-1, -1))
        addView(failure, LayoutParams(-1, -1))
        mapView.onCreate(null)
        mapView.getMapAsync { readyMap ->
            map = readyMap
            readyMap.uiSettings.isAttributionEnabled = true
            readyMap.uiSettings.isCompassEnabled = true
            val style = if (ui.dark) DARK_STYLE else LIGHT_STYLE
            readyMap.setStyle(style) {
                styleLoaded = true
                failure.visibility = View.GONE
                retry.visibility = View.GONE
                installRouteLayer()
                latestProjection?.let { showRoute(it, pendingFit) }
                if (pendingFollow) followLatestPoint() else if (latestProjection == null) focusActualLocationIfAvailable()
                pendingCameraPosition?.let { readyMap.cameraPosition = it }
                pendingCameraPosition = null
            }
            mapView.addOnDidFailLoadingMapListener { _ ->
                failure.visibility = View.VISIBLE
                retry.visibility = View.VISIBLE
            }
            readyMap.addOnMapClickListener { point ->
                val screenPoint = readyMap.projection.toScreenLocation(point)
                val feature = readyMap.queryRenderedFeatures(screenPoint, CHARGING_LAYER).firstOrNull()
                val id = feature?.getStringProperty("spotId")
                if (id != null) onChargingSpotSelected?.invoke(id)
                id != null
            }
            readyMap.addOnCameraIdleListener {
                val bounds = readyMap.projection.visibleRegion.latLngBounds
                onViewportChanged?.invoke(MobileGeoBoundsDto(
                    bounds.getLatSouth(), bounds.getLatNorth(), bounds.getLonWest(), bounds.getLonEast(),
                ))
            }
            readyMap.cameraPosition = CameraPosition.Builder().target(LatLng(0.0, 0.0)).zoom(1.0).build()
        }
        addView(retry, LayoutParams(LayoutParams.WRAP_CONTENT, ui.dp(48), Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL)
            .apply { bottomMargin = ui.dp(12) })
        failure.setOnClickListener { retryStyle() }
    }

    override fun onInterceptTouchEvent(event: MotionEvent): Boolean {
        if (event.actionMasked == MotionEvent.ACTION_MOVE) {
            pendingFollow = false
            onCameraGesture?.invoke()
        }
        return super.onInterceptTouchEvent(event)
    }

    override fun show(projection: MobileRideMapRouteProjectionDto) = showRoute(projection, true)

    override fun showRoute(projection: MobileRideMapRouteProjectionDto, fitCamera: Boolean) {
        latestProjection = projection
        pendingFit = fitCamera
        val activeMap = map ?: return
        if (!styleLoaded) return
        val normalJson = routeFeatureCollection(projection, backgroundGap = false)
        val gapJson = routeFeatureCollection(projection, backgroundGap = true)
        val style = activeMap.style ?: return
        installRouteLayer()
        style.getSourceAs<GeoJsonSource>(ROUTE_SOURCE)?.setGeoJson(normalJson)
        style.getSourceAs<GeoJsonSource>(GAP_ROUTE_SOURCE)?.setGeoJson(gapJson)
        style.getSourceAs<GeoJsonSource>(ENDPOINT_SOURCE)?.setGeoJson(endpointFeatureCollection(projection.points))
        style.getSourceAs<GeoJsonSource>(SINGLETON_SOURCE)?.setGeoJson(singletonFeatureCollection(projection, gap = false))
        style.getSourceAs<GeoJsonSource>(GAP_SINGLETON_SOURCE)?.setGeoJson(singletonFeatureCollection(projection, gap = true))
        val singletonLabels = projection.segments.filter { it.visiblePointCount == 1uL }.map {
            context.getString(R.string.ride_map_singleton_marker_accessibility,
                context.getString(segmentReasonString(it.startReason)))
        }
        mapView.contentDescription = listOf(context.getString(R.string.ride_map_title), *singletonLabels.toTypedArray())
            .joinToString(". ")
        if (fitCamera) fit(projection.cameraRegion)
    }

    override fun showCurrentLocation(latitude: Double, longitude: Double) {
        if (!latitude.isFinite() || !longitude.isFinite() || latitude !in -90.0..90.0 || longitude !in -180.0..180.0) return
        val firstLocation = actualLocation == null
        actualLocation = LatLng(latitude, longitude)
        map?.style?.getSourceAs<GeoJsonSource>(USER_LOCATION_SOURCE)?.setGeoJson(locationFeatureCollection(actualLocation))
        if (firstLocation && latestProjection == null) focusActualLocationIfAvailable()
    }

    override fun clearRoute() {
        latestProjection = null
        val style = map?.style ?: return
        style.getSourceAs<GeoJsonSource>(ROUTE_SOURCE)?.setGeoJson(EMPTY_ROUTES)
        style.getSourceAs<GeoJsonSource>(GAP_ROUTE_SOURCE)?.setGeoJson(EMPTY_ROUTES)
        style.getSourceAs<GeoJsonSource>(ENDPOINT_SOURCE)?.setGeoJson(EMPTY_ROUTES)
        style.getSourceAs<GeoJsonSource>(SINGLETON_SOURCE)?.setGeoJson(EMPTY_ROUTES)
        style.getSourceAs<GeoJsonSource>(GAP_SINGLETON_SOURCE)?.setGeoJson(EMPTY_ROUTES)
        mapView.contentDescription = context.getString(R.string.ride_map_title)
        focusActualLocationIfAvailable()
    }

    override fun currentMapCenter(): LatLng? = map?.cameraPosition?.takeIf { it.zoom >= 5.0 }?.target

    override fun currentCameraPosition(): CameraPosition? = map?.cameraPosition

    override fun restoreCameraPosition(position: CameraPosition?) {
        if (position == null) return
        val activeMap = map
        if (activeMap == null || !styleLoaded) pendingCameraPosition = position
        else activeMap.cameraPosition = position
    }

    override fun showChargingSpots(spots: List<MobileChargingSpotDto>, selectedId: String?) {
        chargingSpots = spots
        selectedChargingSpotId = selectedId
        val style = map?.style ?: return
        val source = style.getSourceAs<GeoJsonSource>(CHARGING_SOURCE) ?: return
        source.setGeoJson(chargingFeatureCollection(spots, selectedId))
    }

    override fun showChargingPopup(view: View?) {
        chargingPopup?.let(::removeView)
        chargingPopup = view
        view?.let {
            addView(it, LayoutParams(-1, LayoutParams.WRAP_CONTENT, Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL).apply {
                leftMargin = ui.dp(16)
                rightMargin = ui.dp(16)
                bottomMargin = ui.dp(16)
            })
            it.bringToFront()
        }
    }

    override fun recenter() {
        pendingFollow = false
        pendingFit = true
        latestProjection?.let { fit(it.cameraRegion) } ?: focusActualLocationIfAvailable()
    }

    override fun followLatestPoint() {
        pendingFollow = true
        val point = latestProjection?.points?.lastOrNull() ?: return
        val activeMap = map ?: return
        // Preserve the user's zoom when following; the first local view uses a street-scale camera.
        activeMap.cameraPosition = CameraPosition.Builder(activeMap.cameraPosition)
            .target(LatLng(point.latitudeDegrees, point.longitudeDegrees))
            .zoom(activeMap.cameraPosition.zoom.coerceAtLeast(14.0)).build()
    }

    override fun setActive(value: Boolean) { active = value; updateLifecycle() }
    override fun onHostStart() { hostStarted = true; updateLifecycle() }
    override fun onHostResume() { hostResumed = true; updateLifecycle() }
    override fun onHostPause() { hostResumed = false; updateLifecycle() }
    override fun onHostStop() { hostStarted = false; hostResumed = false; updateLifecycle() }
    override fun onHostDestroy() {
        onHostStop()
        showChargingPopup(null)
        if (!destroyed) { mapView.onDestroy(); destroyed = true }
    }

    override fun onHostLowMemory() = mapView.onLowMemory()

    private fun updateLifecycle() {
        val shouldStart = hostStarted && active && !destroyed
        val shouldResume = shouldStart && hostResumed
        if (!shouldResume && resumed) { mapView.onPause(); resumed = false }
        if (!shouldStart && started) { mapView.onStop(); started = false }
        if (shouldStart && !started) { mapView.onStart(); started = true }
        if (shouldResume && !resumed) { mapView.onResume(); resumed = true }
    }

    private fun installRouteLayer() {
        val style = map?.style ?: return
        if (style.getSource(ROUTE_SOURCE) == null) style.addSource(GeoJsonSource(ROUTE_SOURCE, EMPTY_ROUTES))
        if (style.getLayer(ROUTE_LAYER) == null) style.addLayer(LineLayer(ROUTE_LAYER, ROUTE_SOURCE).withProperties(
            PropertyFactory.lineColor(ui.brand), PropertyFactory.lineWidth(4f), PropertyFactory.lineOpacity(0.92f),
            PropertyFactory.lineCap("round"), PropertyFactory.lineJoin("round"),
        ))
        if (style.getSource(GAP_ROUTE_SOURCE) == null) style.addSource(GeoJsonSource(GAP_ROUTE_SOURCE, EMPTY_ROUTES))
        if (style.getLayer(GAP_ROUTE_LAYER) == null) style.addLayer(LineLayer(GAP_ROUTE_LAYER, GAP_ROUTE_SOURCE).withProperties(
            PropertyFactory.lineColor(ui.brand), PropertyFactory.lineWidth(4f), PropertyFactory.lineOpacity(0.92f),
            PropertyFactory.lineCap("round"), PropertyFactory.lineJoin("round"),
            PropertyFactory.lineDasharray(arrayOf(2f, 1.5f)),
        ))
        if (style.getSource(ENDPOINT_SOURCE) == null) style.addSource(GeoJsonSource(ENDPOINT_SOURCE, EMPTY_ROUTES))
        if (style.getLayer(ENDPOINT_LAYER) == null) style.addLayer(CircleLayer(ENDPOINT_LAYER, ENDPOINT_SOURCE).withProperties(
            PropertyFactory.circleRadius(5f), PropertyFactory.circleColor(ui.yellow),
            PropertyFactory.circleStrokeColor(ui.background), PropertyFactory.circleStrokeWidth(2f),
        ))
        if (style.getSource(SINGLETON_SOURCE) == null) style.addSource(GeoJsonSource(SINGLETON_SOURCE, EMPTY_ROUTES))
        if (style.getLayer(SINGLETON_LAYER) == null) style.addLayer(CircleLayer(SINGLETON_LAYER, SINGLETON_SOURCE).withProperties(
            PropertyFactory.circleRadius(7f), PropertyFactory.circleColor(ui.cyan),
            PropertyFactory.circleStrokeColor(ui.background), PropertyFactory.circleStrokeWidth(2f),
        ))
        if (style.getSource(GAP_SINGLETON_SOURCE) == null) style.addSource(GeoJsonSource(GAP_SINGLETON_SOURCE, EMPTY_ROUTES))
        if (style.getLayer(GAP_SINGLETON_LAYER) == null) style.addLayer(CircleLayer(GAP_SINGLETON_LAYER, GAP_SINGLETON_SOURCE).withProperties(
            PropertyFactory.circleRadius(7f), PropertyFactory.circleColor(ui.orange),
            PropertyFactory.circleStrokeColor(ui.background), PropertyFactory.circleStrokeWidth(2f),
        ))
        if (style.getSource(CHARGING_SOURCE) == null) style.addSource(GeoJsonSource(CHARGING_SOURCE, EMPTY_ROUTES))
        if (style.getLayer(CHARGING_LAYER) == null) style.addLayer(CircleLayer(CHARGING_LAYER, CHARGING_SOURCE).withProperties(
            PropertyFactory.circleRadius(9f), PropertyFactory.circleColor(ui.yellow),
            PropertyFactory.circleStrokeColor(ui.background), PropertyFactory.circleStrokeWidth(2f),
        ))
        style.getSourceAs<GeoJsonSource>(CHARGING_SOURCE)?.setGeoJson(chargingFeatureCollection(chargingSpots, selectedChargingSpotId))
        if (style.getSource(USER_LOCATION_SOURCE) == null) {
            style.addSource(GeoJsonSource(USER_LOCATION_SOURCE, locationFeatureCollection(actualLocation)))
        }
        if (style.getLayer(USER_LOCATION_LAYER) == null) style.addLayer(CircleLayer(USER_LOCATION_LAYER, USER_LOCATION_SOURCE).withProperties(
            PropertyFactory.circleRadius(6f), PropertyFactory.circleColor(ui.cyan),
            PropertyFactory.circleStrokeColor(ui.background), PropertyFactory.circleStrokeWidth(2f),
        ))
    }

    private fun fit(region: MobileRideMapCameraRegionDto?) {
        val activeMap = map ?: return
        if (region != null && region.latitudeSpanDegrees > 0 && region.longitudeSpanDegrees > 0) {
            val south = (region.centerLatitudeDegrees - region.latitudeSpanDegrees / 2).coerceAtLeast(-85.0)
            val north = (region.centerLatitudeDegrees + region.latitudeSpanDegrees / 2).coerceAtMost(85.0)
            val west = (region.centerLongitudeDegrees - region.longitudeSpanDegrees / 2).coerceAtLeast(-180.0)
            val east = (region.centerLongitudeDegrees + region.longitudeSpanDegrees / 2).coerceAtMost(180.0)
            if (north >= south && east >= west) {
                val bounds = LatLngBounds.from(north, east, south, west)
                activeMap.getCameraForLatLngBounds(bounds, intArrayOf(ui.dp(32), ui.dp(32), ui.dp(32), ui.dp(32)))?.let {
                    activeMap.cameraPosition = it
                }
            }
        } else {
            val points = latestProjection?.points.orEmpty()
            if (points.isNotEmpty()) {
                val bounds = LatLngBounds.fromLatLngs(points.map { LatLng(it.latitudeDegrees, it.longitudeDegrees) })
                activeMap.getCameraForLatLngBounds(bounds, intArrayOf(ui.dp(32), ui.dp(32), ui.dp(32), ui.dp(32)))?.let {
                    activeMap.cameraPosition = it
                }
            } else focusActualLocationIfAvailable()
        }
    }

    private fun focusActualLocationIfAvailable() {
        val point = actualLocation ?: return
        map?.cameraPosition = CameraPosition.Builder().target(point).zoom(14.0).build()
    }

    private fun retryStyle() {
        val activeMap = map ?: return
        val cameraBeforeRetry = activeMap.cameraPosition
        styleLoaded = false
        failure.visibility = View.GONE
        retry.visibility = View.GONE
        activeMap.setStyle(if (ui.dark) DARK_STYLE else LIGHT_STYLE) {
            styleLoaded = true
            installRouteLayer()
            latestProjection?.let { showRoute(it, false) }
            if (pendingFollow) followLatestPoint() else activeMap.cameraPosition = cameraBeforeRetry
        }
    }

    private fun routeFeatureCollection(projection: MobileRideMapRouteProjectionDto, backgroundGap: Boolean): String {
        val reasons = projection.segments.associate { it.segmentId to it.startReason }
        val features = JSONArray()
        routeSegmentsForStyle(projection.points, { it.segmentId }, reasons, backgroundGap).forEach { segment ->
            if (segment.size < 2) return@forEach
            val coordinates = JSONArray()
            segment.forEach { coordinates.put(JSONArray().put(it.longitudeDegrees).put(it.latitudeDegrees)) }
            val geometry = JSONObject().put("type", "LineString").put("coordinates", coordinates)
            features.put(JSONObject().put("type", "Feature").put("properties", JSONObject())
                .put("geometry", geometry))
        }
        return JSONObject().put("type", "FeatureCollection").put("features", features).toString()
    }

    private fun singletonFeatureCollection(projection: MobileRideMapRouteProjectionDto, gap: Boolean): String {
        val reasons = projection.segments.associate { it.segmentId to it.startReason }
        val features = JSONArray()
        val groups = projection.points.groupBy { it.segmentId }.values
        groups.forEach { segment ->
            if (segment.size != 1 || isBackgroundGap(reasons[segment.first().segmentId]) != gap) return@forEach
            val point = segment.single()
            val geometry = JSONObject().put("type", "Point")
                .put("coordinates", JSONArray().put(point.longitudeDegrees).put(point.latitudeDegrees))
            val properties = JSONObject().put("segmentId", point.segmentId.toString())
                .put("startReason", reasons[point.segmentId]?.name ?: "UNKNOWN")
            features.put(JSONObject().put("type", "Feature").put("properties", properties).put("geometry", geometry))
        }
        return JSONObject().put("type", "FeatureCollection").put("features", features).toString()
    }

    private fun endpointFeatureCollection(points: List<MobileRideMapRouteDisplayPointDto>): String {
        val features = JSONArray()
        points.groupBy { it.segmentId }.values.forEach { segment ->
            if (segment.size < 2) return@forEach
            listOf(segment.first(), segment.last()).distinctBy { it.latitudeDegrees to it.longitudeDegrees }
                .forEach { point ->
                    val geometry = JSONObject().put("type", "Point")
                        .put("coordinates", JSONArray().put(point.longitudeDegrees).put(point.latitudeDegrees))
                    features.put(JSONObject().put("type", "Feature").put("properties", JSONObject())
                        .put("geometry", geometry))
                }
        }
        return JSONObject().put("type", "FeatureCollection").put("features", features).toString()
    }

    private fun chargingFeatureCollection(spots: List<MobileChargingSpotDto>, selectedId: String?): String {
        val features = JSONArray()
        spots.forEach { spot ->
            val coordinate = spot.input?.coordinate ?: return@forEach
            val geometry = JSONObject().put("type", "Point")
                .put("coordinates", JSONArray().put(coordinate.longitudeDegrees).put(coordinate.latitudeDegrees))
            val properties = JSONObject().put("spotId", spot.id).put("selected", spot.id == selectedId)
            features.put(JSONObject().put("type", "Feature").put("properties", properties).put("geometry", geometry))
        }
        return JSONObject().put("type", "FeatureCollection").put("features", features).toString()
    }

    private fun locationFeatureCollection(point: LatLng?): String {
        if (point == null) return EMPTY_ROUTES
        val feature = JSONObject().put("type", "Feature").put("properties", JSONObject())
            .put("geometry", JSONObject().put("type", "Point")
                .put("coordinates", JSONArray().put(point.longitude).put(point.latitude)))
        return JSONObject().put("type", "FeatureCollection").put("features", JSONArray().put(feature)).toString()
    }

    companion object {
        private const val ROUTE_SOURCE = "cutout-ride-route"
        private const val ROUTE_LAYER = "cutout-ride-route-line"
        private const val GAP_ROUTE_SOURCE = "cutout-ride-route-gaps"
        private const val GAP_ROUTE_LAYER = "cutout-ride-route-gap-line"
        private const val ENDPOINT_SOURCE = "cutout-ride-endpoints"
        private const val ENDPOINT_LAYER = "cutout-ride-endpoint-circles"
        private const val SINGLETON_SOURCE = "cutout-ride-route-singletons"
        private const val SINGLETON_LAYER = "cutout-ride-route-singleton-marker"
        private const val GAP_SINGLETON_SOURCE = "cutout-ride-route-gap-singletons"
        private const val GAP_SINGLETON_LAYER = "cutout-ride-route-gap-singleton-marker"
        private const val CHARGING_SOURCE = "cutout-charging-spots"
        private const val CHARGING_LAYER = "cutout-charging-spots-circles"
        private const val USER_LOCATION_SOURCE = "cutout-user-location"
        private const val USER_LOCATION_LAYER = "cutout-user-location-dot"
        private const val EMPTY_ROUTES = "{\"type\":\"FeatureCollection\",\"features\":[]}"
        private const val LIGHT_STYLE = "https://tiles.openfreemap.org/styles/positron"
        private const val DARK_STYLE = "https://tiles.openfreemap.org/styles/dark"
    }
}

/** Ride Map screen; the Activity owns bottom navigation and location permissions. */
class RideMapPanel(
    context: Context,
    private val runtime: RideMapRuntime,
    chargingClient: AndroidChargingClient,
    private val onRequestLocation: () -> Unit,
    private val onExport: (String) -> Unit,
    private val historyPolicyForNewRide: () -> uniffi.cutout_mobile_ffi.MobileMusicHistoryPolicyDto = {
        uniffi.cutout_mobile_ffi.MobileMusicHistoryPolicyDto.DISABLED
    },
    private val resolveVehicleName: (String) -> String? = { null },
    private val onOpenLocationSettings: () -> Unit = onRequestLocation,
    mapSurfaceFactory: (Context) -> RideMapSurface = { RideRouteMapView(it) },
) : LinearLayout(context) {
    private val ui = CutoutUi(context)
    private val mapView = mapSurfaceFactory(context)
    private val history = RideHistoryController(runtime)
    private val content = ui.column()
    private val modes = ui.row()
    private val livePage = ui.column()
    private val liveScroll = ScrollView(context).apply { isFillViewport = false }
    private val historyPage = RideHistoryPanel(context, history, mapView, onExport)
    private val liveStatus = ui.text(context.getString(R.string.ride_map_idle), 15f, false, ui.muted)
    private val liveActions = ui.row()
    private val liveSummary = ui.column().apply {
        background = ui.rounded(24)
        setPadding(ui.dp(16), ui.dp(14), ui.dp(16), ui.dp(14))
    }
    private val liveMetrics = ui.row()
    private val liveMetricValues = mutableListOf<TextView>()
    private val liveMetricLabels = mutableListOf<TextView>()
    private val liveVehicle = ui.text("", 12f, true, ui.muted)
    private val liveWarning = ui.text("", 13f, false, ui.muted)
    private val locationAction = ui.action(context.getString(R.string.ride_map_request_location)) {
        val recovery = lastPresentation.locationAcquisition?.recoveryAction ?: lastPresentation.snapshot?.locationAcquisition?.recoveryAction
        if (recovery == MobileRideMapLocationRecoveryActionDto.OPEN_SETTINGS) onOpenLocationSettings() else onRequestLocation()
    }
    private var followsLatestPoint = true
    private var liveRideId: String? = null
    private var latestLiveProjection: MobileRideMapRouteProjectionDto? = null
    private val recenter = ImageButton(context).apply {
        tag = "ride_map_recenter"
        setPadding(ui.dp(12), ui.dp(12), ui.dp(12), ui.dp(12))
        background = ui.rounded(14)
        setOnClickListener {
            if (mode == Mode.CHARGING) {
                chargingPage.centerOnUserLocation()
            } else if (mode == Mode.LIVE && supportsFollow()) {
                followsLatestPoint = true
                mapView.followLatestPoint()
            } else mapView.recenter()
            updateCameraControl()
        }
    }
    private lateinit var mapSlot: FrameLayout
    private enum class Mode { LIVE, HISTORY, CHARGING }
    private var mode = Mode.LIVE
    private val chargingPage = ChargingMapPanel(context, chargingClient, mapView, onRequestLocation).apply {
        tag = "ride_map_charging_page"
    }
    private var lastProjectionRevision: ULong? = null
    private var lastPresentation = RideMapPresentation()
    private val cameraPositions = mutableMapOf<Mode, CameraPosition>()
    private var historyLoaded = false
    private var activePage = false
    private lateinit var closeObserver: AutoCloseable

    init {
        orientation = VERTICAL
        setBackgroundColor(ui.background)
        isSaveEnabled = true
        content.setPadding(ui.dp(18), ui.dp(8), ui.dp(18), 0)
        addView(content, LayoutParams(-1, -1))
        modes.tag = "ride_map_mode_picker"
        modes.background = ui.rounded(11, ui.card, false).also { it.setStroke(ui.dp(1), ui.stroke) }
        modes.setPadding(ui.dp(3), 0, ui.dp(3), 0)
        modes.addView(modeButton(R.string.ride_map_live, Mode.LIVE), LayoutParams(0, ui.dp(44), 1f))
        modes.addView(modeButton(R.string.ride_map_history, Mode.HISTORY),
            LayoutParams(0, ui.dp(44), 1f).apply { leftMargin = ui.dp(2) })
        modes.addView(modeButton(R.string.charging_mode, Mode.CHARGING),
            LayoutParams(0, ui.dp(44), 1f).apply { leftMargin = ui.dp(2) })
        content.addView(ui.title(context.getString(R.string.ride_map_title)))
        content.addView(modes, LayoutParams(-1, -2).apply { topMargin = ui.dp(12) })
        mapSlot = FrameLayout(context).apply { tag = "ride_map_surface_slot" }
        mapSlot.addView(mapView.view, FrameLayout.LayoutParams(-1, -1))
        mapSlot.addView(recenter, FrameLayout.LayoutParams(ui.dp(48), ui.dp(48), Gravity.TOP or Gravity.END).apply {
            setMargins(0, ui.dp(10), ui.dp(10), 0)
        })
        mapView.onCameraGesture = { followsLatestPoint = false; updateCameraControl() }
        val chargingViewportCallback = mapView.onViewportChanged
        mapView.onViewportChanged = { bounds ->
            chargingViewportCallback?.invoke(bounds)
            if (mode == Mode.HISTORY) history.projectSelectedViewport(bounds)
        }
        // Swift's live/history route map occupies 46% of the measured presentation viewport,
        // clamped to 160–360dp. Charging mode places its controls before the map instead.
        content.addView(mapSlot, LayoutParams(-1, 0).apply { topMargin = ui.dp(10) })
        liveScroll.addView(livePage, FrameLayout.LayoutParams(-1, -2))
        content.addView(liveScroll, LayoutParams(-1, 0, 1f).apply { topMargin = ui.dp(10) })
        content.addView(historyPage, LayoutParams(-1, 0, 1f).apply { topMargin = ui.dp(10) })
        content.addView(chargingPage, LayoutParams(-1, 0, 1f).apply { topMargin = ui.dp(10) })
        livePage.addView(liveStatus, LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        livePage.addView(liveSummary, LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
        val labels = listOf(R.string.live_map_distance, R.string.live_map_elapsed, R.string.live_map_moving, R.string.speed)
        labels.forEach { label ->
            val metric = ui.column()
            val value = ui.text("—", 21f, true).apply { tag = "live_map_metric_$label" }
            liveMetricValues += value
            metric.addView(value)
            metric.addView(ui.text(context.getString(label), 12f, false, ui.muted).also { liveMetricLabels += it })
            liveMetrics.addView(metric, LayoutParams(0, -2, 1f))
        }
        liveSummary.addView(liveMetrics, LayoutParams(-1, -2))
        if (resources.configuration.fontScale >= 1.3f) {
            liveMetrics.orientation = VERTICAL
            for (index in 0 until liveMetrics.childCount) liveMetrics.getChildAt(index).layoutParams = LayoutParams(-1, -2).apply {
                if (index > 0) topMargin = ui.dp(10)
            }
        }
        liveSummary.addView(liveVehicle, LayoutParams(-1, -2).apply { topMargin = ui.dp(12) })
        livePage.addView(liveWarning, LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        livePage.addView(locationAction, LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(8) })
        livePage.addView(liveActions, LayoutParams(-1, -2).apply { topMargin = ui.dp(8); bottomMargin = ui.dp(10) })
        historyPage.visibility = View.GONE
        chargingPage.visibility = View.GONE
        chargingPage.onLayoutModeChanged = ::updatePresentationOrder
        history.onChanged = { historyPage.render(); updateMapVisibility(); updateMapHeight(height) }
        updatePresentationOrder()
        refreshModeButtons()
        closeObserver = runtime.observe(::render)
    }

    fun onHostStart() = mapView.onHostStart()
    fun onHostResume() = mapView.onHostResume()
    fun onHostPause() = mapView.onHostPause()
    fun onHostStop() = mapView.onHostStop()
    fun onHostDestroy() {
        closeObserver.close()
        history.onChanged = null
        historyPage.dispose()
        chargingPage.dispose()
        mapView.onViewportChanged = null
        mapView.onChargingSpotSelected = null
        mapView.onCameraGesture = null
        mapView.onHostDestroy()
    }
    fun onHostLowMemory() = mapView.onHostLowMemory()
    fun showCurrentLocation(latitude: Double, longitude: Double) = mapView.showCurrentLocation(latitude, longitude)
    fun updatePhoneLocation(sample: uniffi.cutout_mobile_ffi.MobilePhoneLocationSampleDto?) = chargingPage.updateLocation(sample)
    internal fun saveUiState(): android.os.Bundle = android.os.Bundle().apply {
        mapView.currentCameraPosition()?.let { cameraPositions[mode] = it }
        putString("mode", mode.name)
        Mode.values().forEach { cameraMode ->
            cameraPositions[cameraMode]?.let { position -> putCamera(cameraMode.name, position) }
        }
        val historyState = android.os.Bundle().also(historyPage::saveUiState)
        if (cameraPositions[Mode.HISTORY] == null) historyState.remove("fitted-ride")
        putBundle("history-ui", historyState)
        putBundle("charging-ui", android.os.Bundle().also(chargingPage::savePresentationState))
    }

    /** Restores Map presentation state; ride and location state remain Rust-owned. */
    internal fun restoreUiState(saved: android.os.Bundle?) {
        if (saved == null) return
        Mode.values().forEach { cameraMode ->
            readCamera(cameraMode.name, saved)?.let { cameraPositions[cameraMode] = it }
        }
        historyPage.restoreUiState(saved.getBundle("history-ui"))
        chargingPage.restorePresentationState(saved.getBundle("charging-ui"))
        val restored = saved.getString("mode")?.let { name ->
            Mode.values().firstOrNull { it.name == name }
        } ?: return
        setMode(restored)
        mapView.restoreCameraPosition(cameraPositions[restored])
    }

    private fun android.os.Bundle.putCamera(key: String, position: CameraPosition) {
        val target = position.target ?: return
        putDouble("$key-lat", target.latitude)
        putDouble("$key-lon", target.longitude)
        putDouble("$key-zoom", position.zoom)
        putDouble("$key-tilt", position.tilt)
        putDouble("$key-bearing", position.bearing)
    }

    private fun readCamera(key: String, saved: android.os.Bundle): CameraPosition? {
        val latitude = saved.getDouble("$key-lat", Double.NaN)
        val longitude = saved.getDouble("$key-lon", Double.NaN)
        val zoom = saved.getDouble("$key-zoom", Double.NaN)
        if (!latitude.isFinite() || latitude !in -90.0..90.0 ||
            !longitude.isFinite() || longitude !in -180.0..180.0 || !zoom.isFinite()
        ) return null
        return CameraPosition.Builder()
            .target(LatLng(latitude, longitude))
            .zoom(zoom)
            .tilt(saved.getDouble("$key-tilt", 0.0))
            .bearing(saved.getDouble("$key-bearing", 0.0))
            .build()
    }
    fun setActive(value: Boolean) {
        activePage = value
        mapView.setActive(value)
        if (value && mode == Mode.CHARGING) chargingPage.refreshLocation()
        else if (!value) chargingPage.deactivate()
    }
    fun refreshLocation() {
        if (activePage && mode == Mode.CHARGING) chargingPage.refreshLocation()
    }
    fun handleBack(): Boolean = when (mode) {
        Mode.HISTORY -> historyPage.handleBack()
        Mode.CHARGING -> chargingPage.handleBack()
        Mode.LIVE -> false
    }

    private fun setMode(value: Mode) {
        if (mode == value) return
        mapView.currentCameraPosition()?.let { cameraPositions[mode] = it }
        if (mode == Mode.CHARGING) chargingPage.deactivate()
        mode = value
        updatePresentationOrder()
        when (value) {
            Mode.LIVE -> { lastProjectionRevision = null; render(lastPresentation) }
            Mode.HISTORY -> {
                mapView.clearRoute()
                historyLoaded = true
                history.refresh(preserveSelection = true)
            }
            Mode.CHARGING -> { mapView.clearRoute(); chargingPage.activate() }
        }
        mapView.restoreCameraPosition(cameraPositions[value])
        updateCameraControl()
        refreshModeButtons()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        if (mode != Mode.CHARGING) updateMapHeight(h)
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        super.onMeasure(widthMeasureSpec, heightMeasureSpec)
        if (mode != Mode.CHARGING && ::mapSlot.isInitialized) {
            // onSizeChanged runs after this parent's first child measure, too late to affect
            // the map slot's dimensions in that pass. Set the slot from the measured viewport
            // and remeasure once so the first rendered frame uses the Swift viewport formula.
            val viewportHeightPx = getMeasuredHeight()
            val params = mapSlot.layoutParams as? LayoutParams
            val desired = if (mapSlot.visibility == View.GONE) 0 else currentMapHeight(viewportHeightPx)
            if (params != null && (params.height != desired || params.weight != 0f)) {
                params.height = desired
                params.weight = 0f
                mapSlot.layoutParams = params
                super.onMeasure(widthMeasureSpec, heightMeasureSpec)
            }
        }
    }

    private fun updateMapHeight(availableHeightPx: Int) {
        val params = mapSlot.layoutParams as? LayoutParams ?: return
        val desired = if (mapSlot.visibility == View.GONE) 0 else currentMapHeight(availableHeightPx)
        if (params.height == desired && params.weight == 0f) return
        params.height = desired
        params.weight = 0f
        mapSlot.layoutParams = params
    }

    private fun updatePresentationOrder() {
        if (!::mapSlot.isInitialized || content.childCount < 2) return
        content.removeViews(2, content.childCount - 2)
        val pageSpacing = ui.dp(10)
        val mapIsVisible = shouldShowMap()
        val mapLayout = when (mode) {
            Mode.CHARGING -> if (mapIsVisible) LayoutParams(-1, 0, 1f) else LayoutParams(-1, 0)
            else -> LayoutParams(-1, currentMapHeight(height))
        }.apply { topMargin = pageSpacing }
        mapSlot.layoutParams = mapLayout
        mapSlot.visibility = if (mapIsVisible) View.VISIBLE else View.GONE

        when (mode) {
            Mode.LIVE -> {
                content.addView(mapSlot, mapLayout)
                liveScroll.visibility = View.VISIBLE
                historyPage.visibility = View.GONE
                chargingPage.visibility = View.GONE
                content.addView(liveScroll, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
                content.addView(historyPage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
                content.addView(chargingPage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
            }
            Mode.HISTORY -> {
                content.addView(mapSlot, mapLayout)
                liveScroll.visibility = View.GONE
                historyPage.visibility = View.VISIBLE
                chargingPage.visibility = View.GONE
                content.addView(liveScroll, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
                content.addView(historyPage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
                content.addView(chargingPage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
            }
            Mode.CHARGING -> {
                liveScroll.visibility = View.GONE
                historyPage.visibility = View.GONE
                chargingPage.visibility = View.VISIBLE
                val chargingHeight = if (mapIsVisible) LayoutParams(-1, -2) else LayoutParams(-1, 0, 1f)
                content.addView(chargingPage, chargingHeight.apply { topMargin = pageSpacing })
                content.addView(mapSlot, mapLayout)
                content.addView(liveScroll, LayoutParams(-1, 0, 1f))
                content.addView(historyPage, LayoutParams(-1, 0, 1f))
            }
        }
    }

    private fun shouldShowMap(): Boolean = when (mode) {
        Mode.LIVE -> latestLiveProjection?.points?.isNotEmpty() == true
        Mode.HISTORY -> history.state.rides.isNotEmpty()
        Mode.CHARGING -> chargingPage.isMapMode
    }

    private fun updateMapVisibility() {
        val desired = if (shouldShowMap()) View.VISIBLE else View.GONE
        if (mapSlot.visibility != desired) {
            mapSlot.visibility = desired
            if (mode != Mode.CHARGING) updateMapHeight(height)
        }
    }

    private fun currentMapHeight(availableHeightPx: Int): Int =
        if (mode == Mode.HISTORY && history.state.selected != null)
            RideMapViewportSizing.historyDetailHeightPx(availableHeightPx, resources.displayMetrics.density)
        else RideMapViewportSizing.mapHeightPx(availableHeightPx, resources.displayMetrics.density)

    private fun modeButton(label: Int, target: Mode) = ui.action(context.getString(label), false) { setMode(target) }.apply {
        tag = when (target) {
            Mode.LIVE -> "ride_map_mode_live"
            Mode.HISTORY -> "ride_map_mode_history"
            Mode.CHARGING -> "ride_map_mode_charging"
        }
        textSize = 14f
        setPadding(0, 0, 0, 0)
        background = android.graphics.drawable.RippleDrawable(
            android.content.res.ColorStateList.valueOf(if (ui.dark) 0x22FFFFFF else 0x22000000),
            android.graphics.drawable.InsetDrawable(ui.rounded(9, ui.card, false), 0, ui.dp(6), 0, ui.dp(6)),
            null,
        )
        setTextColor(ui.foreground)
        minimumHeight = ui.dp(44)
        minHeight = ui.dp(44)
    }

    private fun refreshModeButtons() {
        val live = modes.getChildAt(0) as android.widget.Button
        val history = modes.getChildAt(1) as android.widget.Button
        val charging = modes.getChildAt(2) as android.widget.Button
        styleMode(live, mode == Mode.LIVE)
        styleMode(history, mode == Mode.HISTORY)
        styleMode(charging, mode == Mode.CHARGING)
    }

    private fun styleMode(button: android.widget.Button, selected: Boolean) {
        val content = if (button.tag == "ride_map_mode_live") R.string.ride_map_live
            else if (button.tag == "ride_map_mode_history") R.string.ride_map_history else R.string.charging_mode
        val ripple = android.graphics.drawable.RippleDrawable(
            android.content.res.ColorStateList.valueOf(if (ui.dark) 0x22FFFFFF else 0x22000000),
            android.graphics.drawable.InsetDrawable(ui.rounded(9, if (selected) ui.background else ui.card, false), 0, ui.dp(6), 0, ui.dp(6)),
            null,
        )
        button.background = ripple
        button.setTextColor(if (selected) ui.foreground else ui.muted)
        button.isSelected = selected
        button.contentDescription = context.getString(content)
        button.setTypeface(null, if (selected) android.graphics.Typeface.BOLD else android.graphics.Typeface.NORMAL)
    }

    private fun render(value: RideMapPresentation) {
        lastPresentation = value
        val snapshot = value.snapshot
        liveStatus.text = when {
            value.error != null -> value.error
            !value.ready -> context.getString(R.string.ride_map_loading)
            snapshot == null -> context.getString(R.string.ride_map_idle)
            else -> context.getString(when (snapshot.state) {
                MobileRideLifecycleStateDto.ACTIVE -> R.string.live_map_recording
                MobileRideLifecycleStateDto.PAUSED -> R.string.live_map_paused
                MobileRideLifecycleStateDto.STOPPED -> R.string.live_map_stopped
                MobileRideLifecycleStateDto.SAVED -> R.string.live_map_saved
                MobileRideLifecycleStateDto.INTERRUPTED -> R.string.live_map_interrupted
                MobileRideLifecycleStateDto.IMPORTED -> R.string.live_map_imported
                else -> R.string.ride_map_idle
            })
        }
        renderSummary(snapshot)
        // Rust still admits every operation. With no ride, Swift offers Start once storage is ready.
        renderActions(snapshot?.allowedActions ?: if (value.ready && value.error == null)
            listOf(MobileRideEventDto.START) else emptyList())
        val recovery = value.locationAcquisition?.recoveryAction ?: snapshot?.locationAcquisition?.recoveryAction
        locationAction.visibility = if (recovery != null && recovery != MobileRideMapLocationRecoveryActionDto.NONE) View.VISIBLE else View.GONE
        locationAction.text = context.getString(if (recovery == MobileRideMapLocationRecoveryActionDto.OPEN_SETTINGS)
            R.string.live_map_location_settings else R.string.ride_map_request_location)
        if (liveRideId != snapshot?.rideId) {
            liveRideId = snapshot?.rideId
            followsLatestPoint = true
            latestLiveProjection = null
            lastProjectionRevision = null
            if (mode == Mode.LIVE) {
                mapView.clearRoute()
                liveWarning.visibility = View.GONE
                updateMapVisibility()
            }
        }
        updateCameraControl()
        val revision = snapshot?.revision
        if (mode == Mode.LIVE && snapshot != null && revision != lastProjectionRevision) {
            lastProjectionRevision = revision
            val requestedRideId = snapshot.rideId
            val budget = mobileRideMapLimits().liveTailPointLimit
            runtime.withCore({ core ->
                val current = core.currentSnapshot(android.os.SystemClock.elapsedRealtime().toULong())
                if (current == null || current.rideId != requestedRideId || current.revision != revision) null else
                    core.projectPoints(uniffi.cutout_mobile_ffi.MobileRideMapRouteProjectionOptionsDto(
                    null, budget, uniffi.cutout_mobile_ffi.MobileRideMapRoutePrivacyPolicyDto.Precise,
                ))
            }) { result ->
                if (liveRideId == requestedRideId && lastProjectionRevision == revision && mode == Mode.LIVE) {
                    result.onSuccess { projection ->
                        val it = projection ?: return@onSuccess
                        latestLiveProjection = it
                        updateMapVisibility()
                        mapView.showRoute(it, !supportsFollow() && cameraPositions[Mode.LIVE] == null)
                        if (supportsFollow() && followsLatestPoint) mapView.followLatestPoint()
                        renderRouteWarnings(it)
                        updateCameraControl()
                    }
                }
            }
        }
        if (snapshot == null && mode == Mode.LIVE) {
            latestLiveProjection = null
            mapView.clearRoute()
            liveWarning.visibility = View.GONE
            updateMapVisibility()
        }
    }

    private fun renderSummary(snapshot: MobileRideMapCoreSnapshotDto?) {
        liveSummary.visibility = if (snapshot == null) View.GONE else View.VISIBLE
        if (snapshot == null) return
        val summary = snapshot.summary
        liveMetricValues[0].text = RideMetricFormatting.distance(context, summary.distanceMeters)
        liveMetricValues[1].text = when (val elapsed = summary.elapsedTime) {
            is MobileRideElapsedTimeDto.Complete -> duration(elapsed.durationMilliseconds)
            is MobileRideElapsedTimeDto.Partial -> "≥ ${duration(elapsed.durationMilliseconds)}"
            MobileRideElapsedTimeDto.Unavailable -> "—"
        }
        liveMetricValues[2].text = when (val moving = summary.movingTime) {
            is MobileRideMovingTimeDto.Complete -> duration(moving.movingMilliseconds)
            is MobileRideMovingTimeDto.Partial -> "≥ ${duration(moving.movingMilliseconds)}"
            MobileRideMovingTimeDto.Unavailable -> "—"
        }
        val speed = snapshot.liveSpeed
        liveMetricLabels[3].text = if (speed.source == MobileRideMapSpeedSourceDto.PHONE_GPS) ui.label("euc.metric.gps_speed") else context.getString(R.string.speed)
        liveMetricValues[3].text = if (speed.source == MobileRideMapSpeedSourceDto.UNAVAILABLE) "—" else
            String.format(java.util.Locale.getDefault(), "%.1f mph", speed.millimetresPerSecond * 0.0022369362920544)
        liveVehicle.text = snapshot.associatedVehicle?.let { resolveVehicleName(it)?.takeIf(String::isNotBlank)
            ?: context.getString(R.string.ride_map_vehicle_name_unavailable) } ?: context.getString(R.string.ride_map_gps_only)
        liveVehicle.setTextColor(when (snapshot.telemetryState) {
            MobileRideMapCoreTelemetryStateDto.ASSOCIATED_FRESH -> ui.green
            MobileRideMapCoreTelemetryStateDto.ASSOCIATED_NO_TELEMETRY, MobileRideMapCoreTelemetryStateDto.ASSOCIATED_STALE -> ui.yellow
            else -> ui.muted
        })
        val telemetryDescription = when (snapshot.telemetryState) {
            MobileRideMapCoreTelemetryStateDto.ASSOCIATED_FRESH -> context.getString(R.string.live_map_receiving_vehicle)
            MobileRideMapCoreTelemetryStateDto.ASSOCIATED_NO_TELEMETRY -> context.getString(R.string.live_map_waiting_vehicle)
            MobileRideMapCoreTelemetryStateDto.ASSOCIATED_STALE -> context.getString(R.string.live_map_vehicle_unavailable)
            else -> ""
        }
        liveVehicle.contentDescription = listOf(liveVehicle.text, telemetryDescription).filter { it.isNotEmpty() }.joinToString(", ")
    }

    private fun duration(milliseconds: ULong): String = RideMetricFormatting.duration(milliseconds)

    private fun renderRouteWarnings(projection: MobileRideMapRouteProjectionDto) {
        val warnings = buildList {
            if (projection.backgroundGapCount > 0u) add(context.getString(R.string.live_map_route_interrupted))
            if (projection.sourcePointCount > projection.points.size.toULong()) add(context.getString(R.string.ride_map_points_truncated))
            if (projection.sourceSegmentCount > projection.displayedSegmentCount) add(context.getString(R.string.ride_map_segments_omitted))
        }
        liveWarning.text = warnings.joinToString("\n")
        liveWarning.visibility = if (warnings.isEmpty()) View.GONE else View.VISIBLE
    }

    private fun supportsFollow(): Boolean = lastPresentation.snapshot?.state in listOf(MobileRideLifecycleStateDto.ACTIVE, MobileRideLifecycleStateDto.PAUSED)

    private fun updateCameraControl() {
        val title = when {
            mode == Mode.CHARGING -> R.string.charging_location_center
            mode == Mode.LIVE && !supportsFollow() -> R.string.live_map_show_full_ride
            else -> R.string.ride_map_recenter
        }
        recenter.contentDescription = context.getString(title)
        recenter.isSelected = mode == Mode.LIVE && supportsFollow() && followsLatestPoint
        recenter.setImageDrawable(CutoutIcon(if (mode == Mode.LIVE && !supportsFollow()) CutoutSymbol.FIT_ROUTE else CutoutSymbol.POSITION, ui.brand))
        recenter.isEnabled = mode != Mode.LIVE || latestLiveProjection?.points?.isNotEmpty() == true
    }

    private fun renderActions(actions: List<uniffi.cutout_mobile_ffi.MobileRideEventDto>) {
        val required = actions.toSet()
        val desired = listOf(
            uniffi.cutout_mobile_ffi.MobileRideEventDto.START to R.string.ride_map_start,
            uniffi.cutout_mobile_ffi.MobileRideEventDto.PAUSE to R.string.ride_map_pause,
            uniffi.cutout_mobile_ffi.MobileRideEventDto.RESUME to R.string.ride_map_resume,
            uniffi.cutout_mobile_ffi.MobileRideEventDto.STOP to R.string.ride_map_stop,
            uniffi.cutout_mobile_ffi.MobileRideEventDto.SAVE to R.string.ride_map_save,
            uniffi.cutout_mobile_ffi.MobileRideEventDto.DISCARD to R.string.ride_map_discard,
        ).filter { it.first in required }
        val key = desired.joinToString("|") { it.first.name }
        if (liveActions.tag == key) return
        liveActions.tag = key
        liveActions.removeAllViews()
        val stacked = resources.configuration.fontScale >= 1.3f || (resources.configuration.screenWidthDp < 360 && desired.size > 2)
        liveActions.orientation = if (stacked) VERTICAL else HORIZONTAL
        desired.forEachIndexed { index, (event, title) ->
            val action = ui.action(context.getString(title), event == uniffi.cutout_mobile_ffi.MobileRideEventDto.START) {
                if (event == uniffi.cutout_mobile_ffi.MobileRideEventDto.START) onRequestLocation()
                val completion: (Result<uniffi.cutout_mobile_ffi.MobileRideMapCoreSnapshotDto>) -> Unit = { result ->
                    result.onFailure { liveStatus.text = it.message ?: context.getString(R.string.ride_map_error_title) }
                }
                when (event) {
                    uniffi.cutout_mobile_ffi.MobileRideEventDto.START -> runtime.startGpsRide(
                        policy = historyPolicyForNewRide(), completed = completion,
                    )
                    uniffi.cutout_mobile_ffi.MobileRideEventDto.PAUSE -> runtime.pause(completion)
                    uniffi.cutout_mobile_ffi.MobileRideEventDto.RESUME -> runtime.resume(completion)
                    uniffi.cutout_mobile_ffi.MobileRideEventDto.STOP -> runtime.stop(completion)
                    uniffi.cutout_mobile_ffi.MobileRideEventDto.SAVE -> runtime.save(completion)
                    uniffi.cutout_mobile_ffi.MobileRideEventDto.DISCARD -> AlertDialog.Builder(context)
                        .setTitle(R.string.live_map_discard_confirm)
                        .setNegativeButton(android.R.string.cancel, null)
                        .setPositiveButton(R.string.ride_map_discard) { _, _ -> runtime.discard(completion) }.show()
                    else -> Unit
                }
            }.apply { tag = "ride_map_action_${event.name.lowercase()}" }
            when (event) {
                MobileRideEventDto.PAUSE, MobileRideEventDto.RESUME, MobileRideEventDto.SAVE -> {
                    action.background = ui.rounded(14, ui.yellow, false); action.setTextColor(Color.BLACK)
                }
                MobileRideEventDto.STOP -> {
                    action.background = ui.rounded(14, ui.red, false); action.setTextColor(Color.WHITE)
                }
                MobileRideEventDto.DISCARD -> action.setTextColor(ui.red)
                else -> Unit
            }
            liveActions.addView(action, (if (stacked) LayoutParams(-1, -2) else LayoutParams(0, -2, 1f)).apply {
                if (index > 0) { if (stacked) topMargin = ui.dp(8) else leftMargin = ui.dp(8) }
            })
        }
    }

}
