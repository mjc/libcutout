import Foundation

/// Owns capture-generation identity and presentation events, not the PEVCAP writer.
final class CutoutSessionCapturePresentation {
    private let publish: (CaptureEvent) -> Void
    private(set) var currentGeneration: CaptureGeneration?

    init(publish: @escaping (CaptureEvent) -> Void) {
        self.publish = publish
    }

    func begin(generation: CaptureGeneration) {
        currentGeneration = generation
    }

    func publishProgress(_ progress: CaptureProgress) {
        guard let currentGeneration else { return }
        publish(.progress(generation: currentGeneration, progress))
    }

    func publishFailure() {
        guard let currentGeneration else { return }
        publish(.failed(generation: currentGeneration))
    }

    func end() {
        currentGeneration = nil
    }
}
