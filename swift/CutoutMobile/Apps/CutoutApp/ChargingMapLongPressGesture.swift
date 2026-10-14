import CoreGraphics
import Foundation

#if canImport(UIKit)
    import SwiftUI
    import UIKit
#endif

enum ChargingMapLongPressPhase: Equatable {
    case began
    case changed
    case ended
    case cancelled
    case failed
}

@MainActor
final class ChargingMapLongPressActionHandler {
    var onBegan: (CGPoint) -> Void
    private(set) var hasEmittedForCurrentPress = false

    init(onBegan: @escaping (CGPoint) -> Void) {
        self.onBegan = onBegan
    }

    func handle(phase: ChargingMapLongPressPhase, point: CGPoint) {
        if phase == .began {
            guard !hasEmittedForCurrentPress else { return }
            hasEmittedForCurrentPress = true
            onBegan(point)
        } else if phase == .ended || phase == .cancelled || phase == .failed {
            hasEmittedForCurrentPress = false
        }
    }
}

#if canImport(UIKit)
    @MainActor
    struct ChargingMapLongPressGesture: UIGestureRecognizerRepresentable {
        let onBegan: (CGPoint) -> Void

        func makeCoordinator(converter: CoordinateSpaceConverter) -> ChargingMapLongPressCoordinator {
            ChargingMapLongPressCoordinator(onBegan: onBegan)
        }

        func makeUIGestureRecognizer(context: Context) -> UILongPressGestureRecognizer {
            let recognizer = UILongPressGestureRecognizer()
            recognizer.minimumPressDuration = 0.55
            recognizer.allowableMovement = 16
            recognizer.cancelsTouchesInView = false
            recognizer.delaysTouchesBegan = false
            recognizer.delaysTouchesEnded = false
            recognizer.delegate = context.coordinator
            return recognizer
        }

        func updateUIGestureRecognizer(_ recognizer: UILongPressGestureRecognizer, context: Context) {
            context.coordinator.actionHandler.onBegan = onBegan
        }

        func handleUIGestureRecognizerAction(
            _ recognizer: UILongPressGestureRecognizer,
            context: Context
        ) {
            let point = recognizer.state == .began ? context.converter.localLocation : .zero
            context.coordinator.handle(state: recognizer.state, point: point)
        }
    }

    @MainActor
    final class ChargingMapLongPressCoordinator: NSObject, UIGestureRecognizerDelegate {
        let actionHandler: ChargingMapLongPressActionHandler

        init(onBegan: @escaping (CGPoint) -> Void) {
            self.actionHandler = ChargingMapLongPressActionHandler(onBegan: onBegan)
        }

        func handle(state: UIGestureRecognizer.State, point: CGPoint) {
            let phase: ChargingMapLongPressPhase
            switch state {
            case .possible: return
            case .began: phase = .began
            case .changed: phase = .changed
            case .ended: phase = .ended
            case .cancelled: phase = .cancelled
            case .failed: phase = .failed
            @unknown default: return
            }
            actionHandler.handle(phase: phase, point: point)
        }

        func gestureRecognizer(
            _ gestureRecognizer: UIGestureRecognizer,
            shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
        ) -> Bool {
            true
        }
    }
#endif
