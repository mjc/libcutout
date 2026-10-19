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
import uniffi.cutout_mobile_ffi.MobileMusicHistoryDto
import uniffi.cutout_mobile_ffi.MobileMusicHistoryStateDto
import uniffi.cutout_mobile_ffi.mobileRideMapLimits

data class RideHistoryState(
    val rides: List<MobileRideRecordDto> = emptyList(),
    val vehicles: List<MobileRideHistoryVehicleOptionDto> = emptyList(),
    val nextCursor: MobileRideCursorDto? = null,
    val selected: MobileRideRecordDto? = null,
    val listPreviewRide: MobileRideRecordDto? = null,
    val listPreviewRoute: MobileRideMapRouteProjectionDto? = null,
    val route: MobileRideMapRouteProjectionDto? = null,
    val routeRideId: MobileRideIdDto? = null,
    val musicHistory: MobileMusicHistoryDto? = null,
    val musicHistoryState: MobileMusicHistoryStateDto? = null,
    val musicError: String? = null,
    val loading: Boolean = false,
    val loadingMore: Boolean = false,
    val error: String? = null,
)

internal fun <T> restoreRideSelection(
    previousIdentity: ByteArray?,
    rows: List<T>,
    identity: (T) -> ByteArray,
): T? = previousIdentity?.let { selected ->
    rows.firstOrNull { identity(it).contentEquals(selected) }
}

internal fun historyRouteMatchesRide(projectedIdentity: ByteArray?, selectedIdentity: ByteArray): Boolean =
    projectedIdentity?.contentEquals(selectedIdentity) == true

internal fun shouldFitHistoryRoute(previousIdentity: ByteArray?, currentIdentity: ByteArray, explicitFit: Boolean): Boolean =
    explicitFit || previousIdentity?.contentEquals(currentIdentity) != true

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
    val hasActiveFilters: Boolean
        get() = searchText.isNotBlank() || !last30Days || vehicleIdentity != null || includeShortRides
    private var queryGeneration = 0L
    private var detailGeneration = 0L

    var onChanged: (() -> Unit)? = null

    fun refresh(preserveSelection: Boolean = false) {
        val generation = ++queryGeneration
        detailGeneration++
        val selected = state.selected.takeIf { preserveSelection }
        val preview = state.listPreviewRide.takeIf { preserveSelection }
        val retainedRide = selected ?: preview
        val retainedCount = if (preserveSelection) state.rides.size else 0
        state = state.copy(loading = true, loadingMore = false, error = null, selected = selected,
            listPreviewRide = preview, listPreviewRoute = if (preserveSelection) state.listPreviewRoute else null,
            route = if (preserveSelection) state.route else null,
            routeRideId = if (preserveSelection) state.routeRideId else null,
            musicHistory = if (preserveSelection) state.musicHistory else null,
            musicHistoryState = if (preserveSelection) state.musicHistoryState else null,
            musicError = if (preserveSelection) state.musicError else null)
        notifyChanged()
        val currentFilter = filter()
        runtime.withDatabase({ db ->
            val pageLimit = mobileRideMapLimits().historyPageLimit
            val rides = mutableListOf<MobileRideRecordDto>()
            var cursor: MobileRideCursorDto? = null
            var page = db.listRidesFiltered(cursor, currentFilter, pageLimit)
            rides.addAll(page.rides)
            while (page.nextCursor != null && (
                rides.size < retainedCount ||
                    (retainedRide != null && rides.none { it.id.bytes.contentEquals(retainedRide.id.bytes) } &&
                        rides.size < retainedCount + pageLimit.toInt())
            )) {
                cursor = page.nextCursor
                page = db.listRidesFiltered(cursor, currentFilter, pageLimit)
                rides.addAll(page.rides)
            }
            val vehicles = db.listRideHistoryVehicleOptions()
            Triple(rides, page.nextCursor, vehicles)
        }) { result ->
            if (generation != queryGeneration) return@withDatabase
            result.fold(
                onSuccess = { (rides, nextCursor, vehicles) ->
                    state = state.copy(rides = rides, nextCursor = nextCursor, vehicles = vehicles,
                        loading = false, error = null)
                    val retained = restoreRideSelection(retainedRide?.id?.bytes, rides) { it.id.bytes }
                    when {
                        selected != null && retained != null -> select(retained)
                        selected != null -> state = state.copy(selected = null)
                        preview != null -> state = state.copy(
                            listPreviewRide = retained,
                            listPreviewRoute = state.listPreviewRoute.takeIf { retained != null },
                        )
                    }
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
        val retainMatchingRoute = historyRouteMatchesRide(state.routeRideId?.bytes, ride.id.bytes)
        state = state.copy(selected = ride, listPreviewRide = null, listPreviewRoute = null,
            route = state.route.takeIf { retainMatchingRoute },
            routeRideId = state.routeRideId.takeIf { retainMatchingRoute },
            error = null, loading = true)
        notifyChanged()
        runtime.withDatabase({ db ->
            val budget = mobileRideMapLimits().historyPreviewPointLimit
            val route = db.projectRoutePoints(ride.id, MobileRideMapRouteProjectionOptionsDto(
                null, budget, MobileRideMapRoutePrivacyPolicyDto.Precise,
            ))
            val music = runCatching { db.musicHistory(ride.id) }
            val musicState = runCatching { db.musicHistoryState(ride.id) }
            Triple(route, music, musicState)
        }) { result ->
            if (generation != detailGeneration) return@withDatabase
            result.fold(
                onSuccess = { (route, music, musicState) ->
                    state = state.copy(route = route, routeRideId = ride.id, loading = false,
                        musicHistory = music.getOrNull(),
                        musicHistoryState = musicState.getOrNull(),
                        musicError = music.exceptionOrNull()?.message ?: musicState.exceptionOrNull()?.message)
                },
                onFailure = { error -> state = state.copy(loading = false, error = error.message ?: "") },
            )
            notifyChanged()
        }
    }

    fun forgetMusicHistory(ride: MobileRideRecordDto, completed: (Result<Unit>) -> Unit) {
        val generation = detailGeneration
        runtime.withDatabase({ db ->
            db.deleteMusicHistory(ride.id)
            val history = db.musicHistory(ride.id)
            val status = db.musicHistoryState(ride.id)
            history to status
        }) { result ->
            if (generation != detailGeneration) return@withDatabase
            result.fold(
                onSuccess = { (history, status) ->
                    state = state.copy(musicHistory = history, musicHistoryState = status, musicError = null)
                    notifyChanged()
                    completed(Result.success(Unit))
                },
                onFailure = { error ->
                    state = state.copy(musicError = error.message ?: "")
                    notifyChanged()
                    completed(Result.failure(error))
                },
            )
        }
    }

    fun closeDetail() {
        detailGeneration++
        state = state.copy(selected = null, listPreviewRide = state.selected,
            listPreviewRoute = state.route.takeIf {
                state.selected?.let { historyRouteMatchesRide(state.routeRideId?.bytes, it.id.bytes) } == true
            }, route = null, routeRideId = null, loading = false, error = null)
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

    fun clearFilters() {
        if (!hasActiveFilters) return
        searchText = ""
        vehicleIdentity = null
        includeShortRides = false
        last30Days = true
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
