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
    let fill: Color
    let stroke: Color
    let labelColor: Color
    let valueColor: Color
    let cornerRadius: CGFloat
    let verticalPadding: CGFloat

    public init(
        rows: [PevDashboardKeyValueRow],
        fill: Color = PevDashboardColors.cardFill,
        stroke: Color = PevDashboardColors.cardStroke,
        labelColor: Color = PevDashboardColors.mutedText,
        valueColor: Color = PevDashboardColors.primaryText,
        cornerRadius: CGFloat = 22,
        verticalPadding: CGFloat = 12
    ) {
        self.rows = rows
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
                        VStack(alignment: .leading, spacing: 4) {
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
                .frame(minHeight: 31)
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
        .padding(.horizontal, 22)
        .padding(.vertical, verticalPadding)
        .background(
            PevDashboardCardBackground(
                cornerRadius: cornerRadius,
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
    let emptyLabel: String?
    let emptyValue: String?

    public init(
        rows: [PevDashboardReadbackRow],
        emptyLabel: String? = nil,
        emptyValue: String? = nil
    ) {
        self.rows = rows
        self.emptyLabel = emptyLabel
        self.emptyValue = emptyValue
    }

    public var body: some View {
        VStack(spacing: 0) {
            if rows.isEmpty, let emptyLabel, let emptyValue {
                PevDashboardKeyValueRows(
                    rows: [PevDashboardKeyValueRow(id: emptyLabel, label: emptyLabel, value: emptyValue)],
                    verticalPadding: 6
                )
            } else {
                ForEach(rows) { row in
                    VStack(alignment: .leading, spacing: 5) {
                        Group {
                            if dynamicTypeSize.isAccessibilitySize {
                                VStack(alignment: .leading, spacing: 4) {
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
                    .padding(.vertical, 10)
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
                .padding(.horizontal, 22)
                .padding(.vertical, 6)
                .background(PevDashboardCardBackground(cornerRadius: 22))
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
    let fill: Color
    let foreground: Color
    let stroke: Color?
    let width: CGFloat?
    let horizontalPadding: CGFloat
    let height: CGFloat
    let fixedHorizontal: Bool

    public init(
        title: String,
        fill: Color,
        foreground: Color = .black,
        stroke: Color? = nil,
        width: CGFloat? = nil,
        horizontalPadding: CGFloat = 12,
        height: CGFloat = 30,
        fixedHorizontal: Bool = false
    ) {
        self.title = title
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
            .font(.system(.callout, design: .default, weight: .black))
            .foregroundStyle(foreground)
            .fixedSize(horizontal: fixedHorizontal, vertical: false)
            .padding(.horizontal, horizontalPadding)
            .frame(minWidth: width)
            .frame(minHeight: height)
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
    @State private var phase = 0

    public init(title: String, isScanning: Bool) {
        self.title = title
        self.isScanning = isScanning
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
            Spacer(minLength: 12)
            HStack(spacing: 9) {
                ForEach(0..<3, id: \.self) { index in
                    Circle()
                        .frame(width: 13, height: 13)
                        .opacity(!isScanning || index == phase ? 1 : 0.32)
                }
            }
            .foregroundStyle(.yellow)
            .accessibilityHidden(true)
        }
        .foregroundStyle(PevDashboardColors.primaryText)
        .padding(.horizontal, 22)
        .padding(.vertical, 14)
        .frame(minHeight: 64)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 28))
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
