import Foundation
import XCTest

final class MusicCommandAlertOwnershipTests: XCTestCase {
    func testMusicCommandFeedbackHasOneAlertOwnerAtTheAppRoot() throws {
        let testFile = URL(fileURLWithPath: #filePath)
        let appSources =
            testFile
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Apps/CutoutApp")
        let sources = try FileManager.default.contentsOfDirectory(
            at: appSources,
            includingPropertiesForKeys: nil
        ).filter { $0.pathExtension == "swift" }
        let owners = try sources.filter {
            try String(contentsOf: $0, encoding: .utf8).contains("music.command.title")
        }

        XCTAssertEqual(owners.map(\.lastPathComponent), ["CutoutApp.swift"])
    }
}
