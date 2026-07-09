import SwiftUI

public enum PevDashboardColors {
    public static let cardFill = Color(red: 0.067, green: 0.078, blue: 0.106)
    public static let cardStroke = Color(red: 0.165, green: 0.188, blue: 0.239)
    public static let primaryText = Color(red: 0.969, green: 0.953, blue: 0.918)
    public static let mutedText = Color(red: 0.561, green: 0.596, blue: 0.659)
}

public struct PevDashboardCardBackground: View {
    let cornerRadius: CGFloat
    let fill: Color
    let stroke: Color
    let lineWidth: CGFloat

    public init(
        cornerRadius: CGFloat,
        fill: Color = PevDashboardColors.cardFill,
        stroke: Color = PevDashboardColors.cardStroke,
        lineWidth: CGFloat = 1
    ) {
        self.cornerRadius = cornerRadius
        self.fill = fill
        self.stroke = stroke
        self.lineWidth = lineWidth
    }

    public var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(fill)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(stroke, lineWidth: lineWidth)
            )
    }
}
