import SwiftUI

public struct PevRideDisconnectButton: View {
    let action: () -> Void

    public init(action: @escaping () -> Void) {
        self.action = action
    }

    public var body: some View {
        Button("Disconnect", action: action)
            .font(.callout.weight(.bold))
            .foregroundStyle(PevDashboardColors.primaryText)
            .padding(.horizontal, 12)
            .frame(minWidth: 44, minHeight: 44)
            .background(PevDashboardCardBackground(cornerRadius: 8))
            .buttonStyle(.plain)
            .accessibilityIdentifier("dashboard.disconnect")
    }
}
