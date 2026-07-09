import SwiftUI

public struct PevDashboardFaultDetailCard: View {
    let detail: String
    let accent: Color
    let scale: CGFloat
    let fontSize: CGFloat
    let horizontalAlignment: Alignment
    let horizontalPadding: CGFloat
    let height: CGFloat
    let cornerRadius: CGFloat
    let minimumScaleFactor: CGFloat

    public init(
        detail: String,
        accent: Color,
        scale: CGFloat,
        fontSize: CGFloat = 15,
        horizontalAlignment: Alignment = .leading,
        horizontalPadding: CGFloat = 22,
        height: CGFloat = 54,
        cornerRadius: CGFloat = 19,
        minimumScaleFactor: CGFloat = 1
    ) {
        self.detail = detail
        self.accent = accent
        self.scale = scale
        self.fontSize = fontSize
        self.horizontalAlignment = horizontalAlignment
        self.horizontalPadding = horizontalPadding
        self.height = height
        self.cornerRadius = cornerRadius
        self.minimumScaleFactor = minimumScaleFactor
    }

    public var body: some View {
        Text(detail)
            .font(.system(size: fontSize * scale, weight: .black))
            .foregroundStyle(accent)
            .lineLimit(1)
            .minimumScaleFactor(minimumScaleFactor)
            .frame(maxWidth: .infinity, alignment: horizontalAlignment)
            .padding(.horizontal, horizontalPadding * scale)
            .frame(height: height * scale)
            .background(PevDashboardCardBackground(cornerRadius: cornerRadius * scale))
    }
}
