import Foundation
import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class RustPersistenceStoreTests: XCTestCase {
    func testTransientFilesystemFailureCanRetryAndPreserveRecording() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("cutout-bootstrap-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let blockedDirectory = directory.appendingPathComponent("storage", isDirectory: true)
        let databaseURL = blockedDirectory.appendingPathComponent("rides.sqlite")
        try Data("temporary path obstruction".utf8).write(to: blockedDirectory)

        do {
            _ = try await RustPersistenceStore.open(at: databaseURL)
            XCTFail("An obstructed directory must fail instead of installing a nil database permanently")
        } catch {
            XCTAssertTrue(FileManager.default.fileExists(atPath: blockedDirectory.path))
        }
        try FileManager.default.removeItem(at: blockedDirectory)

        let database: RideDatabaseHandle
        do {
            database = try await RustPersistenceStore.open(at: databaseURL)
        } catch MobileRideDatabaseError.AlreadyOpenForDifferentPath {
            throw XCTSkip("Run RustPersistenceStoreTests alone: another fixture owns the process-wide database")
        }
        defer { try? database.shutdown() }
        let recording = MobileRideMapState(database: database)
        let started = try recording.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        let reopened = try await RustPersistenceStore.open(at: databaseURL)
        let recovered = MobileRideMapState(database: reopened)
        XCTAssertNil(recovered.initializationError)
        XCTAssertEqual(recovered.currentSnapshot(atMs: 1_100)?.rideID, started.rideID)
    }
}
