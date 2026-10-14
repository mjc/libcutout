import CoreGraphics
import XCTest

@testable import CutoutApp

#if canImport(UIKit)
    import UIKit
#endif

@MainActor
final class ChargingMapLongPressGestureTests: XCTestCase {
    func testStationaryLongPressEmitsImmediatelyOnBeganOnlyOnce() {
        var received = [CGPoint]()
        let point = CGPoint(x: 40, y: 60)
        let handler = ChargingMapLongPressActionHandler { received.append($0) }

        handler.handle(phase: .began, point: point)
        XCTAssertEqual(received, [point])
        handler.handle(phase: .began, point: point)
        handler.handle(phase: .changed, point: point)
        handler.handle(phase: .ended, point: point)
        handler.handle(phase: .began, point: point)

        XCTAssertEqual(received, [point, point])
    }

    #if canImport(UIKit)
        func testCoordinatorAllowsMapGesturesToRecognizeSimultaneously() {
            let coordinator = ChargingMapLongPressCoordinator { _ in }
            let longPress = UILongPressGestureRecognizer()
            let mapPan = UIPanGestureRecognizer()

            XCTAssertTrue(coordinator.gestureRecognizer(longPress, shouldRecognizeSimultaneouslyWith: mapPan))
        }
    #endif
}
