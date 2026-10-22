import XCTest

@testable import CutoutMobile

final class CoreBluetoothWriteQueueTests: XCTestCase {
    func testSubmittedReceiptAlwaysFollowsNativeWrite() {
        for ready in [true, false] {
            let queue = CoreBluetoothWriteQueue(capacity: 1)
            var didWrite = false
            var receipts: [CoreBluetoothWriteDisposition] = []
            _ = queue.submit(
                canSend: { ready }, write: { didWrite = true },
                onReceipt: { disposition in
                    if disposition == .submitted { XCTAssertTrue(didWrite) }
                    if disposition == .queued { XCTAssertFalse(didWrite) }
                    receipts.append(disposition)
                })
            queue.flush { true }
            XCTAssertTrue(didWrite)
            XCTAssertEqual(receipts, ready ? [.submitted] : [.queued, .submitted])
        }
    }

    func testExpiredQueuedWriteNeverReachesNativeSubmission() {
        let queue = CoreBluetoothWriteQueue(capacity: 2)
        var current = true
        var receipts: [CoreBluetoothWriteDisposition] = []
        _ = queue.submit(
            canSend: { false }, isCurrent: { current },
            write: { XCTFail("Expired write must not reach CoreBluetooth") },
            onReceipt: { receipts.append($0) }
        )
        current = false
        queue.flush { true }
        queue.flush { true }
        XCTAssertEqual(receipts, [.queued, .cancelled])
    }

    func testCancellationDoesNotBlockAnotherCurrentWrite() {
        let queue = CoreBluetoothWriteQueue(capacity: 2)
        var current = true
        var submitted: [Int] = []
        var receipts: [CoreBluetoothWriteDisposition] = []
        _ = queue.submit(
            canSend: { false }, isCurrent: { current }, write: { submitted.append(1) },
            onReceipt: { receipts.append($0) })
        _ = queue.submit(canSend: { false }, write: { submitted.append(2) }, onReceipt: { receipts.append($0) })
        current = false
        queue.flush { true }
        XCTAssertEqual(submitted, [2])
        XCTAssertEqual(receipts, [.queued, .queued, .cancelled, .submitted])
    }

    func testAlreadyExpiredRequestIsCancelledWithoutQueueing() {
        let queue = CoreBluetoothWriteQueue(capacity: 1)
        var receipts: [CoreBluetoothWriteDisposition] = []
        let result = queue.submit(
            canSend: { true }, isCurrent: { false },
            write: { XCTFail("Expired write must not execute") },
            onReceipt: { receipts.append($0) }
        )
        XCTAssertEqual(result, .cancelled)
        XCTAssertEqual(receipts, [.cancelled])
    }
    func testPrioritizedDriveWriteDisplacesOneOrdinaryWriteWithCancellationReceipt() {
        let queue = CoreBluetoothWriteQueue(capacity: 2)
        var submitted: [Int] = []
        var displacedReceipts: [CoreBluetoothWriteDisposition] = []
        _ = queue.submit(canSend: { false }, write: { submitted.append(1) }, onReceipt: { _ in })
        _ = queue.submit(
            canSend: { false }, write: { submitted.append(2) }, onReceipt: { displacedReceipts.append($0) })
        let release = queue.submit(
            canSend: { false }, priority: .release,
            write: { submitted.append(3) }, onReceipt: { _ in })
        XCTAssertEqual(release, .queued)
        XCTAssertEqual(displacedReceipts, [.queued, .cancelled])
        queue.flush { true }
        XCTAssertEqual(submitted, [3, 1], "The existing queue must give drive/release writes precedence")
    }

    func testSupersededDriveIsRemovedBeforeQueueCapacityAdmission() {
        let queue = CoreBluetoothWriteQueue(capacity: 1)
        var current = true
        var oldReceipts: [CoreBluetoothWriteDisposition] = []
        var submitted: [Int] = []
        _ = queue.submit(
            canSend: { false }, priority: .drive, isCurrent: { current },
            write: { submitted.append(1) }, onReceipt: { oldReceipts.append($0) })
        current = false
        let replacement = queue.submit(
            canSend: { false }, priority: .drive,
            write: { submitted.append(2) }, onReceipt: { _ in })
        XCTAssertEqual(replacement, .queued)
        XCTAssertEqual(oldReceipts, [.queued, .cancelled])
        queue.flush { true }
        XCTAssertEqual(submitted, [2])
    }

    func testOneCreditPerFiftyMillisecondsDoesNotStarveOrdinaryTraffic() {
        let queue = CoreBluetoothWriteQueue(capacity: 4)
        var now = 0
        var credit = true
        var submitted: [String] = []
        var driveReceipts: [CoreBluetoothWriteDisposition] = []
        func send(_ name: String) {
            submitted.append(name)
            credit = false
        }
        _ = queue.submit(canSend: { credit }, priority: .drive, write: { send("drive-0") }, onReceipt: { _ in })
        _ = queue.submit(canSend: { credit }, write: { send("telemetry") }, onReceipt: { _ in })
        now = 50
        credit = true
        _ = queue.submit(
            canSend: { credit }, priority: .drive, isCurrent: { now < 100 },
            write: { send("drive-50") }, onReceipt: { driveReceipts.append($0) })
        XCTAssertEqual(submitted, ["drive-0", "telemetry"], "A due drive cannot consume every native credit")
        now = 100
        credit = true
        queue.flush { credit }
        XCTAssertEqual(
            driveReceipts, [.queued, .cancelled], "Bandwidth shortage must expire queued drive, not renew it")
        _ = queue.submit(canSend: { credit }, priority: .release, write: { send("release") }, onReceipt: { _ in })
        XCTAssertEqual(submitted, ["drive-0", "telemetry", "release"])
    }

}
