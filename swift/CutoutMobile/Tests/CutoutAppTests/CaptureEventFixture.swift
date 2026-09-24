import CutoutMobile
import CutoutMobileFFI
import Foundation
import XCTest
@testable import CutoutApp

/// Drives the real Rust lifecycle before publishing native evidence. Replacement
/// fixtures explicitly detach the previous writer, whose receipt may arrive later.
@MainActor
func deliverCaptureFixture(
    _ event: CaptureEvent,
    owner: CutoutSessionStateHandle,
    origin: MobileCaptureOriginDto = .automatic,
    publish: (CaptureEvent) -> Void
) {
    func resolve(_ generation: CaptureGeneration) -> CaptureGeneration {
        generation.rawValue == 0
            ? CaptureGeneration(rawValue: owner.captureLifecycleSnapshot().attempt?.generation.value ?? 0)
            : generation
    }
    let translated: CaptureEvent
    switch event {
    case let .started(requested, url):
        let current = owner.captureLifecycleSnapshot().attempt
        let generation: MobileCaptureGenerationDto
        if requested.rawValue != 0, current?.generation.value == requested.rawValue {
            generation = requested.dto
        } else {
            if let current { _ = owner.retireCaptureWriter(generation: current.generation) }
            guard let admitted = owner.beginCapture(origin: origin) else {
                XCTFail("fixture start was not admitted")
                return
            }
            generation = admitted
            _ = owner.captureWriterStarted(generation: generation)
        }
        if requested.rawValue != 0 { XCTAssertEqual(generation.value, requested.rawValue) }
        translated = .started(generation: CaptureGeneration(rawValue: generation.value), fileURL: url)
    case let .progress(generation, progress):
        let generation = resolve(generation)
        if progress.writerError != nil { _ = owner.captureWriterFailed(generation: generation.dto) }
        translated = .progress(generation: generation, progress)
    case let .notificationRecorded(generation):
        translated = .notificationRecorded(generation: resolve(generation))
    case let .finished(generation, url):
        let generation = resolve(generation)
        _ = owner.retireCaptureWriter(generation: generation.dto)
        _ = owner.completeCaptureWriter(generation: generation.dto, succeeded: true)
        translated = .finished(generation: generation, fileURL: url)
    case let .failed(generation):
        let generation = resolve(generation)
        _ = owner.retireCaptureWriter(generation: generation.dto)
        _ = owner.completeCaptureWriter(generation: generation.dto, succeeded: false)
        translated = .failed(generation: generation)
    case .lifecycle:
        translated = event
    }
    publish(.lifecycle(owner.captureLifecycleSnapshot()))
    publish(translated)
}

@MainActor
extension CaptureFeatureModel {
    convenience init() { self.init(sessionState: CutoutSessionStateHandle()) }

    func deliverCaptureEvent(_ event: CaptureEvent, origin: MobileCaptureOriginDto = .automatic) {
        deliverCaptureFixture(event, owner: sessionState, origin: origin, publish: apply)
    }

    func beginSavingFixture() {
        guard let generation = activeGeneration else { return XCTFail("no active fixture") }
        XCTAssertNotNil(sessionState.beginCaptureFinish(generation: generation.dto))
        apply(.lifecycle(sessionState.captureLifecycleSnapshot()))
    }
}

@MainActor
extension CutoutAppModel {
    func deliverCaptureEvent(_ event: CaptureEvent) {
        capture.deliverCaptureEvent(event)
    }
}
