import CutoutMobile
import SwiftUI

enum LiveActivityPresentationStyle {
    case compact
    case expanded
    case lockScreen
}

struct LiveActivityPresentationCard: View {
    let title: String
    let subtitle: String
    let style: LiveActivityPresentationStyle
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
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
        .background(LiveActivityMockupPalette.cardBackground)
        .overlay(
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .stroke(LiveActivityMockupPalette.border, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.headline.weight(.semibold))
                .foregroundStyle(LiveActivityMockupPalette.primaryText)
            Text(subtitle)
                .font(.caption)
                .foregroundStyle(LiveActivityMockupPalette.secondaryText)
        }
    }

    private var compactBody: some View {
        HStack(alignment: .firstTextBaseline, spacing: 20) {
            VStack(alignment: .leading, spacing: 6) {
                Text(snapshot.identity.displayLabel)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(LiveActivityMockupPalette.secondaryText)
                Text(snapshot.speed.displayValue)
                    .font(.system(size: 34, weight: .bold, design: .rounded))
                    .foregroundStyle(LiveActivityMockupPalette.primaryText)
                Text(snapshot.connectionState.rawValue)
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(LiveActivityMockupPalette.secondaryText)
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
                .foregroundStyle(LiveActivityMockupPalette.secondaryText)

            Text(snapshot.speed.displayValue)
                .font(.system(size: 52, weight: .bold, design: .rounded))
                .foregroundStyle(LiveActivityMockupPalette.primaryText)

            LazyVGrid(columns: [
                GridItem(.flexible(), spacing: 12),
                GridItem(.flexible(), spacing: 12),
            ], spacing: 12) {
                ForEach(snapshot.visibleValues, id: \.label) { value in
                    PevLiveActivityValueCell(
                        value: value,
                        tint: LiveActivityMockupPalette.accent,
                        textColor: LiveActivityMockupPalette.primaryText,
                        secondaryTextColor: LiveActivityMockupPalette.secondaryText,
                        background: LiveActivityMockupPalette.tileBackground,
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
                        .foregroundStyle(LiveActivityMockupPalette.primaryText)
                    Text(snapshot.connectionState.rawValue)
                        .font(.caption)
                        .foregroundStyle(LiveActivityMockupPalette.secondaryText)
                }
                Spacer()
                Text(snapshot.speed.displayValue)
                    .font(.system(size: 48, weight: .bold, design: .rounded))
                    .foregroundStyle(LiveActivityMockupPalette.primaryText)
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
            tint: LiveActivityMockupPalette.accent,
            textColor: LiveActivityMockupPalette.primaryText,
            secondaryTextColor: LiveActivityMockupPalette.secondaryText,
            background: LiveActivityMockupPalette.tileBackground,
            showsStateText: true
        )
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

enum LiveActivityMockupPalette {
    static let background = Color(red: 0.06, green: 0.08, blue: 0.12)
    static let cardBackground = Color(red: 0.10, green: 0.12, blue: 0.17)
    static let tileBackground = Color(red: 0.14, green: 0.17, blue: 0.23)
    static let border = Color.white.opacity(0.10)
    static let accent = Color(red: 0.26, green: 0.70, blue: 0.96)
    static let muted = Color.white.opacity(0.25)
    static let primaryText = Color.white
    static let secondaryText = Color.white.opacity(0.72)
}
