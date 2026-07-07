import SwiftUI

public struct PevRideDisconnectButton: View {
    let scale: CGFloat
    let action: () -> Void

    public init(scale: CGFloat, action: @escaping () -> Void) {
        self.scale = scale
        self.action = action
    }

    public var body: some View {
        PevActionButton(
            title: "Disconnect",
            systemImageName: nil,
            scale: scale,
            isEnabled: true,
            fillsAvailableWidth: false,
            width: nil,
            height: 30 * scale,
            cornerRadius: 8 * scale,
            horizontalPadding: 12 * scale,
            iconSpacing: 0,
            foregroundEnabled: .yellow,
            foregroundDisabled: .yellow,
            fillEnabled: PevDashboardColors.cardFill,
            fillDisabled: PevDashboardColors.cardFill,
            strokeEnabled: PevDashboardColors.cardStroke,
            strokeDisabled: PevDashboardColors.cardStroke,
            action: action
        )
    }
}
