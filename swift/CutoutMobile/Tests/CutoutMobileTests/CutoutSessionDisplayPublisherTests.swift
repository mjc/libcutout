import Foundation
import Synchronization
import XCTest

@testable import CutoutMobile

private final class NativePublisherReference: @unchecked Sendable {
    let publisher: CutoutSessionDisplayPublisher
    init(_ publisher: CutoutSessionDisplayPublisher) { self.publisher = publisher }
}

private final class NativePublicationRecorder: Sendable {
    private let values = Mutex<[UInt64]>([])
    var counts: [UInt64] { values.withLock { $0 } }

    // Construct callbacks outside MainActor so the old implementation can expose
    // its wrong-thread behavior as an assertion instead of terminating XCTest.
    func makePublisher() -> CutoutSessionDisplayPublisher {
        CutoutSessionDisplayPublisher(
            clock: MonotonicClock(), intervalMilliseconds: 10,
            onDisplayStateChange: { [self] value in
                values.withLock { $0.append(value.notificationCount) }
            },
            onRecord: { _ in }
        )
    }
}

final class CutoutSessionDisplayPublisherTests: XCTestCase {
    private func occupyMainQueue(seconds: TimeInterval) { Thread.sleep(forTimeInterval: seconds) }
    private func finishNativeBurst(_ completion: DispatchSemaphore) { completion.wait() }

    @MainActor
    func testPreviouslyQueuedTimerCannotConsumeAReplacementBeforeItsCurrentDeadline() async throws {
        let clock = Mutex(MonotonicMilliseconds(1_000))
        let values = Mutex<[UInt64]>([])
        let publisher = CutoutSessionDisplayPublisher(
            clock: MonotonicClock(now: { clock.withLock { $0 } }), intervalMilliseconds: 100,
            onDisplayStateChange: { value in values.withLock { $0.append(value.notificationCount) } },
            onRecord: { _ in }
        )
        publisher.submit(RideDisplayState(notificationCount: 1), queuedAt: MonotonicMilliseconds(1_000))
        clock.withLock { $0 = MonotonicMilliseconds(1_050) }
        publisher.submit(RideDisplayState(notificationCount: 2), queuedAt: MonotonicMilliseconds(1_050))
        occupyMainQueue(seconds: 0.075)
        clock.withLock { $0 = MonotonicMilliseconds(1_100) }
        publisher.submit(RideDisplayState(notificationCount: 3), queuedAt: MonotonicMilliseconds(1_100))
        clock.withLock { $0 = MonotonicMilliseconds(1_101) }
        publisher.submit(RideDisplayState(notificationCount: 4), queuedAt: MonotonicMilliseconds(1_101))
        try await Task.sleep(for: .milliseconds(30))
        XCTAssertEqual(values.withLock { $0 }, [1, 3])
        try await Task.sleep(for: .milliseconds(120))
        XCTAssertEqual(values.withLock { $0 }, [1, 3, 4])
        publisher.cancel()
    }

    @MainActor
    func testInactivePresentationRetainsLatestWithoutPublishingAndCatchesUpOnActivation() async throws {
        let values = Mutex<[RideDisplayState]>([])
        let publisher = CutoutSessionDisplayPublisher(
            clock: MonotonicClock(), intervalMilliseconds: 10,
            onDisplayStateChange: { value in values.withLock { $0.append(value) } },
            onRecord: { _ in }
        )
        publisher.setActive(false)
        for index in 1...100 {
            let latest = RideDisplayState(notificationCount: UInt64(index))
            publisher.submit(latest, queuedAt: MonotonicClock().now())
        }
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertTrue(values.withLock { $0.isEmpty })
        publisher.setActive(true)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(values.withLock { $0.map(\.notificationCount) }, [100])
        publisher.cancel()
    }

    @MainActor
    func testBurstFromNativeQueueQueuesOnlyLatestPublicationAndCancelDropsPendingState() async throws {
        let recorder = NativePublicationRecorder()
        let publisher = recorder.makePublisher()
        // The main actor is occupied during this native queue burst.
        let reference = NativePublisherReference(publisher)
        let completed = DispatchSemaphore(value: 0)
        DispatchQueue(label: "test.native-presentation").async {
            for index in 1...2_000 {
                let value = RideDisplayState(notificationCount: UInt64(index))
                reference.publisher.submit(value, queuedAt: MonotonicClock().now())
            }
            completed.signal()
        }
        finishNativeBurst(completed)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(recorder.counts, [2_000])
        publisher.setActive(false)
        let pending = RideDisplayState(notificationCount: 2_001)
        publisher.submit(pending, queuedAt: MonotonicClock().now())
        publisher.cancel()
        publisher.setActive(true)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(recorder.counts, [2_000])
    }
}
