import CutoutMobileFFI
import XCTest

@testable import CutoutMobile

@MainActor
final class AppleMusicObservationBridgeTests: XCTestCase {
    func testBackgroundTeardownRejectsQueuedCallbacksAndRestartsOneSubscription() async {
        let service = AppleMusicObservationServiceSpy()
        let rustLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let lifecycle = AppleMusicObservationBridge(
            service: service,
            lifecycle: rustLifecycle,
            effects: effects
        )
        var received = [MusicProviderObservation]()

        await lifecycle.startMonitoring(observedAtMs: { 10 }) { received.append($0) }
        lifecycle.refresh(observedAtMs: 10)
        await service.waitForRefresh(generation: 1)
        await service.completeRefresh(
            generation: 1,
            with: observation(track: "old", observedAtMs: 10)
        )
        await waitUntil { received.last?.snapshot.item?.identifier == "old" }

        _ = lifecycle.stopMonitoring()
        XCTAssertNil(lifecycle.cachedObservation)

        await lifecycle.startMonitoring(observedAtMs: { 20 }) { received.append($0) }
        await service.emitChange(generation: 1)
        await Task.yield()
        XCTAssertEqual(lifecycle.cachedObservation?.snapshot.state, .unavailable)
        XCTAssertFalse(lifecycle.cachedObservation?.snapshot.capabilities.pause ?? true)

        lifecycle.refresh(observedAtMs: 20)
        await service.waitForRefresh(generation: 2)
        await service.completeRefresh(
            generation: 2,
            with: observation(track: "new", observedAtMs: 20)
        )
        await waitUntil { received.last?.snapshot.item?.identifier == "new" }

        XCTAssertEqual(received.map(\.snapshot.item?.identifier), [nil, "old", nil, "new"])
        let maximumActiveSubscriptionCount = await service.maximumActiveSubscriptionCount
        let activeSubscriptionCount = await service.activeSubscriptionCount
        XCTAssertEqual(maximumActiveSubscriptionCount, 1)
        XCTAssertEqual(activeSubscriptionCount, 1)
    }

    func testStalledOldServiceReadCannotReplaceRestartedMetadata() async {
        let service = AppleMusicObservationServiceSpy()
        let rustLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let lifecycle = AppleMusicObservationBridge(
            service: service,
            lifecycle: rustLifecycle,
            effects: effects
        )
        var received = [MusicProviderObservation]()

        await lifecycle.startMonitoring(observedAtMs: { 10 }) { received.append($0) }
        lifecycle.refresh(observedAtMs: 10)
        await service.waitForRefresh(generation: 1)

        _ = lifecycle.stopMonitoring()
        await lifecycle.startMonitoring(observedAtMs: { 20 }) { received.append($0) }
        lifecycle.refresh(observedAtMs: 20)
        await service.waitForRefresh(generation: 2)
        await service.completeRefresh(
            generation: 2,
            with: observation(track: "new", observedAtMs: 20)
        )
        await waitUntil { received.last?.snapshot.item?.identifier == "new" }

        await service.completeRefresh(
            generation: 1,
            with: observation(track: "stale", observedAtMs: 10)
        )
        await Task.yield()

        XCTAssertEqual(lifecycle.cachedObservation?.snapshot.item?.identifier, "new")
        XCTAssertEqual(received.map(\.snapshot.item?.identifier), [nil, nil, "new"])
    }

    func testTimedOutReadReleasesTheReadSlotForRecovery() async {
        let service = AppleMusicObservationServiceSpy()
        let rustLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let lifecycle = AppleMusicObservationBridge(
            service: service,
            lifecycle: rustLifecycle,
            effects: effects,
            playerStateTimeout: .zero
        )
        let clock = AppleMusicTestClock(nowMs: 10)

        await lifecycle.startMonitoring(observedAtMs: { clock.nowMs }) { _ in }
        lifecycle.refresh(observedAtMs: 10)
        clock.nowMs = 10_010
        await service.waitForRefresh(generation: 1)
        await waitUntil {
            !effects.isRunning(.playerStateTimeout(.init(value: 1)))
        }

        lifecycle.refresh(observedAtMs: 10_011)
        let recovered = await service.waitForRefreshCount(2)
        XCTAssertTrue(recovered)
        let refreshCallCount = await service.refreshCallCount
        XCTAssertEqual(refreshCallCount, 2)

        await service.completeRefresh(
            generation: 1,
            with: observation(track: "timed-out", observedAtMs: 10)
        )
        await service.completeRefresh(
            generation: 1,
            with: observation(track: "recovered", observedAtMs: 20)
        )

        _ = lifecycle.stopMonitoring()
        effects.cancelAll()
    }

