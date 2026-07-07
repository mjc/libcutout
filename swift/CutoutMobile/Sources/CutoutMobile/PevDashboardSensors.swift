import SwiftUI

public struct PevDashboardFootpadReadout: View {
    let title: String
    let leftLabel: String
    let leftValue: String
    let rightLabel: String
    let rightValue: String
    let detail: String
    let accent: Color
    let fill: Color
    let stroke: Color
    let textColor: Color
    let secondaryTextColor: Color
    let scale: CGFloat

    public init(
        title: String = "footpad",
        leftLabel: String = "left",
        leftValue: String,
        rightLabel: String = "right",
        rightValue: String,
        detail: String,
        accent: Color,
        fill: Color,
        stroke: Color,
        textColor: Color,
        secondaryTextColor: Color,
        scale: CGFloat
    ) {
        self.title = title
        self.leftLabel = leftLabel
        self.leftValue = leftValue
        self.rightLabel = rightLabel
        self.rightValue = rightValue
        self.detail = detail
        self.accent = accent
        self.fill = fill
        self.stroke = stroke
        self.textColor = textColor
        self.secondaryTextColor = secondaryTextColor
        self.scale = scale
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            HStack {
                Text(title)
                    .font(.system(size: 13 * scale, weight: .bold))
                    .foregroundStyle(secondaryTextColor)
                Spacer()
                Text(detail)
                    .font(.system(size: 12 * scale, weight: .bold))
                    .foregroundStyle(accent)
                    .lineLimit(1)
            }

            HStack(spacing: 10 * scale) {
                footpadSide(label: leftLabel, value: leftValue)
                footpadSide(label: rightLabel, value: rightValue)
            }
        }
        .padding(.horizontal, 16 * scale)
        .padding(.vertical, 12 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 16 * scale,
                fill: fill,
                stroke: stroke
            )
        )
    }

    private func footpadSide(label: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 3 * scale) {
            Text(label)
                .font(.system(size: 10 * scale, weight: .bold))
                .foregroundStyle(secondaryTextColor)
                .textCase(.uppercase)
            Text(value)
                .font(.system(size: 22 * scale, weight: .black))
                .foregroundStyle(textColor)
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.72)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
