import SwiftUI

public struct PevStatusStrip: View {
    let text: String

    var accessibilityLabelText: String { text }

    public init(text: String) {
        self.text = text
    }

    public var body: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(PevDashboardColors.nominal)
                .frame(width: 10, height: 10)
                .accessibilityHidden(true)
            Text(text)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(PevDashboardColors.primaryText)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .frame(minHeight: 42)
        .frame(maxWidth: .infinity)
        .background(
            PevDashboardCardBackground(cornerRadius: 18)
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityLabelText)
    }
}
