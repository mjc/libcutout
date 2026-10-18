package lol.cutout

import uniffi.cutout_mobile_ffi.MobileRideCursorDto
import uniffi.cutout_mobile_ffi.MobileRideHistoryFilterDto
import uniffi.cutout_mobile_ffi.MobileRideHistoryVehicleOptionDto
import uniffi.cutout_mobile_ffi.MobileRideIdDto
import uniffi.cutout_mobile_ffi.MobileRideMapRoutePrivacyPolicyDto
import uniffi.cutout_mobile_ffi.MobileRideMapRouteProjectionDto
import uniffi.cutout_mobile_ffi.MobileRideMapRouteProjectionOptionsDto
import uniffi.cutout_mobile_ffi.MobileRideMapLimitsDto
import uniffi.cutout_mobile_ffi.MobileRideRecordDto
import uniffi.cutout_mobile_ffi.mobileRideMapLimits

data class RideHistoryState(
    val rides: List<MobileRideRecordDto> = emptyList(),
    val vehicles: List<MobileRideHistoryVehicleOptionDto> = emptyList(),
    val nextCursor: MobileRideCursorDto? = null,
    val selected: MobileRideRecordDto? = null,
    val route: MobileRideMapRouteProjectionDto? = null,
    val loading: Boolean = false,
    val loadingMore: Boolean = false,
    val error: String? = null,
)

/** Rust-backed bounded ride-history queries. Stale asynchronous page and detail results are ignored. */
class RideHistoryController(private val runtime: RideMapRuntime, private val now: () -> Long = System::currentTimeMillis) {
    var state = RideHistoryState()
        private set
    var searchText = ""
        private set
    var vehicleIdentity: String? = null
        private set
    var includeShortRides = false
        private set
    var last30Days = true
        private set
    private var queryGeneration = 0L
    private var detailGeneration = 0L

    var onChanged: (() -> Unit)? = null

    fun refresh() {
        val generation = ++queryGeneration
        detailGeneration++
        state = state.copy(loading = true, loadingMore = false, error = null, selected = null, route = null)
        notifyChanged()
        val currentFilter = filter()
        runtime.withDatabase({ db ->
            val page = db.listRidesFiltered(null, currentFilter, mobileRideMapLimits().historyPageLimit)
            val vehicles = db.listRideHistoryVehicleOptions()
            page to vehicles
        }) { result ->
            if (generation != queryGeneration) return@withDatabase
            result.fold(
                onSuccess = { (page, vehicles) ->
                    state = state.copy(rides = page.rides, nextCursor = page.nextCursor, vehicles = vehicles,
                        loading = false, error = null)
                },
                onFailure = { error -> state = state.copy(loading = false, error = error.message ?: "") },
            )
            notifyChanged()
        }
    }

    fun loadMore() {
        val cursor = state.nextCursor ?: return
        if (state.loadingMore || state.loading) return
        val generation = queryGeneration
        state = state.copy(loadingMore = true, error = null)
        notifyChanged()
        runtime.withDatabase({ db ->
            db.listRidesFiltered(cursor, filter(), mobileRideMapLimits().historyPageLimit)
        }) { result ->
            if (generation != queryGeneration) return@withDatabase
            result.fold(
                onSuccess = { page ->
                    val existing = state.rides.mapTo(hashSetOf()) { it.id.bytes.toList() }
                    state = state.copy(
                        rides = state.rides + page.rides.filter { existing.add(it.id.bytes.toList()) },
                        nextCursor = page.nextCursor, loadingMore = false,
                    )
                },
                onFailure = { error -> state = state.copy(loadingMore = false, error = error.message ?: "") },
            )
            notifyChanged()
        }
    }

    fun select(ride: MobileRideRecordDto) {
        val generation = ++detailGeneration
        state = state.copy(selected = ride, route = null, error = null, loading = true)
        notifyChanged()
        runtime.withDatabase({ db ->
            val budget = mobileRideMapLimits().historyPreviewPointLimit
            db.projectRoutePoints(ride.id, MobileRideMapRouteProjectionOptionsDto(
                null, budget, MobileRideMapRoutePrivacyPolicyDto.Precise,
            ))
        }) { result ->
            if (generation != detailGeneration) return@withDatabase
            result.fold(
                onSuccess = { route -> state = state.copy(route = route, loading = false) },
                onFailure = { error -> state = state.copy(loading = false, error = error.message ?: "") },
            )
            notifyChanged()
        }
    }

    fun closeDetail() {
        detailGeneration++
        state = state.copy(selected = null, route = null, loading = false, error = null)
        notifyChanged()
    }

    fun exportRide(id: MobileRideIdDto, path: String, completed: (Result<String>) -> Unit) {
        runtime.withDatabase({ database ->
            java.io.File(path).parentFile?.mkdirs()
            database.exportRideJson(id, path)
            path
        }, completed)
    }

    fun setSearch(value: String) {
        if (searchText == value) return
        searchText = value
        refresh()
    }

    fun setVehicle(identity: String?) {
        if (vehicleIdentity == identity) return
        vehicleIdentity = identity
        refresh()
    }

    fun setIncludeShortRides(value: Boolean) {
        if (includeShortRides == value) return
        includeShortRides = value
        refresh()
    }

    fun setLast30Days(value: Boolean) {
        if (last30Days == value) return
        last30Days = value
        refresh()
    }

    private fun filter() = MobileRideHistoryFilterDto(
        if (last30Days) (now() - 30L * 24 * 60 * 60 * 1000).coerceAtLeast(0).toULong() else null,
        vehicleIdentity,
        searchText.trim().takeIf(String::isNotEmpty),
        includeShortRides,
    )

    private fun notifyChanged() { onChanged?.invoke() }
}
