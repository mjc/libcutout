import CutoutMobile
import SwiftUI

func bmsGroupAccessibilityValue(_ value: String, isHighlighted: Bool) -> String {
    isHighlighted
        ? localizedAppText("bms.group.accessibility.highlighted", value)
        : value
}

struct BmsChip: View {
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    let title: String
    let accent: PevAccent

    static func usesGlassEffect(reduceTransparency: Bool) -> Bool {
        !reduceTransparency
    }

    var body: some View {
        Text(title)
            .font(.callout.weight(.bold))
            .foregroundStyle(.black.opacity(accent == .green ? 0.82 : 0.92))
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, minHeight: 44)
            .background(chipBackground)
    }

    @ViewBuilder
    private var chipBackground: some View {
        if #available(iOS 26, macOS 26, *), Self.usesGlassEffect(reduceTransparency: reduceTransparency) {
            Capsule()
                .fill(accent.color)
                .glassEffect(.regular.tint(accent.color.opacity(0.78)), in: .capsule)
        } else {
            Capsule().fill(accent.color)
        }
    }
}

struct BmsGroupCell: View {
    let group: BmsGroupSnapshot
    let isHighlighted: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 8) {
                HStack(spacing: 4) {
                    Text(group.index, format: .number)
                    BmsAlertIndicator(alertLevel: group.alertLevel)
                }
                .font(.subheadline)
                .foregroundStyle(PevColors.muted)
                Text(groupVoltageText(group))
                    .font(.title3.weight(.black))
                    .monospacedDigit()
            }
            .frame(maxWidth: .infinity)
            .frame(minHeight: 70)
            .background(PevDashboardCardBackground(cornerRadius: 10, stroke: strokeColor, lineWidth: 1.2))
            .overlay(alignment: .topTrailing) {
                if isHighlighted {
                    Image(systemName: "scope")
                        .font(.caption2.weight(.bold))
                        .padding(5)
                        .accessibilityHidden(true)
                }
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(group.accessibilityLabel)
        .accessibilityValue(
            bmsGroupAccessibilityValue(group.accessibilityValue, isHighlighted: isHighlighted)
        )
        .accessibilityHint(group.detailSelectionAccessibilityHint)
        .accessibilityIdentifier("bms.group.\(group.index)")
    }

    private var strokeColor: Color {
        if isHighlighted {
            return PevColors.orange
        }
        return PevColors.green
    }
}

struct BmsStripCell: View {
    let group: BmsGroupSnapshot
    let isHighlighted: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 2) {
                HStack(spacing: 2) {
                    Text(group.index, format: .number.precision(.integerLength(2...)))
                    BmsAlertIndicator(alertLevel: group.alertLevel)
                }
                .font(.caption2)
                .foregroundStyle(PevColors.muted)
                Text(groupVoltageText(group))
                    .font(.caption.weight(.black))
                    .monospacedDigit()
            }
            .frame(maxWidth: .infinity, minHeight: 60)
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
        .accessibilityLabel(group.accessibilityLabel)
        .accessibilityValue(
            bmsGroupAccessibilityValue(group.accessibilityValue, isHighlighted: isHighlighted)
        )
        .accessibilityHint(group.detailSelectionAccessibilityHint)
        .accessibilityIdentifier("bms.group.\(group.index)")
    }

    private var strokeColor: Color {
        switch group.alertLevel {
        case .critical:
            PevColors.warningStroke
        case .warning:
            PevColors.orange
        case .nominal, .unknown:
            isHighlighted ? PevColors.orange : PevColors.green
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

struct BmsGroupIndexCell: View {
    let group: BmsGroupSnapshot
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(group.index, format: .number)
                .font(.body)
                .foregroundStyle(PevColors.muted)
                .frame(maxWidth: .infinity)
                .frame(minHeight: 44)
                .background(PevDashboardCardBackground(cornerRadius: 8, stroke: isSelected ? PevColors.orange : PevColors.green, lineWidth: 1.2))
                .overlay(alignment: .topTrailing) {
                    if isSelected {
                        Image(systemName: "checkmark.circle.fill")
                            .font(.caption2.weight(.bold))
                            .padding(4)
                            .accessibilityHidden(true)
                    }
                }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(group.accessibilityLabel)
        .accessibilityValue(group.accessibilityValue)
        .accessibilityHint(group.detailSelectionAccessibilityHint)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

struct BmsModeChip: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.body.weight(.bold))
            .foregroundStyle(PevColors.primaryText)
            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
    }
}

struct BmsModeGrid: View {
    let modes: [PevBmsMode]

    var body: some View {
        PevDashboardGrid(
            columns: [GridItem(.adaptive(minimum: 100), spacing: 10)],
            spacing: 10
        ) {
            ForEach(modes) { mode in
                BmsModeChip(title: mode.title)
            }
        }
    }
}
