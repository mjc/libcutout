import CutoutMobile
import CutoutMobileFFI
import Foundation
import MapKit
import Observation

typealias ChargingMapPageQuery =
    @Sendable (
        MobileGeoBoundsDto, MobileChargingFilterDto, MobileMapPointCursorDto?, UInt32
    ) async throws -> MobileChargingPageDto
typealias ChargingMapSpotQuery = @Sendable (String) async throws -> MobileChargingSpotDto?
typealias ChargingMapNearbyQuery =
    @Sendable (
        MobileGeoBoundsDto, MobileMapCoordinateDto, MobileChargingFilterDto, UInt32
    ) async throws -> [MobileChargingSpotDto]
typealias ChargingMapVoteWrite =
    @Sendable (
        String, UInt64, MobileChargingVoteDto?
    ) async throws -> Void

typealias ChargingMapWithdrawal = @Sendable (String, UInt64) async throws -> Void
typealias ChargingBlockedOperationsQuery = @Sendable () async throws -> [MobileChargingSyncConflictDto]
typealias ChargingOperationResolution = @Sendable (
    String, UInt64, MobileChargingResolutionDto
) async throws -> Void

func withChargingDatabase<Result: Sendable>(
    open: @Sendable () async throws -> RideDatabaseHandle = { try await RustPersistenceStore.open() },
    _ operation: @Sendable (RideDatabaseHandle) async throws -> Result
) async throws -> Result {
    do {
        let database = try await open()
        return try await operation(database)
    } catch let error as MobileChargingError {
        throw error
    } catch let error as CancellationError {
        throw error
    } catch {
        throw MobileChargingError.Unavailable
    }
}

@MainActor
@Observable
final class ChargingMapModel {
    private static let pageSize: UInt32 = 100
    private static let resultLimit = 500

    private let queryPage: ChargingMapPageQuery
    private let readSpot: ChargingMapSpotQuery
    private let nearbySpots: ChargingMapNearbyQuery
    private let writeVote: ChargingMapVoteWrite
    private let writeWithdrawal: ChargingMapWithdrawal
    private let queryBlockedOperations: ChargingBlockedOperationsQuery
    private let resolveOperation: ChargingOperationResolution
    private let communitySync: ChargingCommunitySync?
    private let reconciliation = MobileChargingReconciliation()
    private let discovery: MobileChargingDiscovery
    private var bounds: MobileGeoBoundsDto?
    private var cursor: MobileMapPointCursorDto?
    private var generation: UInt64 = 0
    private var selectionSequence: UInt64 = 0
    private var loadingTask: Task<Void, Never>?
    private var discoveryTask: Task<Void, Never>?
    private var communitySyncTask: Task<Void, Never>?
    private var communitySyncGeneration: UInt64 = 0
    private var communitySyncNeedsAnotherRun = false
    private var communitySyncRetryRequested = false
    private var discoveryGeneration: UInt64 = 0

    private(set) var spots = [MobileChargingSpotDto]()
    private(set) var selectedSpot: MobileChargingSpotDto?
    private(set) var filter: MobileChargingFilterDto
    private(set) var queryID = 0
    private(set) var isLoading = false
    private(set) var hasMore = false
    private(set) var limitReached = false
    private(set) var error: MobileChargingError?
    private(set) var pendingVoteIDs = Set<String>()
    private(set) var pendingWithdrawalIDs = Set<String>()
    private(set) var voteError: MobileChargingError?
    private(set) var userLocation: MobileMapCoordinateDto?
    private(set) var discoveryTarget: MobileMapCoordinateDto?
    private(set) var blockedChargingOperations = [MobileChargingSyncConflictDto]()
    private(set) var blockedOperationsError: MobileChargingError?

