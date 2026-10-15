import CutoutMobile
import XCTest

@testable import CutoutApp

@MainActor
private final class StorageMaintenanceGate {
    private var operation: CheckedContinuation<StorageMaintenanceResult, Never>?
    private var started: CheckedContinuation<Void, Never>?
    private(set) var callCount = 0
    private var hasStarted = false

    func run() async -> StorageMaintenanceResult {
        callCount += 1
        hasStarted = true
        started?.resume()
        started = nil
        return await withCheckedContinuation { operation = $0 }
    }

    func waitUntilStarted() async {
        guard !hasStarted else { return }
        await withCheckedContinuation { started = $0 }
    }

    func finish(with result: StorageMaintenanceResult) {
        operation?.resume(returning: result)
        operation = nil
    }
}

final class StorageCleanupModelTests: XCTestCase {
    @MainActor
    func testRunningMaintenancePreventsDismissalAndDuplicateSubmission() async {
        let gate = StorageMaintenanceGate()
        let model = StorageCleanupModel { await gate.run() }

        let firstRun = Task { await model.optimize() }
        await gate.waitUntilStarted()
        XCTAssertTrue(model.isRunning)
        XCTAssertFalse(model.canDismiss)

        await model.optimize()
        XCTAssertEqual(gate.callCount, 1)

        firstRun.cancel()
        XCTAssertTrue(model.isRunning)
        gate.finish(with: .completed(bytesBefore: 100, bytesAfter: 80, compressedEventCount: 2))

        await firstRun.value
        XCTAssertTrue(model.canDismiss)
        XCTAssertEqual(
            model.state,
            .completed(bytesBefore: 100, bytesAfter: 80, compressedEventCount: 2)
        )
    }

    @MainActor
    func testRefusalCanBeRetried() async {
        var results: [StorageMaintenanceResult] = [
            .insufficientSpace(
                requiredBytes: 16_384,
                availableBytes: 8_192
            ),
            .completed(
                bytesBefore: 100,
                bytesAfter: 80,
                compressedEventCount: 2
            ),
        ]
        let model = StorageCleanupModel {
            results.removeFirst()
        }

        await model.optimize()
        XCTAssertEqual(
            model.state,
            .failed(.insufficientSpace(requiredBytes: 16_384, availableBytes: 8_192))
        )

        await model.optimize()
        XCTAssertEqual(
            model.state,
            .completed(bytesBefore: 100, bytesAfter: 80, compressedEventCount: 2)
        )
    }

    @MainActor
    func testCaptureImportRefusalIsPreserved() async {
        let model = StorageCleanupModel { .pevcapImportInProgress }

        await model.optimize()

        XCTAssertEqual(model.state, .failed(.pevcapImportInProgress))
    }
}
