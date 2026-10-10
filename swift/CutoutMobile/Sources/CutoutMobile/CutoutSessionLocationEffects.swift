import Foundation

/// Routes each phone-location update to capture and ride-map recording.
struct CutoutSessionLocationEffects: Sendable {
    private let recordCaptureUpdate: @Sendable (PhoneLocationUpdate) -> CaptureLocationWriteResult
    private let ingestRideMapUpdate: @Sendable (PhoneLocationUpdate) -> Void
    private let handleCaptureResult: @Sendable (CaptureLocationWriteResult) -> Void

    init(
        recordCaptureUpdate: @escaping @Sendable (PhoneLocationUpdate) -> CaptureLocationWriteResult,
        ingestRideMapUpdate: @escaping @Sendable (PhoneLocationUpdate) -> Void,
        handleCaptureResult: @escaping @Sendable (CaptureLocationWriteResult) -> Void
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