    func testTimedOutReadPublishesAStaleObservationWithNewerTimestamp() async {
        let service = AppleMusicObservationServiceSpy()
        let rustLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let lifecycle = AppleMusicObservationBridge(
            service: service,
            lifecycle: rustLifecycle,
            effects: effects,
            playerStateTimeout: .milliseconds(20)
        )
        var received = [MusicProviderObservation]()
        let clock = AppleMusicTestClock(nowMs: 10)

        await lifecycle.startMonitoring(observedAtMs: { clock.nowMs }) { received.append($0) }
        lifecycle.refresh(observedAtMs: 10)
        await service.waitForRefresh(generation: 1)
        await service.completeRefresh(
            generation: 1,
            with: observation(track: "old", observedAtMs: 10)
        )
        await waitUntil { received.last?.snapshot.item?.identifier == "old" }

        lifecycle.refresh(observedAtMs: 20)
        clock.nowMs = 10_020
        await waitUntil { received.last?.snapshot.state == .stale }

        XCTAssertEqual(received.count, 3)
        XCTAssertEqual(received[2].snapshot.item?.identifier, "old")
        XCTAssertGreaterThan(
            received[2].snapshot.observedAtMs,
            received[1].snapshot.observedAtMs
        )

        _ = lifecycle.stopMonitoring()
        effects.cancelAll()
    }

    func testRestartImmediatelyInvalidatesThePreviousLiveProjection() async {
        let service = AppleMusicObservationServiceSpy()
        let rustLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let lifecycle = AppleMusicObservationBridge(
            service: service,
            lifecycle: rustLifecycle,
            effects: effects
        )
        var received = [MusicProviderObservation]()

        await lifecycle.startMonitoring(observedAtMs: { 10 }) { received.append($0) }
        lifecycle.refresh(observedAtMs: 10)
        await service.waitForRefresh(generation: 1)
        await service.completeRefresh(
            generation: 1,
            with: observation(track: "old", observedAtMs: 10)
        )
        await waitUntil { received.last?.snapshot.item?.identifier == "old" }

        await lifecycle.startMonitoring(observedAtMs: { 20 }) { received.append($0) }

        XCTAssertEqual(received.last?.snapshot.item?.identifier, "old")
        XCTAssertEqual(received.last?.snapshot.state, .stale)
        XCTAssertFalse(received.last?.snapshot.capabilities.pause ?? true)
        _ = lifecycle.stopMonitoring()
        effects.cancelAll()
    }

    func testNotificationDuringReadSchedulesOneCoalescedRefresh() async {
        let service = AppleMusicObservationServiceSpy()
        let rustLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let lifecycle = AppleMusicObservationBridge(
            service: service,
            lifecycle: rustLifecycle,
            effects: effects
        )

        await lifecycle.startMonitoring(observedAtMs: { 20 }) { _ in }
        lifecycle.refresh(observedAtMs: 10)
        await service.waitForRefresh(generation: 1)
        await service.emitChange(generation: 1)
        await service.completeRefresh(
            generation: 1,
            with: observation(track: "first", observedAtMs: 10)
        )

        let didRefresh = await service.waitForRefreshCount(2)
        let refreshCallCount = await service.refreshCallCount
        XCTAssertTrue(didRefresh)
        XCTAssertEqual(refreshCallCount, 2)
        _ = lifecycle.stopMonitoring()
        effects.cancelAll()
    }

    @MainActor
    func testPlaybackNotificationGenerationBalancesBeginAndEnd() {
        var generation = AppleMusicNotificationGeneration()
        var events = [String]()

        generation.begin { events.append("begin") }
        generation.begin { events.append("duplicate-begin") }
        generation.end { events.append("end") }
        generation.end { events.append("duplicate-end") }

        XCTAssertEqual(events, ["begin", "end"])
        XCTAssertFalse(generation.isActive)
    }

