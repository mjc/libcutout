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

@MainActor
@Observable
final class ChargingMapModel {
    private static let pageSize: UInt32 = 100
    private static let resultLimit = 500
    private static let nearbyLimit: UInt32 = 1
    private static let metroWindowMeters = 100_000.0

    private let queryPage: ChargingMapPageQuery
    private let readSpot: ChargingMapSpotQuery
    private let nearbySpots: ChargingMapNearbyQuery
    private let writeVote: ChargingMapVoteWrite
    private let writeWithdrawal: ChargingMapWithdrawal
    private let queryBlockedOperations: ChargingBlockedOperationsQuery
    private let resolveOperation: ChargingOperationResolution
    private let communitySync: ChargingCommunitySync?
    private var withdrawnAtMutation = [String: (mutation: UInt64, revision: UInt64)]()
    private var bounds: MobileGeoBoundsDto?
    private var cursor: MobileMapPointCursorDto?
    private var generation: UInt64 = 0
    private var mutationSequence: UInt64 = 0
    private var spotMutationSequence = [String: UInt64]()
    private var selectionSequence: UInt64 = 0
    private var voteReadbacks = [String: MobileChargingSpotDto]()
    private var loadingTask: Task<Void, Never>?
    private var discoveryTask: Task<Void, Never>?
    private var communitySyncTask: Task<Void, Never>?
    private var communitySyncGeneration: UInt64 = 0
    private var communitySyncNeedsAnotherRun = false
    private var communitySyncRetryRequested = false
    private var discoveryGeneration: UInt64 = 0
    private var discoveryState: InitialDiscoveryState = .eligible
    private var initialNearbySearchWasEmpty = false

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
            .registrationUncertain:
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

