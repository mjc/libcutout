import SwiftUI

public struct PevRideDisconnectButton: View {
    let action: () -> Void

    public init(action: @escaping () -> Void) {
        self.action = action
    }

    public var body: some View {
        PevActionButton(
            title: "Disconnect",
            systemImageName: nil,
            isEnabled: true,
            fillsAvailableWidth: false,
            width: nil,
            height: 30,
            cornerRadius: 8,
            horizontalPadding: 12,
            iconSpacing: 0,
            foregroundEnabled: PevDashboardColors.primaryText,
            foregroundDisabled: PevDashboardColors.primaryText,
            fillEnabled: PevDashboardColors.cardFill,
            fillDisabled: PevDashboardColors.cardFill,
            strokeEnabled: PevDashboardColors.cardStroke,
            strokeDisabled: PevDashboardColors.cardStroke,
            accessibilityIdentifierText: "dashboard.disconnect",
            action: action
        )
    }
}
