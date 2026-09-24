import XCTest
import Foundation
@testable import CutoutApp
import CutoutMobile
import CutoutMobileFFI

@MainActor
final class CaptureFeatureModelTests: XCTestCase {
    func testRejectedLabelKeepsAcceptedStateAndErrorSurvivesProgress() {
        let capture = CaptureFeatureModel()
        capture.deliverCaptureEvent(.started(fileURL: URL(fileURLWithPath: "/tmp/labels.jsonl")), origin: .manual)
        let generation = capture.activeGeneration
        capture.startLabel(.lowBeamOn, record: { requestedGeneration, _ in
            XCTAssertEqual(requestedGeneration, generation)
            return [.lowBeamOn]
        })
        capture.startLabel(.lowBeamOff, record: { _, _ in throw MobileCaptureAnnotationError.CapacityReached })
        XCTAssertEqual(capture.activeLabels, [.lowBeamOn])
        XCTAssertNotNil(capture.annotationErrorText)
        XCTAssertTrue(capture.canAnnotate, "Stopping the accepted interval must remain available")
        for tick in 1...3 {
            capture.deliverCaptureEvent(.progress(CaptureProgress(
                elapsedMilliseconds: UInt64(tick * 1000), notificationCount: UInt64(tick),
                fileSizeBytes: 300, queuedMessageCount: 0, writerError: nil
            )))
            XCTAssertNotNil(capture.annotationErrorText)
        }
        XCTAssertEqual(capture.recordingSummary, "Receiving Bluetooth data")
        capture.stopLabel(.lowBeamOn, record: { _, _ in [] })
        XCTAssertTrue(capture.activeLabels.isEmpty)
        XCTAssertNil(capture.annotationErrorText)
    }

    func testWriterStartFailureWithoutStartedIsVisible() {
        let capture = CaptureFeatureModel()
        let generation = capture.sessionState.beginCapture(origin: .manual)!
        XCTAssertTrue(capture.sessionState.captureWriterStartFailed(generation: generation))
        capture.apply(.lifecycle(capture.sessionState.captureLifecycleSnapshot()))
        capture.apply(.failed(generation: CaptureGeneration(rawValue: generation.value)))
        XCTAssertEqual(capture.status, .failed)
        XCTAssertNil(capture.activeGeneration)
        XCTAssertTrue(capture.completed.isEmpty)
    }

    func testLabelsAreNotAcceptedBeforeTheWriterStartsOrAfterItCompletes() {
        let capture = CaptureFeatureModel()
        var requests = 0
        capture.startLabel(.ride, record: { _, _ in requests += 1; return [.ride] })
        XCTAssertEqual(requests, 0)
        let url = URL(fileURLWithPath: "/tmp/labels.jsonl")
        capture.deliverCaptureEvent(.started(fileURL: url))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        capture.startLabel(.ride, record: { _, _ in requests += 1; return [.ride] })
        XCTAssertEqual(requests, 0)
        XCTAssertTrue(capture.activeLabels.isEmpty)
        XCTAssertEqual(capture.status, .saved(fileName: "labels.jsonl"))
    }

