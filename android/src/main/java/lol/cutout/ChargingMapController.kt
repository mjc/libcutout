package lol.cutout

import uniffi.cutout_mobile_ffi.*

data class ChargingMapState(
    val spots: List<MobileChargingSpotDto> = emptyList(),
    val selected: MobileChargingSpotDto? = null,
    val cursor: MobileMapPointCursorDto? = null,
    val loading: Boolean = false,
    val loadingMore: Boolean = false,
    val limitReached: Boolean = false,
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
    private val reconciliation = MobileChargingReconciliation()
    private var queryGeneration = 0L
    private var selectionGeneration = 0L
    private var bounds: MobileGeoBoundsDto? = null
    private var disposed = false

    fun currentBounds(): MobileGeoBoundsDto? = bounds

    fun close() {
        if (disposed) return
        disposed = true
        queryGeneration += 1
        selectionGeneration += 1
        reconciliation.close()
    }

    fun loadBlockedOperations() {
        if (disposed) return
        state = state.copy(loadingBlockedOperations = true, error = null)
        notifyChanged()
        client.withDatabase({ it.listBlockedChargingOperations() }) { result ->
            if (disposed) return@withDatabase
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
        if (disposed) {
            completed(Result.failure(IllegalStateException("Charging map is closed")))
            return
        }
        state = state.copy(busySpotId = conflict.operationId, error = null)
        notifyChanged()
        client.withDatabase({ database ->
            database.resolveChargingOperation(conflict.operationId, conflict.localRevision, resolution)
        }) { result ->
            if (disposed) {
                completed(result)
                return@withDatabase
            }
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
        if (disposed) {
            completed(Result.failure(IllegalStateException("Charging map is closed")))
            return
        }
        state = state.copy(busySpotId = conflict.operationId, error = null)
        notifyChanged()
        client.withDatabase({ database ->
            database.replaceBlockedChargingOperation(conflict.operationId, conflict.localRevision, input)
        }) { result ->
            if (disposed) {
                completed(result)
                return@withDatabase
            }
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            if (result.isSuccess) {
                loadBlockedOperations()
                refreshCurrentViewport()
            } else notifyChanged()
            completed(result)
        }
    }

    fun load(bounds: MobileGeoBoundsDto, append: Boolean = false) {
        if (disposed) return
        if (append && (state.cursor == null || state.loading || state.loadingMore || state.limitReached)) return
        val generation = queryGeneration + 1
        queryGeneration = generation
        this.bounds = bounds
        val requestTicket = reconciliation.beginObservation()
        state = state.copy(
            loading = !append,
            loadingMore = append,
            error = null,
        )
        notifyChanged()
        val cursor = if (append) state.cursor else null
        val requestFilter = filter
        client.withDatabase({ database ->
            database.chargingSpotsInBounds(bounds, requestFilter, cursor, PAGE_SIZE)
        }) { result ->
            if (disposed || generation != queryGeneration) return@withDatabase
            result.fold(
                onSuccess = { page -> applyPage(page, append, requestTicket) },
                onFailure = { error ->
                    state = state.copy(loading = false, loadingMore = false, error = error.message)
                },
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

    fun select(id: String?) {
        if (disposed) return
        val generation = ++selectionGeneration
        if (id == null) {
            state = state.copy(selected = null, error = null)
            notifyChanged()
            return
        }
        if (reconciliation.isWithdrawn(id)) {
            state = state.copy(
                spots = state.spots.filterNot { it.id == id },
                selected = null,
                error = null,
            )
            notifyChanged()
            return
        }
        val existing = state.spots.firstOrNull { it.id == id }
        state = state.copy(selected = existing, error = null)
        notifyChanged()
        val requestTicket = reconciliation.beginObservation()
        client.withDatabase({ it.chargingSpot(id) }) { result ->
            if (disposed || generation != selectionGeneration) return@withDatabase
            result.fold(
                onSuccess = { spot ->
                    if (spot == null) {
                        state = state.copy(selected = null, error = "Charging spot is unavailable.")
                    } else {
                        applyReadback(requestTicket, spot, establishSelection = true)
                    }
                },
                onFailure = { error -> state = state.copy(error = error.message) },
            )
            notifyChanged()
        }
    }

    fun reloadSpot(id: String, completed: (Result<MobileChargingSpotDto?>) -> Unit = {}) {
        if (disposed) {
            completed(Result.failure(IllegalStateException("Charging map is closed")))
            return
        }
        client.withDatabase({ it.chargingSpot(id) }) { result ->
            if (disposed) {
                completed(Result.failure(IllegalStateException("Charging map is closed")))
                return@withDatabase
            }
            completed(result)
        }
    }

    fun vote(spot: MobileChargingSpotDto, selected: MobileChargingVoteDto, completed: (Result<MobileChargingSpotDto?>) -> Unit) {
        if (disposed) {
            completed(Result.failure(IllegalStateException("Charging map is closed")))
            return
        }
        if (reconciliation.isWithdrawn(spot.id)) {
            completed(Result.success(null))
            return
        }
        state = state.copy(busySpotId = spot.id, error = null)
        notifyChanged()
        val requestTicket = reconciliation.beginObservation()
        client.withDatabase({ database ->
            val vote = mobileChargingVoteSelection(spot.vote, selected)
            database.voteChargingSpot(spot.id, spot.localRevision, vote)
            database.chargingSpot(spot.id)
        }) { result ->
            if (disposed) {
                completed(result)
                return@withDatabase
            }
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            result.getOrNull()?.let { applyReadback(requestTicket, it) }
            notifyChanged()
            completed(result)
        }
    }

    fun save(spot: MobileChargingSpotDto?, input: MobileChargingInputDto, completed: (Result<MobileChargingSpotDto?>) -> Unit) {
        if (disposed) {
            completed(Result.failure(IllegalStateException("Charging map is closed")))
            return
        }
        state = state.copy(busySpotId = spot?.id ?: NEW_SPOT, error = null)
        notifyChanged()
        val requestTicket = reconciliation.beginObservation()
        client.withDatabase({ database ->
            val receipt = if (spot == null) database.saveChargingSpot(input)
            else database.editChargingSpot(spot.id, spot.localRevision, input)
            database.chargingSpot(receipt.id)
        }) { result ->
            if (disposed) {
                completed(result)
                return@withDatabase
            }
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            result.getOrNull()?.let { applyReadback(requestTicket, it) }
            refreshCurrentViewport()
            notifyChanged()
            completed(result)
        }
    }

    fun withdraw(spot: MobileChargingSpotDto, completed: (Result<Unit>) -> Unit) {
        if (disposed) {
            completed(Result.failure(IllegalStateException("Charging map is closed")))
            return
        }
        if (reconciliation.isWithdrawn(spot.id)) {
            state = state.copy(
                spots = state.spots.filterNot { it.id == spot.id },
                selected = state.selected?.takeUnless { it.id == spot.id },
            )
            notifyChanged()
            completed(Result.success(Unit))
            return
        }
        state = state.copy(busySpotId = spot.id, error = null)
        notifyChanged()
        client.withDatabase({ database -> database.withdrawChargingSpot(spot.id, spot.localRevision); Unit }) { result ->
            if (disposed) {
                completed(result)
                return@withDatabase
            }
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            if (result.isSuccess) {
                reconciliation.recordWithdrawal(spot.id, spot.localRevision)
                selectionGeneration += 1
                state = state.copy(
                    spots = state.spots.filterNot { it.id == spot.id },
                    selected = state.selected?.takeUnless { it.id == spot.id },
                )
                refreshCurrentViewport()
            }
            notifyChanged()
            completed(result)
        }
    }

    fun verifyAccess(spot: MobileChargingSpotDto, completed: (Result<MobileChargingSpotDto?>) -> Unit) {
        if (disposed) {
            completed(Result.failure(IllegalStateException("Charging map is closed")))
            return
        }
        state = state.copy(busySpotId = spot.id, error = null)
        notifyChanged()
        val requestTicket = reconciliation.beginObservation()
        client.withDatabase({ database ->
            database.verifyChargingSpot(spot.id, spot.localRevision)
            database.chargingSpot(spot.id)
        }) { result ->
            if (disposed) {
                completed(result)
                return@withDatabase
            }
            state = state.copy(busySpotId = null, error = result.exceptionOrNull()?.message)
            result.getOrNull()?.let { applyReadback(requestTicket, it) }
            notifyChanged()
            completed(result)
        }
    }

    private fun applyPage(page: MobileChargingPageDto, append: Boolean, requestTicket: ULong) {
        val merged = reconciliation.reconcilePage(
            requestTicket,
            append,
            state.spots,
            state.selected?.id,
            page.spots,
        ).take(RESULT_LIMIT)
        val limitReached = page.nextCursor != null && merged.size >= RESULT_LIMIT
        val selectedId = state.selected?.id
        val selected = when {
            selectedId == null -> null
            reconciliation.isWithdrawn(selectedId) -> null
            else -> merged.firstOrNull { it.id == selectedId } ?: state.selected
        }
        state = state.copy(
            spots = merged,
            selected = selected,
            cursor = page.nextCursor,
            loading = false,
            loadingMore = false,
            limitReached = limitReached,
            error = null,
        )
    }

    private fun applyReadback(
        requestTicket: ULong,
        spot: MobileChargingSpotDto,
        establishSelection: Boolean = false,
    ) {
        val accepted = reconciliation.reconcileReadback(requestTicket, spot)
        if (accepted == null) {
            if (reconciliation.isWithdrawn(spot.id)) {
                state = state.copy(
                    spots = state.spots.filterNot { it.id == spot.id },
                    selected = state.selected?.takeUnless { it.id == spot.id },
                )
            }
            return
        }
        val visible = when (accepted.status) {
            MobileChargingStatusDto.QUEUED, MobileChargingStatusDto.PUBLISHED -> true
            MobileChargingStatusDto.HIDDEN, MobileChargingStatusDto.WITHDRAWN, MobileChargingStatusDto.REMOVED -> false
        }
        val spots = if (visible) {
            state.spots.map { if (it.id == accepted.id) accepted else it }
        } else {
            state.spots.filterNot { it.id == accepted.id }
        }
        val selected = when {
            reconciliation.isWithdrawn(accepted.id) -> state.selected?.takeUnless { it.id == accepted.id }
            accepted.status == MobileChargingStatusDto.WITHDRAWN || accepted.status == MobileChargingStatusDto.REMOVED ->
                state.selected?.takeUnless { it.id == accepted.id }
            state.selected?.id == accepted.id -> accepted
            establishSelection && visible -> accepted
            else -> state.selected
        }
        state = state.copy(spots = spots, selected = selected)
    }

    private fun notifyChanged() { onChanged?.invoke() }

    companion object {
        const val PAGE_SIZE = 100u
        const val RESULT_LIMIT = 500
        const val NEW_SPOT = "__new_charging_spot__"
    }
}
