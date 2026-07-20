import SwiftUI

public struct PevDashboardProgressBar: View {
    let label: String
    let value: String
    let progress: Double
    let accent: Color
    let track: Color
    let labelColor: Color
    let valueColor: Color
    let scale: CGFloat
    let labelFontSize: CGFloat
    let valueFontSize: CGFloat
    let height: CGFloat

    var clampedProgress: Double {
        max(0, min(1, progress))
    }

    public init(
        label: String,
        value: String,
        progress: Double,
        accent: Color,
        track: Color,
        labelColor: Color,
        valueColor: Color,
        scale: CGFloat,
        labelFontSize: CGFloat = 15,
        valueFontSize: CGFloat = 14,
        height: CGFloat = 17
    ) {
        self.label = label
        self.value = value
        self.progress = progress
        self.accent = accent
        self.track = track
        self.labelColor = labelColor
        self.valueColor = valueColor
        self.scale = scale
        self.labelFontSize = labelFontSize
        self.valueFontSize = valueFontSize
        self.height = height
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7 * scale) {
            HStack {
                Text(label)
                    .font(.system(size: labelFontSize * scale, weight: .semibold))
                    .foregroundStyle(labelColor)
                Spacer()
                Text(value)
                    .font(.system(size: valueFontSize * scale, weight: .black))
                    .foregroundStyle(valueColor)
                    .monospacedDigit()
            }

            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(track)
                    Capsule()
                        .fill(accent)
                        .frame(width: clampedProgress * proxy.size.width)
                }
            }
            .frame(height: height * scale)
        }
        .accessibilityRepresentation {
            ProgressView(value: clampedProgress)
                .accessibilityLabel(label)
                .accessibilityValue(value)
        }
    }
}

public struct PevDashboardProgressCard: View {
    let label: String
    let value: String
    let detail: String
    let progress: Double
    let accent: Color
    let fill: Color
    let stroke: Color
    let track: Color
    let labelColor: Color
    let valueColor: Color
    let detailColor: Color
    let scale: CGFloat

    public init(
        label: String,
        value: String,
        detail: String,
        progress: Double,
        accent: Color,
        fill: Color,
        stroke: Color,
        track: Color,
        labelColor: Color,
        valueColor: Color,
        detailColor: Color,
        scale: CGFloat
    ) {
        self.label = label
        self.value = value
        self.detail = detail
        self.progress = progress
        self.accent = accent
        self.fill = fill
        self.stroke = stroke
        self.track = track
        self.labelColor = labelColor
        self.valueColor = valueColor
        self.detailColor = detailColor
        self.scale = scale
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 11 * scale) {
            PevDashboardProgressBar(
                label: label,
                value: value,
                progress: progress,
                accent: accent,
                track: track,
                labelColor: labelColor,
                valueColor: valueColor,
                scale: scale,
                labelFontSize: 14,
                valueFontSize: 25
            )

            if !detail.isEmpty {
                Text(detail)
                    .font(.system(size: 13 * scale, weight: .bold))
                    .foregroundStyle(detailColor)
                    .lineLimit(2)
            }
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 17 * scale)
        .frame(maxWidth: .infinity)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 25 * scale,
                fill: fill,
                stroke: stroke
            )
        )
        .accessibilityRepresentation {
            ProgressView(value: max(0, min(1, progress)))
                .accessibilityLabel(label)
                .accessibilityValue(detail.isEmpty ? value : "\(value), \(detail)")
        }
    }
}

public struct PevDashboardWarningCard: View {
    let title: String
    let detail: String
    let accent: Color
    let detailColor: Color
    let fill: Color
    let stroke: Color
    let scale: CGFloat
    let titleFontSize: CGFloat
    let detailFontSize: CGFloat
    let cornerRadius: CGFloat
    let height: CGFloat?

    public init(
        title: String,
        detail: String,
        accent: Color,
        detailColor: Color,
        fill: Color,
        stroke: Color,
        scale: CGFloat,
        titleFontSize: CGFloat = 20,
        detailFontSize: CGFloat = 13,
        cornerRadius: CGFloat = 23,
        height: CGFloat? = nil
    ) {
        self.title = title
        self.detail = detail
        self.accent = accent
        self.detailColor = detailColor
        self.fill = fill
        self.stroke = stroke
        self.scale = scale
        self.titleFontSize = titleFontSize
        self.detailFontSize = detailFontSize
        self.cornerRadius = cornerRadius
        self.height = height
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7 * scale) {
            Text(title)
                .font(.system(size: titleFontSize * scale, weight: .black))
                .foregroundStyle(accent)
            Text(detail)
                .font(.system(size: detailFontSize * scale, weight: .bold))
                .foregroundStyle(detailColor)
                .lineLimit(2)
                .minimumScaleFactor(0.72)
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 16 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: height.map { $0 * scale })
        .background(
            PevDashboardCardBackground(
                cornerRadius: cornerRadius * scale,
                fill: fill,
                stroke: stroke
            )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(detail)
    }
}
