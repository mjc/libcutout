import SwiftUI

public struct PevDashboardMetricTile: View {
    let label: String
    let value: String
    let unit: String
    let detail: String
    let accent: Color
    let scale: CGFloat
    let fill: Color
    let stroke: Color
    let labelColor: Color
    let valueColor: Color
    let unitColor: Color
    let detailColor: Color
    let cornerRadius: CGFloat
    let minHeight: CGFloat
    let labelFontSize: CGFloat
    let valueFontSize: CGFloat
    let unitFontSize: CGFloat
    let detailFontSize: CGFloat
    let valueMinimumScaleFactor: CGFloat

    var accessibilityValueText: String {
        pevDashboardAccessibilityValue([value, unit, detail])
    }

    public init(
        label: String,
        value: String,
        unit: String = "",
        detail: String = "",
        accent: Color,
        scale: CGFloat,
        fill: Color = PevDashboardColors.cardFill,
        stroke: Color = PevDashboardColors.cardStroke,
        labelColor: Color = PevDashboardColors.mutedText,
        valueColor: Color = PevDashboardColors.primaryText,
        unitColor: Color? = nil,
        detailColor: Color = PevDashboardColors.mutedText,
        cornerRadius: CGFloat = 20,
        minHeight: CGFloat = 106,
        labelFontSize: CGFloat = 14,
        valueFontSize: CGFloat = 25,
        unitFontSize: CGFloat = 13,
        detailFontSize: CGFloat = 13,
        valueMinimumScaleFactor: CGFloat = 0.82
    ) {
        self.label = label
        self.value = value
        self.unit = unit
        self.detail = detail
        self.accent = accent
        self.scale = scale
        self.fill = fill
        self.stroke = stroke
        self.labelColor = labelColor
        self.valueColor = valueColor
        self.unitColor = unitColor ?? accent
        self.detailColor = detailColor
        self.cornerRadius = cornerRadius
        self.minHeight = minHeight
        self.labelFontSize = labelFontSize
        self.valueFontSize = valueFontSize
        self.unitFontSize = unitFontSize
        self.detailFontSize = detailFontSize
        self.valueMinimumScaleFactor = valueMinimumScaleFactor
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            Text(label)
                .font(.system(size: labelFontSize * scale, weight: .bold))
                .foregroundStyle(labelColor)

            HStack(alignment: .firstTextBaseline, spacing: 4 * scale) {
                Text(value)
                    .font(.system(size: valueFontSize * scale, weight: .black))
                    .foregroundStyle(valueColor)
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(valueMinimumScaleFactor)
                Spacer(minLength: 4 * scale)
                if !unit.isEmpty {
                    Text(unit)
                        .font(.system(size: unitFontSize * scale, weight: .black))
                        .foregroundStyle(unitColor)
                }
            }

            if !detail.isEmpty {
                Text(detail)
                    .font(.system(size: detailFontSize * scale, weight: .bold))
                    .foregroundStyle(detailColor)
                    .lineLimit(1)
                    .minimumScaleFactor(0.68)
            }
        }
        .padding(.horizontal, 16 * scale)
        .padding(.vertical, 14 * scale)
        .frame(maxWidth: .infinity, minHeight: minHeight * scale, alignment: .topLeading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: cornerRadius * scale,
                fill: fill,
                stroke: stroke
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
    let accent: Color
    let fill: Color
    let stroke: Color
    let track: Color
    let textColor: Color
    let secondaryTextColor: Color
    let scale: CGFloat

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
        progress: Double,
        accent: Color,
        fill: Color = PevDashboardColors.cardFill,
        stroke: Color = PevDashboardColors.cardStroke,
        track: Color = PevDashboardColors.cardStroke,
        textColor: Color = PevDashboardColors.primaryText,
        secondaryTextColor: Color = PevDashboardColors.mutedText,
        scale: CGFloat
    ) {
        self.eyebrow = eyebrow
        self.value = value
        self.unit = unit
        self.detail = detail
        self.progress = progress
        self.accent = accent
        self.fill = fill
        self.stroke = stroke
        self.track = track
        self.textColor = textColor
        self.secondaryTextColor = secondaryTextColor
        self.scale = scale
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 2 * scale) {
            Text(eyebrow)
                .font(.system(size: 14 * scale, weight: .bold))
                .foregroundStyle(secondaryTextColor)
            HStack(alignment: .firstTextBaseline, spacing: 8 * scale) {
                Text(value)
                    .font(.system(size: 58 * scale, weight: .black))
                    .foregroundStyle(textColor)
                    .monospacedDigit()
                Text(unit)
                    .font(.system(size: 15 * scale, weight: .black))
                    .foregroundStyle(accent)
            }
            Text(detail)
                .font(.system(size: 13 * scale, weight: .bold))
                .foregroundStyle(secondaryTextColor)
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(track)
                    Capsule()
                        .fill(accent)
                        .frame(width: clampedProgress * proxy.size.width)
                }
            }
            .frame(height: 12 * scale)
        }
        .padding(.horizontal, 18 * scale)
        .padding(.vertical, 16 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 24 * scale,
                fill: fill,
                stroke: stroke
            )
        )
        .accessibilityRepresentation {
            ProgressView(value: clampedProgress)
                .accessibilityLabel(eyebrow)
                .accessibilityValue(accessibilityValueText)
        }
    }
}

public struct PevDashboardWideCard: View {
    let title: String?
    let value: String
    let detail: String?
    let accent: Color
    let fill: Color
    let stroke: Color
    let textColor: Color
    let secondaryTextColor: Color
    let scale: CGFloat

    var accessibilityLabelText: String {
        title ?? value
    }

    var accessibilityValueText: String {
        title == nil
            ? (detail ?? "")
            : pevDashboardAccessibilityValue([value, detail ?? ""])
    }

    public init(
        title: String?,
        value: String,
        detail: String?,
        accent: Color,
        fill: Color = PevDashboardColors.cardFill,
        stroke: Color = PevDashboardColors.cardStroke,
        textColor: Color = PevDashboardColors.primaryText,
        secondaryTextColor: Color = PevDashboardColors.mutedText,
        scale: CGFloat
    ) {
        self.title = title
        self.value = value
        self.detail = detail
        self.accent = accent
        self.fill = fill
        self.stroke = stroke
        self.textColor = textColor
        self.secondaryTextColor = secondaryTextColor
        self.scale = scale
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7 * scale) {
            if let title {
                Text(title)
                    .font(.system(size: 14 * scale, weight: .bold))
                    .foregroundStyle(secondaryTextColor)
            }
            Text(value)
                .font(.system(size: 25 * scale, weight: .black))
                .foregroundStyle(textColor)
                .lineLimit(2)
                .minimumScaleFactor(0.74)
            if let detail, !detail.isEmpty {
                Text(detail)
                    .font(.system(size: 13 * scale, weight: .black))
                    .foregroundStyle(accent)
                    .lineLimit(2)
                    .minimumScaleFactor(0.72)
            }
        }
        .padding(.horizontal, 18 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 24 * scale,
                fill: fill,
                stroke: stroke,
                lineWidth: 1.2
            )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityLabelText)
        .accessibilityValue(accessibilityValueText)
    }
}
