import SwiftUI

public struct PevLiveActivityValueCell: View {
    @ScaledMetric(relativeTo: .caption2) private var compactLabelSize: CGFloat = 7
    @ScaledMetric(relativeTo: .body) private var compactValueSize: CGFloat = 11
    @ScaledMetric(relativeTo: .caption2) private var compactUnitSize: CGFloat = 7

    let value: LiveActivityRideValue
    let tint: Color
    let compact: Bool
    let showProgress: Bool
    let showsStateText: Bool

    public init(
        value: LiveActivityRideValue,
        tint: Color,
        compact: Bool = false,
        showProgress: Bool = false,
        showsStateText: Bool = false
    ) {
        self.value = value
        self.tint = tint
        self.compact = compact
        self.showProgress = showProgress
        self.showsStateText = showsStateText
    }

    public var body: some View {
        let progress = value.progressValue

        VStack(alignment: .leading, spacing: compact ? 2 : 4) {
            Text(value.label)
                .font(compact ? .system(size: compactLabelSize, weight: .bold) : .caption2.weight(.semibold))
                .foregroundStyle(PevLiveActivityPalette.secondaryText)
                .textCase(compact ? .uppercase : nil)

            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline, spacing: compact ? 3 : 4) {
                    valueText
                    if let unit = value.unit {
                        Text(unit)
                            .font(compact ? .system(size: compactUnitSize, weight: .semibold) : .caption.weight(.semibold))
                            .foregroundStyle(PevLiveActivityPalette.secondaryText)
                    }
                }

                valueText
            }

            if showsStateText {
                Text(value.state.rawValue)
                    .font(.caption2)
                    .foregroundStyle(PevLiveActivityPalette.secondaryText)
            } else {
                if showProgress, let progress {
                    ProgressView(value: progress)
                        .progressViewStyle(.linear)
                        .tint(tint)
                        .frame(height: 2)
                }
            }
        }
        .padding(.horizontal, compact ? 7 : 9)
        .padding(.vertical, compact ? 4 : 7)
        .frame(minWidth: compact ? 60 : 66, maxWidth: .infinity, minHeight: compact ? 34 : 50, alignment: .leading)
        .background(PevLiveActivityPalette.cellBackground)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(value.label)
        .accessibilityValue(value.accessibilityValue)
    }

    private var availableValueColor: Color {
        showsStateText ? PevLiveActivityPalette.primaryText : tint
    }

    private var valueText: some View {
        Text(value.displayValue)
            .font(compact ? .system(size: compactValueSize, weight: .semibold, design: .rounded) : .headline.weight(.semibold))
            .foregroundStyle(value.state == .available ? availableValueColor : PevLiveActivityPalette.secondaryText)
            .lineLimit(1)
    }
}