    var communitySyncStatus: ChargingCommunitySyncStatus {
        let status = communitySync?.status ?? .notStarted
        switch status {
        case .syncing, .offline, .authenticationRequired, .throttled, .storageUnavailable,
            .registrationUncertain, .deletionPending, .deleted:
            return status
        case .notStarted, .synced, .pending, .conflict, .rejected:
            guard blockedOperationsError == nil, !blockedChargingOperations.isEmpty else {
                return status
            }
            return blockedChargingOperations.contains(where: { $0.reason == .conflict })
                ? .conflict
                : .rejected
        }
    }
    var communitySyncPendingCount: UInt32 { communitySync?.pendingCount ?? 0 }
    var communitySyncDownloaded: UInt32 { communitySync?.downloaded ?? 0 }
    var communitySyncUploaded: UInt32 { communitySync?.uploaded ?? 0 }
    var communitySyncLastUpdated: Date? { communitySync?.lastUpdated }

    init(
        filter: MobileChargingFilterDto = MobileChargingFilterDto(
            cost: nil, connector: nil, voltageVolts: nil, venue: nil
        ),
        queryPage: @escaping ChargingMapPageQuery,
        readSpot: @escaping ChargingMapSpotQuery,
        nearbySpots: @escaping ChargingMapNearbyQuery = { _, _, _, _ in [] },
        writeVote: @escaping ChargingMapVoteWrite,
        writeWithdrawal: @escaping ChargingMapWithdrawal = { _, _ in throw MobileChargingError.Unavailable },
        queryBlockedOperations: @escaping ChargingBlockedOperationsQuery = { [] },
        resolveOperation: @escaping ChargingOperationResolution = { _, _, _ in
            throw MobileChargingError.Unavailable
        },
        communitySync: ChargingCommunitySync? = nil
    ) {
        self.filter = filter
        self.discovery = MobileChargingDiscovery(filter: filter)
        self.queryPage = queryPage
        self.readSpot = readSpot
        self.nearbySpots = nearbySpots
        self.writeVote = writeVote
        self.writeWithdrawal = writeWithdrawal
        self.queryBlockedOperations = queryBlockedOperations
        self.resolveOperation = resolveOperation
        self.communitySync = communitySync
    }

    convenience init(
        database: RideDatabaseHandle? = nil,
        filter: MobileChargingFilterDto = MobileChargingFilterDto(
            cost: nil, connector: nil, voltageVolts: nil, venue: nil)
    ) {
        let openDatabase: @Sendable () async throws -> RideDatabaseHandle = {
            if let database { return database }
            return try await RustPersistenceStore.open()
        }
        self.init(
            filter: filter,
            queryPage: { bounds, filter, cursor, limit in
                try await withChargingDatabase(open: openDatabase) { database in
                    try await database.chargingSpotsInBounds(
                        bounds: bounds, selection: filter, cursor: cursor, limit: limit
                    )
                }
            },
            readSpot: { id in
                try await withChargingDatabase(open: openDatabase) { try await $0.chargingSpot(id: id) }
            },
            nearbySpots: { bounds, origin, filter, limit in
                try await withChargingDatabase(open: openDatabase) { database in
                    try await database.nearbyChargingSpots(
                        bounds: bounds, origin: origin, selection: filter, limit: limit
                    )
                }
            },
            writeVote: { id, revision, vote in
                try await withChargingDatabase(open: openDatabase) { database in
                    _ = try await database.voteChargingSpot(id: id, expectedRevision: revision, vote: vote)
                }
            },
            writeWithdrawal: { id, revision in
                try await withChargingDatabase(open: openDatabase) { database in
                    _ = try await database.withdrawChargingSpot(id: id, expectedRevision: revision)
                }
            },
            queryBlockedOperations: {
                try await withChargingDatabase(open: openDatabase) { try await $0.listBlockedChargingOperations() }
            },
            resolveOperation: { operationID, revision, resolution in
                try await withChargingDatabase(open: openDatabase) { database in
                    try await database.resolveChargingOperation(
                        operationId: operationID, expectedLocalRevision: revision,
                        resolution: resolution
                    )
                }
            },
            communitySync: database == nil ? .shared : nil
        )
    }

    func communitySyncRequested() {
        requestCommunitySync(explicitRetry: false)
    }

    func communityAccountDeletionCompleted() {
        communitySyncGeneration &+= 1
        communitySync?.cancel()
        communitySyncTask?.cancel()
        communitySyncTask = nil
        communitySyncNeedsAnotherRun = false
        communitySyncRetryRequested = false
        Task { [weak self] in
            guard let self else { return }
            await refreshLocalCacheAfterSync()
            await loadBlockedOperations()
        }
    }

