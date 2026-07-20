import CutoutMobile
import SwiftUI

struct BmsChip: View {
    let title: String
    let accent: PevAccent

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
        if #available(iOS 26, macOS 26, *) {
            Capsule()
                .fill(accent.color)
                .glassEffect(.regular.tint(accent.color.opacity(0.78)), in: .capsule)
        } else {
            Capsule().fill(accent.color)
        }
    }
}

struct BmsBottomTab: View {
    let title: String
    let isSelected: Bool
    let scale: CGFloat
    let action: (() -> Void)?

    var body: some View {
        Button {
            action?()
        } label: {
            PevDashboardTabLabel(
                title: title,
                isSelected: isSelected,
                scale: scale,
                selectedColor: PevColors.yellow,
                unselectedColor: PevColors.muted,
                fontSize: 15,
                indicatorWidth: 24,
                indicatorHeight: 3,
                spacing: 7
            )
        }
        .buttonStyle(.plain)
        .disabled(action == nil)
        .frame(minHeight: 44)
        .accessibilityValue(isSelected ? "Selected" : action == nil ? "Unavailable" : "Available")
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

struct BmsGroupCell: View {
    let group: BmsGroupSnapshot
    let isHighlighted: Bool
    let isSelected: Bool
    let scale: CGFloat

    var body: some View {
        VStack(spacing: 8 * scale) {
            Text("\(group.index)")
                .font(.subheadline)
                .foregroundStyle(PevColors.muted)
            Text(groupVoltageText(group))
                .font(.title3.weight(.black))
                .monospacedDigit()
        }
        .frame(maxWidth: .infinity)
        .frame(minHeight: 70 * scale)
        .background(PevDashboardCardBackground(cornerRadius: 10 * scale, stroke: strokeColor, lineWidth: 1.2))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(group.accessibilityLabel)
        .accessibilityValue(group.accessibilityValue + (isHighlighted ? ", highlighted" : ""))
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }

    private var strokeColor: Color {
        if isSelected {
            return PevColors.yellow
        }
        if isHighlighted {
            return PevColors.orange
        }
        return PevColors.green
    }
}

struct BmsStripCell: View {
    let group: BmsGroupSnapshot
    let isHighlighted: Bool
    let scale: CGFloat

    var body: some View {
        VStack(spacing: 2 * scale) {
            Text(String(format: "%02d", group.index))
                .font(.caption2)
                .foregroundStyle(PevColors.muted)
            Text(groupVoltageText(group))
                .font(.caption.weight(.black))
                .monospacedDigit()
        }
        .frame(maxWidth: .infinity, minHeight: 60 * scale)
        .background(PevDashboardCardBackground(cornerRadius: 8 * scale, stroke: strokeColor, lineWidth: 1.2))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(group.accessibilityLabel)
        .accessibilityValue(group.accessibilityValue + (isHighlighted ? ", highlighted" : ""))
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

struct BmsGroupIndexCell: View {
    let group: BmsGroupSnapshot
    let isSelected: Bool
    let scale: CGFloat
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text("\(group.index)")
                .font(.body)
                .foregroundStyle(PevColors.muted)
                .frame(maxWidth: .infinity)
                .frame(minHeight: 44)
                .background(PevDashboardCardBackground(cornerRadius: 8 * scale, stroke: isSelected ? PevColors.orange : PevColors.green, lineWidth: 1.2))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(group.accessibilityLabel)
        .accessibilityValue(group.accessibilityValue)
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
