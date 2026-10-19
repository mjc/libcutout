package lol.cutout

import android.app.AlertDialog
import android.icu.text.MeasureFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.view.Gravity
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.ScrollView
import android.widget.TextView
import android.text.Editable
import android.text.TextWatcher
import java.text.DateFormat
import java.io.File
import java.util.Date
import java.util.Locale
import java.util.UUID
import uniffi.cutout_mobile_ffi.MobileRideElapsedTimeDto
import uniffi.cutout_mobile_ffi.MobileRideHistoryVehicleOptionDto
import uniffi.cutout_mobile_ffi.MobileRideMapRoutePresenceDto
import uniffi.cutout_mobile_ffi.MobileRideMovingTimeDto
import uniffi.cutout_mobile_ffi.MobileRideRecordDto
import uniffi.cutout_mobile_ffi.MobileMusicHistoryStatusDto
import uniffi.cutout_mobile_ffi.MobileMusicHistoryStateDto
import uniffi.cutout_mobile_ffi.MobileMusicRideEventKindDto
import uniffi.cutout_mobile_ffi.MobileMusicProviderDto
import uniffi.cutout_mobile_ffi.MobileRideSegmentStartReasonDto
import uniffi.cutout_mobile_ffi.spotifyContentUrl

/** Rust-backed ride history list and selected ride detail. */
class RideHistoryPanel(
    context: Context,
    private val controller: RideHistoryController,
    private val mapView: RideMapSurface,
    private val onExport: (String) -> Unit,
) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val content = ui.column()
    private val filters = ui.row()
    private val dateFilter = ui.action(context.getString(R.string.ride_map_last_30_days)) { showDateMenu() }
    private val vehicleFilter = ui.action(context.getString(R.string.ride_map_all_vehicles)) { showVehicleMenu() }
    private val clearFilters = ui.action("×", false) {
        search.setText("")
        controller.clearFilters()
    }.apply {
        tag = "ride_map_clear_filters"
        textSize = 22f
        contentDescription = context.getString(R.string.ride_map_clear_filters)
    }
    private val search = EditText(context).apply {
        tag = "ride_map_search"
        hint = context.getString(R.string.ride_map_search)
        textSize = 16f
        setTextColor(ui.foreground)
        setHintTextColor(ui.muted)
        setSingleLine(true)
        background = ui.rounded(14)
        setPadding(ui.dp(14), ui.dp(12), ui.dp(14), ui.dp(12))
        minimumHeight = ui.dp(48)
        contentDescription = context.getString(R.string.ride_map_search)
    }
    private val rows = ui.column()
    private val message = ui.text("", 14f, false, ui.muted)
    private var searchRunnable: Runnable? = null
    private var exportError: String? = null
    private var musicForgetError: String? = null
    private var fittedRouteRideIdentity: ByteArray? = null
    private var explicitRouteFitRequested = false

    init {
        isFillViewport = true
        clipToPadding = false
        content.setPadding(0, 0, 0, ui.dp(12))
        addView(content, LayoutParams(-1, -2))
        content.addView(search, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
        filters.setPadding(ui.dp(4), 0, ui.dp(4), 0)
        filters.background = ui.rounded(14, ui.card, false).also { it.setStroke(ui.dp(1), ui.stroke) }
        filters.addView(dateFilter, LinearLayout.LayoutParams(0, ui.dp(44), 1f))
        filters.addView(ui.divider(), LinearLayout.LayoutParams(ui.dp(1), ui.dp(20)))
        filters.addView(vehicleFilter, LinearLayout.LayoutParams(0, ui.dp(44), 1f))
        filters.addView(ui.divider(), LinearLayout.LayoutParams(ui.dp(1), ui.dp(20)))
        filters.addView(clearFilters, LinearLayout.LayoutParams(ui.dp(44), ui.dp(44)))
        content.addView(filters, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        content.addView(message, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
        content.addView(rows, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        search.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                searchRunnable?.let(::removeCallbacks)
                val next = Runnable { controller.setSearch(s?.toString().orEmpty()) }
                searchRunnable = next
                postDelayed(next, 300)
            }
            override fun afterTextChanged(s: Editable?) = Unit
        })
        message.visibility = View.GONE
    }

    fun render() {
        dateFilter.text = context.getString(if (controller.last30Days) R.string.ride_map_last_30_days else R.string.ride_map_all_time)
        vehicleFilter.text = controller.vehicleIdentity?.let { identity ->
            controller.state.vehicles.firstOrNull { it.platformIdentifier == identity }?.displayName?.takeIf(String::isNotBlank)
                ?: context.getString(R.string.ride_map_gps_only)
        } ?: context.getString(R.string.ride_map_all_vehicles)
        clearFilters.isEnabled = controller.hasActiveFilters
        clearFilters.alpha = if (controller.hasActiveFilters) 1f else .48f

        val state = controller.state
        val selected = state.selected
        if (selected != null) renderDetail(selected, state)
        else renderList(state)
    }

    /** Consume system Back only while a ride detail is open. */
    fun handleBack(): Boolean {
        if (controller.state.selected == null) return false
        controller.closeDetail()
        return true
    }

    fun dispose() {
        searchRunnable?.let(::removeCallbacks)
        searchRunnable = null
    }

    private fun renderList(state: RideHistoryState) {
        state.listPreviewRoute?.let(mapView::show) ?: mapView.clearRoute()
        rows.removeAllViews()
        if (state.loading) {
            message.text = context.getString(R.string.ride_map_loading_history)
            message.visibility = View.VISIBLE
        } else if (state.error != null) {
            message.text = state.error
            message.visibility = View.VISIBLE
        } else if (state.rides.isEmpty()) {
            message.text = context.getString(R.string.ride_map_empty_history)
            message.visibility = View.VISIBLE
        } else {
            message.visibility = View.GONE
        }
        if (state.rides.isNotEmpty()) {
            rows.addView(ui.text(context.getString(R.string.ride_map_recent), 20f, true),
                LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = ui.dp(10) })
            state.rides.forEachIndexed { index, ride ->
                val row = rideRow(ride)
                row.setOnClickListener { controller.select(ride) }
                rows.addView(row, LinearLayout.LayoutParams(-1, -2).apply {
                    if (index > 0) topMargin = ui.dp(8)
                })
            }
        }
        if (state.nextCursor != null) {
            val button = ui.action(context.getString(R.string.ride_map_load_more)) { controller.loadMore() }.apply {
                tag = "ride_map_load_more"
                isEnabled = !state.loadingMore
            }
            rows.addView(button, LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(10) })
        }
        if (state.loadingMore) {
            message.text = context.getString(R.string.ride_map_loading_history)
            message.visibility = View.VISIBLE
        }
    }

    private fun rideRow(ride: MobileRideRecordDto): LinearLayout {
        val card = ui.column().apply {
            tag = "ride_history_${ride.id.bytes.contentHashCode()}"
            setPadding(ui.dp(14), ui.dp(12), ui.dp(14), ui.dp(12))
            background = ui.rounded(14)
            isClickable = true
            isFocusable = true
            minimumHeight = ui.dp(76)
            contentDescription = "${recordedAt(ride.createdAtMilliseconds)}. ${vehicleLabel(ride)}. " +
                "${distance(ride)}. ${elapsed(ride)}. ${moving(ride)}"
        }
        val identity = ui.column().apply {
            addView(ui.text(recordedAt(ride.createdAtMilliseconds), 17f, true))
            addView(ui.text(vehicleLabel(ride), 14f, false, ui.muted),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
        }
        val measurements = ui.column().apply {
            addView(ui.text(distance(ride), 14f, true))
            addView(ui.text(context.getString(R.string.ride_map_elapsed_time, elapsed(ride)), 12f, color = ui.muted),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
            addView(ui.text(context.getString(R.string.ride_map_moving_time, moving(ride)), 12f, color = ui.muted),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
        }
        if (resources.configuration.fontScale >= 1.3f) {
            card.addView(identity)
            card.addView(measurements, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(10) })
        } else {
            card.orientation = LinearLayout.HORIZONTAL
            card.gravity = Gravity.CENTER_VERTICAL
            card.addView(identity, LinearLayout.LayoutParams(0, -2, 1f))
            for (index in 0 until measurements.childCount) (measurements.getChildAt(index) as TextView).gravity = Gravity.END
            card.addView(measurements, LinearLayout.LayoutParams(-2, -2).apply { marginStart = ui.dp(12) })
        }
        card.addView(android.widget.ImageView(context).apply {
            setImageDrawable(CutoutIcon(CutoutSymbol.CHEVRON, ui.muted))
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, LinearLayout.LayoutParams(ui.dp(16), ui.dp(20)).apply { marginStart = ui.dp(12) })
        return card
    }

    private fun renderDetail(ride: MobileRideRecordDto, state: RideHistoryState) {
        rows.removeAllViews()
        val routeMatchesRide = historyRouteMatchesRide(state.routeRideId?.bytes, ride.id.bytes)
        val route = state.route.takeIf { routeMatchesRide }
        val back = ui.action(context.getString(R.string.ride_map_back_to_history)) { controller.closeDetail() }
        rows.addView(back, LinearLayout.LayoutParams(-1, ui.dp(48)))
        rows.addView(ui.text(context.getString(R.string.ride_map_detail), 22f, true),
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(14) })
        rows.addView(ui.text(recordedAt(ride.createdAtMilliseconds), 15f, false, ui.muted),
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
        rows.addView(ui.text(vehicleLabel(ride), 16f, true),
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        rows.addView(ui.text("${distance(ride)} · ${context.getString(R.string.ride_map_elapsed_time, elapsed(ride))} · " +
            context.getString(R.string.ride_map_moving_time, moving(ride)), 14f, false, ui.muted),
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        rows.addView(ui.text("${context.getString(R.string.ride_map_average_speed)} · ${averageSpeed(ride)}", 14f, false, ui.muted),
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        route?.let { route ->
            val routeText = when (route.routePresence) {
                MobileRideMapRoutePresenceDto.VISIBLE -> context.getString(R.string.ride_map_recorded_points_detail,
                    ride.summary.pointCount.toInt())
                MobileRideMapRoutePresenceDto.EMPTY_VIEWPORT -> context.getString(R.string.ride_map_no_route)
                MobileRideMapRoutePresenceDto.EMPTY_RIDE -> context.getString(R.string.ride_map_no_route)
            }
            rows.addView(ui.text(routeText, 14f, false, ui.muted),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(10) })
            if (route.routePresence == MobileRideMapRoutePresenceDto.VISIBLE) {
                rows.addView(ui.text(context.getString(R.string.ride_map_projected_points, route.points.size), 13f, false, ui.muted), spaced())
                if (route.sourcePointCount > route.points.size.toULong()) {
                    rows.addView(ui.text(context.getString(R.string.ride_map_points_truncated), 13f, false, ui.muted), spaced())
                    rows.addView(ui.action(context.getString(R.string.ride_map_route_preview)) {
                        explicitRouteFitRequested = true
                        controller.select(ride)
                    }, LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(6) })
                }
                rows.addView(ui.text(context.getString(R.string.ride_map_segments_detail, route.sourceSegmentCount.toInt()), 13f, false, ui.muted), spaced())
                if (route.candidateSegmentCount > route.displayedSegmentCount) {
                    rows.addView(ui.text(context.getString(R.string.ride_map_segments_omitted), 13f, false, ui.muted), spaced())
                }
                route.segments.forEachIndexed { index, segment ->
                    rows.addView(ui.text(context.getString(R.string.ride_map_segment_points,
                        index + 1, segment.visiblePointCount.toInt()) + " · " + segmentReason(segment.startReason),
                        13f, false, ui.muted), spaced())
                }
                if (route.backgroundGapCount > 0uL) rows.addView(ui.text(
                    context.getString(R.string.ride_map_background_gaps, route.backgroundGapCount.toInt()),
                    13f, false, ui.muted), spaced())
            }
            val shouldFit = shouldFitHistoryRoute(fittedRouteRideIdentity, ride.id.bytes, explicitRouteFitRequested)
            mapView.showRoute(route, fitCamera = shouldFit)
            if (shouldFit) fittedRouteRideIdentity = ride.id.bytes.copyOf()
            explicitRouteFitRequested = false
        } ?: mapView.clearRoute()
        renderMusic(ride, state)
        state.error?.let { error ->
            rows.addView(ui.text(error, 14f, false, ui.red),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(10) })
            if (!state.loading) {
                rows.addView(ui.action(context.getString(R.string.ride_map_retry_detail)) {
                    explicitRouteFitRequested = true
                    controller.select(ride)
                }, LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(8) })
            }
        }
        exportError?.let {
            rows.addView(ui.text(it, 14f, false, ui.red),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(10) })
        }
        if (state.loading) rows.addView(ui.text(context.getString(R.string.ride_map_loading), 14f, false, ui.muted),
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        rows.addView(ui.action(context.getString(R.string.ride_map_export_json)) { export(ride) },
            LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(12) })
        rows.addView(ui.action(context.getString(R.string.ride_map_share)) { shareSummary(ride) },
            LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(8) })
    }

    private fun shareSummary(ride: MobileRideRecordDto) {
        val summary = context.getString(R.string.ride_map_detail) + "\n" + distance(ride) + " · " +
            context.getString(R.string.ride_map_elapsed_time, elapsed(ride)) + " · " +
            context.getString(R.string.ride_map_moving_time, moving(ride))
        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, summary)
        }, context.getString(R.string.ride_map_share)))
    }

    private fun renderMusic(ride: MobileRideRecordDto, state: RideHistoryState) {
        val history = state.musicHistory
        val retainedState = state.musicHistoryState
        val events = history?.events.orEmpty().filterNot {
            it.kind == MobileMusicRideEventKindDto.PROVIDER_DISCONNECTED ||
                (it.kind == MobileMusicRideEventKindDto.STOPPED && it.itemIdentifier == null && it.title == null)
        }
        val canForget = events.isNotEmpty() || retainedState == MobileMusicHistoryStateDto.REDACTED ||
            retainedState == MobileMusicHistoryStateDto.HUMAN_READABLE
        if (events.isEmpty() && !canForget && state.musicError == null && history?.status != MobileMusicHistoryStatusDto.UNAVAILABLE) return
        rows.addView(ui.text(context.getString(R.string.ride_map_music_timeline), 18f, true), spaced())
        when {
            state.musicError != null || history?.status == MobileMusicHistoryStatusDto.UNAVAILABLE ->
                rows.addView(ui.text(context.getString(R.string.ride_map_music_unavailable), 14f, false, ui.muted), spaced())
            events.isEmpty() -> rows.addView(ui.text(context.getString(musicStateString(retainedState)), 14f, false, ui.muted), spaced())
            else -> events.forEach { event ->
                val item = event.title?.takeIf(String::isNotBlank) ?: context.getString(R.string.ride_map_music_event_no_title)
                val artist = event.artist?.takeIf(String::isNotBlank)
                val provider = context.getString(musicProviderString(event.provider))
                val kind = context.getString(musicKindString(event.kind))
                val time = DateFormat.getTimeInstance(DateFormat.SHORT, Locale.getDefault()).format(Date(event.wallClockAtMs.toLong()))
                val label = ui.text(context.getString(R.string.ride_map_music_event,
                    time, provider, listOfNotNull(item, artist, kind).joinToString(" · ")), 14f, false)
                val contentLink = if (event.provider == MobileMusicProviderDto.SPOTIFY)
                    event.itemIdentifier?.let(::spotifyContentUrl) else null
                if (contentLink == null) {
                    rows.addView(label, spaced())
                } else {
                    val eventRow = ui.row()
                    eventRow.addView(label, LinearLayout.LayoutParams(0, -2, 1f))
                    eventRow.addView(ui.action(context.getString(R.string.ride_map_open_spotify), false) {
                        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(contentLink)))
                    }.apply { textSize = 13f; setPadding(ui.dp(8), ui.dp(8), ui.dp(8), ui.dp(8)); minimumHeight = ui.dp(44) },
                        LinearLayout.LayoutParams(-2, ui.dp(44)).apply { marginStart = ui.dp(8) })
                    rows.addView(eventRow, spaced())
                }
            }
        }
        musicForgetError?.let { rows.addView(ui.text(context.getString(R.string.ride_map_music_forget_failed), 14f, false, ui.red), spaced()) }
        if (canForget) rows.addView(ui.action(context.getString(R.string.ride_map_music_forget)) {
            AlertDialog.Builder(context).setMessage(R.string.ride_map_music_forget_confirm)
                .setNegativeButton(R.string.charging_cancel, null)
                .setPositiveButton(R.string.ride_map_music_forget) { _, _ ->
                    musicForgetError = null
                    controller.forgetMusicHistory(ride) { result ->
                        result.onFailure { musicForgetError = it.message }
                        render()
                    }
                }.show()
        }, LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(8) })
    }

    private fun musicStateString(value: MobileMusicHistoryStateDto?): Int = when (value) {
        MobileMusicHistoryStateDto.MISSING, null -> R.string.ride_map_music_state_missing
        MobileMusicHistoryStateDto.DISABLED -> R.string.ride_map_music_state_disabled
        MobileMusicHistoryStateDto.REDACTED -> R.string.ride_map_music_state_redacted
        MobileMusicHistoryStateDto.HUMAN_READABLE -> R.string.ride_map_music_state_human
        MobileMusicHistoryStateDto.DELETED -> R.string.ride_map_music_state_deleted
    }

    private fun musicProviderString(value: MobileMusicProviderDto): Int = when (value) {
        MobileMusicProviderDto.APPLE_MUSIC -> R.string.ride_map_music_provider_apple
        MobileMusicProviderDto.SPOTIFY -> R.string.ride_map_music_provider_spotify
        MobileMusicProviderDto.SOUND_CLOUD -> R.string.ride_map_music_provider_soundcloud
    }

    private fun musicKindString(value: MobileMusicRideEventKindDto): Int = when (value) {
        MobileMusicRideEventKindDto.PLAY -> R.string.ride_map_music_kind_play
        MobileMusicRideEventKindDto.PAUSE -> R.string.ride_map_music_kind_pause
        MobileMusicRideEventKindDto.SKIP -> R.string.ride_map_music_kind_skip
        MobileMusicRideEventKindDto.ITEM_CHANGED -> R.string.ride_map_music_kind_changed
        MobileMusicRideEventKindDto.STOPPED -> R.string.ride_map_music_kind_stopped
        MobileMusicRideEventKindDto.PROVIDER_DISCONNECTED -> R.string.ride_map_music_kind_stopped
    }

    private fun segmentReason(value: MobileRideSegmentStartReasonDto): String = context.getString(when (value) {
        MobileRideSegmentStartReasonDto.INITIAL -> R.string.ride_map_segment_reason_initial
        MobileRideSegmentStartReasonDto.RESUME -> R.string.ride_map_segment_reason_resume
        MobileRideSegmentStartReasonDto.BACKGROUND_GAP -> R.string.ride_map_segment_reason_gap
        MobileRideSegmentStartReasonDto.IMPORT_BOUNDARY -> R.string.ride_map_segment_reason_import
        MobileRideSegmentStartReasonDto.STATIONARY -> R.string.ride_map_segment_reason_stationary
    })

    private fun averageSpeed(ride: MobileRideRecordDto): String {
        val metresPerSecond = ride.summary.averageSpeedMillimetresPerSecond?.toDouble()?.div(1000.0) ?: return "--"
        val country = Locale.getDefault().country
        val imperial = country in setOf("US", "LR", "MM")
        val amount = if (imperial) metresPerSecond * 2.2369362921 else metresPerSecond * 3.6
        val unit = if (imperial) MeasureUnit.MILE_PER_HOUR else MeasureUnit.KILOMETER_PER_HOUR
        return MeasureFormat.getInstance(Locale.getDefault(), MeasureFormat.FormatWidth.SHORT).format(Measure(amount, unit))
    }

    private fun spaced() = LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) }

    private fun export(ride: MobileRideRecordDto) {
        exportError = null
        val filename = "ride-${UUID.randomUUID()}.json"
        val directory = File(context.cacheDir, "ride-exports")
        controller.exportRide(ride.id, File(directory, filename).absolutePath) { result ->
            result.fold(
                onSuccess = onExport,
                onFailure = { error ->
                    exportError = error.message ?: context.getString(R.string.ride_map_error_title)
                    render()
                },
            )
        }
    }

    private fun showDateMenu() {
        PopupMenu(context, dateFilter).apply {
            menu.add(context.getString(R.string.ride_map_last_30_days)).setOnMenuItemClickListener {
                controller.setLast30Days(true); true
            }
            menu.add(context.getString(R.string.ride_map_all_time)).setOnMenuItemClickListener {
                controller.setLast30Days(false); true
            }
            menu.add(context.getString(R.string.ride_map_short_rides)).apply {
                isCheckable = true
                isChecked = controller.includeShortRides
                setOnMenuItemClickListener {
                    controller.setIncludeShortRides(!controller.includeShortRides)
                    true
                }
            }
            show()
        }
    }

    private fun showVehicleMenu() {
        PopupMenu(context, vehicleFilter).apply {
            menu.add(context.getString(R.string.ride_map_all_vehicles)).setOnMenuItemClickListener {
                controller.setVehicle(null); true
            }
            controller.state.vehicles.forEach { option ->
                menu.add(vehicleOptionLabel(option)).setOnMenuItemClickListener {
                    controller.setVehicle(option.platformIdentifier); true
                }
            }
            show()
        }
    }

    private fun vehicleOptionLabel(option: MobileRideHistoryVehicleOptionDto): String =
        option.displayName?.takeIf(String::isNotBlank) ?: context.getString(R.string.ride_map_vehicle_name_unavailable)

    private fun vehicleLabel(ride: MobileRideRecordDto): String =
        ride.associatedVehicleName?.takeIf(String::isNotBlank)
            ?: ride.candidateVehicleName?.takeIf(String::isNotBlank)
            ?: if (ride.associatedVehicle != null || ride.candidateVehicle != null)
                context.getString(R.string.ride_map_vehicle_name_unavailable)
            else context.getString(R.string.ride_map_gps_only)

    private fun recordedAt(milliseconds: ULong): String =
        if (milliseconds == 0uL) context.getString(R.string.ride_map_untitled)
        else DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, Locale.getDefault())
            .format(Date(milliseconds.toLong()))

    private fun distance(ride: MobileRideRecordDto): String =
        RideMetricFormatting.distance(context, ride.summary.distanceMillimetres.toDouble() / 1000.0)

    private fun elapsed(ride: MobileRideRecordDto): String = when (val value = ride.summary.elapsedTime) {
        is MobileRideElapsedTimeDto.Complete -> duration(value.durationMilliseconds)
        is MobileRideElapsedTimeDto.Partial -> "≥ ${duration(value.durationMilliseconds)}"
        MobileRideElapsedTimeDto.Unavailable -> "--"
    }

    private fun moving(ride: MobileRideRecordDto): String = when (val value = ride.summary.movingTime) {
        is MobileRideMovingTimeDto.Complete -> duration(value.movingMilliseconds)
        is MobileRideMovingTimeDto.Partial -> "≥ ${duration(value.movingMilliseconds)}"
        MobileRideMovingTimeDto.Unavailable -> "--"
    }

    private fun duration(milliseconds: ULong): String = RideMetricFormatting.duration(milliseconds)

}
