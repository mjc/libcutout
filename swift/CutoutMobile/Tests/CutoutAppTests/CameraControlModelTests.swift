import CutoutMobile
import XCTest
@testable import CutoutApp

@MainActor
final class CameraControlModelTests: XCTestCase {
    func testRecordingRequestRejectsInvalidPortBeforeStartingWork() async {
        let model = CameraControlModel()

        model.requestRecording(address: "192.168.1.254", port: "invalid", start: true)

        XCTAssertEqual(model.recordingRequestKey, "camera.error.invalid_port")
        XCTAssertFalse(model.isRequestingRecording)
    }

    func testStillRequestRejectsOutOfRangePortBeforeStartingWork() async {
        let model = CameraControlModel()

        model.requestStillCapture(address: "192.168.1.254", port: "65536")

        XCTAssertEqual(model.stillRequestKey, "camera.error.invalid_port")
        XCTAssertFalse(model.isRequestingStill)
    }

    func testRecordingOutcomeAndErrorKeys() async {
        let outcomeCases: [(CameraCommandOutcome, String)] = [
            (.acknowledged, "camera.recording.request_start_sent"),
            (.refused, "camera.command.refused"),
            (.timedOut, "camera.command.timed_out"),
            (.unknown, "camera.command.unknown"),
            (.failed, "camera.command.failed"),
        ]

        for (outcome, expectedKey) in outcomeCases {
            let model = CameraControlModel(recordingCommand: { _, _, _ in outcome })
            model.requestRecording(address: "192.168.1.254", port: "80", start: true)

            await waitUntil { !model.isRequestingRecording }

            XCTAssertEqual(model.recordingRequestKey, expectedKey)
        }

        let errorCases: [(CameraCommandRequestError, String)] = [
            (.pathUnavailable, "camera.connection.detail.wifi_required"),
            (.unsupported, "camera.recording.unavailable"),
            (.inFlight, "camera.command.busy"),
            (.originMismatch, "camera.error.origin_mismatch"),
        ]

        for (error, expectedKey) in errorCases {
            let model = CameraControlModel(recordingCommand: { _, _, _ in throw error })
            model.requestRecording(address: "192.168.1.254", port: "80", start: true)

            await waitUntil { !model.isRequestingRecording }

            XCTAssertEqual(model.recordingRequestKey, expectedKey)
        }

        let failedModel = CameraControlModel(recordingCommand: { _, _, _ in throw TestCommandError.failed })
        failedModel.requestRecording(address: "192.168.1.254", port: "80", start: true)

        await waitUntil { !failedModel.isRequestingRecording }

        XCTAssertEqual(failedModel.recordingRequestKey, "camera.error.recording_request_failed")
    }

    func testStillOutcomeAndErrorKeys() async {
        let model = CameraControlModel(stillCommand: { _, _ in .refused })
        model.requestStillCapture(address: "192.168.1.254", port: "80")

        await waitUntil { !model.isRequestingStill }

        XCTAssertEqual(model.stillRequestKey, "camera.command.refused")

        let errorCases: [(CameraCommandRequestError, String)] = [
            (.pathUnavailable, "camera.connection.detail.wifi_required"),
            (.unsupported, "camera.still.unavailable"),
            (.inFlight, "camera.command.busy"),
            (.originMismatch, "camera.error.origin_mismatch"),
        ]

        for (error, expectedKey) in errorCases {
            let errorModel = CameraControlModel(stillCommand: { _, _ in throw error })
            errorModel.requestStillCapture(address: "192.168.1.254", port: "80")

            await waitUntil { !errorModel.isRequestingStill }

            XCTAssertEqual(errorModel.stillRequestKey, expectedKey)
        }

        let failedModel = CameraControlModel(stillCommand: { _, _ in throw TestCommandError.failed })
        failedModel.requestStillCapture(address: "192.168.1.254", port: "80")

        await waitUntil { !failedModel.isRequestingStill }

        XCTAssertEqual(failedModel.stillRequestKey, "camera.error.still_request_failed")
    }

    func testRecordingCompletionAfterCancellationDoesNotPublishResult() async {
        var pendingCommand: CheckedContinuation<CameraCommandOutcome, Never>?
        let model = CameraControlModel(recordingCommand: { _, _, _ in
            await withCheckedContinuation { continuation in
                pendingCommand = continuation
            }
        })

        model.requestRecording(address: "192.168.1.254", port: "80", start: true)
        for _ in 0 ..< 1_000 where pendingCommand == nil {
            await Task.yield()
        }
        XCTAssertNotNil(pendingCommand)

        model.cancelOutstandingWork()
        pendingCommand?.resume(returning: .acknowledged)
        await waitUntil { !model.isRequestingRecording }

        XCTAssertNil(model.recordingRequestKey)
        XCTAssertFalse(model.isRequestingRecording)
    }

    private func waitUntil(
        _ condition: @MainActor () -> Bool
    ) async {
        for _ in 0 ..< 1_000 where !condition() {
            await Task.yield()
        }
    }
}

private enum TestCommandError: Error {
    case failed
}
