import SwiftUI

public struct PevDashboardMetricTile: View {
    @Environment(\.colorSchemeContrast) private var colorSchemeContrast
    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor
    @Environment(\.colorScheme) private var colorScheme
    let label: String
    let metricValue: PevDashboardMetricValue
    var value: String { metricValue.displayText }
    let unit: String
    let detail: String
    let detailColor: Color
    let cornerRadius: CGFloat
    let minHeight: CGFloat

    var accessibilityValueText: String {
        metricValue.accessibilityValue(unit: unit, detail: detail)
    }

    private var resolvedValueColor: Color {
        resolvedColor(PevDashboardColors.primaryText)
    }

    private func resolvedColor(_ color: Color) -> Color {
        guard colorSchemeContrast == .increased || differentiateWithoutColor else { return color }
        return colorScheme == .dark ? .white : .black
    }

    public init(
        label: String,
        metricValue: PevDashboardMetricValue,
        unit: String = "",
        detail: String = "",
        cornerRadius: CGFloat = 20,
        minHeight: CGFloat = 106
    ) {
        self.init(
            label: label,
            metricValue: metricValue,
            unit: unit,
            detail: detail,
            detailColor: PevDashboardColors.primaryText,
            cornerRadius: cornerRadius,
            minHeight: minHeight
        )
    }

    private init(
        label: String,
        metricValue: PevDashboardMetricValue,
        unit: String,
        detail: String,
        detailColor: Color,
        cornerRadius: CGFloat,
        minHeight: CGFloat
    ) {
        self.label = label
        self.metricValue = metricValue
        self.unit = unit
        self.detail = detail
        self.detailColor = detailColor
        self.cornerRadius = cornerRadius
        self.minHeight = minHeight
    }

    public init(
        _ tile: PevDashboardTile,
        cornerRadius: CGFloat = 20,
        minHeight: CGFloat = 106
    ) {
        self.init(
            label: tile.label,
            metricValue: tile.metricValue,
            unit: tile.unit,
            detail: tile.detail,
            detailColor: PevDashboardColors.mutedText,
            cornerRadius: cornerRadius,
            minHeight: minHeight
        )
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(label)
                .font(.subheadline.weight(.bold))
                .foregroundStyle(resolvedColor(PevDashboardColors.mutedText))
                .accessibilityHidden(true)

            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text(value)
                    .font(.title3.weight(.black))
                    .foregroundStyle(resolvedValueColor)
                    .monospacedDigit()
                    .accessibilityHidden(true)
                Spacer(minLength: 4)
                if !unit.isEmpty {
                    Text(unit)
                        .font(.subheadline.weight(.black))
                        .foregroundStyle(resolvedColor(PevDashboardColors.primaryText))
                        .accessibilityHidden(true)
                }
            }

            if !detail.isEmpty {
                Text(detail)
                    .font(.subheadline.weight(.bold))
                    .foregroundStyle(resolvedColor(detailColor))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityHidden(true)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity, minHeight: minHeight, alignment: .topLeading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: cornerRadius,
                fill: PevDashboardColors.cardFill,
                stroke: PevDashboardColors.cardStroke
            )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityValue(accessibilityValueText)
    }
}

public struct PevDashboardHeroCard: View {
    let eyebrow: String
    let value: String
    let unit: String
    let detail: String
    let progress: Double

    var clampedProgress: Double {
        max(0, min(1, progress))
    }

    var accessibilityValueText: String {
        pevDashboardAccessibilityValue([value, unit, detail])
    }

    public init(
        eyebrow: String,
        value: String,
        unit: String,
        detail: String,
        progress: Double
    ) {
        self.eyebrow = eyebrow
        self.value = value
        self.unit = unit
        self.detail = detail
        self.progress = progress
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(eyebrow)
                .font(.subheadline.weight(.bold))
                .foregroundStyle(PevDashboardColors.mutedText)
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(value)
                    .font(.largeTitle.weight(.black))
                    .foregroundStyle(PevDashboardColors.primaryText)
                    .monospacedDigit()
                Text(unit)
                    .font(.headline.weight(.black))
                    .foregroundStyle(PevDashboardColors.mutedText)
            }
            Text(detail)
                .font(.subheadline.weight(.bold))
                .foregroundStyle(PevDashboardColors.mutedText)
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(PevDashboardColors.cardStroke)
                    Capsule()
                        .fill(PevDashboardColors.primaryText)
                        .frame(width: clampedProgress * proxy.size.width)
                }
            }
            .frame(height: 12)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 24,
                fill: PevDashboardColors.cardFill,
                stroke: PevDashboardColors.cardStroke
            )
        )
        .accessibilityRepresentation {
            ProgressView(value: clampedProgress)
                .tint(PevDashboardColors.primaryText)
                .accessibilityLabel(eyebrow)
                .accessibilityValue(accessibilityValueText)
        }
    }
}

public struct PevDashboardWideCard: View {
    let title: String?
    let metricValue: PevDashboardMetricValue
    var value: String { metricValue.displayText }
    let detail: String?
    let stroke: Color

    var accessibilityLabelText: String {
        title ?? metricValue.accessibilityText
    }

    var accessibilityValueText: String {
        guard title != nil else { return detail ?? "" }
        return metricValue.accessibilityValue(unit: "", detail: detail ?? "")
    }

    public init(
        title: String?,
        metricValue: PevDashboardMetricValue,
        detail: String?,
        stroke: Color = PevDashboardColors.cardStroke
    ) {
        self.title = title
        self.metricValue = metricValue
        self.detail = detail
        self.stroke = stroke
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            if let title {
                Text(title)
                    .font(.subheadline.weight(.bold))
                    .foregroundStyle(PevDashboardColors.mutedText)
            }
            Text(value)
                .font(.title3.weight(.black))
                .foregroundStyle(PevDashboardColors.primaryText)
            if let detail, !detail.isEmpty {
                Text(detail)
                    .font(.subheadline.weight(.black))
                    .foregroundStyle(PevDashboardColors.mutedText)
            }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 24,
                stroke: stroke,
                lineWidth: 1.2
            )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityLabelText)
        .accessibilityValue(accessibilityValueText)
    }
}
