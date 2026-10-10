import XCTest

@testable import CutoutApp

@MainActor
final class RideBackgroundTaskTests: XCTestCase {
    func testExpirationEndsLeaseSynchronouslyBeforeCancellingCheckpoint() async throws {
        let application = BackgroundTaskApplicationSpy()
        let lease = RideBackgroundTask(
            onExpiration: { application.events.append("cancel checkpoint") },
            beginTask: application.begin
        )
        let expire = try XCTUnwrap(application.expiration)

        expire()

        XCTAssertEqual(application.events, ["end lease", "cancel checkpoint"])
        lease.end()
        expire()
        await Task.yield()
        XCTAssertEqual(application.events, ["end lease", "cancel checkpoint"])
    }

    func testNormalCompletionIgnoresLateExpirationAndEndsOnce() async throws {
        let application = BackgroundTaskApplicationSpy()
        var lease: RideBackgroundTask? = RideBackgroundTask(
            onExpiration: { application.events.append("cancel checkpoint") },
            beginTask: application.begin
        )
        let expire = try XCTUnwrap(application.expiration)

        lease?.end()
        expire()
        await Task.yield()
        lease?.end()
        lease = nil

        XCTAssertEqual(application.events, ["end lease"])
    }

    func testDeinitializationEndsUnfinishedLease() {
        let application = BackgroundTaskApplicationSpy()
        var lease: RideBackgroundTask? = RideBackgroundTask(
            onExpiration: { application.events.append("cancel checkpoint") },
            beginTask: application.begin
        )
        XCTAssertNotNil(lease)

        lease = nil

        XCTAssertEqual(application.events, ["end lease"])
    }
}

@MainActor
private final class BackgroundTaskApplicationSpy {
    var events: [String] = []
    var expiration: (@MainActor @Sendable () -> Void)?

    func begin(onExpiration: @escaping @MainActor @Sendable () -> Void) -> RideBackgroundTask.EndTask? {
        expiration = onExpiration
        return { self.events.append("end lease") }
    }
}
