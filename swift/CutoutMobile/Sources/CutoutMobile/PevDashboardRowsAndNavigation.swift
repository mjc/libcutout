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
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

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
                Group {
                    if dynamicTypeSize.isAccessibilitySize {
                        VStack(alignment: .leading, spacing: 4 * scale) {
                            keyValueLabel(row)
                            keyValueValue(row)
                        }
                    } else {
                        HStack {
                            keyValueLabel(row)
                            Spacer()
                            keyValueValue(row)
                        }
                    }
                }
                .frame(minHeight: 31 * scale)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(row.label)
                .accessibilityValue(row.value)

                if row.id != rows.last?.id {
                    Rectangle()
                        .fill(stroke)
                        .frame(height: 1)
                        .accessibilityHidden(true)
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

    private func keyValueLabel(_ row: PevDashboardKeyValueRow) -> some View {
        Text(row.label)
            .font(.subheadline.weight(.bold))
            .foregroundStyle(labelColor)
    }

    private func keyValueValue(_ row: PevDashboardKeyValueRow) -> some View {
        Text(row.value)
            .font(.headline.weight(.black))
            .foregroundStyle(row.valueColor ?? valueColor)
            .monospacedDigit()
    }
}


public struct PevDashboardReadbackRow: Identifiable {
    public let id: String
    public let label: String
    public let value: String
    public let detail: String

    public init(id: String, label: String, value: String, detail: String) {
        self.id = id
        self.label = label
        self.value = value
        self.detail = detail
    }
}

public struct PevDashboardReadbackRows: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let rows: [PevDashboardReadbackRow]
    let scale: CGFloat
    let emptyLabel: String?
    let emptyValue: String?

    public init(
        rows: [PevDashboardReadbackRow],
        scale: CGFloat,
        emptyLabel: String? = nil,
        emptyValue: String? = nil
    ) {
        self.rows = rows
        self.scale = scale
        self.emptyLabel = emptyLabel
        self.emptyValue = emptyValue
    }

    public var body: some View {
        VStack(spacing: 0) {
            if rows.isEmpty, let emptyLabel, let emptyValue {
                PevDashboardKeyValueRows(
                    rows: [PevDashboardKeyValueRow(id: emptyLabel, label: emptyLabel, value: emptyValue)],
                    scale: scale,
                    verticalPadding: 6
                )
            } else {
                ForEach(rows) { row in
                    VStack(alignment: .leading, spacing: 5 * scale) {
                        Group {
                            if dynamicTypeSize.isAccessibilitySize {
                                VStack(alignment: .leading, spacing: 4 * scale) {
                                    readbackLabel(row)
                                    readbackValue(row)
                                }
                            } else {
                                HStack {
                                    readbackLabel(row)
                                    Spacer()
                                    readbackValue(row)
                                }
                            }
                        }
                        Text(row.detail)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(PevDashboardColors.mutedText)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(.vertical, 10 * scale)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(row.label)
                    .accessibilityValue(
                        pevDashboardAccessibilityValue([row.value, row.detail])
                    )

                    if row.id != rows.last?.id {
                        Rectangle()
                            .fill(PevDashboardColors.cardStroke)
                            .frame(height: 1)
                            .accessibilityHidden(true)
                    }
                }
                .padding(.horizontal, 22 * scale)
                .padding(.vertical, 6 * scale)
                .background(PevDashboardCardBackground(cornerRadius: 22 * scale))
            }
        }
    }

    private func readbackLabel(_ row: PevDashboardReadbackRow) -> some View {
        Text(row.label)
            .font(.subheadline.weight(.bold))
            .foregroundStyle(PevDashboardColors.mutedText)
    }

    private func readbackValue(_ row: PevDashboardReadbackRow) -> some View {
        Text(row.value)
            .font(.headline.weight(.black))
            .monospacedDigit()
            .foregroundStyle(PevDashboardColors.primaryText)
    }
}

public struct PevDashboardSectionLabel: View {
    let title: String
    let font: Font
    let color: Color

    public init(
        title: String,
        font: Font = .subheadline.weight(.semibold),
        color: Color = PevDashboardColors.mutedText
    ) {
        self.title = title
        self.font = font
        self.color = color
    }

    public var body: some View {
        Text(title)
            .font(font)
            .foregroundStyle(color)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityAddTraits(.isHeader)
    }
}

public struct PevDashboardStatusPill: View {
    @Environment(\.colorSchemeContrast) private var colorSchemeContrast

