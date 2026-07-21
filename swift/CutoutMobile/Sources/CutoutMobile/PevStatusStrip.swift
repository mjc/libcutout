import SwiftUI

public struct PevStatusStrip: View {
    let text: String
    let indicatorColor: Color
    let background: Color
    let foreground: Color
    let cornerRadius: CGFloat

    public init(
        text: String,
        indicatorColor: Color,
        background: Color = PevDashboardColors.cardFill,
        foreground: Color = PevDashboardColors.primaryText,
        cornerRadius: CGFloat = 18
    ) {
        self.text = text
        self.indicatorColor = indicatorColor
        self.background = background
        self.foreground = foreground
        self.cornerRadius = cornerRadius
    }

    public var body: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(indicatorColor)
                .frame(width: 10, height: 10)
                .accessibilityHidden(true)
            Text(text)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(foreground)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .frame(minHeight: 42)
        .frame(maxWidth: .infinity)
        .background(
            PevDashboardCardBackground(cornerRadius: cornerRadius, fill: background)
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text)
    }
}