    private enum InitialDiscoveryState: Equatable {
        case eligible
        case waitingForLocation
        case searching
        case settled
        case cancelled
    }

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
        database: RideDatabaseHandle,
        filter: MobileChargingFilterDto = MobileChargingFilterDto(
            cost: nil, connector: nil, voltageVolts: nil, venue: nil)
    ) {
        self.init(
            filter: filter,
            queryPage: { bounds, filter, cursor, limit in
                try await database.chargingSpotsInBounds(
                    bounds: bounds, selection: filter, cursor: cursor, limit: limit
                )
            },
            readSpot: { id in try await database.chargingSpot(id: id) },
            nearbySpots: { bounds, origin, filter, limit in
                try await database.nearbyChargingSpots(
                    bounds: bounds, origin: origin, selection: filter, limit: limit
                )
            },
            writeVote: { id, revision, vote in
                _ = try await database.voteChargingSpot(id: id, expectedRevision: revision, vote: vote)
            },
            writeWithdrawal: { id, revision in
                _ = try await database.withdrawChargingSpot(id: id, expectedRevision: revision)
            },
            queryBlockedOperations: {
                try await database.listBlockedChargingOperations()
            },
            resolveOperation: { operationID, revision, resolution in
                try await database.resolveChargingOperation(
                    operationId: operationID, expectedLocalRevision: revision,
                    resolution: resolution
                )
            },
            communitySync: nil
        )
    }

    convenience init() {
        self.init(
            queryPage: { bounds, filter, cursor, limit in
                do {
                    let database = try await RustPersistenceStore.open()
                    return try await database.chargingSpotsInBounds(
                        bounds: bounds, selection: filter, cursor: cursor, limit: limit
                    )
                } catch let error as MobileChargingError { throw error } catch { throw MobileChargingError.Unavailable }
            },
            readSpot: { id in
                do {
                    let database = try await RustPersistenceStore.open()
                    return try await database.chargingSpot(id: id)
                } catch let error as MobileChargingError { throw error } catch { throw MobileChargingError.Unavailable }
            },
            nearbySpots: { bounds, origin, filter, limit in
                do {
                    let database = try await RustPersistenceStore.open()
                    return try await database.nearbyChargingSpots(
                        bounds: bounds, origin: origin, selection: filter, limit: limit
                    )
                } catch let error as MobileChargingError { throw error } catch { throw MobileChargingError.Unavailable }
            },
            writeVote: { id, revision, vote in
                do {
                    let database = try await RustPersistenceStore.open()
                    _ = try await database.voteChargingSpot(id: id, expectedRevision: revision, vote: vote)
                } catch let error as MobileChargingError { throw error } catch { throw MobileChargingError.Unavailable }
            },
            writeWithdrawal: { id, revision in
                do {
                    let database = try await RustPersistenceStore.open()
                    _ = try await database.withdrawChargingSpot(id: id, expectedRevision: revision)
                } catch let error as MobileChargingError { throw error } catch { throw MobileChargingError.Unavailable }
            },
            queryBlockedOperations: {
                do {
                    let database = try await RustPersistenceStore.open()
                    return try await database.listBlockedChargingOperations()
                } catch let error as MobileChargingError { throw error } catch { throw MobileChargingError.Unavailable }
            },
            resolveOperation: { operationID, revision, resolution in
                do {
                    let database = try await RustPersistenceStore.open()
                    try await database.resolveChargingOperation(
                        operationId: operationID, expectedLocalRevision: revision,
                        resolution: resolution
                    )
                } catch let error as MobileChargingError { throw error } catch { throw MobileChargingError.Unavailable }
            },
            communitySync: .shared
        )
    }

    func communitySyncRequested() {
        requestCommunitySync(explicitRetry: false)
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
        mutationSequence &+= 1
        spotMutationSequence[id] = mutationSequence
        if bounds != nil {
            await fetchPage(append: false)
        } else {
            advanceQuery()
        }
        let requestGeneration = generation

        let requestMutation = spotMutationSequence[id, default: 0]
        let requestSelection = selectionSequence
        do {
            let updated = try await readSpot(id)
            guard generation == requestGeneration,
                spotMutationSequence[id, default: 0] == requestMutation
            else { return }
            guard let updated else {
                spots.removeAll { $0.id == id }
                if selectedSpot?.id == id { deselect() }
                return
            }
            if updated.status == .published { withdrawnAtMutation.removeValue(forKey: id) }
            if updated.status == .hidden || updated.status == .withdrawn || updated.status == .removed {
                spots.removeAll { $0.id == id }
            } else if let index = spots.firstIndex(where: { $0.id == id }) {
                spots[index] = updated
            }
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
                    || result == .rejected || communitySyncDownloaded > 0
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
        let requestMutation = spotMutationSequence[id, default: 0]
        let requestSelection = selectionSequence
        do {
            let updated = try await readSpot(id)
            guard generation == requestGeneration,
                selectionSequence == requestSelection,
                selectedSpot?.id == id,
                spotMutationSequence[id, default: 0] == requestMutation,
                !Task.isCancelled
            else { return }
            guard let updated else {
                deselect()
                return
            }
            guard let current = selectedSpot, updated.localRevision >= current.localRevision else { return }
            selectedSpot = updated
            if updated.status == .hidden || updated.status == .withdrawn || updated.status == .removed {
                spots.removeAll { $0.id == id }
            } else if let index = spots.firstIndex(where: { $0.id == id }),
                updated.localRevision >= spots[index].localRevision
            {
                spots[index] = updated
            }
        } catch {
            // Keep the current detail visible when the local read cannot complete.
        }
    }

    private func rearmEmptyInitialDiscoveryAfterCacheUpdate() {
        guard communitySyncDownloaded > 0,
            initialNearbySearchWasEmpty,
            discoveryState == .settled,
            spots.isEmpty
        else { return }
        initialNearbySearchWasEmpty = false
        discoveryState = .eligible
        if userLocation == nil {
            discoveryState = .waitingForLocation
        } else {
            beginInitialDiscovery()
        }
    }

    func setViewport(_ bounds: MobileGeoBoundsDto) {
        guard self.bounds != bounds else { return }
        self.bounds = bounds
        advanceQuery()
    }

    func setFilter(_ filter: MobileChargingFilterDto) {
        guard self.filter != filter else { return }
        self.filter = filter
        if discoveryState != .eligible { cancelAutomaticDiscovery() }
        spots = []
        selectedSpot = nil
        cursor = nil
        hasMore = false
        limitReached = false
        advanceQuery()
    }

    func setUserLocation(_ coordinate: MobileMapCoordinateDto) {
        userLocation = coordinate
        if discoveryState == .waitingForLocation { beginInitialDiscovery() }
    }

    func cancelAutomaticDiscovery() {
        discoveryTarget = nil
        guard discoveryState != .cancelled else { return }
        discoveryState = .cancelled
        initialNearbySearchWasEmpty = false
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
        let requestMutation = spotMutationSequence[id, default: 0]
        do {
            guard let updated = try await readSpot(id) else {
                guard generation == requestGeneration,
                    spotMutationSequence[id, default: 0] == requestMutation
                else { return }
                error = .NotFound
                return
            }
            guard generation == requestGeneration,
                spotMutationSequence[id, default: 0] == requestMutation
            else { return }
            guard !Task.isCancelled else { return }

            mutationSequence &+= 1
            spotMutationSequence[id] = mutationSequence
            voteReadbacks[id] = updated
            if let index = spots.firstIndex(where: { $0.id == id }) {
                spots[index] = updated
            }
            selectedSpot = updated
            error = nil
        } catch let error as MobileChargingError {
            guard generation == requestGeneration,
                spotMutationSequence[id, default: 0] == requestMutation,
                !Task.isCancelled
            else { return }
            self.error = error
        } catch is CancellationError {
            return
        } catch {
            guard generation == requestGeneration,
                spotMutationSequence[id, default: 0] == requestMutation,
                !Task.isCancelled
            else { return }
            self.error = .StorageFailure
        }
    }

    func consumeDiscoveryTarget() -> MobileMapCoordinateDto? {
        defer { discoveryTarget = nil }
        return discoveryTarget
    }

    func withdraw(_ spot: MobileChargingSpotDto) async -> Bool {
        guard spot.owned, !pendingWithdrawalIDs.contains(spot.id), !pendingVoteIDs.contains(spot.id),
            withdrawnAtMutation[spot.id] == nil
        else { return false }
        pendingWithdrawalIDs.insert(spot.id)
        error = nil
        defer { pendingWithdrawalIDs.remove(spot.id) }
        do {
            try await writeWithdrawal(spot.id, spot.localRevision)
            mutationSequence &+= 1
            withdrawnAtMutation[spot.id] = (mutationSequence, spot.localRevision)
            spotMutationSequence[spot.id] = mutationSequence
            voteReadbacks.removeValue(forKey: spot.id)
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
    func deselect() {
        selectionSequence &+= 1
        selectedSpot = nil
        voteError = nil
    }

    func vote(_ selection: MobileChargingVoteDto, forSpotID: String? = nil) async {
        guard let selected = selectedSpot, !pendingVoteIDs.contains(selected.id),
            !pendingWithdrawalIDs.contains(selected.id), withdrawnAtMutation[selected.id] == nil
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
                let updated = try await readSpot(id)
                self.finishVote(id: id, updated: updated, error: nil)
            } catch let error as MobileChargingError {
                self.finishVote(id: id, updated: nil, error: error)
            } catch is CancellationError {
                self.finishVote(id: id, updated: nil, error: nil)
            } catch {
                self.finishVote(id: id, updated: nil, error: .StorageFailure)
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
        let requestMutationSequence = spotMutationSequence
        let task = Task { [queryPage] in
            do {
                let page = try await queryPage(requestBounds, requestFilter, requestCursor, Self.pageSize)
                self.apply(
                    page, append: append, generation: requestGeneration,
                    mutationSequence: requestMutationSequence
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
        mutationSequence requestMutationSequence: [String: UInt64]
    ) {
        guard self.generation == generation else { return }
        if Task.isCancelled {
            cancelled(generation: generation)
            return
        }
        var merged = append ? spots : []
        for incoming in page.spots {
            if let withdrawal = withdrawnAtMutation[incoming.id] {
                guard requestMutationSequence[incoming.id, default: 0] >= withdrawal.mutation,
                    incoming.localRevision > withdrawal.revision,
                    incoming.status == .published
                else { continue }
                withdrawnAtMutation.removeValue(forKey: incoming.id)
            }
            let currentIndex = merged.firstIndex(where: { $0.id == incoming.id })
            let changedSinceRequest =
                spotMutationSequence[incoming.id, default: 0]
                > requestMutationSequence[incoming.id, default: 0]
            var candidate =
                changedSinceRequest
                ? (voteReadbacks[incoming.id] ?? currentIndex.map { merged[$0] } ?? incoming)
                : incoming
            if let known = spots.first(where: { $0.id == incoming.id })
                ?? (selectedSpot?.id == incoming.id ? selectedSpot : nil),
                known.localRevision > candidate.localRevision
            {
                candidate = known
            }
            if let currentIndex {
                if candidate.localRevision >= merged[currentIndex].localRevision {
                    merged[currentIndex] = candidate
                }
            } else if merged.count < Self.resultLimit {
                merged.append(candidate)
            }
        }
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
        if !append { considerInitialDiscovery(hasLocalResults: !merged.isEmpty) }
    }

    private func considerInitialDiscovery(hasLocalResults: Bool) {
        guard discoveryState == .eligible else { return }
        guard !hasLocalResults else {
            initialNearbySearchWasEmpty = false
            discoveryState = .settled
            return
        }
        guard userLocation != nil else {
            discoveryState = .waitingForLocation
            return
        }
        beginInitialDiscovery()
    }

    private func beginInitialDiscovery() {
        guard discoveryState == .waitingForLocation || discoveryState == .eligible,
            let origin = userLocation
        else { return }
        discoveryState = .searching
        discoveryGeneration &+= 1
        let requestGeneration = discoveryGeneration
        let requestFilter = filter
        let region = MKCoordinateRegion(
            center: CLLocationCoordinate2D(
                latitude: origin.latitudeDegrees, longitude: origin.longitudeDegrees
            ),
            latitudinalMeters: Self.metroWindowMeters,
            longitudinalMeters: Self.metroWindowMeters
        )
        guard let metroBounds = RideMapCanvasView.geoBounds(for: region) else {
            discoveryState = .settled
            discoveryTask = nil
            return
        }
        let task = Task { [nearbySpots] in
            do {
                let matches = try await nearbySpots(metroBounds, origin, requestFilter, Self.nearbyLimit)
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
        guard discoveryGeneration == generation, discoveryState == .searching else { return }
        if Task.isCancelled {
            cancelInitialDiscovery(generation: generation)
            return
        }
        discoveryState = .settled
        discoveryTask = nil
        initialNearbySearchWasEmpty = nearest == nil
        discoveryTarget = nearest?.input?.coordinate
    }

    private func failInitialDiscovery(_ error: MobileChargingError, generation: UInt64) {
        guard discoveryGeneration == generation, discoveryState == .searching else { return }
        if Task.isCancelled {
            cancelInitialDiscovery(generation: generation)
            return
        }
        discoveryState = .settled
        discoveryTask = nil
        initialNearbySearchWasEmpty = false
        self.error = error
    }

    private func cancelInitialDiscovery(generation: UInt64) {
        guard discoveryGeneration == generation else { return }
        discoveryState = .cancelled
        discoveryTask = nil
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

    private func finishVote(id: String, updated: MobileChargingSpotDto?, error: MobileChargingError?) {
        pendingVoteIDs.remove(id)
        if let error, selectedSpot?.id == id { voteError = error }
        guard let updated, updated.id == id, withdrawnAtMutation[id] == nil else { return }
        let newestRevision = max(
            max(
                spots.first(where: { $0.id == id })?.localRevision ?? 0,
                selectedSpot?.id == id ? selectedSpot?.localRevision ?? 0 : 0),
            voteReadbacks[id]?.localRevision ?? 0
        )
        guard updated.localRevision > newestRevision else { return }
        mutationSequence &+= 1
        spotMutationSequence[id] = mutationSequence
        voteReadbacks[id] = updated
        if let index = spots.firstIndex(where: { $0.id == id }) { spots[index] = updated }
        if selectedSpot?.id == id { selectedSpot = updated }
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