    func testFailedStartupPreservesThePreviousArtifactPresentation() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/saved.jsonl")
        capture.deliverCaptureEvent(.started(fileURL: url))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        XCTAssertFalse(capture.requestStart(device: nil, description: "new attempt") {
            let generation = capture.sessionState.beginCapture(origin: .manual)!
            XCTAssertTrue(capture.sessionState.captureWriterStartFailed(generation: generation))
            capture.apply(.lifecycle(capture.sessionState.captureLifecycleSnapshot()))
            capture.apply(.failed(generation: CaptureGeneration(rawValue: generation.value)))
            return false
        })
        XCTAssertEqual(capture.status, .saved(fileName: "saved.jsonl"))
        XCTAssertEqual(capture.completed.count, 1)
        XCTAssertEqual(capture.completed.first?.outcome, .saved)
        XCTAssertNil(capture.activeGeneration)
        XCTAssertTrue(capture.lifecycle.canStart)
    }

    func testQueuedStartPublicationsRetainEachDevicesIdentity() {
        let capture = CaptureFeatureModel()
        let firstDevice = CaptureDeviceIdentity(row: row(id: "a", advertisement: "GotWay_A"))
        let secondDevice = CaptureDeviceIdentity(row: row(id: "b", advertisement: "GotWay_B"))
        var publications: [CaptureEvent] = []
        func queueStart(device: CaptureDeviceIdentity, path: String) -> CaptureGeneration {
            var generation: MobileCaptureGenerationDto!
            XCTAssertTrue(capture.requestStart(device: device, description: nil) {
                generation = capture.sessionState.beginCapture(origin: .manual)!
                _ = capture.sessionState.captureWriterStarted(generation: generation)
                publications += [
                    .lifecycle(capture.sessionState.captureLifecycleSnapshot()),
                    .started(generation: CaptureGeneration(rawValue: generation.value), fileURL: URL(fileURLWithPath: path)),
                ]
                return true
            })
            return CaptureGeneration(rawValue: generation.value)
        }
        let first = queueStart(device: firstDevice, path: "/tmp/a")
        _ = capture.sessionState.retireCaptureWriter(generation: first.dto)
        let second = queueStart(device: secondDevice, path: "/tmp/b")
        publications.forEach(capture.apply)
        capture.deliverCaptureEvent(.finished(generation: first, fileURL: URL(fileURLWithPath: "/tmp/a")))
        XCTAssertEqual(capture.completed.first?.device, firstDevice)
        XCTAssertEqual(capture.device, secondDevice)
        XCTAssertEqual(capture.activeGeneration, second)
    }

    func testSameModelDevicesKeepAdvertisementAndPlatformIdentity() {
        let first = row(id: "device-a", advertisement: "GotWay_A")
        let second = row(id: "device-b", advertisement: "GotWay_B")
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/falcon.jsonl")
        XCTAssertTrue(capture.requestStart(device: CaptureDeviceIdentity(row: first), description: nil) {
            capture.deliverCaptureEvent(.started(fileURL: url), origin: .manual)
            return true
        })
        capture.updateDevice(second)
        capture.deliverCaptureEvent(.finished(fileURL: url))

        XCTAssertEqual(capture.completed.first?.device?.title, "Begode Falcon")
        XCTAssertEqual(capture.completed.first?.device?.advertisedName, "GotWay_A")
        XCTAssertEqual(capture.completed.first?.device?.platformIdentifier, "device-a")
        XCTAssertEqual(capture.completed.first?.isUserInitiated, true)
        XCTAssertNotEqual(CaptureDeviceIdentity(row: first), CaptureDeviceIdentity(row: second))
    }

    func testDetectedIdentityRefreshReachesCompletedArtifactWithoutAnotherPacket() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/automatic.jsonl")
        XCTAssertTrue(capture.requestStart(
            device: CaptureDeviceIdentity(row: row(id: "device-a", advertisement: "Old advertisement")), description: nil
        ) {
            capture.deliverCaptureEvent(.started(fileURL: url))
            return true
        })
        capture.updateDevice(row(id: "device-a", advertisement: "GotWay_A"))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        XCTAssertEqual(capture.completed.first?.device?.advertisedName, "GotWay_A")
        XCTAssertEqual(capture.completed.first?.isUserInitiated, false)
    }

    func testRecordingSummaryDoesNotExposeFileNamesOrLabelDiagnosticText() {
        let capture = CaptureFeatureModel()
        capture.deliverCaptureEvent(.started(fileURL: URL(fileURLWithPath: "/tmp/private-name.jsonl")), origin: .manual)
        XCTAssertEqual(capture.recordingSummary, "Waiting for Bluetooth data")
        capture.deliverCaptureEvent(.notificationRecorded)
        capture.startLabel(.ride, record: { _, _ in [.ride] })
        XCTAssertEqual(capture.recordingSummary, "Receiving Bluetooth data")
        capture.beginSavingFixture()
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
        capture.deliverCaptureEvent(.started(generation: first, fileURL: firstURL))
        capture.deliverCaptureEvent(.started(generation: second, fileURL: secondURL))
        capture.deliverCaptureEvent(.finished(generation: first, fileURL: firstURL))

        XCTAssertEqual(capture.activeGeneration, second)
        XCTAssertEqual(capture.status, .recordingLocally(fileName: "second.jsonl"))
        XCTAssertEqual(capture.completed.first?.fileURL, firstURL)
        XCTAssertEqual(capture.completed.first?.outcome, .saved)
    }

    func testFailureRetainsPartialArtifactWithoutCallingItSaved() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/partial.jsonl")
        capture.deliverCaptureEvent(.started(fileURL: url))
        capture.deliverCaptureEvent(.failed)
        XCTAssertEqual(capture.completed.first?.fileURL, url)
        XCTAssertEqual(capture.completed.first?.outcome, .failed)
        XCTAssertEqual(capture.status, .failed)
        XCTAssertNil(capture.activeGeneration)
    }

    func testNewGenerationResetsProgressLabelsAndFinishAdmission() {
        let capture = CaptureFeatureModel()
        capture.deliverCaptureEvent(.started(generation: .init(rawValue: 1), fileURL: URL(fileURLWithPath: "/tmp/a")), origin: .manual)
        capture.startLabel(.ride, record: { _, _ in [.ride] })
        capture.beginSavingFixture()
        capture.deliverCaptureEvent(.started(generation: .init(rawValue: 2), fileURL: URL(fileURLWithPath: "/tmp/b")))
        XCTAssertNil(capture.progress)
        XCTAssertTrue(capture.activeLabels.isEmpty)
        XCTAssertFalse(capture.isFinishing)
    }

    func testDuplicateTerminalCallbackDoesNotDuplicateSavedArtifact() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/a")
        capture.deliverCaptureEvent(.started(fileURL: url))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        XCTAssertEqual(capture.completed.count, 1)
    }
}
