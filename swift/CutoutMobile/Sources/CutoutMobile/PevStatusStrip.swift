import SwiftUI

public struct PevStatusStrip: View {
    let text: String
    let isFailure: Bool

    var accessibilityLabelText: String { text }

    public init(text: String, isFailure: Bool = false) {
        self.text = text
        self.isFailure = isFailure
    }

    public var body: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(isFailure ? PevDashboardColors.red : PevDashboardColors.nominal)
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
