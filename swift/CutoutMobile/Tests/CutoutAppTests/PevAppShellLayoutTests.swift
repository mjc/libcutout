import SwiftUI
import XCTest

@testable import CutoutApp

#if os(macOS)
    import AppKit

    @MainActor
    final class PevAppShellLayoutTests: XCTestCase {
        func testBackReplacesBrandWithoutChangingTheContentFrame() {
            for width: CGFloat in [288, 358, 448] {
                for textSize in [DynamicTypeSize.large, .accessibility3] {
                    let parent = size(width: width, textSize: textSize, back: nil)
                    let child = size(width: width, textSize: textSize, back: {})
                    XCTAssertEqual(child.width, parent.width, accuracy: 0.5)
                    XCTAssertEqual(
                        child.height, parent.height, accuracy: 0.5,
                        "Opening a More page must not add a header row or move its content, \(width), \(textSize)"
                    )
                }
            }
        }

        private func size(width: CGFloat, textSize: DynamicTypeSize, back: (() -> Void)?) -> NSSize {
            let view = PevAppShell(sectionTitle: "Camera", back: back) {
                Color.clear.frame(height: 100)
            }
            .environment(\.dynamicTypeSize, textSize)
            .frame(width: width)
            return NSHostingView(rootView: view).fittingSize
        }
    }
#endif