    func retryCommunitySync() {
        requestCommunitySync(explicitRetry: true)
    }

    func applicationDidBecomeActive() {
        requestCommunitySync(explicitRetry: false)
    }

    func applicationDidEnterBackground() {
        communitySyncGeneration &+= 1
        communitySync?.cancel()
        communitySyncTask?.cancel()
        communitySyncTask = nil
    }

    func loadBlockedOperations() async {
        do {
            blockedChargingOperations = try await queryBlockedOperations()
            blockedOperationsError = nil
        } catch let error as MobileChargingError {
            blockedOperationsError = error
        } catch {
            blockedOperationsError = .StorageFailure
        }
    }

    func resolveChargingOperation(
        _ operation: MobileChargingSyncConflictDto,
        resolution: MobileChargingResolutionDto
    ) async throws {
        try await resolveOperation(operation.operationId, operation.localRevision, resolution)
        await loadBlockedOperations()
        await refreshAfterResolution(id: operation.id)
        communitySyncRequested()
    }

    private func refreshAfterResolution(id: String) async {
        if bounds != nil {
            await fetchPage(append: false)
        } else {
            advanceQuery()
        }
        let requestGeneration = generation
        let requestTicket = reconciliation.beginObservation()

        let requestSelection = selectionSequence
        do {
            let updated = try await readSpot(id)
            guard generation == requestGeneration else { return }
            guard let updated else {
                spots.removeAll { $0.id == id }
                if selectedSpot?.id == id { deselect() }
                return
            }
            guard
                let updated = reconciliation.reconcileResolution(
                    requestTicket: requestTicket, spot: updated
                )
            else { return }
            updatePageMembership(with: updated)
            if selectionSequence == requestSelection, selectedSpot?.id == id {
                selectedSpot = updated
            }
        } catch {
            // Keep the durable decision even when its local presentation read fails.
        }
    }

    private func requestCommunitySync(explicitRetry: Bool) {
        guard let communitySync else { return }
        if communitySyncTask != nil {
            communitySyncNeedsAnotherRun = true
            communitySyncRetryRequested = communitySyncRetryRequested || explicitRetry
            return
        }
        communitySyncGeneration &+= 1
        let requestGeneration = communitySyncGeneration
        communitySyncTask = Task { [weak self] in
            guard let self else { return }
            var retry = explicitRetry || communitySyncRetryRequested
            communitySyncRetryRequested = false
            repeat {
                let result = await communitySync.run(explicitRetry: retry)
                guard communitySyncGeneration == requestGeneration, !Task.isCancelled else { return }
                if result == .synced || result == .pending || result == .conflict
                    || result == .rejected || result == .deletionPending || result == .deleted
                    || communitySyncDownloaded > 0
                {
                    await refreshLocalCacheAfterSync()
                    guard communitySyncGeneration == requestGeneration, !Task.isCancelled else {
                        return
                    }
                }
                await loadBlockedOperations()
                guard communitySyncGeneration == requestGeneration, !Task.isCancelled else {
                    return
                }
                retry = communitySyncRetryRequested
                communitySyncRetryRequested = false
                let shouldRunAgain = communitySyncNeedsAnotherRun
                communitySyncNeedsAnotherRun = false
                if !shouldRunAgain { break }
            } while true
            if communitySyncGeneration == requestGeneration {
                communitySyncTask = nil
            }
        }
    }

    private func refreshLocalCacheAfterSync() async {
        if bounds != nil { await fetchPage(append: false) }
        await refreshSelectedSpotAfterSync()
        rearmEmptyInitialDiscoveryAfterCacheUpdate()
    }

    private func refreshSelectedSpotAfterSync() async {
        guard let selected = selectedSpot else { return }
        let id = selected.id
        let requestGeneration = generation
        let requestTicket = reconciliation.beginObservation()
        let requestSelection = selectionSequence
        do {
            let updated = try await readSpot(id)
            guard generation == requestGeneration,
                selectionSequence == requestSelection,
                selectedSpot?.id == id,
                !Task.isCancelled
            else { return }
            guard let updated,
                let updated = reconciliation.reconcileReadback(
                    requestTicket: requestTicket, spot: updated
                )
            else {
                deselect()
                return
            }
            selectedSpot = updated
            updatePageMembership(with: updated)
        } catch {
            // Keep the current detail visible when the local read cannot complete.
        }
    }

