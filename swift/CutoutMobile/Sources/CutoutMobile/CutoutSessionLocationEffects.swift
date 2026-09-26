import Foundation

/// Routes each phone-location update to capture and ride-map recording.
struct CutoutSessionLocationEffects {
    private let recordCaptureUpdate: (PhoneLocationUpdate) -> CaptureLocationWriteResult
    private let ingestRideMapUpdate: (PhoneLocationUpdate) -> Void
    private let handleCaptureResult: (CaptureLocationWriteResult) -> Void

    init(
        recordCaptureUpdate: @escaping (PhoneLocationUpdate) -> CaptureLocationWriteResult,
        ingestRideMapUpdate: @escaping (PhoneLocationUpdate) -> Void,
        handleCaptureResult: @escaping (CaptureLocationWriteResult) -> Void
    ) {
        self.recordCaptureUpdate = recordCaptureUpdate
        self.ingestRideMapUpdate = ingestRideMapUpdate
        self.handleCaptureResult = handleCaptureResult
    }

    func ingest(_ update: PhoneLocationUpdate) {
        let result = recordCaptureUpdate(update)
        handleCaptureResult(result)
        ingestRideMapUpdate(update)
    }
}
