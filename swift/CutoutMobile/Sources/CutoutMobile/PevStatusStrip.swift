import SwiftUI

public struct PevStatusStrip: View {
    let text: String
    let scale: CGFloat
    let indicatorColor: Color
    let background: Color
    let foreground: Color
    let cornerRadius: CGFloat

    public init(
        text: String,
        scale: CGFloat,
        indicatorColor: Color,
        background: Color = PevDashboardColors.cardFill,
        foreground: Color = PevDashboardColors.primaryText,
        cornerRadius: CGFloat = 18
    ) {
        self.text = text
        self.scale = scale
        self.indicatorColor = indicatorColor
        self.background = background
        self.foreground = foreground
        self.cornerRadius = cornerRadius
    }

    public var body: some View {
        HStack(spacing: 10 * scale) {
            Circle()
                .fill(indicatorColor)
                .frame(width: 10 * scale, height: 10 * scale)
                .accessibilityHidden(true)
            Text(text)
                .font(.system(size: 13 * scale, weight: .semibold))
                .foregroundStyle(foreground)
                .lineLimit(2)
                .minimumScaleFactor(0.78)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16 * scale)
        .frame(minHeight: 42 * scale)
        .frame(maxWidth: .infinity)
        .background(
            PevDashboardCardBackground(cornerRadius: cornerRadius * scale, fill: background)
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text)
    }
}
