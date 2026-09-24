import Foundation

/// Owns capture-generation identity and presentation events, not the PEVCAP writer.
final class CutoutSessionCapturePresentation {
    private let publish: (CaptureEvent) -> Void
    private var nextGeneration: UInt64 = 0
    private(set) var currentGeneration: CaptureGeneration?

    init(publish: @escaping (CaptureEvent) -> Void) {
        self.publish = publish
    }

    func begin() -> CaptureGeneration {
        nextGeneration = nextGeneration == .max ? 1 : nextGeneration + 1
        let generation = CaptureGeneration(rawValue: nextGeneration)
        currentGeneration = generation
        return generation
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
