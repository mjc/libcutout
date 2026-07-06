#if canImport(ActivityKit) && canImport(WidgetKit) && !os(macOS)
import ActivityKit
import CutoutMobile
import SwiftUI
import WidgetKit

@main
struct CutoutLiveActivityWidgetBundle: WidgetBundle {
    var body: some Widget {
        CutoutRideLiveActivityWidget()
    }
}

private struct CutoutRideLiveActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: LiveActivityRideAttributes.self) { context in
            LockScreenRideActivityView(snapshot: context.state.snapshot)
                .activityBackgroundTint(RideActivityPalette.background)
                .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    HStack(spacing: 5) {
                        BrandMark(size: 15)
                        Text("CUTOUT")
                            .font(.system(size: 12, weight: .bold))
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    HStack(spacing: 5) {
                        Text(context.state.snapshot.identity.displayLabel)
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(RideActivityPalette.secondaryText)
                            .lineLimit(1)
                            .minimumScaleFactor(0.75)
                        Circle()
                            .fill(context.state.snapshot.connectionState == .connected ? RideActivityPalette.connected : RideActivityPalette.warning)
                            .frame(width: 6, height: 6)
                    }
                }
                DynamicIslandExpandedRegion(.bottom) {
                    DynamicIslandRideActivityView(snapshot: context.state.snapshot)
                }
            } compactLeading: {
                HStack(spacing: 4) {
                    BrandMark(size: 18)
                    Text(context.state.snapshot.speed.displayValue)
                        .font(.caption.weight(.bold))
                }
            } compactTrailing: {
                HStack(spacing: 4) {
                    Text(context.state.snapshot.battery.displayValue)
                        .font(.caption.weight(.semibold))
                    Circle()
                        .fill(context.state.snapshot.connectionState == .connected ? RideActivityPalette.connected : RideActivityPalette.warning)
                        .frame(width: 5, height: 5)
                }
            } minimal: {
                BrandMark(size: 16)
            }
        }
    }
}

private struct DynamicIslandRideActivityView: View {
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            VStack(spacing: 4) {
                RideSpeedGauge(snapshot: snapshot, diameter: 70)
                EucModeMark()
                    .frame(width: 48, height: 24)
            }
            .frame(width: 78)
            VStack(spacing: 5) {
                MetricGrid(snapshot: snapshot, compact: true)
                SafetyFooter(snapshot: snapshot, compact: true)
            }
        }
        .padding(.top, 3)
        .foregroundStyle(.white)
    }
}

private struct LockScreenRideActivityView: View {
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            RideActivityHeader(snapshot: snapshot, compact: false)
            HStack(alignment: .center, spacing: 12) {
                VStack(spacing: 5) {
                    RideSpeedGauge(snapshot: snapshot, diameter: 78)
                    EucModeMark()
                        .frame(width: 54, height: 28)
                }
                .frame(width: 88)
                MetricGrid(snapshot: snapshot)
            }
            SafetyFooter(snapshot: snapshot, compact: false)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .foregroundStyle(.white)
    }
}

private struct RideActivityHeader: View {
    let snapshot: LiveActivityRideSnapshot
    let compact: Bool

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            BrandMark(size: compact ? 16 : 18)
            Text("CUTOUT")
                .font(.system(size: compact ? 12 : 13, weight: .bold))
            Spacer(minLength: 8)
            Text(snapshot.identity.displayLabel)
                .font(.system(size: compact ? 10 : 12, weight: .medium))
                .foregroundStyle(RideActivityPalette.secondaryText)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Circle()
                .fill(snapshot.connectionState == .connected ? RideActivityPalette.connected : RideActivityPalette.warning)
                .frame(width: 7, height: 7)
        }
    }
}

private struct RideSpeedGauge: View {
    let snapshot: LiveActivityRideSnapshot
    let diameter: CGFloat

    var body: some View {
        ZStack {
            Circle()
                .trim(from: 0.12, to: 0.88)
                .stroke(RideActivityPalette.track, style: StrokeStyle(lineWidth: 4, lineCap: .round))
                .rotationEffect(.degrees(38))
            Circle()
                .trim(from: 0.12, to: 0.68)
                .stroke(
                    AngularGradient(
                        colors: [RideActivityPalette.accent, RideActivityPalette.accent2],
                        center: .center
                    ),
                    style: StrokeStyle(lineWidth: 5, lineCap: .round)
                )
                .rotationEffect(.degrees(38))
            VStack(spacing: 0) {
                Text(snapshot.speed.displayValue)
                    .font(.system(size: diameter * 0.35, weight: .bold, design: .rounded))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                Text(snapshot.speed.unit ?? "mph")
                    .font(.system(size: diameter * 0.13, weight: .medium))
                    .foregroundStyle(RideActivityPalette.secondaryText)
            }
        }
        .frame(width: diameter, height: diameter)
    }
}

private struct MetricGrid: View {
    let snapshot: LiveActivityRideSnapshot
    var compact = false

