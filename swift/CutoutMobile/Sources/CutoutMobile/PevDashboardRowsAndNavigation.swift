import SwiftUI

public struct PevDashboardKeyValueRow: Identifiable {
    public let id: String
    public let label: String
    public let value: String
    public let valueColor: Color?

    public init(id: String, label: String, value: String, valueColor: Color? = nil) {
        self.id = id
        self.label = label
        self.value = value
        self.valueColor = valueColor
    }
}

public struct PevDashboardKeyValueRows: View {
    let rows: [PevDashboardKeyValueRow]
    let scale: CGFloat
    let fill: Color
    let stroke: Color
    let labelColor: Color
    let valueColor: Color
    let cornerRadius: CGFloat
    let verticalPadding: CGFloat

    public init(
        rows: [PevDashboardKeyValueRow],
        scale: CGFloat,
        fill: Color = PevDashboardColors.cardFill,
        stroke: Color = PevDashboardColors.cardStroke,
        labelColor: Color = PevDashboardColors.mutedText,
        valueColor: Color = PevDashboardColors.primaryText,
        cornerRadius: CGFloat = 22,
        verticalPadding: CGFloat = 12
    ) {
        self.rows = rows
        self.scale = scale
        self.fill = fill
        self.stroke = stroke
        self.labelColor = labelColor
        self.valueColor = valueColor
        self.cornerRadius = cornerRadius
        self.verticalPadding = verticalPadding
    }

    public var body: some View {
        VStack(spacing: 0) {
            ForEach(rows) { row in
                HStack {
                    Text(row.label)
                        .font(.system(size: 14 * scale, weight: .bold))
                        .foregroundStyle(labelColor)
                    Spacer()
                    Text(row.value)
                        .font(.system(size: 15 * scale, weight: .black))
                        .foregroundStyle(row.valueColor ?? valueColor)
                        .monospacedDigit()
                }
                .frame(height: 31 * scale)

                if row.id != rows.last?.id {
                    Rectangle()
                        .fill(stroke)
                        .frame(height: 1)
                }
            }
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, verticalPadding * scale)
        .background(
            PevDashboardCardBackground(
                cornerRadius: cornerRadius * scale,
                fill: fill,
                stroke: stroke
            )
        )
    }
}

public struct PevDashboardSectionLabel: View {
    let title: String
    let scale: CGFloat
    let fontSize: CGFloat
    let weight: Font.Weight
    let color: Color

    public init(
        title: String,
        scale: CGFloat,
        fontSize: CGFloat = 15,
        weight: Font.Weight = .semibold,
        color: Color = PevDashboardColors.mutedText
    ) {
        self.title = title
        self.scale = scale
        self.fontSize = fontSize
        self.weight = weight
        self.color = color
    }

    public var body: some View {
        Text(title)
            .font(.system(size: fontSize * scale, weight: weight))
            .foregroundStyle(color)
            .lineLimit(1)
            .minimumScaleFactor(0.8)
    }
}

public struct PevDashboardStatusPill: View {
    let title: String
    let scale: CGFloat
    let fill: Color
    let foreground: Color
    let stroke: Color?
    let width: CGFloat?
    let fontSize: CGFloat
    let horizontalPadding: CGFloat
    let height: CGFloat
    let fixedHorizontal: Bool

    public init(
        title: String,
        scale: CGFloat,
        fill: Color,
        foreground: Color = .black,
        stroke: Color? = nil,
        width: CGFloat? = nil,
        fontSize: CGFloat = 14,
        horizontalPadding: CGFloat = 12,
        height: CGFloat = 30,
        fixedHorizontal: Bool = false
    ) {
        self.title = title
        self.scale = scale
        self.fill = fill
        self.foreground = foreground
        self.stroke = stroke
        self.width = width
        self.fontSize = fontSize
        self.horizontalPadding = horizontalPadding
        self.height = height
        self.fixedHorizontal = fixedHorizontal
    }

