package lol.cutout

import android.content.Context
import android.icu.text.MeasureFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import android.view.Gravity
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.ScrollView
import android.widget.Switch
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
    private val includeShort = Switch(context).apply {
        text = context.getString(R.string.ride_map_short_rides)
        textSize = 14f
        setTextColor(ui.foreground)
        minimumHeight = ui.dp(48)
        isChecked = controller.includeShortRides
        setOnCheckedChangeListener { _, checked -> controller.setIncludeShortRides(checked) }
    }
    private val rows = ui.column()
    private val message = ui.text("", 14f, false, ui.muted)
    private var searchRunnable: Runnable? = null
    private var exportError: String? = null

    init {
        isFillViewport = true
        clipToPadding = false
        content.setPadding(0, 0, 0, ui.dp(12))
        addView(content, LayoutParams(-1, -2))
        content.addView(search, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(4) })
        filters.addView(dateFilter, LinearLayout.LayoutParams(0, ui.dp(48), 1f))
        filters.addView(vehicleFilter, LinearLayout.LayoutParams(0, ui.dp(48), 1f).apply { leftMargin = ui.dp(8) })
        content.addView(filters, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        content.addView(includeShort, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(2) })
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
        includeShort.setOnCheckedChangeListener(null)
        includeShort.isChecked = controller.includeShortRides
        includeShort.setOnCheckedChangeListener { _, checked -> controller.setIncludeShortRides(checked) }

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
        mapView.clearRoute()
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
        card.addView(ui.text(recordedAt(ride.createdAtMilliseconds), 17f, true))
        card.addView(ui.text(vehicleLabel(ride), 14f, false, ui.muted),
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(5) })
        card.addView(ui.text("${distance(ride)} · ${context.getString(R.string.ride_map_elapsed_time, elapsed(ride))} · " +
            context.getString(R.string.ride_map_moving_time, moving(ride)), 13f, false, ui.muted),
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(7) })
        return card
    }

    private fun renderDetail(ride: MobileRideRecordDto, state: RideHistoryState) {
        rows.removeAllViews()
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
        state.route?.let { route ->
            val routeText = when (route.routePresence) {
                MobileRideMapRoutePresenceDto.VISIBLE -> context.getString(R.string.ride_map_recorded_points,
                    route.points.size)
                MobileRideMapRoutePresenceDto.EMPTY_VIEWPORT -> context.getString(R.string.ride_map_no_route)
                MobileRideMapRoutePresenceDto.EMPTY_RIDE -> context.getString(R.string.ride_map_no_route)
            }
            rows.addView(ui.text(routeText, 14f, false, ui.muted),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(10) })
            mapView.show(route)
        } ?: mapView.clearRoute()
        state.error?.let {
            rows.addView(ui.text(it, 14f, false, ui.red),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(10) })
        }
        exportError?.let {
            rows.addView(ui.text(it, 14f, false, ui.red),
                LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(10) })
        }
        if (state.loading) rows.addView(ui.text(context.getString(R.string.ride_map_loading), 14f, false, ui.muted),
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(8) })
        rows.addView(ui.action(context.getString(R.string.ride_map_share)) { export(ride) },
            LinearLayout.LayoutParams(-1, ui.dp(48)).apply { topMargin = ui.dp(12) })
    }

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

    private fun distance(ride: MobileRideRecordDto): String {
        val meters = ride.summary.distanceMillimetres.toDouble() / 1000.0
        return MeasureFormat.getInstance(Locale.getDefault(), MeasureFormat.FormatWidth.SHORT)
            .format(Measure(meters, MeasureUnit.METER))
    }

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

    private fun duration(milliseconds: ULong): String {
        val seconds = (milliseconds / 1000u).toLong()
        val hours = seconds / 3600
        val minutes = seconds % 3600 / 60
        val remaining = seconds % 60
        return if (hours > 0) String.format(Locale.getDefault(), "%d:%02d:%02d", hours, minutes, remaining)
        else String.format(Locale.getDefault(), "%d:%02d", minutes, remaining)
    }

}