    func testPassiveObservationReadsOnlyTheCache() async {
        let service = AppleMusicObservationServiceSpy()
        let rustLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let lifecycle = AppleMusicObservationBridge(
            service: service,
            lifecycle: rustLifecycle,
            effects: effects
        )

        XCTAssertNil(lifecycle.cachedObservation)
        var refreshCallCount = await service.refreshCallCount
        XCTAssertEqual(refreshCallCount, 0)

        await lifecycle.startMonitoring(observedAtMs: { 0 }) { _ in }
        XCTAssertEqual(lifecycle.cachedObservation?.snapshot.state, .unavailable)
        XCTAssertNil(lifecycle.cachedObservation?.snapshot.item)
        refreshCallCount = await service.refreshCallCount
        XCTAssertEqual(refreshCallCount, 0)
    }

    func testSceneSuspensionEmitsOneRustOwnedObservationGap() async {
        let service = AppleMusicObservationServiceSpy()
        let rustLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let lifecycle = AppleMusicObservationBridge(
            service: service,
            lifecycle: rustLifecycle,
            effects: effects
        )
        await lifecycle.startMonitoring(observedAtMs: { 10 }) { _ in }

        XCTAssertTrue(rustLifecycle.suspend().observationGap)
        XCTAssertFalse(rustLifecycle.suspend().observationGap)
        _ = lifecycle.stopMonitoring()
        effects.cancelAll()
    }

    private func observation(track: String, observedAtMs: UInt64) -> MusicProviderObservation {
        MusicProviderObservation(
            snapshot: MobileMusicSnapshotDto(
                provider: .appleMusic,
                sessionId: "system-music-player",
                state: .playing,
                item: MobileMusicItemDto(identifier: track, title: track, artist: nil),
                positionMilliseconds: 1_000,
                durationMilliseconds: 2_000,
                observedAtMs: observedAtMs,
                capabilities: MobileMusicCapabilitiesDto(
                    previous: true,
                    play: false,
                    pause: true,
                    next: true,
                    openProvider: true
                )
            )
        )
    }

    private func waitUntil(
        maxTurns: Int = 10_000,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @escaping @MainActor () -> Bool
    ) async {
        for _ in 0..<maxTurns {
            if condition() { return }
            await Task.yield()
        }
        XCTFail("timed out waiting for lifecycle state", file: file, line: line)
    }
}

@MainActor
private final class AppleMusicTestClock {
    var nowMs: UInt64

    init(nowMs: UInt64) {
        self.nowMs = nowMs
    }
}

private actor AppleMusicObservationServiceSpy: AppleMusicObservationService {
    private var callback: (@Sendable (UInt64) -> Void)?
    private var activeGeneration: UInt64?
    private var pendingRefreshes = [UInt64: [CheckedContinuation<MusicProviderObservation?, Never>]]()
    private var seenRefreshGenerations = Set<UInt64>()
    private(set) var maximumActiveSubscriptionCount = 0
    private(set) var refreshCallCount = 0

    var activeSubscriptionCount: Int { activeGeneration == nil ? 0 : 1 }

    func subscribe(generation: UInt64, onChange: @escaping @Sendable (UInt64) -> Void) {
        activeGeneration = generation
        callback = onChange
        maximumActiveSubscriptionCount = max(maximumActiveSubscriptionCount, activeSubscriptionCount)
    }

    func unsubscribe(generation: UInt64) {
        guard activeGeneration == generation else { return }
        activeGeneration = nil
        callback = nil
    }

    func observation(generation: UInt64, observedAtMs _: UInt64) async -> MusicProviderObservation? {
        refreshCallCount += 1
        seenRefreshGenerations.insert(generation)
        return await withCheckedContinuation { continuation in
            pendingRefreshes[generation, default: []].append(continuation)
        }
    }

    func emitChange(generation: UInt64) {
        callback?(generation)
    }

    func waitForRefresh(generation: UInt64) async {
        while !seenRefreshGenerations.contains(generation) {
            await Task.yield()
        }
    }

    func waitForRefreshCount(_ count: Int, timeout: Duration = .seconds(1)) async -> Bool {
        let deadline = ContinuousClock.now.advanced(by: timeout)
        while refreshCallCount < count {
            guard ContinuousClock.now < deadline else { return false }
            await Task.yield()
        }
        return true
    }

    func completeRefresh(
        generation: UInt64,
        with observation: MusicProviderObservation
    ) {
        guard var continuations = pendingRefreshes[generation], !continuations.isEmpty else { return }
        continuations.removeFirst().resume(returning: observation)
        if continuations.isEmpty {
            pendingRefreshes.removeValue(forKey: generation)
        } else {
            pendingRefreshes[generation] = continuations
        }
    }
}
