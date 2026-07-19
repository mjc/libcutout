import CoreGraphics

public struct DashboardLayoutFrame: Equatable, Sendable {
    public let top: CGFloat
    public let height: CGFloat

    public init(top: CGFloat, height: CGFloat) {
        self.top = top
        self.height = height
    }

    public var bottom: CGFloat { top + height }
}

public struct DashboardViewport: Equatable, Sendable {
    public static let topNavigationHeight: CGFloat = 64
    public static let navigationHeight: CGFloat = 76

    public let width: CGFloat
    public let height: CGFloat
    public let safeAreaTop: CGFloat
    public let safeAreaBottom: CGFloat

    public init(width: CGFloat, height: CGFloat, safeAreaTop: CGFloat, safeAreaBottom: CGFloat) {
        self.width = width
        self.height = height
        self.safeAreaTop = safeAreaTop
        self.safeAreaBottom = safeAreaBottom
    }

    public static func contentScale(width: CGFloat, height: CGFloat) -> CGFloat {
        min(1, max(0.75, min(width / 390, height / 844)))
    }

    public func navigationFrame(height: CGFloat = DashboardViewport.navigationHeight) -> DashboardLayoutFrame {
        DashboardLayoutFrame(top: self.height - safeAreaBottom - height, height: height)
    }

    public func isNavigationAnchored(frame: DashboardLayoutFrame, tolerance: CGFloat = 2) -> Bool {
        frame.height > 0
            && frame.bottom <= height + tolerance
            && abs(frame.bottom - (height - safeAreaBottom)) <= tolerance
            && frame.top >= safeAreaTop
    }

    public func contentBottom(navigation: DashboardLayoutFrame, gap: CGFloat) -> CGFloat {
        navigation.top - gap
    }
}
