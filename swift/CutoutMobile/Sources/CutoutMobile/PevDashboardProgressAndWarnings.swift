import SwiftUI

public struct PevDashboardProgressBar: View {
    let label: String
    let metricValue: PevDashboardMetricValue
    let progress: Double?
    let track: Color
    let height: CGFloat

    var clampedProgress: Double? {
        guard case .available = metricValue else { return nil }
        return progress.map { max(0, min(1, $0)) }
    }

    public init(
        label: String,
        metricValue: PevDashboardMetricValue,
        progress: Double?,
        track: Color = PevDashboardColors.cardFill,
        height: CGFloat = 17
    ) {
        self.label = label
        self.metricValue = metricValue
        self.progress = progress
        self.track = track
        self.height = height
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack {
                Text(label)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevDashboardColors.mutedText)
                Spacer()
                Text(metricValue.displayText)
                    .font(.headline.weight(.black))
                    .foregroundStyle(PevDashboardColors.primaryText)
                    .monospacedDigit()
            }

            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(track)
                    if let clampedProgress {
                        Capsule()
                            .fill(PevDashboardColors.primaryText)
                            .frame(width: clampedProgress * proxy.size.width)
                    }
                }
            }
            .frame(height: height)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityValue(metricValue.accessibilityText)
    }
}

public struct PevDashboardProgressCard: View {
    let label: String
    let metricValue: PevDashboardMetricValue
    let detail: String
    let progress: Double?

    public init(
        label: String,
        metricValue: PevDashboardMetricValue,
        detail: String,
        progress: Double?
    ) {
        self.label = label
        self.metricValue = metricValue
        self.detail = detail
        self.progress = progress
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 11) {
            PevDashboardProgressBar(
                label: label,
                metricValue: metricValue,
                progress: progress,
                track: PevDashboardColors.cardStroke
            )

            if !detail.isEmpty {
                Text(detail)
                    .font(.subheadline.weight(.bold))
                    .foregroundStyle(PevDashboardColors.mutedText)
            }
        }
        .padding(.horizontal, 22)
        .padding(.vertical, 17)
        .frame(maxWidth: .infinity)
        .background(
            PevDashboardCardBackground(
                cornerRadius: 25,
                fill: PevDashboardColors.cardFill,
                stroke: PevDashboardColors.cardStroke
            )
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel(label)
        .accessibilityValue(
            detail.isEmpty
                ? metricValue.accessibilityText
                : "\(metricValue.accessibilityText), \(detail)"
        )
    }
}

public enum PevDashboardWarningCardTone: Sendable, Equatable {
    case vesc

    fileprivate var accent: Color {
        PevDashboardColors.purple
    }

    fileprivate var fill: Color {
        PevDashboardColors.purple.opacity(0.18)
    }

    fileprivate var stroke: Color {
        PevDashboardColors.purple.opacity(0.55)
    }

    fileprivate var cornerRadius: CGFloat {
        24
    }
}

public struct PevDashboardWarningCard: View {
    @Environment(\.colorSchemeContrast) private var colorSchemeContrast
    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor
    @Environment(\.colorScheme) private var colorScheme
    let title: String
    let detail: String
    let accent: Color
    let detailColor: Color
    let fill: Color
    let stroke: Color
    let cornerRadius: CGFloat
    public private(set) var tone: PevDashboardWarningCardTone?

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
        tone = nil
    }

    public init(title: String, detail: String, tone: PevDashboardWarningCardTone) {
        self.init(
            title: title,
            detail: detail,
            accent: tone.accent,
            detailColor: PevDashboardColors.primaryText,
            fill: tone.fill,
            stroke: tone.stroke,
            cornerRadius: tone.cornerRadius
        )
        self.tone = tone
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(accent)
                    .accessibilityHidden(true)
                Text(title)
                    .font(.title3.weight(.black))
                    .foregroundStyle(resolvedColor(PevDashboardColors.primaryText))
                    .accessibilityHidden(true)
            }
            Text(detail)
                .font(.subheadline.weight(.bold))
                .foregroundStyle(resolvedColor(detailColor))
                .accessibilityHidden(true)
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
        .accessibilitySortPriority(2)
    }

    private func resolvedColor(_ color: Color) -> Color {
        pevDashboardResolvedForegroundColor(
            color,
            contrast: colorSchemeContrast,
            differentiateWithoutColor: differentiateWithoutColor,
            colorScheme: colorScheme
        )
    }
}
