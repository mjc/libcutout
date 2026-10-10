import CoreLocation
import CutoutMobileFFI
import Foundation
import SQLite3
import Synchronization
import XCTest

@testable import CutoutMobile

final class PhoneLocationStorageBackpressureTests: XCTestCase {
    @MainActor
    func testActualSQLiteStallAnd64OwnedCallbacksLeaveMainResponsiveAndRetainEveryPoint() async throws {
        let state = MobileRideMapState()
        let callbacks = try await Task.detached {
            _ = try state.startGpsOnly(atMs: 1_000)
            return try (0..<64).map { index in
                try state.admitLocationCallback(
                    receiptMonotonicMs: UInt64(1_000 + index * 1_000),
                    receiptWallClockUnixMs: UInt64(1_700_000_000_000 + index * 1_000),
                    samples: [Self.sample(index)]
                )
            }
        }.value
        let lockEntered = expectation(description: "SQLite write transaction owns worker")
        let release = DispatchSemaphore(value: 0)
        let unlocked = Mutex(false)
        defer { release.signal() }
        DispatchQueue(label: "test.cutout.location-sqlite-stall").async {
            var connection: OpaquePointer?
            XCTAssertEqual(sqlite3_open(MobileRideMapState.debugDatabasePath, &connection), SQLITE_OK)
            defer { sqlite3_close(connection) }
            XCTAssertEqual(sqlite3_exec(connection, "BEGIN IMMEDIATE", nil, nil, nil), SQLITE_OK)
            lockEntered.fulfill()
            _ = release.wait(timeout: .now() + .seconds(2))
            unlocked.withLock { $0 = true }
            XCTAssertEqual(sqlite3_exec(connection, "COMMIT", nil, nil, nil), SQLITE_OK)
        }
        await fulfillment(of: [lockEntered], timeout: 1)
        // A write-only command waits for SQLite rather than attempting a read-snapshot upgrade.
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        let markerStore = RideSessionMarkerStore(database: database)
        let writeFinished = Mutex(false)
        markerStore.save(Data([1]))
        let write = Task {
            await markerStore.waitForPersistence()
            writeFinished.withLock { $0 = true }
        }
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertFalse(writeFinished.withLock { $0 })
        let first = Task.detached { try state.finishLocationCallback(callbacks[0]) }
        let entered = expectation(description: "65th callback enters native recording owner")
        let admitted = expectation(description: "65th callback admitted after durable capacity returns")
        let next = Mutex<MobileRideMapLocationCallback?>(nil)
        let adapter = CutoutSessionPhoneLocationAdapter(
            clock: MonotonicClock { MonotonicMilliseconds(65_000) },
            wallClock: { Date(timeIntervalSince1970: 1_700_000_064) },
            onSnapshot: { _, _ in },
            onLocationUpdate: { update in
                XCTAssertFalse(Thread.isMainThread)
                entered.fulfill()
                do {
                    let callback = try state.admitLocationCallback(
                        receiptMonotonicMs: update.receiptMonotonic.rawValue,
                        receiptWallClockUnixMs: try XCTUnwrap(unixMilliseconds(for: update.receiptWallClock)),
                        samples: update.samples
                    )
                    next.withLock { $0 = callback }
                } catch { XCTFail("Lossless native admission failed: \(error)") }
                admitted.fulfill()
            }, onAvailabilityChange: {},
            managerFactory: {
                XCTAssertFalse(Thread.isMainThread)
                return CLLocationManager()
            }, servicesEnabledQuery: { true }
        )
        let location = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 40.0064, longitude: -105), altitude: 0,
            horizontalAccuracy: 1, verticalAccuracy: 1, course: 0, speed: 8,
            timestamp: Date(timeIntervalSince1970: 1_700_000_064)
        )
        adapter.deliverLocationsForTesting([location])
        await fulfillment(of: [entered], timeout: 1)
        XCTAssertNil(next.withLock { $0 }, "The 64 occupied slots retain producer backpressure")
        adapter.updateDemand(.record)
        adapter.updateDemand(.idle)
        let heartbeat = expectation(description: "Main queue advances while the producer is blocked")
        DispatchQueue.main.async { heartbeat.fulfill() }
        await fulfillment(of: [heartbeat], timeout: 1)
        XCTAssertFalse(unlocked.withLock { $0 }, "Main progress must precede actual SQLite release")
        release.signal()
        await write.value
        var outcomes = try await first.value
        outcomes += try await Task.detached {
            try callbacks.dropFirst().flatMap { try state.finishLocationCallback($0) }
        }.value
        await fulfillment(of: [admitted], timeout: 2)
        let last = try XCTUnwrap(next.withLock { $0 })
        outcomes += try await Task.detached { try state.finishLocationCallback(last) }.value
        let points = outcomes.compactMap { outcome -> MobileRideMapPointDto? in
            if case let .accepted(point) = outcome.decision { return point }
            if case let .storageError(message) = outcome.decision { XCTFail(message) }
            return nil
        }
        XCTAssertEqual(points.count, 65)
        XCTAssertEqual(Set(outcomes.compactMap(\.requestID)).count, 65)
        for (index, point) in points.enumerated() {
            XCTAssertEqual(point.sequence, UInt64(index))
            XCTAssertEqual(point.monotonicMs, UInt64(1_000 + index * 1_000))
            XCTAssertEqual(point.wallClockUnixMs, UInt64(1_700_000_000_000 + index * 1_000))
            XCTAssertEqual(point.latitudeDegrees, Self.sample(index).latitudeDegrees, accuracy: 0.0000001)
        }
        let repeated = try await state.checkpoint()
        XCTAssertTrue(repeated.isEmpty, "Settled receipts are emitted once")
        try await Task.detached {
            XCTAssertEqual(try state.pointsAfter(afterCursor: nil, limit: 256).points.count, 65)
            _ = try state.stop(atMs: 66_000)
            _ = try state.discard()
        }.value
        try markerStore.clear()
        await markerStore.waitForPersistence()
    }

    private static func sample(_ index: Int) -> MobilePhoneLocationSampleDto {
        MobilePhoneLocationSampleDto(
            wallClockUnixMs: UInt64(1_700_000_000_000 + index * 1_000), sourceTimestampUnixSeconds: nil,
            latitudeDegrees: 40 + Double(index) / 10_000, longitudeDegrees: -105,
            altitudeMeters: 0, horizontalAccuracyMeters: 1, verticalAccuracyMeters: 1,
            speedMetersPerSecond: 8, speedAccuracyMetersPerSecond: 1,
            courseDegrees: 0, courseAccuracyDegrees: 1
        )
    }
}
