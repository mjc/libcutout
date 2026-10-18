package lol.cutout

import android.content.Context
import android.graphics.Color
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
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

/** Small renderer contract so layout and mode behavior can be tested without MapLibre JNI. */
interface RideMapSurface {
    val view: View
    var onViewportChanged: ((MobileGeoBoundsDto) -> Unit)?
    var onChargingSpotSelected: ((String) -> Unit)?
    fun show(projection: MobileRideMapRouteProjectionDto)
    fun showCurrentLocation(latitude: Double, longitude: Double)
    fun clearRoute()
    fun currentMapCenter(): LatLng?
    fun currentCameraPosition(): CameraPosition?
    fun restoreCameraPosition(position: CameraPosition?)
    fun showChargingSpots(spots: List<MobileChargingSpotDto>, selectedId: String? = null)
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
        if (availableHeightPx <= 0 || !density.isFinite() || density <= 0f) return 0
        val availableDp = availableHeightPx / density
        val heightDp = minOf(availableDp, minOf(maxOf(availableDp * 0.46f, 160f), 360f))
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
    private var pendingCameraPosition: CameraPosition? = null
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
                latestProjection?.let(::show)
                focusActualLocationIfAvailable()
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

    override fun show(projection: MobileRideMapRouteProjectionDto) {
        latestProjection = projection
        val activeMap = map ?: return
        if (!styleLoaded) return
        val json = routeFeatureCollection(projection.points)
        val style = activeMap.style ?: return
        val source = style.getSourceAs<GeoJsonSource>(ROUTE_SOURCE)
        if (source == null) {
            style.addSource(GeoJsonSource(ROUTE_SOURCE, json))
            style.addLayer(LineLayer(ROUTE_LAYER, ROUTE_SOURCE).withProperties(
                PropertyFactory.lineColor(ui.brand),
                PropertyFactory.lineWidth(4f),
                PropertyFactory.lineOpacity(0.92f),
            ))
            style.addSource(GeoJsonSource(ENDPOINT_SOURCE, endpointFeatureCollection(projection.points)))
            style.addLayer(CircleLayer(ENDPOINT_LAYER, ENDPOINT_SOURCE).withProperties(
                PropertyFactory.circleRadius(5f),
                PropertyFactory.circleColor(ui.yellow),
                PropertyFactory.circleStrokeColor(ui.background),
                PropertyFactory.circleStrokeWidth(2f),
            ))
        } else source.setGeoJson(json)
        style.getSourceAs<GeoJsonSource>(ENDPOINT_SOURCE)?.setGeoJson(endpointFeatureCollection(projection.points))
        fit(projection.cameraRegion)
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
        style.getSourceAs<GeoJsonSource>(ENDPOINT_SOURCE)?.setGeoJson(EMPTY_ROUTES)
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

    override fun recenter() {
        latestProjection?.let { fit(it.cameraRegion) } ?: focusActualLocationIfAvailable()
    }

    override fun setActive(value: Boolean) { active = value; updateLifecycle() }
    override fun onHostStart() { hostStarted = true; updateLifecycle() }
    override fun onHostResume() { hostResumed = true; updateLifecycle() }
    override fun onHostPause() { hostResumed = false; updateLifecycle() }
    override fun onHostStop() { hostStarted = false; hostResumed = false; updateLifecycle() }
    override fun onHostDestroy() {
        onHostStop()
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
        ))
        if (style.getSource(ENDPOINT_SOURCE) == null) style.addSource(GeoJsonSource(ENDPOINT_SOURCE, EMPTY_ROUTES))
        if (style.getLayer(ENDPOINT_LAYER) == null) style.addLayer(CircleLayer(ENDPOINT_LAYER, ENDPOINT_SOURCE).withProperties(
            PropertyFactory.circleRadius(5f), PropertyFactory.circleColor(ui.yellow),
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
        styleLoaded = false
        failure.visibility = View.GONE
        retry.visibility = View.GONE
        activeMap.setStyle(if (ui.dark) DARK_STYLE else LIGHT_STYLE) {
            styleLoaded = true
            installRouteLayer()
            latestProjection?.let(::show)
        }
    }

    private fun routeFeatureCollection(points: List<MobileRideMapRouteDisplayPointDto>): String {
        val bySegment = points.groupBy { it.segmentId }
        val features = JSONArray()
        bySegment.values.forEach { segment ->
            if (segment.size < 2) return@forEach
            val coordinates = JSONArray()
            segment.forEach { coordinates.put(JSONArray().put(it.longitudeDegrees).put(it.latitudeDegrees)) }
            val geometry = JSONObject().put("type", "LineString").put("coordinates", coordinates)
            features.put(JSONObject().put("type", "Feature").put("properties", JSONObject())
                .put("geometry", geometry))
        }
        return JSONObject().put("type", "FeatureCollection").put("features", features).toString()
    }

    private fun endpointFeatureCollection(points: List<MobileRideMapRouteDisplayPointDto>): String {
        val features = JSONArray()
        points.groupBy { it.segmentId }.values.forEach { segment ->
            if (segment.isEmpty()) return@forEach
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
        private const val ENDPOINT_SOURCE = "cutout-ride-endpoints"
        private const val ENDPOINT_LAYER = "cutout-ride-endpoint-circles"
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
    mapSurfaceFactory: (Context) -> RideMapSurface = { RideRouteMapView(it) },
) : LinearLayout(context) {
    private val ui = CutoutUi(context)
    private val mapView = mapSurfaceFactory(context)
    private val history = RideHistoryController(runtime)
    private val content = ui.column()
    private val modes = ui.row()
    private val livePage = ui.column()
    private val historyPage = RideHistoryPanel(context, history, mapView, onExport)
    private val liveStatus = ui.text(context.getString(R.string.ride_map_idle), 15f, false, ui.muted)
    private val liveActions = ui.row()
    private val liveSummary = ui.text("", 13f, false, ui.muted)
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
        modes.addView(modeButton(context.getString(R.string.ride_map_live)) { setMode(Mode.LIVE) }.apply { tag = "ride_map_mode_live" },
            LayoutParams(0, ui.dp(48), 1f))
        modes.addView(modeButton(context.getString(R.string.ride_map_history)) { setMode(Mode.HISTORY) }.apply { tag = "ride_map_mode_history" },
            LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(6) })
        modes.addView(modeButton(context.getString(R.string.charging_mode)) { setMode(Mode.CHARGING) }.apply { tag = "ride_map_mode_charging" },
            LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(6) })
        content.addView(ui.title(context.getString(R.string.ride_map_title)))
        content.addView(modes, LayoutParams(-1, -2).apply { topMargin = ui.dp(12) })
        mapSlot = FrameLayout(context).apply { tag = "ride_map_surface_slot" }
        mapSlot.addView(mapView.view, FrameLayout.LayoutParams(-1, -1))
        val recenter = ui.action(context.getString(R.string.ride_map_recenter)) { mapView.recenter() }.apply {
            tag = "ride_map_recenter"
            contentDescription = context.getString(R.string.ride_map_recenter)
        }
        mapSlot.addView(recenter, FrameLayout.LayoutParams(-2, ui.dp(48), Gravity.BOTTOM or Gravity.END).apply {
            setMargins(0, 0, ui.dp(10), ui.dp(10))
        })
        // Swift's live/history route map occupies 46% of the measured presentation viewport,
        // clamped to 160–360dp. Charging mode places its controls before the map instead.
        content.addView(mapSlot, LayoutParams(-1, 0).apply { topMargin = ui.dp(10) })
        content.addView(livePage, LayoutParams(-1, 0, 1f).apply { topMargin = ui.dp(10) })
        content.addView(historyPage, LayoutParams(-1, 0, 1f).apply { topMargin = ui.dp(10) })
        content.addView(chargingPage, LayoutParams(-1, 0, 1f).apply { topMargin = ui.dp(10) })
        livePage.addView(liveStatus, LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        livePage.addView(liveSummary, LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
        livePage.addView(liveActions, LayoutParams(-1, -2).apply { topMargin = ui.dp(8); bottomMargin = ui.dp(10) })
        historyPage.visibility = View.GONE
        chargingPage.visibility = View.GONE
        chargingPage.onLayoutModeChanged = ::updatePresentationOrder
        history.onChanged = { historyPage.render() }
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
        mapView.onHostDestroy()
    }
    fun onHostLowMemory() = mapView.onHostLowMemory()
    fun showCurrentLocation(latitude: Double, longitude: Double) = mapView.showCurrentLocation(latitude, longitude)
    fun updatePhoneLocation(sample: uniffi.cutout_mobile_ffi.MobilePhoneLocationSampleDto?) = chargingPage.updateLocation(sample)
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
                if (historyLoaded) historyPage.render() else {
                    historyLoaded = true
                    history.refresh()
                }
            }
            Mode.CHARGING -> { mapView.clearRoute(); chargingPage.activate() }
        }
        mapView.restoreCameraPosition(cameraPositions[value])
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
            val desired = RideMapViewportSizing.mapHeightPx(viewportHeightPx, resources.displayMetrics.density)
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
        val desired = RideMapViewportSizing.mapHeightPx(availableHeightPx, resources.displayMetrics.density)
        if (params.height == desired && params.weight == 0f) return
        params.height = desired
        params.weight = 0f
        mapSlot.layoutParams = params
    }

    private fun updatePresentationOrder() {
        if (!::mapSlot.isInitialized || content.childCount < 2) return
        content.removeViews(2, content.childCount - 2)
        val pageSpacing = ui.dp(10)
        val mapIsVisible = mode != Mode.CHARGING || chargingPage.isMapMode
        val mapLayout = when (mode) {
            Mode.CHARGING -> if (mapIsVisible) LayoutParams(-1, 0, 1f) else LayoutParams(-1, 0)
            else -> LayoutParams(-1, RideMapViewportSizing.mapHeightPx(height, resources.displayMetrics.density))
        }.apply { topMargin = pageSpacing }
        mapSlot.layoutParams = mapLayout
        mapSlot.visibility = if (mapIsVisible) View.VISIBLE else View.GONE

        when (mode) {
            Mode.LIVE -> {
                content.addView(mapSlot, mapLayout)
                livePage.visibility = View.VISIBLE
                historyPage.visibility = View.GONE
                chargingPage.visibility = View.GONE
                content.addView(livePage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
                content.addView(historyPage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
                content.addView(chargingPage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
            }
            Mode.HISTORY -> {
                content.addView(mapSlot, mapLayout)
                livePage.visibility = View.GONE
                historyPage.visibility = View.VISIBLE
                chargingPage.visibility = View.GONE
                content.addView(livePage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
                content.addView(historyPage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
                content.addView(chargingPage, LayoutParams(-1, 0, 1f).apply { topMargin = pageSpacing })
            }
            Mode.CHARGING -> {
                livePage.visibility = View.GONE
                historyPage.visibility = View.GONE
                chargingPage.visibility = View.VISIBLE
                val chargingHeight = if (mapIsVisible) LayoutParams(-1, -2) else LayoutParams(-1, 0, 1f)
                content.addView(chargingPage, chargingHeight.apply { topMargin = pageSpacing })
                content.addView(mapSlot, mapLayout)
                content.addView(livePage, LayoutParams(-1, 0, 1f))
                content.addView(historyPage, LayoutParams(-1, 0, 1f))
            }
        }
    }

    private fun modeButton(label: String, click: () -> Unit) = ui.action(label, false, click)

    private fun refreshModeButtons() {
        val live = modes.getChildAt(0) as android.widget.Button
        val history = modes.getChildAt(1) as android.widget.Button
        val charging = modes.getChildAt(2) as android.widget.Button
        styleMode(live, mode == Mode.LIVE)
        styleMode(history, mode == Mode.HISTORY)
        styleMode(charging, mode == Mode.CHARGING)
    }

    private fun styleMode(button: android.widget.Button, selected: Boolean) {
        button.background = ui.rounded(14, if (selected) ui.yellow else ui.card, !selected)
        button.setTextColor(if (selected) Color.BLACK else ui.foreground)
        button.isSelected = selected
    }

    private fun render(value: RideMapPresentation) {
        lastPresentation = value
        val snapshot = value.snapshot
        liveStatus.text = when {
            value.error != null -> value.error
            snapshot == null -> context.getString(R.string.ride_map_idle)
            else -> snapshot.state.name.lowercase().replaceFirstChar(Char::uppercase)
        }
        liveSummary.text = snapshot?.let {
            "${context.getString(R.string.ride_map_recorded_points, it.summary.pointCount.coerceAtMost(Int.MAX_VALUE.toULong()).toInt())} · " +
                context.getString(R.string.ride_map_segments, it.segmentCount.coerceAtMost(Int.MAX_VALUE.toULong()).toInt())
        }.orEmpty()
        renderActions(snapshot?.allowedActions.orEmpty())
        val revision = snapshot?.revision
        if (mode == Mode.LIVE && snapshot != null && revision != lastProjectionRevision) {
            lastProjectionRevision = revision
            val budget = mobileRideMapLimits().liveTailPointLimit
            runtime.withCore({ core ->
                core.projectPoints(uniffi.cutout_mobile_ffi.MobileRideMapRouteProjectionOptionsDto(
                    null, budget, uniffi.cutout_mobile_ffi.MobileRideMapRoutePrivacyPolicyDto.Precise,
                ))
            }) { result ->
                if (lastProjectionRevision == revision && mode == Mode.LIVE) {
                    result.onSuccess {
                        mapView.show(it)
                        mapView.restoreCameraPosition(cameraPositions[Mode.LIVE])
                    }
                }
            }
        }
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
                    uniffi.cutout_mobile_ffi.MobileRideEventDto.DISCARD -> runtime.discard(completion)
                    else -> Unit
                }
            }.apply { tag = "ride_map_action_${event.name.lowercase()}" }
            liveActions.addView(action, LayoutParams(0, ui.dp(48), 1f).apply {
                if (index > 0) leftMargin = ui.dp(6)
            })
        }
    }

}