    var body: some View {
        Grid(horizontalSpacing: 0, verticalSpacing: 0) {
            GridRow {
                MetricCell(value: snapshot.battery, tint: RideActivityPalette.connected, compact: compact, showProgress: true)
                MetricCell(value: snapshot.packVoltage, tint: RideActivityPalette.primaryText, compact: compact)
                MetricCell(value: snapshot.pwm, tint: RideActivityPalette.accent2, compact: compact, showProgress: true)
            }
            GridRow {
                MetricCell(value: snapshot.mode, tint: RideActivityPalette.orange, compact: compact)
                MetricCell(value: snapshot.duration, tint: RideActivityPalette.primaryText, compact: compact)
                MetricCell(value: snapshot.distance, tint: RideActivityPalette.primaryText, compact: compact)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: compact ? 8 : 10, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: compact ? 8 : 10, style: .continuous)
                .stroke(RideActivityPalette.border, lineWidth: 1)
        }
    }
}

private struct CompactMetricRow: View {
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        HStack(spacing: 0) {
            MetricCell(value: snapshot.battery, tint: RideActivityPalette.connected, compact: true)
            MetricCell(value: snapshot.packVoltage, tint: RideActivityPalette.primaryText, compact: true)
            MetricCell(value: snapshot.pwm, tint: RideActivityPalette.accent2, compact: true)
        }
        .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .stroke(RideActivityPalette.border, lineWidth: 1)
        }
    }
}

private struct MetricCell: View {
    let value: LiveActivityRideValue
    let tint: Color
    var compact = false
    var showProgress = false

    var body: some View {
        VStack(alignment: .leading, spacing: compact ? 2 : 3) {
            Text(value.label)
                .font(.system(size: compact ? 7 : 9, weight: .bold))
                .foregroundStyle(RideActivityPalette.secondaryText)
                .textCase(.uppercase)
            HStack(alignment: .firstTextBaseline, spacing: 3) {
                Text(value.displayValue)
                    .font(.system(size: compact ? 11 : 15, weight: .semibold, design: .rounded))
                    .foregroundStyle(value.state == .available ? tint : RideActivityPalette.secondaryText)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                if let unit = value.unit {
                    Text(unit)
                        .font(.system(size: compact ? 7 : 9, weight: .semibold))
                        .foregroundStyle(RideActivityPalette.secondaryText)
                }
            }
            ProgressView(value: value.state == .available ? 0.68 : 0.0)
                .progressViewStyle(.linear)
                .tint(tint)
                .frame(height: value.unit == "%" ? 2 : 0)
                .opacity(value.unit == "%" && showProgress ? 1 : 0)
        }
        .padding(.horizontal, compact ? 7 : 9)
        .padding(.vertical, compact ? 4 : 7)
        .frame(minWidth: compact ? 60 : 66, maxWidth: .infinity, minHeight: compact ? 34 : 50, alignment: .leading)
        .background(RideActivityPalette.cellBackground)
    }
}

private struct SafetyFooter: View {
    let snapshot: LiveActivityRideSnapshot
    let compact: Bool

    var body: some View {
        HStack(spacing: compact ? 6 : 8) {
            FooterChip(systemName: "checkmark.circle.fill", value: snapshot.headroom, tint: RideActivityPalette.connected)
            Divider().overlay(RideActivityPalette.border)
            FooterChip(systemName: "speaker.wave.2.fill", value: snapshot.beeps, tint: RideActivityPalette.accent)
            Divider().overlay(RideActivityPalette.border)
            FooterChip(systemName: "thermometer.medium", value: snapshot.temperature, tint: RideActivityPalette.primaryText)
        }
        .font(.system(size: compact ? 9 : 10, weight: .medium))
        .frame(height: compact ? 14 : 16)
    }
}

private struct FooterChip: View {
    let systemName: String
    let value: LiveActivityRideValue
    let tint: Color

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: systemName)
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(tint)
            Text(value.displayValue)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
                .foregroundStyle(value.state == .available ? RideActivityPalette.primaryText : RideActivityPalette.secondaryText)
        }
        .frame(maxWidth: .infinity)
    }
}

private struct BrandMark: View {
    let size: CGFloat

    var body: some View {
        Text("C")
            .font(.system(size: size, weight: .black, design: .rounded))
            .italic()
            .foregroundStyle(
                LinearGradient(
                    colors: [RideActivityPalette.accent, RideActivityPalette.accent2],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
    }
}

private struct EucModeMark: View {
    var body: some View {
        Image("EucGlyph")
            .resizable()
            .renderingMode(.original)
            .interpolation(.high)
            .scaledToFit()
            .accessibilityHidden(true)
    }
}

private extension LiveActivityRideValue {
    var displayValue: String {
        switch state {
        case .available, .stale:
            value
        case .notApplicable:
            "n/a"
        case .unavailable, .deferred:
            "--"
        }
    }
}

private enum RideActivityPalette {
    static let background = Color(red: 0.02, green: 0.03, blue: 0.06)
    static let cellBackground = Color.white.opacity(0.025)
    static let border = Color.white.opacity(0.10)
    static let track = Color.white.opacity(0.14)
    static let primaryText = Color.white
    static let secondaryText = Color.white.opacity(0.62)
    static let accent = Color(red: 0.47, green: 0.23, blue: 1.0)
    static let accent2 = Color(red: 0.82, green: 0.32, blue: 1.0)
    static let connected = Color(red: 0.24, green: 0.90, blue: 0.35)
    static let warning = Color(red: 1.0, green: 0.66, blue: 0.22)
    static let orange = Color(red: 1.0, green: 0.55, blue: 0.22)
}
#else
@main
enum CutoutLiveActivityExtensionFallback {
    static func main() {}
}
#endif
