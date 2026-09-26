import CutoutMobile
import CutoutMobileFFI
import Foundation

@testable import CutoutApp

@MainActor
final class RideHistoryAvailability {
    var isAvailable = true
}

final class GatedRideHistoryQuery: RideHistoryQuerying, @unchecked Sendable {
    private let base: MobileRideMapState
    private let failAfterRelease: Bool
    private let lock = NSLock()
    private var shouldGateNextProjection = false
    private let projectionStarted = DispatchSemaphore(value: 0)
    private let projectionRelease = DispatchSemaphore(value: 0)
    private let projectionFinished = DispatchSemaphore(value: 0)
    private var shouldGateNextHistoryPage = false
    private var gatedHistoryPageRideIDs = [String]()
    private var historyPageCursorPresence = [Bool]()
    private let historyPageStarted = DispatchSemaphore(value: 0)
    private let historyPageRelease = DispatchSemaphore(value: 0)
    private let historyPageFinished = DispatchSemaphore(value: 0)
    private var historyPageFilters = [MobileRideHistoryFilterDto?]()
    private var supplementaryContextQueryCount = 0

    var supplementaryContextQueryCountSnapshot: Int {
        lock.lock()
        defer { lock.unlock() }
        return supplementaryContextQueryCount
    }

    init(base: MobileRideMapState, failAfterRelease: Bool) {
        self.base = base
        self.failAfterRelease = failAfterRelease
    }

    func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        cancellation: MobileRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        lock.lock()
        let shouldGate = shouldGateNextProjection
        shouldGateNextProjection = false
        lock.unlock()
        let result = Result {
            try base.projectStoredPoints(
                rideID: rideID,
                budget: budget,
                viewport: viewport,
                privacy: privacy,
                cancellation: cancellation
            )
        }
        if shouldGate {
            projectionStarted.signal()
            projectionRelease.wait()
            defer { projectionFinished.signal() }
            if failAfterRelease {
                throw MobileRideMapError.storageError("late gated viewport failure")
            }
        }
        return try result.get()
    }

    func storedMusicHistoryAsync(rideID: String) async throws -> MobileMusicHistoryDto {
        try await base.storedMusicHistoryAsync(rideID: rideID)
    }

    func storedHistoryVehicleOptions() throws -> [MobileRideMapHistoryVehicleOptionDto] {
        try base.storedHistoryVehicleOptions()
    }

    func storedHistoryRide(rideID: String) throws -> MobileRideMapHistorySummaryDto? {
        try base.storedHistoryRide(rideID: rideID)
    }

    func storedHistoryPage(
        cursor: MobileRideCursorDto?,
        limit: UInt32,
        filter: MobileRideHistoryFilterDto?
    ) throws -> MobileRideMapHistoryPageDto {
        let page = try base.storedHistoryPage(cursor: cursor, limit: limit, filter: filter)
        lock.lock()
        historyPageFilters.append(filter)
        historyPageCursorPresence.append(cursor != nil)
        let shouldGate = shouldGateNextHistoryPage
        shouldGateNextHistoryPage = false
        if shouldGate {
            gatedHistoryPageRideIDs = page.summaries.map(\.rideID)
        }
        lock.unlock()
        if shouldGate {
            historyPageStarted.signal()
            historyPageRelease.wait()
            historyPageFinished.signal()
        }
        return page
    }

    func projectStoredHistoryContext(
        filter: MobileRideHistoryFilterDto,
        selectedRideID: String?,
        budget: MobileRideMapHistoryContextBudget,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy
    ) throws -> MobileRideMapHistoryContextProjection {
        lock.lock()
        supplementaryContextQueryCount += 1
        lock.unlock()
        return try base.projectStoredHistoryContext(
            filter: filter,
            selectedRideID: selectedRideID,
            budget: budget,
            viewport: viewport,
            privacy: privacy
        )
    }

    func waitUntilGatedProjectionStarts() async -> Bool {
        await waitForSignal(projectionStarted)
    }

    func armNextProjection() {
        lock.lock()
        shouldGateNextProjection = true
        lock.unlock()
    }

    func releaseGatedProjection() {
        projectionRelease.signal()
    }

    func waitUntilGatedProjectionFinishes() async -> Bool {
        await waitForSignal(projectionFinished)
    }

    func waitUntilGatedHistoryPageStarts() async -> Bool {
        await waitForSignal(historyPageStarted)
    }

    func armNextHistoryPage() {
        lock.lock()
        shouldGateNextHistoryPage = true
        lock.unlock()
    }

    func releaseGatedHistoryPage() {
        historyPageRelease.signal()
    }

    func waitUntilGatedHistoryPageFinishes() async -> Bool {
        await waitForSignal(historyPageFinished)
    }

    var gatedHistoryPageRideIDsSnapshot: [String] {
        lock.lock()
        defer { lock.unlock() }
        return gatedHistoryPageRideIDs
    }

    var historyPageFiltersSnapshot: [MobileRideHistoryFilterDto?] {
        lock.lock()
        defer { lock.unlock() }
        return historyPageFilters
    }

    var historyPageCursorPresenceSnapshot: [Bool] {
        lock.lock()
        defer { lock.unlock() }
        return historyPageCursorPresence
    }

    func waitUntilHistoryPageFilter(_ searchText: String) async -> Bool {
        let deadline = ContinuousClock.now + .seconds(5)
        while ContinuousClock.now < deadline {
            if historyPageFiltersSnapshot.contains(where: { $0?.searchText == searchText }) {
                return true
            }
            try? await Task.sleep(for: .milliseconds(1))
        }
        return false
    }

    @MainActor
    private func waitForSignal(_ semaphore: DispatchSemaphore) async -> Bool {
        await withCheckedContinuation { continuation in
            DispatchQueue.global().async {
                continuation.resume(returning: semaphore.wait(timeout: .now() + 5) == .success)
            }
        }
    }
}