    let title: String
    let scale: CGFloat
    let fill: Color
    let foreground: Color
    let stroke: Color?
    let width: CGFloat?
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
        self.horizontalPadding = horizontalPadding
        self.height = height
        self.fixedHorizontal = fixedHorizontal
    }

    public var body: some View {
        Text(title)
            .font(.callout.weight(.black))
            .foregroundStyle(foreground)
            .fixedSize(horizontal: fixedHorizontal, vertical: true)
            .padding(.horizontal, horizontalPadding * scale)
            .frame(minWidth: width.map { $0 * scale })
            .frame(minHeight: height * scale)
            .background(
                Capsule()
                    .fill(fill)
                    .overlay(
                        Capsule().stroke(
                            stroke ?? .clear,
                            lineWidth: stroke == nil
                                ? 0
                                : pevDashboardResolvedLineWidth(base: 1, contrast: colorSchemeContrast)
                        )
                    )
            )
    }
}


public struct PevDashboardScanningPill: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    let title: String
    let isScanning: Bool
    let scale: CGFloat
    @State private var phase = 0

    public init(title: String, isScanning: Bool, scale: CGFloat) {
        self.title = title
        self.isScanning = isScanning
        self.scale = scale
    }

    static func shouldAnimate(isScanning: Bool, reduceMotion: Bool) -> Bool {
        isScanning && !reduceMotion
    }

    public var body: some View {
        HStack {
            Text(title)
                .font(.headline.weight(.bold))
                .fixedSize(horizontal: false, vertical: true)
                .layoutPriority(1)
            Spacer(minLength: 12 * scale)
            HStack(spacing: 9 * scale) {
                ForEach(0..<3, id: \.self) { index in
                    Circle()
                        .frame(width: 13 * scale, height: 13 * scale)
                        .opacity(!isScanning || index == phase ? 1 : 0.32)
                }
            }
            .foregroundStyle(.yellow)
            .accessibilityHidden(true)
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 14 * scale)
        .frame(minHeight: 64 * scale)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 28 * scale))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .task(id: Self.shouldAnimate(isScanning: isScanning, reduceMotion: reduceMotion)) {
            guard Self.shouldAnimate(isScanning: isScanning, reduceMotion: reduceMotion) else {
                return
            }
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(260))
                phase = (phase + 1) % 3
            }
        }
    }
}

public struct PevDashboardTabLabel: View {
    let title: String
    let isSelected: Bool
    let scale: CGFloat
    let selectedColor: Color
    let unselectedColor: Color
    let indicatorWidth: CGFloat
    let indicatorHeight: CGFloat
    let spacing: CGFloat

    public init(
        title: String,
        isSelected: Bool,
        scale: CGFloat,
        selectedColor: Color,
        unselectedColor: Color,
        indicatorWidth: CGFloat = 28,
        indicatorHeight: CGFloat = 4,
        spacing: CGFloat = 8
    ) {
        self.title = title
        self.isSelected = isSelected
        self.scale = scale
        self.selectedColor = selectedColor
        self.unselectedColor = unselectedColor
        self.indicatorWidth = indicatorWidth
        self.indicatorHeight = indicatorHeight
        self.spacing = spacing
    }

    public var body: some View {
        VStack(spacing: spacing * scale) {
            Text(title)
                .font(.caption.weight(isSelected ? .black : .semibold))
                .foregroundStyle(isSelected ? selectedColor : unselectedColor)
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
                .frame(maxWidth: .infinity)
                .frame(height: 1)
                .accessibilityHidden(true)

            HStack(spacing: 0) {
                ForEach(tabs) { tab in
                    tabContent(tab)
                }
            }
        }
        .frame(minHeight: 58 * scale, alignment: .top)
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private func tabContent(_ tab: PevScreenTab) -> some View {
        Button {
            guard let destination = tab.destinationTarget else { return }
            selectTarget(destination)
        } label: {
            tabLabel(tab)
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity, minHeight: 44)
        .disabled(!tab.isEnabled || tab.destinationTarget == nil)
        .accessibilityIdentifier("dashboard.nav.\(tab.title.lowercased())")
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
        .accessibilityValue(tab.isSelected ? "Selected" : tab.isEnabled ? "Available" : "Unavailable")
        .accessibilityHint(tab.disabledReason ?? "")
        .accessibilityAddTraits(tab.isSelected ? .isSelected : [])
    }
}
