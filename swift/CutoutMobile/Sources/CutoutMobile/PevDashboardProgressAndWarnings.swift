import SwiftUI

public struct PevDashboardProgressBar: View {
    let label: String
    let value: String
    let progress: Double
    let accent: Color
    let track: Color
    let labelColor: Color
    let valueColor: Color
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
        height: CGFloat = 17
    ) {
        self.label = label
        self.value = value
        self.progress = progress
        self.accent = accent
        self.track = track
        self.labelColor = labelColor
        self.valueColor = valueColor
        self.height = height
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack {
                Text(label)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(labelColor)
                Spacer()
                Text(value)
                    .font(.headline.weight(.black))
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
            .frame(height: height)
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
        detailColor: Color
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
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 11) {
            PevDashboardProgressBar(
                label: label,
                value: value,
                progress: progress,
                accent: accent,
                track: track,
                labelColor: labelColor,
                valueColor: valueColor
            )

            if !detail.isEmpty {
                Text(detail)
                    .font(.subheadline.weight(.bold))
                    .foregroundStyle(detailColor)
            }
        }
        .padding(.horizontal, 22)
        .padding(.vertical, 17)
        .frame(maxWidth: .infinity)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 25,
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
    let cornerRadius: CGFloat

    var accessibilityValueText: String { detail }

    public init(
        title: String,
        detail: String,
        accent: Color,
        detailColor: Color,
        fill: Color,
        stroke: Color,
        cornerRadius: CGFloat = 23
    ) {
        self.title = title
        self.detail = detail
        self.accent = accent
        self.detailColor = detailColor
        self.fill = fill
        self.stroke = stroke
        self.cornerRadius = cornerRadius
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title)
                .font(.title3.weight(.black))
                .foregroundStyle(accent)
            Text(detail)
                .font(.subheadline.weight(.bold))
                .foregroundStyle(detailColor)
        }
        .padding(.horizontal, 22)
        .padding(.vertical, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            PevDashboardCardBackground(
                cornerRadius: cornerRadius,
                fill: fill,
                stroke: stroke
            )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(accessibilityValueText)
    }
}