    public var body: some View {
        Text(title)
            .font(.system(size: fontSize * scale, weight: .black))
            .foregroundStyle(foreground)
            .lineLimit(1)
            .minimumScaleFactor(0.75)
            .fixedSize(horizontal: fixedHorizontal, vertical: false)
            .padding(.horizontal, horizontalPadding * scale)
            .frame(width: width.map { $0 * scale })
            .frame(height: height * scale)
            .background(
                Capsule()
                    .fill(fill)
                    .overlay(
                        Capsule().stroke(stroke ?? .clear, lineWidth: stroke == nil ? 0 : 1)
                    )
            )
    }
}

public struct PevDashboardTabLabel: View {
    let title: String
    let isSelected: Bool
    let scale: CGFloat
    let selectedColor: Color
    let unselectedColor: Color
    let fontSize: CGFloat
    let indicatorWidth: CGFloat
    let indicatorHeight: CGFloat
    let spacing: CGFloat

    public init(
        title: String,
        isSelected: Bool,
        scale: CGFloat,
        selectedColor: Color,
        unselectedColor: Color,
        fontSize: CGFloat = 14,
        indicatorWidth: CGFloat = 28,
        indicatorHeight: CGFloat = 4,
        spacing: CGFloat = 8
    ) {
        self.title = title
        self.isSelected = isSelected
        self.scale = scale
        self.selectedColor = selectedColor
        self.unselectedColor = unselectedColor
        self.fontSize = fontSize
        self.indicatorWidth = indicatorWidth
        self.indicatorHeight = indicatorHeight
        self.spacing = spacing
    }

    public var body: some View {
        VStack(spacing: spacing * scale) {
            Text(title)
                .font(.system(size: fontSize * scale, weight: isSelected ? .black : .semibold))
                .foregroundStyle(isSelected ? selectedColor : unselectedColor)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
            Capsule()
                .fill(isSelected ? selectedColor : Color.clear)
                .frame(width: indicatorWidth * scale, height: indicatorHeight * scale)
        }
        .contentShape(Rectangle())
    }
}

public struct PevDashboardTabStrip: View {
    let tabs: [PevScreenTab]
    let scale: CGFloat
    let selectedColor: Color
    let unselectedColor: Color
    let selectTarget: (PevNavigationTarget) -> Void

    public init(
        tabs: [PevScreenTab],
        scale: CGFloat,
        selectedColor: Color,
        unselectedColor: Color,
        selectTarget: @escaping (PevNavigationTarget) -> Void
    ) {
        self.tabs = tabs
        self.scale = scale
        self.selectedColor = selectedColor
        self.unselectedColor = unselectedColor
        self.selectTarget = selectTarget
    }

    public var body: some View {
        VStack(spacing: 12 * scale) {
            Rectangle()
                .fill(PevDashboardColors.cardStroke)
                .frame(width: 254 * scale, height: 1)

            HStack(spacing: 0) {
                ForEach(tabs) { tab in
                    tabContent(tab)
                }
            }
            .frame(width: 254 * scale)
        }
        .frame(height: 58 * scale, alignment: .top)
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private func tabContent(_ tab: PevScreenTab) -> some View {
        if tab.isEnabled, let destination = tab.destinationTarget {
            Button {
                selectTarget(destination)
            } label: {
                tabLabel(tab)
            }
            .buttonStyle(.plain)
        } else {
            tabLabel(tab)
        }
    }

    private func tabLabel(_ tab: PevScreenTab) -> some View {
        PevDashboardTabLabel(
            title: tab.title,
            isSelected: tab.isSelected,
            scale: scale,
            selectedColor: selectedColor,
            unselectedColor: tab.isEnabled ? unselectedColor : unselectedColor.opacity(0.75)
        )
        .frame(maxWidth: .infinity)
        .opacity(tab.isEnabled ? 1 : 0.45)
        .accessibilityValue(tab.isEnabled ? "Available" : "Unavailable")
        .accessibilityHint(tab.isEnabled ? "" : "This tab is unavailable.")
        .accessibilityAddTraits(tab.isEnabled ? [] : .isButton)
    }
}
