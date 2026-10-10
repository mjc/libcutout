import CutoutMobileFFI
import Foundation
import Synchronization
import XCTest

@testable import CutoutMobile

final class CutoutSessionRideMapRecorderTests: XCTestCase {
    func testConnectionAdmissionPublishesItsReceiptWithoutStartingAPollingLoop() async throws {
        let state = MobileRideMapState()
        let started = try await state.startGpsOnlyCommand(atMs: 1_000, musicHistoryPolicy: .disabled)
        // The process-wide test database can retain the last selected vehicle. Exercise
        // admission for this ride's actual candidate, rather than a conflicting identity.
        let persisted = try XCTUnwrap(try state.storedHistoryRide(rideID: started.rideID))
        let platformIdentifier = persisted.candidateVehicle ?? "recorded-wheel"
        XCTAssertNil(started.associatedVehicle)
        defer {
            _ = try? state.stop(atMs: 1_003)
            _ = try? state.discard()
        }
        let connection = CutoutSessionStateHandle()
        let token = try XCTUnwrap(
            connection.beginConnectionAttempt(platformIdentifier: platformIdentifier, nowMs: 1_000).token)
        _ = connection.connectionLinkEstablished(token: token)
        _ = connection.observeConnectionNotification(
            token: token,
            bytes: Data([
                2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0, 38, 208, 3,
            ])
        )
        let resolved = connection.resolveDeviceSession(token: token, identificationComplete: true, nowMs: 1_001)
        XCTAssertEqual(resolved.connection.readiness, .verified)
        XCTAssertEqual(resolved.connection.token, token)
        XCTAssertNotNil(resolved.identity)
        let published = expectation(description: "connection receipt publishes without an idle timer")
        let snapshots = Mutex<[MobileRideMapSnapshotDto]>([])
        let recorder = makeRecorder(
            state: state,
            publishSnapshot: { snapshot in
                let isFirstReceipt = snapshots.withLock { snapshots in
                    snapshots.append(snapshot)
                    return snapshots.count == 1
                }
                if isFirstReceipt { published.fulfill() }
            })

        recorder.observeConnection(
            at: MonotonicMilliseconds(1_002), token: token, speedObservation: nil,
            connectionState: connection, resetTripMeter: { _ in }
        )
        await fulfillment(of: [published], timeout: 3)
        withExtendedLifetime(recorder) {}
        XCTAssertEqual(snapshots.withLock { $0.count }, 1)
        XCTAssertEqual(snapshots.withLock { $0.last?.rideID }, started.rideID)
        XCTAssertEqual(snapshots.withLock { $0.last?.associatedVehicle }, platformIdentifier)
        XCTAssertEqual(snapshots.withLock { $0.last?.state }, .active)
    }

    func testNativeCallbacksPublishAllOrderedMaterialPointsWithoutPolling() async throws {
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 1_000, musicHistoryPolicy: .disabled)
        let published = expectation(description: "each location receipt publishes directly")
        published.expectedFulfillmentCount = 8
        let outcomes = Mutex<[MobileRideMapOutcomeDto]>([])
        let recorder = makeRecorder(
            state: state,
            publishDecisions: { batch in
                outcomes.withLock { $0.append(contentsOf: batch.outcomes) }
                published.fulfill()
            })
        for index in 0..<8 {
            let wallClock = 1_700_000_000_000 + UInt64(index) * 1_000
            recorder.ingestLocation(
                PhoneLocationUpdate(
                    receiptMonotonic: MonotonicMilliseconds(1_000 + UInt64(index) * 1_000),
                    receiptWallClock: Date(timeIntervalSince1970: Double(wallClock) / 1_000),
                    samples: [
                        MobilePhoneLocationSampleDto(
                            wallClockUnixMs: wallClock, sourceTimestampUnixSeconds: nil,
                            latitudeDegrees: 40.0 + Double(index) * 0.00002, longitudeDegrees: -105,
                            altitudeMeters: 1_600, horizontalAccuracyMeters: 3, verticalAccuracyMeters: nil,
                            speedMetersPerSecond: 2, speedAccuracyMetersPerSecond: nil,
                            courseDegrees: nil, courseAccuracyDegrees: nil
                        )
                    ]
                ))
        }
        await fulfillment(of: [published], timeout: 3)
        withExtendedLifetime(recorder) {}
        let settled = outcomes.withLock { $0 }
        let pending = settled.filter {
            if case .pending = $0.decision { return true }
            return false
        }
        let accepted = settled.filter {
            if case .accepted = $0.decision { return true }
            return false
        }
        XCTAssertEqual(pending.count, 8)
        XCTAssertEqual(accepted.count, 8)
        XCTAssertEqual(pending.map(\.requestID), accepted.map(\.requestID))
        XCTAssertEqual(Set(accepted.compactMap(\.requestID)).count, 8)
        XCTAssertEqual(accepted.map(\.snapshot.summary.pointCount), (1...8).map(UInt64.init))
        for outcome in settled {
            switch outcome.decision {
            case .pending, .accepted, .ignored: break
            case .rejected, .storageError: XCTFail("material sample failed: \(outcome.decision)")
            }
        }
        for (index, pair) in zip(pending, accepted).enumerated() {
            let (admitted, outcome) = pair
            guard case let .accepted(point) = outcome.decision else { return XCTFail("material sample was lost") }
            guard case let .pending(pendingPoint) = admitted.decision else {
                return XCTFail("missing admission receipt")
            }
            XCTAssertEqual(point, pendingPoint)
            XCTAssertEqual(point.sequence, UInt64(index))
            XCTAssertEqual(point.monotonicMs, 1_000 + UInt64(index) * 1_000)
            XCTAssertEqual(point.wallClockUnixMs, 1_700_000_000_000 + UInt64(index) * 1_000)
            XCTAssertEqual(point.latitudeDegrees, 40.0 + Double(index) * 0.00002)
        }
        try await recorder.checkpoint()
        XCTAssertEqual(outcomes.withLock { $0 }, settled, "checkpoint must not repeat published receipts")
    }

    private func makeRecorder(
        state: MobileRideMapState,
        publishSnapshot: @escaping @Sendable (MobileRideMapSnapshotDto) -> Void = { _ in },
        publishDecisions: @escaping @Sendable (RideMapDecisionBatch) -> Void = { _ in }
    ) -> CutoutSessionRideMapRecorder {
        CutoutSessionRideMapRecorder(
            state: state, clock: MonotonicClock(now: { MonotonicMilliseconds(1_000) }),
            wallClock: { Date(timeIntervalSince1970: 1_700_000_000) },
            publishSnapshot: publishSnapshot, publishDecisions: publishDecisions,
            publishError: { error, _ in XCTFail("unexpected map error: \(error)") },
            publishAvailability: { _, _ in }, recordDiagnostic: { _ in }, onLocationDemandChanged: {}
        )
    }
}
