import CutoutMobile
import Foundation
import Observation

typealias CameraRecordingCommand = @MainActor (String, UInt16, Bool) async throws -> CameraCommandOutcome
typealias CameraStillCommand = @MainActor (String, UInt16) async throws -> CameraCommandOutcome

@MainActor
@Observable
final class CameraControlModel {
    private(set) var recordingRequestKey: String?
    private(set) var isRequestingRecording = false
    private(set) var stillRequestKey: String?
    private(set) var isRequestingStill = false

    @ObservationIgnored private let annotateCapture: ((String, String) -> Void)?
    @ObservationIgnored private let recordingCommand: CameraRecordingCommand
    @ObservationIgnored private let stillCommand: CameraStillCommand
    @ObservationIgnored private var recordingTask: Task<Void, Never>?
    @ObservationIgnored private var stillTask: Task<Void, Never>?
    @ObservationIgnored private var recordingGeneration: UInt64 = 0
    @ObservationIgnored private var stillGeneration: UInt64 = 0

    init(
        adapter: CameraLocalNetworkAdapter = CameraLocalNetworkAdapter(),
        annotateCapture: ((String, String) -> Void)? = nil,
        recordingCommand: CameraRecordingCommand? = nil,
        stillCommand: CameraStillCommand? = nil
    ) {
        self.annotateCapture = annotateCapture
        self.recordingCommand = recordingCommand ?? { address, port, start in
            try await adapter.requestOnboardRecording(address: address, port: port, start: start)
        }
        self.stillCommand = stillCommand ?? { address, port in
            try await adapter.requestStillCapture(address: address, port: port)
        }
    }

    func requestRecording(address: String, port: String, start: Bool) {
        guard let portNumber = UInt16(port) else {
            recordingRequestKey = "camera.error.invalid_port"
            return
        }

        isRequestingRecording = true
        recordingRequestKey = nil
        recordingTask?.cancel()
        recordingGeneration &+= 1
        let generation = recordingGeneration
        recordingTask = Task { @MainActor in
            defer {
                if generation == recordingGeneration {
                    isRequestingRecording = false
                    recordingTask = nil
                }
            }
            do {
                let outcome = try await recordingCommand(address, portNumber, start)
                guard generation == recordingGeneration, !Task.isCancelled else { return }
                recordingRequestKey = cameraCommandOutcomeKey(
                    outcome,
                    acknowledged: start
                        ? "camera.recording.request_start_sent"
                        : "camera.recording.request_stop_sent"
                )
                annotateCapture?(
                    "camera_recording_request",
                    "\(start ? "start" : "stop"):\(outcome.annotationValue)"
                )
            } catch is CancellationError {
                // Cancellation is an expected lifecycle event.
            } catch CameraCommandRequestError.pathUnavailable {
                if generation == recordingGeneration { recordingRequestKey = "camera.connection.detail.wifi_required" }
            } catch CameraCommandRequestError.unsupported {
                if generation == recordingGeneration { recordingRequestKey = "camera.recording.unavailable" }
            } catch CameraCommandRequestError.inFlight {
                if generation == recordingGeneration { recordingRequestKey = "camera.command.busy" }
            } catch CameraCommandRequestError.originMismatch {
                if generation == recordingGeneration { recordingRequestKey = "camera.error.origin_mismatch" }
            } catch {
                if generation == recordingGeneration, !Task.isCancelled { recordingRequestKey = "camera.error.recording_request_failed" }
            }
        }
    }

    func requestStillCapture(address: String, port: String) {
        guard let portNumber = UInt16(port) else {
            stillRequestKey = "camera.error.invalid_port"
            return
        }

        isRequestingStill = true
        stillRequestKey = nil
        stillTask?.cancel()
        stillGeneration &+= 1
        let generation = stillGeneration
        stillTask = Task { @MainActor in
            defer {
                if generation == stillGeneration {
                    isRequestingStill = false
                    stillTask = nil
                }
            }
            do {
                let outcome = try await stillCommand(address, portNumber)
                guard generation == stillGeneration, !Task.isCancelled else { return }
                stillRequestKey = cameraCommandOutcomeKey(
                    outcome,
                    acknowledged: "camera.still.request_sent"
                )
                annotateCapture?("camera_still_request", outcome.annotationValue)
            } catch is CancellationError {
                // Cancellation is an expected lifecycle event.
            } catch CameraCommandRequestError.pathUnavailable {
                if generation == stillGeneration { stillRequestKey = "camera.connection.detail.wifi_required" }
            } catch CameraCommandRequestError.unsupported {
                if generation == stillGeneration { stillRequestKey = "camera.still.unavailable" }
            } catch CameraCommandRequestError.inFlight {
                if generation == stillGeneration { stillRequestKey = "camera.command.busy" }
            } catch CameraCommandRequestError.originMismatch {
                if generation == stillGeneration { stillRequestKey = "camera.error.origin_mismatch" }
            } catch {
                if generation == stillGeneration, !Task.isCancelled { stillRequestKey = "camera.error.still_request_failed" }
            }
        }
    }

    func cancelOutstandingWork() {
        recordingGeneration &+= 1
        stillGeneration &+= 1
        recordingTask?.cancel()
        stillTask?.cancel()
        recordingTask = nil
        stillTask = nil
        isRequestingRecording = false
        isRequestingStill = false
    }
}

private func cameraCommandOutcomeKey(
    _ outcome: CameraCommandOutcome,
    acknowledged: String
) -> String {
    switch outcome {
    case .acknowledged:
        acknowledged
    case .refused:
        "camera.command.refused"
    case .timedOut:
        "camera.command.timed_out"
    case .unknown:
        "camera.command.unknown"
    case .failed:
        "camera.command.failed"
    }
}

private extension CameraCommandOutcome {
    var annotationValue: String {
        switch self {
        case .acknowledged: "acknowledged"
        case .refused: "refused"
        case .timedOut: "timed_out"
        case .unknown: "unknown"
        case .failed: "failed"
        }
    }
}
