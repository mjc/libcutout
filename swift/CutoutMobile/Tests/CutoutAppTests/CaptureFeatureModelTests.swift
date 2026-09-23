import XCTest
import Foundation
@testable import CutoutApp
import CutoutMobile

@MainActor
final class CaptureFeatureModelTests: XCTestCase {
    func testLabelsAreNotAcceptedBeforeTheWriterStartsOrAfterItCompletes() {
        let capture = CaptureFeatureModel()
        var annotations: [String] = []
        capture.startLabel(.ride, annotate: { annotations.append($0) })
        XCTAssertTrue(annotations.isEmpty)
        let url = URL(fileURLWithPath: "/tmp/labels.jsonl")
        capture.apply(.started(fileURL: url))
        capture.apply(.finished(fileURL: url))
        capture.startLabel(.ride, annotate: { annotations.append($0) })
        XCTAssertTrue(annotations.isEmpty)
        XCTAssertTrue(capture.activeLabels.isEmpty)
        XCTAssertEqual(capture.status, .saved(fileName: "labels.jsonl"))
    }

    func testSameModelDevicesKeepAdvertisementAndPlatformIdentity() {
        let first = row(id: "device-a", advertisement: "GotWay_A")
        let second = row(id: "device-b", advertisement: "GotWay_B")
        let capture = CaptureFeatureModel()
        capture.device = CaptureDeviceIdentity(row: first)
        capture.isUserInitiated = true
        let url = URL(fileURLWithPath: "/tmp/falcon.jsonl")
        capture.apply(.started(fileURL: url))
        capture.updateDevice(second)
        capture.apply(.finished(fileURL: url))

        XCTAssertEqual(capture.completed.first?.device?.title, "Begode Falcon")
        XCTAssertEqual(capture.completed.first?.device?.advertisedName, "GotWay_A")
        XCTAssertEqual(capture.completed.first?.device?.platformIdentifier, "device-a")
        XCTAssertEqual(capture.completed.first?.isUserInitiated, true)
        XCTAssertNotEqual(CaptureDeviceIdentity(row: first), CaptureDeviceIdentity(row: second))
    }

    func testDetectedIdentityRefreshReachesCompletedArtifactWithoutAnotherPacket() {
        let capture = CaptureFeatureModel()
        capture.device = CaptureDeviceIdentity(row: row(id: "device-a", advertisement: "Old advertisement"))
        let url = URL(fileURLWithPath: "/tmp/automatic.jsonl")
        capture.apply(.started(fileURL: url))
        capture.updateDevice(row(id: "device-a", advertisement: "GotWay_A"))
        capture.apply(.finished(fileURL: url))
        XCTAssertEqual(capture.completed.first?.device?.advertisedName, "GotWay_A")
        XCTAssertEqual(capture.completed.first?.isUserInitiated, false)
    }

    func testRecordingSummaryDoesNotExposeFileNamesOrLabelDiagnosticText() {
        let capture = CaptureFeatureModel()
        capture.apply(.started(fileURL: URL(fileURLWithPath: "/tmp/private-name.jsonl")))
        XCTAssertEqual(capture.recordingSummary, "Waiting for Bluetooth data")
        capture.apply(.notificationRecorded)
        capture.startLabel(.ride, annotate: { _ in })
        XCTAssertEqual(capture.recordingSummary, "Receiving Bluetooth data")
        capture.isFinishing = true
        XCTAssertEqual(capture.recordingSummary, "Saving…")
    }

    private func row(id: String, advertisement: String) -> DevicePickerRow {
        DevicePickerRow(
            id: id, title: "Begode Falcon", subtitle: "", detail: "",
            state: .supported(action: "Connect"), symbolName: "circle",
            advertisedName: advertisement
        )
    }

    func testFinishedCaptureRetainsArtifactAndOldCompletionDoesNotReplaceNewRecording() {
        let capture = CaptureFeatureModel()
        let first = CaptureGeneration(rawValue: 1)
        let second = CaptureGeneration(rawValue: 2)
        let firstURL = URL(fileURLWithPath: "/tmp/first.jsonl")
        let secondURL = URL(fileURLWithPath: "/tmp/second.jsonl")
        capture.apply(.started(generation: first, fileURL: firstURL))
        capture.apply(.started(generation: second, fileURL: secondURL))
        capture.apply(.finished(generation: first, fileURL: firstURL))

        XCTAssertEqual(capture.activeGeneration, second)
        XCTAssertEqual(capture.status, .recordingLocally(fileName: "second.jsonl"))
        XCTAssertEqual(capture.completed.first?.fileURL, firstURL)
        XCTAssertEqual(capture.completed.first?.outcome, .saved)
    }

    func testFailureRetainsPartialArtifactWithoutCallingItSaved() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/partial.jsonl")
        capture.apply(.started(fileURL: url))
        capture.apply(.failed)
        XCTAssertEqual(capture.completed.first?.fileURL, url)
        XCTAssertEqual(capture.completed.first?.outcome, .failed)
        XCTAssertEqual(capture.status, .failed)
        XCTAssertNil(capture.activeGeneration)
    }

    func testNewGenerationResetsProgressLabelsAndFinishAdmission() {
        let capture = CaptureFeatureModel()
        capture.apply(.started(generation: .init(rawValue: 1), fileURL: URL(fileURLWithPath: "/tmp/a")))
        capture.startLabel(.ride, annotate: { _ in })
        capture.isFinishing = true
        capture.apply(.started(generation: .init(rawValue: 2), fileURL: URL(fileURLWithPath: "/tmp/b")))
        XCTAssertNil(capture.progress)
        XCTAssertTrue(capture.activeLabels.isEmpty)
        XCTAssertFalse(capture.isFinishing)
    }

    func testDuplicateTerminalCallbackDoesNotDuplicateSavedArtifact() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/a")
        capture.apply(.started(fileURL: url))
        capture.apply(.finished(fileURL: url))
        capture.apply(.finished(fileURL: url))
        XCTAssertEqual(capture.completed.count, 1)
    }
}
