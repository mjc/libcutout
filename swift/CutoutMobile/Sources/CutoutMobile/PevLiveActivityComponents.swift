import SwiftUI

public struct PevLiveActivityValueCell: View {
    let value: LiveActivityRideValue
    let tint: Color
    let textColor: Color
    let secondaryTextColor: Color
    let background: Color
    let compact: Bool
    let showProgress: Bool
    let showsStateText: Bool

    public init(
        value: LiveActivityRideValue,
        tint: Color,
        textColor: Color,
        secondaryTextColor: Color,
        background: Color,
        compact: Bool = false,
        showProgress: Bool = false,
        showsStateText: Bool = false
    ) {
        self.value = value
        self.tint = tint
        self.textColor = textColor
        self.secondaryTextColor = secondaryTextColor
        self.background = background
        self.compact = compact
        self.showProgress = showProgress
        self.showsStateText = showsStateText
    }

    public var body: some View {
        let progress = value.progressValue

        VStack(alignment: .leading, spacing: compact ? 2 : 4) {
            Text(value.label)
                .font(compact ? .system(size: 7, weight: .bold) : .caption2.weight(.semibold))
                .foregroundStyle(secondaryTextColor)
                .textCase(compact ? .uppercase : nil)

            HStack(alignment: .firstTextBaseline, spacing: compact ? 3 : 4) {
                Text(value.displayValue)
                    .font(compact ? .system(size: 11, weight: .semibold, design: .rounded) : .headline.weight(.semibold))
                    .foregroundStyle(value.state == .available ? availableValueColor : secondaryTextColor)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                if let unit = value.unit {
                    Text(unit)
                        .font(compact ? .system(size: 7, weight: .semibold) : .caption.weight(.semibold))
                        .foregroundStyle(secondaryTextColor)
                }
            }

            if showsStateText {
                Text(value.state.rawValue)
                    .font(.caption2)
                    .foregroundStyle(secondaryTextColor)
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
        .background(background)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(value.label))
        .accessibilityValue(Text(value.accessibilityText))
    }

    private var availableValueColor: Color {
        showsStateText ? textColor : tint
    }
}
