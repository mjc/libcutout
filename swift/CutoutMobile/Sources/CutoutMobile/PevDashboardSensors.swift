import SwiftUI

public struct PevDashboardFootpadReadout: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let title: String
    let leftLabel: String
    let leftMetricValue: PevDashboardMetricValue
    let rightLabel: String
    let rightMetricValue: PevDashboardMetricValue
    let detail: String
    let accessibilityValueText: String

    public init(
        title: String = pevLocalizedText("footpad.title"),
        leftLabel: String = pevLocalizedText("footpad.left"),
        leftMetricValue: PevDashboardMetricValue,
        rightLabel: String = pevLocalizedText("footpad.right"),
        rightMetricValue: PevDashboardMetricValue,
        detail: String
    ) {
        self.title = title
        self.leftLabel = leftLabel
        self.leftMetricValue = leftMetricValue
        self.rightLabel = rightLabel
        self.rightMetricValue = rightMetricValue
        self.detail = detail
        accessibilityValueText = [
            leftLabel,
            leftMetricValue.accessibilityText,
            rightLabel,
            rightMetricValue.accessibilityText,
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
                    footpadSide(label: leftLabel, metricValue: leftMetricValue)
                    footpadSide(label: rightLabel, metricValue: rightMetricValue)
                }
            } else {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 10) {
                        footpadSide(label: leftLabel, metricValue: leftMetricValue)
                        footpadSide(label: rightLabel, metricValue: rightMetricValue)
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        footpadSide(label: leftLabel, metricValue: leftMetricValue)
                        footpadSide(label: rightLabel, metricValue: rightMetricValue)
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

    private func footpadSide(label: String, metricValue: PevDashboardMetricValue) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 5) {
                Text(label)
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(PevDashboardColors.mutedText)
            }
            Text(metricValue.displayText)
                .font(.title3.weight(.black))
                .foregroundStyle(PevDashboardColors.primaryText)
                .monospacedDigit()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
