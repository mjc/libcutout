package lol.cutout

import uniffi.cutout_mobile_ffi.*

data class ChargingMapState(
    val spots: List<MobileChargingSpotDto> = emptyList(),
    val selected: MobileChargingSpotDto? = null,
    val cursor: MobileMapPointCursorDto? = null,
    val loading: Boolean = false,
    val loadingMore: Boolean = false,
    val error: String? = null,
    val busySpotId: String? = null,
    val blockedOperations: List<MobileChargingSyncConflictDto> = emptyList(),
    val loadingBlockedOperations: Boolean = false,
)

/** Bounded charging-directory reads and writes through the app's shared Rust database owner. */
class ChargingMapController(private val client: AndroidChargingClient) {
    var state = ChargingMapState()
        private set
    var filter = MobileChargingFilterDto(null, null, null, null)
        private set
    var onChanged: (() -> Unit)? = null
    private var queryGeneration = 0L
    private var selectionGeneration = 0L
    private var bounds: MobileGeoBoundsDto? = null

    fun currentBounds(): MobileGeoBoundsDto? = bounds

    fun loadBlockedOperations() {
        state = state.copy(loadingBlockedOperations = true, error = null)
        notifyChanged()
        client.withDatabase({ it.listBlockedChargingOperations() }) { result ->
            result.fold(
                onSuccess = { state = state.copy(blockedOperations = it, loadingBlockedOperations = false) },
                onFailure = { state = state.copy(loadingBlockedOperations = false, error = it.message) },
            )
            notifyChanged()
        }
    }

    fun resolveBlocked(
        conflict: MobileChargingSyncConflictDto,
        resolution: MobileChargingResolutionDto,
        completed: (Result<Unit>) -> Unit,
    ) {
        state = state.copy(busySpotId = conflict.operationId, error = null)
        notifyChanged()
        client.withDatabase({ database ->
            database.resolveChargingOperation(conflict.operationId, conflict.localRevision, resolution)
        }) { result ->
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            if (result.isSuccess) loadBlockedOperations() else notifyChanged()
            completed(result)
        }
    }

    fun replaceBlocked(
        conflict: MobileChargingSyncConflictDto,
        input: MobileChargingInputDto,
        completed: (Result<MobileChargingWriteReceiptDto>) -> Unit,
    ) {
        state = state.copy(busySpotId = conflict.operationId, error = null)
        notifyChanged()
        client.withDatabase({ database ->
            database.replaceBlockedChargingOperation(conflict.operationId, conflict.localRevision, input)
        }) { result ->
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            if (result.isSuccess) {
                loadBlockedOperations()
                refreshCurrentViewport()
            } else notifyChanged()
            completed(result)
        }
    }

    fun load(bounds: MobileGeoBoundsDto, append: Boolean = false) {
        if (append && (state.cursor == null || state.loading || state.loadingMore)) return
        val generation = queryGeneration + 1
        queryGeneration = generation
        this.bounds = bounds
        state = state.copy(
            loading = !append,
            loadingMore = append,
            error = null,
        )
        notifyChanged()
        val cursor = if (append) state.cursor else null
        client.withDatabase({ database ->
            database.chargingSpotsInBounds(bounds, filter, cursor, PAGE_SIZE)
        }) { result ->
            if (generation != queryGeneration) return@withDatabase
            result.fold(
                onSuccess = { page ->
                    val combined = if (append) state.spots + page.spots.filter { incoming ->
                        state.spots.none { it.id == incoming.id }
                    } else page.spots
                    state = state.copy(spots = combined, cursor = page.nextCursor, loading = false, loadingMore = false)
                },
                onFailure = { error -> state = state.copy(loading = false, loadingMore = false, error = error.message) },
            )
            notifyChanged()
        }
    }

    fun refreshCurrentViewport() { bounds?.let { load(it) } }

    fun setFilter(value: MobileChargingFilterDto) {
        if (filter == value) return
        filter = value
        bounds?.let { load(it) }
    }

    fun restoreFilter(value: MobileChargingFilterDto) {
        filter = value
    }

    fun select(id: String?) {
        val generation = ++selectionGeneration
        if (id == null) {
            state = state.copy(selected = null, error = null)
            notifyChanged()
            return
        }
        val existing = state.spots.firstOrNull { it.id == id }
        state = state.copy(selected = existing, error = null)
        notifyChanged()
        client.withDatabase({ it.chargingSpot(id) }) { result ->
            if (generation != selectionGeneration) return@withDatabase
            result.fold(
                onSuccess = { spot ->
                    state = state.copy(selected = spot, error = if (spot == null) "Charging spot is unavailable." else null)
                    spot?.let { replaceSpot(it) }
                },
                onFailure = { error -> state = state.copy(error = error.message) },
            )
            notifyChanged()
        }
    }

    fun reloadSpot(id: String, completed: (Result<MobileChargingSpotDto?>) -> Unit = {}) {
        client.withDatabase({ it.chargingSpot(id) }, completed)
    }

    fun vote(spot: MobileChargingSpotDto, selected: MobileChargingVoteDto, completed: (Result<MobileChargingSpotDto?>) -> Unit) {
        state = state.copy(busySpotId = spot.id, error = null)
        notifyChanged()
        client.withDatabase({ database ->
            val vote = mobileChargingVoteSelection(spot.vote, selected)
            database.voteChargingSpot(spot.id, spot.localRevision, vote)
            database.chargingSpot(spot.id)
        }) { result ->
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            result.getOrNull()?.let(::replaceSpot)
            notifyChanged()
            completed(result)
        }
    }

    fun save(spot: MobileChargingSpotDto?, input: MobileChargingInputDto, completed: (Result<MobileChargingSpotDto?>) -> Unit) {
        state = state.copy(busySpotId = spot?.id ?: NEW_SPOT, error = null)
        notifyChanged()
        client.withDatabase({ database ->
            val receipt = if (spot == null) database.saveChargingSpot(input)
            else database.editChargingSpot(spot.id, spot.localRevision, input)
            database.chargingSpot(receipt.id)
        }) { result ->
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            result.getOrNull()?.let(::replaceSpot)
            refreshCurrentViewport()
            notifyChanged()
            completed(result)
        }
    }

    fun withdraw(spot: MobileChargingSpotDto, completed: (Result<Unit>) -> Unit) {
        state = state.copy(busySpotId = spot.id, error = null)
        notifyChanged()
        client.withDatabase({ database -> database.withdrawChargingSpot(spot.id, spot.localRevision); Unit }) { result ->
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            if (result.isSuccess) select(null)
            refreshCurrentViewport()
            notifyChanged()
            completed(result)
        }
    }

    fun verifyAccess(spot: MobileChargingSpotDto, completed: (Result<MobileChargingSpotDto?>) -> Unit) {
        state = state.copy(busySpotId = spot.id, error = null)
        notifyChanged()
        client.withDatabase({ database ->
            database.verifyChargingSpot(spot.id, spot.localRevision)
            database.chargingSpot(spot.id)
        }) { result ->
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            result.getOrNull()?.let(::replaceSpot)
            notifyChanged()
            completed(result)
        }
    }

    private fun replaceSpot(spot: MobileChargingSpotDto) {
        state = state.copy(
            spots = state.spots.map { if (it.id == spot.id) spot else it },
            selected = if (state.selected?.id == spot.id) spot else state.selected,
        )
    }

    private fun notifyChanged() { onChanged?.invoke() }

    companion object {
        const val PAGE_SIZE = 100u
        const val NEW_SPOT = "__new_charging_spot__"
    }
}
