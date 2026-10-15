import CoreGraphics
import XCTest

@testable import CutoutApp

#if canImport(UIKit)
    import UIKit
#endif

@MainActor
final class ChargingMapLongPressGestureTests: XCTestCase {
    #if canImport(UIKit)
        func testCoordinatorAllowsMapGesturesToRecognizeSimultaneously() {
            let coordinator = ChargingMapLongPressCoordinator()
            let longPress = UILongPressGestureRecognizer()
            let mapPan = UIPanGestureRecognizer()

            XCTAssertTrue(coordinator.gestureRecognizer(longPress, shouldRecognizeSimultaneouslyWith: mapPan))
        }
    #endif
}
