import SwiftUI

public struct PevDashboardFootpadReadout: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

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
    let accessibilityValueText: String

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
        scale: CGFloat,
        accessibilityValue: String? = nil
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
        accessibilityValueText = accessibilityValue ?? [
            leftLabel,
            leftValue,
            rightLabel,
            rightValue,
            detail,
        ].joined(separator: ", ")
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            HStack {
                Text(title)
                    .font(.caption.weight(.bold))
                    .foregroundStyle(secondaryTextColor)
                Spacer()
                Text(detail)
                    .font(.caption.weight(.bold))
                    .foregroundStyle(accent)
            }

            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 10 * scale) {
                    footpadSide(label: leftLabel, value: leftValue)
                    footpadSide(label: rightLabel, value: rightValue)
                }
            } else {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 10 * scale) {
                        footpadSide(label: leftLabel, value: leftValue)
                        footpadSide(label: rightLabel, value: rightValue)
                    }

                    VStack(alignment: .leading, spacing: 10 * scale) {
                        footpadSide(label: leftLabel, value: leftValue)
                        footpadSide(label: rightLabel, value: rightValue)
                    }
                }
            }
        }
        .padding(.horizontal, 14 * scale)
        .padding(.vertical, 10 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 14 * scale,
                fill: fill,
                stroke: stroke
            )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(accessibilityValueText)
    }

    private func footpadSide(label: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 3 * scale) {
            HStack(spacing: 5 * scale) {
                Text(label)
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(secondaryTextColor)
                    .textCase(.uppercase)
            }
            Text(value)
                .font(.title3.weight(.black))
                .foregroundStyle(textColor)
                .monospacedDigit()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
