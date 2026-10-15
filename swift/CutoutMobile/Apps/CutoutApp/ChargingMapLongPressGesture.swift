import CoreGraphics
import Foundation

#if canImport(UIKit)
    import SwiftUI
    import UIKit
#endif

#if canImport(UIKit)
    @MainActor
    struct ChargingMapLongPressGesture: UIGestureRecognizerRepresentable {
        let onBegan: (CGPoint) -> Void

        func makeCoordinator(converter: CoordinateSpaceConverter) -> ChargingMapLongPressCoordinator {
            ChargingMapLongPressCoordinator()
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

        func handleUIGestureRecognizerAction(
            _ recognizer: UILongPressGestureRecognizer,
            context: Context
        ) {
            guard recognizer.state == .began else { return }
            onBegan(context.converter.localLocation)
        }
    }

    @MainActor
    final class ChargingMapLongPressCoordinator: NSObject, UIGestureRecognizerDelegate {
        func gestureRecognizer(
            _ gestureRecognizer: UIGestureRecognizer,
            shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
        ) -> Bool {
            true
        }
    }
#endif
