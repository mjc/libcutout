import Foundation
import Synchronization
import XCTest

@testable import CutoutMobile

final class RustPersistenceStoreTests: XCTestCase {
    func testExistingDatabaseAndSidecarsReceiveLockSafeProtectionBeforeRustIsAskedToOpenThem() async throws {
        let fileManager = ProtectionRecordingFileManager()
        let directory = fileManager.temporaryDirectory.appendingPathComponent(
            "RustPersistenceStoreTests.protection.\(UUID().uuidString)", isDirectory: true
        )
        try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? fileManager.removeItem(at: directory) }
        let databaseURL = directory.appendingPathComponent("ride.sqlite")
        let paths = [databaseURL.path, databaseURL.path + "-wal", databaseURL.path + "-shm"]
        for path in paths { try Data().write(to: URL(fileURLWithPath: path)) }

        let protectedPathsAtOpen = Mutex<[String]>([])
        do {
            _ = try await RustPersistenceStore.open(
                at: databaseURL, fileManager: fileManager,
                openDatabase: { _ in
                    protectedPathsAtOpen.withLock { paths in
                        paths = fileManager.protectionRecords.withLock { $0.map(\.path) }
                    }
                    throw BoundaryOpenStopped.expected
                }
            )
            XCTFail("controlled opener must stop after observing native metadata")
        } catch {
            XCTAssertEqual(error as? BoundaryOpenStopped, .expected)
        }
        XCTAssertTrue(Set(paths).isSubset(of: Set(protectedPathsAtOpen.withLock { $0 })))
        let records = fileManager.protectionRecords.withLock { $0 }
        for path in paths {
            let first = try XCTUnwrap(records.first { $0.path == path })
            XCTAssertEqual(first.protection, .completeUntilFirstUserAuthentication)
            XCTAssertEqual(first.contentsBefore, Data(), "existing file must be protected before SQLite opens it")
            XCTAssertEqual(first.contentsAfter, first.contentsBefore, "protection must not change recorded bytes")
        }
    }

    func testSidecarProtectionFailurePreventsOpeningDatabaseWithoutChangingContents() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "RustPersistenceStoreTests.failed-protection.\(UUID().uuidString)", isDirectory: true
        )
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let databaseURL = directory.appendingPathComponent("ride.sqlite")
        let walURL = URL(fileURLWithPath: databaseURL.path + "-wal")
        try Data().write(to: databaseURL)
        try Data().write(to: walURL)
        let fileManager = ProtectionRecordingFileManager(failProtectionPath: walURL.path)
        let openingInvoked = Mutex(false)
        do {
            _ = try await RustPersistenceStore.open(
                at: databaseURL, fileManager: fileManager,
                openDatabase: { _ in
                    openingInvoked.withLock { $0 = true }
                    throw BoundaryOpenStopped.expected
                }
            )
            XCTFail("storage must not open with an unprotected existing WAL")
        } catch {
            XCTAssertEqual((error as? CocoaError)?.code, .fileWriteNoPermission)
        }
        XCTAssertFalse(openingInvoked.withLock { $0 })
        XCTAssertEqual(try Data(contentsOf: databaseURL), Data())
        XCTAssertEqual(try Data(contentsOf: walURL), Data())
    }

    func testDatabaseDirectoryIsExcludedBeforeSidecarsAreCreated() throws {
        let fileManager = FileManager.default
        let temporaryRoot = fileManager.temporaryDirectory
            .appendingPathComponent(
                "RustPersistenceStoreTests.\(UUID().uuidString)", isDirectory: true
            )
        let databaseDirectory = temporaryRoot.appendingPathComponent("Cutout", isDirectory: true)
        try fileManager.createDirectory(at: databaseDirectory, withIntermediateDirectories: true)
        defer { try? fileManager.removeItem(at: temporaryRoot) }

        try RustPersistenceStore.excludeFromBackup(at: databaseDirectory)

        let sidecarNames = ["ride.sqlite-wal", "ride.sqlite-shm"]
        for name in sidecarNames {
            try Data().write(to: databaseDirectory.appendingPathComponent(name))
        }

        let resourceValues = try databaseDirectory.resourceValues(
            forKeys: [.isExcludedFromBackupKey]
        )
        XCTAssertEqual(resourceValues.isExcludedFromBackup, true)
        for name in sidecarNames {
            XCTAssertTrue(
                fileManager.fileExists(atPath: databaseDirectory.appendingPathComponent(name).path)
            )
        }
    }
}

private enum BoundaryOpenStopped: Error, Equatable, Sendable {
    case expected
}

private final class ProtectionRecordingFileManager: FileManager, @unchecked Sendable {
    struct Record: Sendable {
        let path: String
        let protection: FileProtectionType
        let contentsBefore: Data?
        let contentsAfter: Data?
    }

    let protectionRecords = Mutex<[Record]>([])
    private let failProtectionPath: String?

    init(failProtectionPath: String? = nil) {
        self.failProtectionPath = failProtectionPath
        super.init()
    }

    override func setAttributes(_ attributes: [FileAttributeKey: Any], ofItemAtPath path: String) throws {
        let before = try? Data(contentsOf: URL(fileURLWithPath: path))
        if path == failProtectionPath, attributes[.protectionKey] != nil {
            throw CocoaError(.fileWriteNoPermission)
        }
        try super.setAttributes(attributes, ofItemAtPath: path)
        guard let protection = attributes[.protectionKey] as? FileProtectionType else { return }
        let after = try? Data(contentsOf: URL(fileURLWithPath: path))
        protectionRecords.withLock {
            $0.append(Record(path: path, protection: protection, contentsBefore: before, contentsAfter: after))
        }
    }
}
