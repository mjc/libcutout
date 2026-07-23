import SwiftUI

public struct PevDashboardFootpadReadout: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let title: String
    let leftLabel: String
    let leftValue: String
    let rightLabel: String
    let rightValue: String
    let detail: String
    let accessibilityValueText: String

    public init(
        title: String = "footpad",
        leftLabel: String = "left",
        leftValue: String,
        rightLabel: String = "right",
        rightValue: String,
        detail: String,
        accessibilityValue: String? = nil
    ) {
        self.title = title
        self.leftLabel = leftLabel
        self.leftValue = leftValue
        self.rightLabel = rightLabel
        self.rightValue = rightValue
        self.detail = detail
        accessibilityValueText = accessibilityValue ?? [
            leftLabel,
            leftValue,
            rightLabel,
            rightValue,
            detail,
        ].formatted(.list(type: .and))
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(title)
                    .font(.caption.weight(.bold))
                    .foregroundStyle(PevDashboardColors.mutedText)
                Spacer()
                Text(detail)
                    .font(.caption.weight(.bold))
                    .foregroundStyle(PevDashboardColors.mutedText)
            }

            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 10) {
                    footpadSide(label: leftLabel, value: leftValue)
                    footpadSide(label: rightLabel, value: rightValue)
                }
            } else {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 10) {
                        footpadSide(label: leftLabel, value: leftValue)
                        footpadSide(label: rightLabel, value: rightValue)
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        footpadSide(label: leftLabel, value: leftValue)
                        footpadSide(label: rightLabel, value: rightValue)
                    }
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 14,
                fill: PevDashboardColors.cardFill,
                stroke: PevDashboardColors.cardStroke
            )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(accessibilityValueText)
    }

    private func footpadSide(label: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 5) {
                Text(label)
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(PevDashboardColors.mutedText)
            }
            Text(value)
                .font(.title3.weight(.black))
                .foregroundStyle(PevDashboardColors.primaryText)
                .monospacedDigit()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