    private func rearmEmptyInitialDiscoveryAfterCacheUpdate() {
        guard
            let request = discovery.communityCacheUpdated(
                downloadedCount: communitySyncDownloaded, hasLocalResults: !spots.isEmpty
            )
        else { return }
        beginInitialDiscovery(request)
    }

    func setViewport(_ bounds: MobileGeoBoundsDto) {
        guard self.bounds != bounds else { return }
        self.bounds = bounds
        advanceQuery()
    }

    func setFilter(_ filter: MobileChargingFilterDto) {
        guard self.filter != filter else { return }
        self.filter = filter
        discovery.setFilter(filter: filter)
        discoveryTarget = nil
        discoveryGeneration &+= 1
        discoveryTask?.cancel()
        discoveryTask = nil
        spots = []
        selectedSpot = nil
        cursor = nil
        hasMore = false
        limitReached = false
        advanceQuery()
    }

    func setUserLocation(_ coordinate: MobileMapCoordinateDto) {
        userLocation = coordinate
        if let request = discovery.locationAvailable(location: coordinate) {
            beginInitialDiscovery(request)
        }
    }

    func cancelAutomaticDiscovery() {
        discovery.userOverride()
        discoveryTarget = nil
        discoveryGeneration &+= 1
        discoveryTask?.cancel()
        discoveryTask = nil
    }

    func load() async {
        await fetchPage(append: false)
    }

    func loadMore() async {
        guard hasMore, !isLoading, !limitReached else { return }
        await fetchPage(append: true)
    }

    /// Refreshes a spot after its durable save receipt has already been received.
    /// This method performs reads only; it never retries the save operation.
    func refreshSavedSpot(id: String) async {
        if bounds != nil {
            while true {
                let viewportQueryID = queryID
                await load()
                guard !Task.isCancelled else { return }
                guard error == nil else { return }
                if queryID == viewportQueryID { break }
            }
        }

        let requestGeneration = generation
        let requestTicket = reconciliation.beginObservation()
        do {
            guard let updated = try await readSpot(id) else {
                guard generation == requestGeneration else { return }
                error = .NotFound
                return
            }
            guard generation == requestGeneration else { return }
            guard !Task.isCancelled else { return }

            guard
                let updated = reconciliation.reconcileReadback(
                    requestTicket: requestTicket, spot: updated
                )
            else { return }
            updatePageMembership(with: updated)
            selectedSpot = updated
            error = nil
        } catch let error as MobileChargingError {
            guard generation == requestGeneration,
                !Task.isCancelled
            else { return }
            self.error = error
        } catch is CancellationError {
            return
        } catch {
            guard generation == requestGeneration,
                !Task.isCancelled
            else { return }
            self.error = .StorageFailure
        }
    }

    func consumeDiscoveryTarget() -> MobileMapCoordinateDto? {
        let target = discoveryTarget
        discoveryTarget = nil
        return target
    }

    func withdraw(_ spot: MobileChargingSpotDto) async -> Bool {
        guard spot.owned, !pendingWithdrawalIDs.contains(spot.id), !pendingVoteIDs.contains(spot.id),
            !reconciliation.isWithdrawn(id: spot.id)
        else { return false }
        pendingWithdrawalIDs.insert(spot.id)
        error = nil
        defer { pendingWithdrawalIDs.remove(spot.id) }
        do {
            try await writeWithdrawal(spot.id, spot.localRevision)
            reconciliation.recordWithdrawal(id: spot.id, revision: spot.localRevision)
            spots.removeAll { $0.id == spot.id }
            if selectedSpot?.id == spot.id { deselect() }
            communitySyncRequested()
            return true
        } catch let error as MobileChargingError {
            self.error = error
        } catch {
            self.error = .StorageFailure
        }
        return false
    }

