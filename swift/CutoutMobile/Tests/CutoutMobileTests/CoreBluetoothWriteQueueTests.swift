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
}
