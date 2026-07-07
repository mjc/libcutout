import CutoutMobile
import SwiftUI

struct BmsChip: View {
    let title: String
    let accent: MockupAccent
    let scale: CGFloat
    let maxWidth: CGFloat?

    var body: some View {
        Text(title)
            .font(.system(size: 15 * scale, weight: .bold))
            .foregroundStyle(.black.opacity(accent == .green ? 0.82 : 0.92))
            .lineLimit(1)
            .minimumScaleFactor(0.72)
            .padding(.horizontal, 16 * scale)
            .frame(maxWidth: maxWidth, minHeight: 30 * scale)
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
                selectedColor: MockupColors.yellow,
                unselectedColor: MockupColors.muted,
                fontSize: 15,
                indicatorWidth: 24,
                indicatorHeight: 3,
                spacing: 7
            )
        }
        .buttonStyle(.plain)
        .disabled(action == nil)
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
                .font(.system(size: 14 * scale, weight: .medium))
                .foregroundStyle(MockupColors.muted)
            Text(groupVoltageText(group))
                .font(.system(size: 20 * scale, weight: .black))
                .monospacedDigit()
                .minimumScaleFactor(0.84)
        }
        .frame(maxWidth: .infinity)
        .frame(height: 70 * scale)
        .background(PevDashboardCardBackground(cornerRadius: 10 * scale, stroke: strokeColor, lineWidth: 1.2))
    }

    private var strokeColor: Color {
        if isSelected {
            return MockupColors.yellow
        }
        if isHighlighted {
            return MockupColors.orange
        }
        return MockupColors.green
    }
}

struct BmsStripCell: View {
    let group: BmsGroupSnapshot
    let isHighlighted: Bool
    let scale: CGFloat

    var body: some View {
        VStack(spacing: 2 * scale) {
            Text(String(format: "%02d", group.index))
                .font(.system(size: 8 * scale, weight: .medium))
                .foregroundStyle(MockupColors.muted)
            Text(groupVoltageText(group))
                .font(.system(size: 9 * scale, weight: .black))
                .monospacedDigit()
                .minimumScaleFactor(0.7)
        }
        .frame(width: 31 * scale, height: 44 * scale)
        .background(PevDashboardCardBackground(cornerRadius: 8 * scale, stroke: strokeColor, lineWidth: 1.2))
    }

    private var strokeColor: Color {
        switch group.alertLevel {
        case .critical:
            MockupColors.warningStroke
        case .warning:
            MockupColors.orange
        case .nominal, .unknown:
            isHighlighted ? MockupColors.orange : MockupColors.green
        }
    }
}

struct BmsGroupIndexCell: View {
    let group: BmsGroupSnapshot
    let isSelected: Bool
    let scale: CGFloat

    var body: some View {
        Text("\(group.index)")
            .font(.system(size: 14 * scale, weight: .medium))
            .foregroundStyle(MockupColors.muted)
            .frame(maxWidth: .infinity)
            .frame(height: 34 * scale)
            .background(PevDashboardCardBackground(cornerRadius: 8 * scale, stroke: isSelected ? MockupColors.orange : MockupColors.green, lineWidth: 1.2))
    }
}

struct BmsModeChip: View {
    let title: String
    let isSelected: Bool
    let scale: CGFloat

    var body: some View {
        Text(title)
            .font(.system(size: 15 * scale, weight: .bold))
            .foregroundStyle(isSelected ? .black : MockupColors.primaryText)
            .padding(.horizontal, 16 * scale)
            .frame(height: 32 * scale)
            .background(Capsule().fill(isSelected ? MockupColors.yellow : MockupColors.iconFill))
    }
}
