import CutoutMobile
import SwiftUI

func bmsGroupAccessibilityValue(_ value: String, isHighlighted: Bool) -> String {
    isHighlighted
        ? localizedAppText("bms.group.accessibility.highlighted", value)
        : value
}

private extension View {
    func bmsGroupAccessibility(
        _ group: BmsGroupSnapshot,
        isHighlighted: Bool
    ) -> some View {
        accessibilityElement(children: .combine)
            .accessibilityLabel(group.accessibilityLabel)
            .accessibilityValue(
                bmsGroupAccessibilityValue(group.accessibilityValue, isHighlighted: isHighlighted)
            )
            .accessibilityHint(group.detailSelectionAccessibilityHint)
            .accessibilityIdentifier("bms.group.\(group.index)")
            .accessibilityAddTraits(isHighlighted ? .isSelected : [])
    }
}

struct BmsStripCell: View {
    @ScaledMetric(relativeTo: .caption) private var minimumHeight = 60.0

    let group: BmsGroupSnapshot
    let isHighlighted: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 2) {
                HStack(spacing: 2) {
                    if let pack = group.packNumber, let reading = group.packReadingIndex {
                        Text("P\(pack) · \(reading.formatted(.number.precision(.integerLength(2...))))")
                    } else {
                        Text(group.index, format: .number.precision(.integerLength(2...)))
                    }
                    BmsAlertIndicator(alertLevel: group.alertLevel)
                }
                .font(.caption2)
                .foregroundStyle(PevColors.muted)
                Text(group.voltageMetricValue.displayText)
                    .font(.callout.weight(.semibold))
                    .monospacedDigit()
            }
            .frame(maxWidth: .infinity, minHeight: minimumHeight)
            .background(PevDashboardCardBackground(cornerRadius: 8, stroke: strokeColor, lineWidth: 1.2))
            .overlay(alignment: .topTrailing) {
                if isHighlighted {
                    Image(systemName: "scope")
                        .font(.caption2.weight(.bold))
                        .padding(4)
                        .accessibilityHidden(true)
                }
            }
        }
        .buttonStyle(.plain)
        .bmsGroupAccessibility(group, isHighlighted: isHighlighted)
    }

    private var strokeColor: Color {
        switch group.alertLevel {
        case .critical:
            PevColors.warningStroke
        case .warning:
            PevColors.orange
        case .nominal, .unknown:
            PevColors.muted.opacity(0.2)
        }
    }
}

struct BmsAlertIndicator: View {
    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor

    let alertLevel: BmsAlertLevel

    static func systemImageName(
        for alertLevel: BmsAlertLevel,
        differentiateWithoutColor: Bool
    ) -> String? {
        return switch alertLevel {
        case .critical:
            differentiateWithoutColor ? "exclamationmark.triangle.fill" : "exclamationmark.triangle"
        case .warning:
            "exclamationmark.triangle"
        case .unknown:
            "questionmark.circle"
        case .nominal:
            nil
        }
    }

    @ViewBuilder
    var body: some View {
        if let systemImageName = Self.systemImageName(
            for: alertLevel,
            differentiateWithoutColor: differentiateWithoutColor
        ) {
            Image(systemName: systemImageName)
                .accessibilityHidden(true)
        }
    }
}
