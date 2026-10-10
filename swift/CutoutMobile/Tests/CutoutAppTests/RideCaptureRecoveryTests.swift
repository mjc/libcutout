import CryptoKit
import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

final class RideCaptureRecoveryTests: XCTestCase {
    func testRecoveryChecksReviewedBytesPreservesExistingRideAndIsIdempotent() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let state = MobileRideMapState()
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        let original = try state.startGpsOnly(atMs: 1_000)
        _ = try state.stop(atMs: 2_000)
        _ = try state.save()
        let now = UInt64(Date().timeIntervalSince1970 * 1_000)
        let createdAt = now
        let source = try capture(in: directory, startingAt: createdAt)
        let bytes = try Data(contentsOf: source)
        let digest = SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
        let previousIDs = try state.storedSummaries(limit: 50).map(\.rideID)

        XCTAssertThrowsError(
            try RideCaptureRecovery.run(
                database: database, source: source, createdAtMilliseconds: createdAt,
                expectedDigest: String(repeating: "0", count: 64), nowMilliseconds: now
            ))
        XCTAssertEqual(try state.storedSummaries(limit: 50).map(\.rideID), previousIDs)

        let imported = try RideCaptureRecovery.run(
            database: database, source: source, createdAtMilliseconds: createdAt,
            expectedDigest: digest, nowMilliseconds: now
        )
        XCTAssertFalse(imported.duplicate)
        XCTAssertEqual(imported.pointCount, 3)
        XCTAssertGreaterThan(imported.distanceMillimetres, 0)
        XCTAssertEqual(imported.createdAtMilliseconds, createdAt)
        XCTAssertEqual(imported.previousRideIDs, previousIDs)
        XCTAssertTrue(imported.recentRideIDs.contains(original.rideID))
        XCTAssertTrue(imported.recentRideIDs.contains(imported.rideID))
        XCTAssertEqual(try state.storedHistoryRide(rideID: original.rideID)?.state, .saved)
        XCTAssertEqual(try Data(contentsOf: source), bytes)

        let repeated = try RideCaptureRecovery.run(
            database: database, source: source, createdAtMilliseconds: createdAt,
            expectedDigest: digest, nowMilliseconds: now
        )
        XCTAssertTrue(repeated.duplicate)
        XCTAssertEqual(repeated.rideID, imported.rideID)
        XCTAssertEqual(repeated.recentRideIDs, imported.recentRideIDs)
    }

    func testRecoveryKeepsOriginalDateAndDoesNotAddAnOldRideToRecentHistory() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        let now = UInt64(Date().timeIntervalSince1970 * 1_000)
        let createdAt = now - 31 * 24 * 60 * 60 * 1_000
        let source = try capture(in: directory, startingAt: createdAt)
        let digest = SHA256.hash(data: try Data(contentsOf: source)).map { String(format: "%02x", $0) }.joined()
        let imported = try RideCaptureRecovery.run(
            database: database, source: source, createdAtMilliseconds: createdAt,
            expectedDigest: digest, nowMilliseconds: now
        )
        XCTAssertEqual(imported.createdAtMilliseconds, createdAt)
        XCTAssertEqual(imported.pointCount, 3)
        XCTAssertFalse(imported.recentRideIDs.contains(imported.rideID))
        XCTAssertEqual(
            try MobileRideMapState(database: database).storedHistoryRide(rideID: imported.rideID)?.summary.pointCount, 3
        )
    }

    private func capture(in directory: URL, startingAt start: UInt64) throws -> URL {
        let source = directory.appendingPathComponent("recovery.jsonl")
        let builder = MobilePevcapCaptureBuilder(
            wallClockStartUnixMs: MobileWallClockUnixMillisDto(milliseconds: start),
            platformId: "recovery-test-wheel", writeLimit: nil
        )
        XCTAssertTrue(builder.setCaptureStartMonotonicMs(monotonicMs: 1_000))
        XCTAssertTrue(builder.startWriter(path: source.path))
        defer { _ = builder.finishWriter() }
        for index in 0..<3 {
            let offset = UInt64(index) * 1_000
            XCTAssertEqual(
                builder.recordLocationSample(
                    receiptMonotonicMs: MobileMonotonicMillisDto(milliseconds: 1_000 + offset),
                    sample: MobilePhoneLocationSampleDto(
                        wallClockUnixMs: start + offset, sourceTimestampUnixSeconds: nil,
                        latitudeDegrees: 39.7 + Double(index) * 0.0001, longitudeDegrees: -104.9,
                        altitudeMeters: 1_600, horizontalAccuracyMeters: 5,
                        verticalAccuracyMeters: nil, speedMetersPerSecond: nil,
                        speedAccuracyMetersPerSecond: nil, courseDegrees: nil, courseAccuracyDegrees: nil
                    ), simulated: nil, producedByAccessory: nil
                ), .accepted)
        }
        XCTAssertTrue(builder.finishWriter())
        return source
    }
}
