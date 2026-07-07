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

public struct PevDashboardStatusPill: View {
    let title: String
    let scale: CGFloat
    let fill: Color
    let foreground: Color
    let fontSize: CGFloat
    let horizontalPadding: CGFloat
    let height: CGFloat
    let fixedHorizontal: Bool

    public init(
        title: String,
        scale: CGFloat,
        fill: Color,
        foreground: Color = .black,
        fontSize: CGFloat = 14,
        horizontalPadding: CGFloat = 12,
        height: CGFloat = 30,
        fixedHorizontal: Bool = false
    ) {
        self.title = title
        self.scale = scale
        self.fill = fill
        self.foreground = foreground
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
            .frame(height: height * scale)
            .background(Capsule().fill(fill))
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