    func select(_ spot: MobileChargingSpotDto) {
        selectionSequence &+= 1
        selectedSpot = spot
        voteError = nil
    }

    func select(id: String?) {
        guard let id else {
            deselect()
            return
        }
        if let spot = spots.first(where: { $0.id == id }) { select(spot) }
    }

    func deselect() {
        selectionSequence &+= 1
        selectedSpot = nil
        voteError = nil
    }

    func vote(_ selection: MobileChargingVoteDto, forSpotID: String? = nil) async {
        guard let selected = selectedSpot, !pendingVoteIDs.contains(selected.id),
            !pendingWithdrawalIDs.contains(selected.id),
            !reconciliation.isWithdrawn(id: selected.id)
        else { return }
        guard forSpotID == nil || forSpotID == selected.id else { return }
        let id = selected.id
        let chosen = mobileChargingVoteSelection(current: selected.vote, selected: selection)
        pendingVoteIDs.insert(id)
        voteError = nil
        let task = Task { [readSpot, writeVote] in
            do {
                try await writeVote(id, selected.localRevision, chosen)
                self.communitySyncRequested()
                let requestTicket = self.reconciliation.beginObservation()
                let updated = try await readSpot(id)
                self.finishVote(
                    id: id, updated: updated, error: nil, requestTicket: requestTicket
                )
            } catch let error as MobileChargingError {
                self.finishVote(id: id, updated: nil, error: error, requestTicket: nil)
            } catch is CancellationError {
                self.finishVote(id: id, updated: nil, error: nil, requestTicket: nil)
            } catch {
                self.finishVote(
                    id: id, updated: nil, error: .StorageFailure, requestTicket: nil
                )
            }
        }
        await task.value
    }

    private func advanceQuery() {
        queryID &+= 1
        generation &+= 1
        loadingTask?.cancel()
        loadingTask = nil
        cursor = nil
        hasMore = false
        limitReached = false
        error = nil
        isLoading = false
    }

    private func fetchPage(append: Bool) async {
        guard let bounds else { return }
        if !append {
            generation &+= 1
            loadingTask?.cancel()
            cursor = nil
        }
        let requestGeneration = generation
        let requestCursor = append ? cursor : nil
        let requestFilter = filter
        let requestBounds = bounds
        let requestTicket = reconciliation.beginObservation()
        let task = Task { [queryPage] in
            do {
                let page = try await queryPage(requestBounds, requestFilter, requestCursor, Self.pageSize)
                self.apply(
                    page, append: append, generation: requestGeneration,
                    requestTicket: requestTicket
                )
            } catch let error as MobileChargingError {
                self.fail(error, append: append, generation: requestGeneration)
            } catch is CancellationError {
                self.cancelled(generation: requestGeneration)
            } catch {
                self.fail(.StorageFailure, append: append, generation: requestGeneration)
            }
        }
        loadingTask = task
        isLoading = true
        await withTaskCancellationHandler {
            await task.value
        } onCancel: {
            task.cancel()
        }
    }

    private func apply(
        _ page: MobileChargingPageDto,
        append: Bool,
        generation: UInt64,
        requestTicket: UInt64
    ) {
        guard self.generation == generation else { return }
        if Task.isCancelled {
            cancelled(generation: generation)
            return
        }
        let merged = Array(
            reconciliation.reconcilePage(
                requestTicket: requestTicket, append: append, currentPage: spots,
                selectedSpotId: selectedSpot?.id,
                incoming: page.spots
            ).prefix(Self.resultLimit))
        spots = merged
        cursor = page.nextCursor
        hasMore = page.nextCursor != nil && spots.count < Self.resultLimit
        limitReached = page.nextCursor != nil && spots.count >= Self.resultLimit
        if let id = selectedSpot?.id {
            selectedSpot = spots.first(where: { $0.id == id }) ?? selectedSpot
        }
        error = nil
        isLoading = false
        loadingTask = nil
        if !append, let request = discovery.viewportLoaded(hasLocalResults: !merged.isEmpty) {
            beginInitialDiscovery(request)
        }
    }

