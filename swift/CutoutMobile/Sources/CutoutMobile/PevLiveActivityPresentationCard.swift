import SwiftUI

public enum PevLiveActivityPresentationStyle {
    case compact
    case expanded
    case lockScreen
}

public struct PevLiveActivityPresentationCard: View {
    let title: String
    let subtitle: String
    let style: PevLiveActivityPresentationStyle
    let snapshot: LiveActivityRideSnapshot

    public init(
        title: String,
        subtitle: String,
        style: PevLiveActivityPresentationStyle,
        snapshot: LiveActivityRideSnapshot
    ) {
        self.title = title
        self.subtitle = subtitle
        self.style = style
        self.snapshot = snapshot
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header
            switch style {
            case .compact:
                compactBody
            case .expanded:
                expandedBody
            case .lockScreen:
                lockScreenBody
            }
        }
        .padding(style == .compact ? 18 : 24)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevLiveActivityPalette.background)
        .overlay(
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .stroke(PevLiveActivityPalette.border, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.headline.weight(.semibold))
                .foregroundStyle(PevLiveActivityPalette.primaryText)
            Text(subtitle)
                .font(.caption)
                .foregroundStyle(PevLiveActivityPalette.secondaryText)
        }
    }

    private var compactBody: some View {
        HStack(alignment: .firstTextBaseline, spacing: 20) {
            VStack(alignment: .leading, spacing: 6) {
                Text(snapshot.identity.displayLabel)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(PevLiveActivityPalette.secondaryText)
                Text(snapshot.speed.displayValue)
                    .font(.system(size: 34, weight: .bold, design: .rounded))
                    .foregroundStyle(PevLiveActivityPalette.primaryText)
                Text(snapshot.connectionState.rawValue)
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(PevLiveActivityPalette.secondaryText)
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 8) {
                liveMetric(snapshot.battery)
                liveMetric(snapshot.headroom)
            }
        }
    }

    private var expandedBody: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(snapshot.identity.displayLabel)
                .font(.callout.weight(.semibold))
                .foregroundStyle(PevLiveActivityPalette.secondaryText)

            Text(snapshot.speed.displayValue)
                .font(.system(size: 52, weight: .bold, design: .rounded))
                .foregroundStyle(PevLiveActivityPalette.primaryText)

            PevDashboardGrid(columns: [
                GridItem(.flexible(), spacing: 12),
                GridItem(.flexible(), spacing: 12),
            ], spacing: 12) {
                ForEach(snapshot.visibleValues, id: \.label) { value in
                    PevLiveActivityValueCell(
                        value: value,
                        tint: PevLiveActivityPalette.accent,
                        textColor: PevLiveActivityPalette.primaryText,
                        secondaryTextColor: PevLiveActivityPalette.secondaryText,
                        background: PevLiveActivityPalette.cellBackground,
                        showsStateText: true
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                }
            }
        }
    }

    private var lockScreenBody: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(snapshot.identity.displayLabel)
                        .font(.headline.weight(.semibold))
                        .foregroundStyle(PevLiveActivityPalette.primaryText)
                    Text(snapshot.connectionState.rawValue)
                        .font(.caption)
                        .foregroundStyle(PevLiveActivityPalette.secondaryText)
                }
                Spacer()
                Text(snapshot.speed.displayValue)
                    .font(.system(size: 48, weight: .bold, design: .rounded))
                    .foregroundStyle(PevLiveActivityPalette.primaryText)
            }

            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 10) {
                    liveMetric(snapshot.battery)
                    liveMetric(snapshot.packVoltage)
                    liveMetric(snapshot.pwm)
                }

                VStack(alignment: .leading, spacing: 10) {
                    liveMetric(snapshot.mode)
                    liveMetric(snapshot.duration)
                    liveMetric(snapshot.distance)
                }

                VStack(alignment: .leading, spacing: 10) {
                    liveMetric(snapshot.headroom)
                    liveMetric(snapshot.beeps)
                    liveMetric(snapshot.temperature)
                }
            }
        }
    }

    private func liveMetric(_ value: LiveActivityRideValue) -> some View {
        PevLiveActivityValueCell(
            value: value,
            tint: PevLiveActivityPalette.accent,
            textColor: PevLiveActivityPalette.primaryText,
            secondaryTextColor: PevLiveActivityPalette.secondaryText,
            background: PevLiveActivityPalette.cellBackground,
            showsStateText: true
        )
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}
