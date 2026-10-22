import SwiftUI

enum RideViewportLayout {
    // TabView already reduces the content proposal for its safe area. Only cap
    // that proposal if the measured player overlaps it; subtracting the safe-area
    // inset again leaves a blank band and clips the final scroll rows early.
    static func visibleHeight(
        in geometry: GeometryProxy, musicPlayerFrame: CGRect?, bottomSafeAreaInset: CGFloat = 0
    ) -> CGFloat {
        visibleHeight(
            contentFrame: geometry.frame(in: .global), musicPlayerFrame: musicPlayerFrame,
            bottomSafeAreaInset: bottomSafeAreaInset)
    }

    static func visibleHeight(
        contentFrame: CGRect, musicPlayerFrame: CGRect?, bottomSafeAreaInset: CGFloat = 0
    ) -> CGFloat {
        // A measured accessory replaces the native inset; it is not an additional deduction.
        let chromeTop = musicPlayerFrame?.minY ?? (contentFrame.maxY - max(0, bottomSafeAreaInset))
        let unobscuredBottom = min(contentFrame.maxY, chromeTop)
        return min(contentFrame.height, max(0, unobscuredBottom - contentFrame.minY))
    }

    // Keep room for the first ride row or live controls instead of using screen bounds.
    static func mapHeight(for availableHeight: CGFloat) -> CGFloat {
        guard availableHeight.isFinite, availableHeight > 0 else { return 0 }
        return min(availableHeight, min(max(availableHeight * 0.46, 160), 360))
    }
}

struct RideMapLoadingSurface: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 26, macOS 26, *) {
            content.glassEffect(.regular, in: .capsule)
        } else {
            content
                .background(PevColors.cardFill, in: Capsule())
                .overlay { Capsule().stroke(PevColors.cardStroke, lineWidth: 1) }
        }
    }
}