    private func beginInitialDiscovery(_ request: MobileChargingDiscoveryRequestDto) {
        discoveryGeneration &+= 1
        let requestGeneration = discoveryGeneration
        let task = Task { [nearbySpots] in
            do {
                let matches = try await nearbySpots(
                    request.bounds, request.origin, request.filter, request.limit
                )
                self.finishInitialDiscovery(matches.first, generation: requestGeneration)
            } catch let error as MobileChargingError {
                self.failInitialDiscovery(error, generation: requestGeneration)
            } catch is CancellationError {
                self.cancelInitialDiscovery(generation: requestGeneration)
            } catch {
                self.failInitialDiscovery(.StorageFailure, generation: requestGeneration)
            }
        }
        discoveryTask = task
    }

    private func finishInitialDiscovery(_ nearest: MobileChargingSpotDto?, generation: UInt64) {
        guard discoveryGeneration == generation else { return }
        if Task.isCancelled {
            cancelInitialDiscovery(generation: generation)
            return
        }
        let coordinate = nearest?.input?.coordinate
        let target = discovery.searchCompleted(nearest: coordinate)
        discoveryTask = nil
        discoveryTarget = target
    }

    private func failInitialDiscovery(_ error: MobileChargingError, generation: UInt64) {
        guard discoveryGeneration == generation else { return }
        if Task.isCancelled {
            cancelInitialDiscovery(generation: generation)
            return
        }
        discovery.searchFailed()
        discoveryTask = nil
        self.error = error
    }

    private func cancelInitialDiscovery(generation: UInt64) {
        guard discoveryGeneration == generation else { return }
        discovery.searchCancelled()
        discoveryTask = nil
        discoveryTarget = nil
    }

    private func fail(_ error: MobileChargingError, append: Bool, generation: UInt64) {
        guard self.generation == generation else { return }
        if Task.isCancelled {
            cancelled(generation: generation)
            return
        }
        self.error = error
        isLoading = false
        loadingTask = nil
        if !append { hasMore = false }
    }

    private func cancelled(generation: UInt64) {
        guard self.generation == generation else { return }
        isLoading = false
        loadingTask = nil
    }

    private func finishVote(
        id: String, updated: MobileChargingSpotDto?, error: MobileChargingError?,
        requestTicket: UInt64?
    ) {
        pendingVoteIDs.remove(id)
        if let error, selectedSpot?.id == id { voteError = error }
        guard let updated, let requestTicket, updated.id == id,
            let updated = reconciliation.reconcileReadback(
                requestTicket: requestTicket, spot: updated
            )
        else { return }
        updatePageMembership(with: updated)
        if selectedSpot?.id == id { selectedSpot = updated }
    }

    private func updatePageMembership(with spot: MobileChargingSpotDto) {
        switch spot.status {
        case .hidden, .withdrawn, .removed:
            spots.removeAll { $0.id == spot.id }
        case .queued, .published:
            if let index = spots.firstIndex(where: { $0.id == spot.id }) { spots[index] = spot }
        }
    }
}

enum ChargingSpotPresentation {
    static let unknownNameKey = "charging.spot.name.unknown"
    static let unknownPowerKey = "charging.spot.power.unknown"
    static let pendingStatusKey = "charging.spot.status.pending"
    static let communityCountsKey = "charging.spot.votes.community"

    static var unknownName: String { localizedAppText(unknownNameKey) }
    static var unknownPower: String { localizedAppText(unknownPowerKey) }
    static var pendingStatus: String { localizedAppText(pendingStatusKey) }
    static var communityCounts: String { localizedAppText(communityCountsKey) }

    static func voteAccessibilityValue(
        count: UInt64?, selected: Bool, locale: Locale = .current
    ) -> String {
        let selection = localizedAppText(
            selected ? "charging.vote.selected" : "charging.vote.not_selected",
            arguments: [],
            bundle: appLocalizationBundle,
            locale: locale
        )
        guard let count else { return selection }
        return localizedAppText(
            "charging.vote.accessibility_count.\(count == 1 ? "one" : "other")",
            arguments: [count.formatted(.number.locale(locale)), selection],
            bundle: appLocalizationBundle,
            locale: locale
        )
    }
}
