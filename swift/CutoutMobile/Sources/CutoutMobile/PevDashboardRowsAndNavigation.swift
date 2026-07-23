import SwiftUI

public struct PevDashboardKeyValueRow: Identifiable {
    public let id: String
    public let label: String
    public let metricValue: PevDashboardMetricValue
    public var value: String { metricValue.displayText }
    public var accessibilityValueText: String { metricValue.accessibilityText }
    public let valueColor: Color?

    public init(id: String, label: String, value: String, valueColor: Color? = nil) {
        self.init(
            id: id,
            label: label,
            metricValue: .available(display: value, accessibility: value),
            valueColor: valueColor
        )
    }

    public init(
        id: String,
        label: String,
        metricValue: PevDashboardMetricValue,
        valueColor: Color? = nil
    ) {
        self.id = id
        self.label = label
        self.metricValue = metricValue
        self.valueColor = valueColor
    }
}

public struct PevDashboardKeyValueRows: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let rows: [PevDashboardKeyValueRow]
    let verticalPadding: CGFloat

    public init(
        rows: [PevDashboardKeyValueRow],
        verticalPadding: CGFloat = 12
    ) {
        self.rows = rows
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
                .accessibilityValue(row.accessibilityValueText)

                if row.id != rows.last?.id {
                    Rectangle()
                        .fill(PevDashboardColors.cardStroke)
                        .frame(height: 1)
                        .accessibilityHidden(true)
                }
            }
        }
        .padding(.horizontal, 22)
        .padding(.vertical, verticalPadding)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 22
            )
        )
    }

    private func keyValueLabel(_ row: PevDashboardKeyValueRow) -> some View {
        Text(row.label)
            .font(.subheadline.weight(.bold))
            .foregroundStyle(PevDashboardColors.mutedText)
    }

    private func keyValueValue(_ row: PevDashboardKeyValueRow) -> some View {
        Text(row.value)
            .font(.headline.weight(.black))
            .foregroundStyle(row.valueColor ?? PevDashboardColors.primaryText)
            .monospacedDigit()
    }
}


public struct PevDashboardSectionLabel: View {
    let title: String
    let font: Font

    public init(
        title: String,
        font: Font = .subheadline.weight(.semibold)
    ) {
        self.title = title
        self.font = font
    }

    public var body: some View {
        Text(title)
            .font(font)
            .foregroundStyle(PevDashboardColors.mutedText)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityHeading(.h2)
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
    let symbolName: String?
    @State private var phase = 0

    public init(title: String, isScanning: Bool, symbolName: String? = nil) {
        self.title = title
        self.isScanning = isScanning
        self.symbolName = symbolName
    }

    static func shouldAnimate(isScanning: Bool, reduceMotion: Bool) -> Bool {
        isScanning && !reduceMotion
    }

    static func showsIndicators(isScanning: Bool) -> Bool {
        isScanning
    }

    public var body: some View {
        HStack {
            Text(title)
                .font(.headline.weight(.bold))
                .fixedSize(horizontal: false, vertical: true)
                .layoutPriority(1)
            Spacer(minLength: 12)
            if let symbolName {
                Image(systemName: symbolName)
                    .font(.title3.weight(.bold))
                    .foregroundStyle(.yellow)
                    .accessibilityHidden(true)
            } else if Self.showsIndicators(isScanning: isScanning) {
                HStack(spacing: 9) {
                    ForEach(0..<3, id: \.self) { index in
                        Circle()
                            .frame(width: 13, height: 13)
                            .opacity(index == phase ? 1 : 0.32)
                    }
                }
                .foregroundStyle(.yellow)
                .accessibilityHidden(true)
            }
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
                do {
                    try await Task.sleep(for: .milliseconds(260))
                } catch {
                    return
                }
                phase = (phase + 1) % 3
            }
        }
    }
}
