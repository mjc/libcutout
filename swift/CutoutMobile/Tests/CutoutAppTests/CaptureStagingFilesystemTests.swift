#if os(iOS)
    import CryptoKit
    import CutoutMobileFFI
    import Foundation
    import XCTest

    @testable import CutoutMobile

    final class CaptureStagingFilesystemTests: XCTestCase {
        func testDatabaseDirectoryPolicyAndSidecarCreation() async throws {
            let directory = try privateDirectory(.libraryDirectory)
            defer { try? FileManager.default.removeItem(at: directory) }
            try FileManager.default.setAttributes(
                [.protectionKey: FileProtectionType.complete], ofItemAtPath: directory.path
            )
            let path = directory.appendingPathComponent("ride.sqlite")
            let database = try await RustPersistenceStore.open(at: path)
            defer { try? database.shutdown() }
            let markerStore = RideSessionMarkerStore(database: database)
            markerStore.save(Data([1, 2, 3]))
            await markerStore.waitForPersistence()

            XCTAssertEqual(
                try directory.resourceValues(forKeys: [.isExcludedFromBackupKey]).isExcludedFromBackup,
                true
            )
            for name in ["ride.sqlite", "ride.sqlite-wal", "ride.sqlite-shm"] {
                let file = directory.appendingPathComponent(name)
                XCTAssertTrue(FileManager.default.fileExists(atPath: file.path), name)
                #if !targetEnvironment(simulator)
                    // Simulator omits NSFileProtectionKey entirely. Only a
                    // physical run can verify the inherited protection class.
                    let attributes = try FileManager.default.attributesOfItem(atPath: file.path)
                    let protection =
                        (attributes[.protectionKey] as? String)
                        ?? (attributes[.protectionKey] as? FileProtectionType)?.rawValue
                    XCTAssertEqual(
                        protection,
                        FileProtectionType.completeUntilFirstUserAuthentication.rawValue,
                        name
                    )
                #endif
            }
        }

        func testDocumentsHeaderRewritePreservesRecordsPermissionsAndAppendWriter() throws {
            let directory = try privateDirectory(.documentDirectory)
            defer { try? FileManager.default.removeItem(at: directory) }
            let capture = directory.appendingPathComponent("capture.jsonl")
            let sibling = directory.appendingPathComponent("capture.jsonl.tmp")
            let sentinel = Data("unrelated sibling\n".utf8)
            try sentinel.write(to: sibling)
            let builder = captureBuilder()
            defer { _ = builder.finishWriter() }
            XCTAssertTrue(builder.startWriter(path: capture.path))
            XCTAssertEqual(builder.recordLinkUp(monotonicMs: timestamp(1_001), maxWriteLen: nil), .accepted)
            XCTAssertEqual(
                builder.recordNotification(
                    monotonicMs: timestamp(1_002),
                    characteristic: Data([0xFF, 0xE1]),
                    service: Data([0xFF, 0xE0]),
                    bytes: Data([0, 15, 171, 255])
                ),
                .accepted
            )
            XCTAssertEqual(builder.flushWriterOutcome(), .flushed)
            let original = try Data(contentsOf: capture)
            let originalBody = try body(original)
            let originalMode = try mode(capture)

            XCTAssertEqual(builder.addAnnotation(annotation: "runtime=ios-filesystem"), .accepted)
            XCTAssertEqual(builder.flushWriterOutcome(), .flushed)
            let rewritten = try Data(contentsOf: capture)
            XCTAssertNotEqual(rewritten, original)
            XCTAssertEqual(try body(rewritten), originalBody)
            XCTAssertEqual(try mode(capture), originalMode)
            XCTAssertEqual(try Data(contentsOf: sibling), sentinel)
            let newline = try XCTUnwrap(rewritten.firstIndex(of: 10))
            let line = try XCTUnwrap(JSONSerialization.jsonObject(with: rewritten[..<newline]) as? [String: Any])
            let header = try XCTUnwrap(line["header"] as? [String: Any])
            XCTAssertEqual(header["annotations"] as? [String], ["runtime=ios-filesystem"])

            XCTAssertEqual(builder.recordLinkDown(monotonicMs: timestamp(1_003)), .accepted)
            XCTAssertTrue(builder.finishWriter())
            let finished = try Data(contentsOf: capture)
            XCTAssertTrue(try body(finished).starts(with: originalBody))
            XCTAssertGreaterThan(finished.count, rewritten.count)
            XCTAssertEqual(
                try FileManager.default.contentsOfDirectory(atPath: directory.path).sorted(),
                ["capture.jsonl", "capture.jsonl.tmp"])
            let databaseDirectory = try privateDirectory(.libraryDirectory)
            defer { try? FileManager.default.removeItem(at: databaseDirectory) }
            let database = try openRideDatabase(path: databaseDirectory.appendingPathComponent("verify.sqlite").path)
            defer { try? database.shutdown() }
            let preview = try database.preflightPevcap(path: capture.path, encoding: .jsonl)
            XCTAssertEqual(preview.artifactDigest, sha256(finished))
            XCTAssertEqual(preview.recordCount, 3)
        }

        func testLibraryManagedImportPublishesReadonlyBytesAndDuplicateKeepsInode() throws {
            let documents = try privateDirectory(.documentDirectory)
            let library = try privateDirectory(.libraryDirectory)
            defer {
                try? FileManager.default.removeItem(at: documents)
                try? FileManager.default.removeItem(at: library)
            }
            let capture = try finishedCapture(in: documents)
            let bytes = try Data(contentsOf: capture)
            let database = try openRideDatabase(path: library.appendingPathComponent("import.sqlite").path)
            defer { try? database.shutdown() }
            let preview = try database.preflightPevcap(path: capture.path, encoding: .jsonl)
            XCTAssertEqual(preview.artifactDigest, sha256(bytes))
            let receipt = try database.confirmPevcapImport(preview: preview, createdAtMilliseconds: 1_700_000_000_100)
            XCTAssertFalse(receipt.duplicate)
            let managed = URL(fileURLWithPath: receipt.managedArtifactPath)
            XCTAssertTrue(managed.path.hasPrefix(library.path + "/"))
            XCTAssertEqual(try Data(contentsOf: managed), bytes)
            XCTAssertEqual(try mode(managed) & 0o222, 0)
            let originalInode = try inode(managed)
            let refreshed = try database.preflightPevcap(path: capture.path, encoding: .jsonl)
            let duplicate = try database.confirmPevcapImport(
                preview: refreshed, createdAtMilliseconds: 1_700_000_000_101)
            XCTAssertTrue(duplicate.duplicate)
            XCTAssertEqual(duplicate.managedArtifactPath, receipt.managedArtifactPath)
            XCTAssertEqual(try inode(managed), originalInode)
            XCTAssertEqual(try Data(contentsOf: managed), bytes)
            XCTAssertEqual(
                try FileManager.default.contentsOfDirectory(atPath: managed.deletingLastPathComponent().path),
                [managed.lastPathComponent])
        }

        func testLibraryManagedImportReusesExistingArtifactWithoutReplacingIt() throws {
            let documents = try privateDirectory(.documentDirectory)
            let library = try privateDirectory(.libraryDirectory)
            defer {
                try? FileManager.default.removeItem(at: documents)
                try? FileManager.default.removeItem(at: library)
            }
            let capture = try finishedCapture(in: documents)
            let databasePath = library.appendingPathComponent("existing.sqlite").path
            let database = try openRideDatabase(path: databasePath)
            defer { try? database.shutdown() }
            let preview = try database.preflightPevcap(path: capture.path, encoding: .jsonl)
            let managedDirectory = URL(fileURLWithPath: databasePath + ".pevcap-imports", isDirectory: true)
            try FileManager.default.createDirectory(at: managedDirectory, withIntermediateDirectories: false)
            let managed = managedDirectory.appendingPathComponent(preview.artifactDigest + ".jsonl")
            let bytes = try Data(contentsOf: capture)
            XCTAssertEqual(preview.artifactDigest, sha256(bytes))
            try bytes.write(to: managed)
            try FileManager.default.setAttributes([.posixPermissions: 0o440], ofItemAtPath: managed.path)
            let originalInode = try inode(managed)
            let receipt = try database.confirmPevcapImport(preview: preview, createdAtMilliseconds: 1_700_000_000_100)
            XCTAssertFalse(receipt.duplicate)
            XCTAssertEqual(receipt.managedArtifactPath, managed.path)
            XCTAssertEqual(try inode(managed), originalInode)
            XCTAssertEqual(try mode(managed), 0o440)
            XCTAssertEqual(try Data(contentsOf: managed), bytes)
            XCTAssertEqual(
                try FileManager.default.contentsOfDirectory(atPath: managedDirectory.path), [managed.lastPathComponent])
        }

        private func privateDirectory(_ kind: FileManager.SearchPathDirectory) throws -> URL {
            XCTAssertTrue(
                NSHomeDirectory().contains("/Containers/Data/Application/"),
                "Filesystem runtime tests require an application sandbox: \(NSHomeDirectory())"
            )
            let parent = try XCTUnwrap(FileManager.default.urls(for: kind, in: .userDomainMask).first)
            XCTAssertTrue(parent.path.hasPrefix(NSHomeDirectory() + "/"))
            let directory = parent.appendingPathComponent("cutout-staging-test-" + UUID().uuidString, isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            print("Capture staging runtime: \(ProcessInfo.processInfo.operatingSystemVersionString); \(directory.path)")
            return directory
        }

        private func captureBuilder() -> MobilePevcapCaptureBuilder {
            let builder = MobilePevcapCaptureBuilder(
                wallClockStartUnixMs: MobileWallClockUnixMillisDto(milliseconds: 1_700_000_000_000),
                platformId: "ios-staging-test",
                writeLimit: nil
            )
            XCTAssertTrue(builder.setCaptureStartMonotonicMs(monotonicMs: 1_000))
            return builder
        }

        private func finishedCapture(in directory: URL) throws -> URL {
            let capture = directory.appendingPathComponent("source.jsonl")
            let builder = captureBuilder()
            defer { _ = builder.finishWriter() }
            XCTAssertTrue(builder.startWriter(path: capture.path))
            XCTAssertEqual(builder.recordLinkUp(monotonicMs: timestamp(1_001), maxWriteLen: nil), .accepted)
            XCTAssertEqual(builder.recordLinkDown(monotonicMs: timestamp(1_002)), .accepted)
            XCTAssertTrue(builder.finishWriter())
            return capture
        }

        private func timestamp(_ milliseconds: UInt64) -> MobileMonotonicMillisDto {
            MobileMonotonicMillisDto(milliseconds: milliseconds)
        }

        private func sha256(_ data: Data) -> String {
            SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        }

        private func body(_ data: Data) throws -> Data {
            let newline = try XCTUnwrap(data.firstIndex(of: 10))
            return data[data.index(after: newline)...]
        }

        private func mode(_ url: URL) throws -> UInt16 {
            let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
            return try XCTUnwrap(attributes[.posixPermissions] as? NSNumber).uint16Value
        }

        private func inode(_ url: URL) throws -> UInt64 {
            let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
            return try XCTUnwrap(attributes[.systemFileNumber] as? NSNumber).uint64Value
        }
    }
#endif
