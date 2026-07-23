import SwiftUI
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

extension LiveActivityRideValue {
    static func speedPresentation(for speed: Self) -> Self {
        guard speed.unit != nil else {
            return .unavailable(label: speed.label, accessibilityDetail: speed.accessibilityDetail)
        }
        return speed
    }
}

private enum PevLiveActivitySystemColors {
    #if os(iOS)
    static let background = Color(uiColor: .systemBackground)
    static let accent = Color(uiColor: .systemPurple)
    static let accent2 = Color(uiColor: .systemPink)
    static let connected = Color(uiColor: .systemGreen)
    static let warning = Color(uiColor: .systemOrange)
    static let orange = Color(uiColor: .systemOrange)
    #elseif os(macOS)
    static let background = Color(nsColor: .windowBackgroundColor)
    static let accent = Color(nsColor: .systemPurple)
    static let accent2 = Color(nsColor: .systemPink)
    static let connected = Color(nsColor: .systemGreen)
    static let warning = Color(nsColor: .systemOrange)
    static let orange = Color(nsColor: .systemOrange)
    #endif
}

public enum PevLiveActivityMetricRole: CaseIterable, Hashable, Sendable {
    case battery
    case packVoltage
    case pwm
    case mode
    case duration
    case distance
    case chargeEstimate
    case headroom
    case temperature

    public var isRepeatedInSafetyFooter: Bool {
        self == .headroom || self == .temperature
    }

    public func accessibilitySortPriority(for severity: LiveActivityRideHeadroomSeverity?) -> Double {
        self == .headroom && severity == .reduceAcceleration ? 2 : 0
    }
}

public struct PevLiveActivityBrandMark: View {
    @ScaledMetric(relativeTo: .title2) private var scaledBaseSize: CGFloat = 16

    let size: CGFloat

    public init(size: CGFloat) {
        self.size = size
    }

    public var body: some View {
        Text("C")
            .font(.system(size: size * scaledBaseSize / 16, weight: .black, design: .rounded))
            .italic()
            .foregroundStyle(
                LinearGradient(
                    colors: [PevLiveActivityPalette.accent, PevLiveActivityPalette.accent2],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
            .accessibilityLabel("CutOut")
    }
}

public struct PevLiveActivityHeader: View {
    @ScaledMetric(relativeTo: .caption) private var compactWordmarkSize: CGFloat = 12
    @ScaledMetric(relativeTo: .subheadline) private var expandedWordmarkSize: CGFloat = 13
    @ScaledMetric(relativeTo: .caption2) private var compactIdentitySize: CGFloat = 10
    @ScaledMetric(relativeTo: .caption) private var expandedIdentitySize: CGFloat = 12

    let snapshot: LiveActivityRideSnapshot
    let compact: Bool

    public init(snapshot: LiveActivityRideSnapshot, compact: Bool) {
        self.snapshot = snapshot
        self.compact = compact
    }

    public var body: some View {
        HStack(alignment: .center, spacing: 8) {
            PevLiveActivityBrandMark(size: compact ? 16 : 18)
            Text("CUTOUT")
                .font(.system(size: compact ? compactWordmarkSize : expandedWordmarkSize, weight: .bold))
            Spacer(minLength: 8)
            Text(snapshot.identity.displayLabel)
                .font(.system(size: compact ? compactIdentitySize : expandedIdentitySize, weight: .medium))
                .foregroundStyle(PevLiveActivityPalette.secondaryText)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Circle()
                .fill(snapshot.connectionState == .connected ? PevLiveActivityPalette.connected : PevLiveActivityPalette.warning)
                .frame(width: 7, height: 7)
                .accessibilityHidden(true)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(LiveActivityRideSnapshot.activityAccessibilityLabel)
        .accessibilityValue(snapshot.identity.accessibilityValue(for: snapshot.connectionState))
    }
}

public struct PevLiveActivitySpeedGauge: View {
    @ScaledMetric(relativeTo: .title2) private var speedScale: CGFloat = 1
    @ScaledMetric(relativeTo: .caption) private var unitScale: CGFloat = 1

    let snapshot: LiveActivityRideSnapshot
    let diameter: CGFloat

    public init(snapshot: LiveActivityRideSnapshot, diameter: CGFloat) {
        self.snapshot = snapshot
        self.diameter = diameter
    }

    public var body: some View {
        let speed = LiveActivityRideValue.speedPresentation(for: snapshot.speed)
        let gaugeEnd = 0.12 + (0.76 * (speed.speedGaugeProgressValue ?? 0.0))

        ZStack {
            Circle()
                .trim(from: 0.12, to: 0.88)
                .stroke(PevLiveActivityPalette.track, style: StrokeStyle(lineWidth: 4, lineCap: .round))
                .rotationEffect(.degrees(38))
            Circle()
                .trim(from: 0.12, to: gaugeEnd)
                .stroke(
                    AngularGradient(
                        colors: [PevLiveActivityPalette.accent, PevLiveActivityPalette.accent2],
                        center: .center
                    ),
                    style: StrokeStyle(lineWidth: 5, lineCap: .round)
                )
                .rotationEffect(.degrees(38))
            VStack(spacing: 0) {
                Text(speed.displayValue)
                    .font(.system(size: diameter * 0.35 * speedScale, weight: .bold, design: .rounded))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                if let unit = speed.unit {
                    Text(unit)
                        .font(.system(size: diameter * 0.13 * unitScale, weight: .medium))
                        .foregroundStyle(PevLiveActivityPalette.secondaryText)
                }
            }
        }
        .frame(width: diameter, height: diameter)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(speed.label)
        .accessibilityValue(speed.accessibilityValue)
    }
}

public struct PevLiveActivityMetricGrid: View {
    let snapshot: LiveActivityRideSnapshot
    var compact = false

    public init(snapshot: LiveActivityRideSnapshot, compact: Bool = false) {
        self.snapshot = snapshot
        self.compact = compact
    }

    public var body: some View {
        Grid(horizontalSpacing: 0, verticalSpacing: 0) {
            GridRow {
                metricCell(role: .battery, value: snapshot.battery, tint: PevLiveActivityPalette.connected, showProgress: true)
                metricCell(role: .packVoltage, value: snapshot.packVoltage, tint: PevLiveActivityPalette.primaryText)
                metricCell(role: .pwm, value: snapshot.pwm, tint: PevLiveActivityPalette.accent2, showProgress: true)
            }
            GridRow {
                metricCell(role: .mode, value: snapshot.mode, tint: PevLiveActivityPalette.orange)
                metricCell(role: .duration, value: snapshot.duration, tint: PevLiveActivityPalette.primaryText)
                metricCell(role: .distance, value: snapshot.distance, tint: PevLiveActivityPalette.primaryText)
            }
            GridRow {
                metricCell(role: .chargeEstimate, value: snapshot.chargeEstimate, tint: PevLiveActivityPalette.connected)
                metricCell(role: .headroom, value: snapshot.headroom, tint: PevLiveActivityPalette.orange)
                metricCell(role: .temperature, value: snapshot.temperature, tint: PevLiveActivityPalette.primaryText)
            }
        }
        .frame(maxWidth: .infinity)
        .clipShape(RoundedRectangle(cornerRadius: compact ? 8 : 10, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: compact ? 8 : 10, style: .continuous)
                .stroke(PevLiveActivityPalette.border, lineWidth: 1)
        }
    }

    private func metricCell(
        role: PevLiveActivityMetricRole,
        value: LiveActivityRideValue,
        tint: Color,
        showProgress: Bool = false
    ) -> some View {
        PevLiveActivityValueCell(
            value: value,
            tint: tint,
            compact: compact,
            showProgress: showProgress
        )
        .accessibilityHidden(role.isRepeatedInSafetyFooter)
    }
}

public struct PevLiveActivitySafetyFooter: View {
    @ScaledMetric(relativeTo: .caption2) private var compactFontSize: CGFloat = 9
    @ScaledMetric(relativeTo: .caption2) private var expandedFontSize: CGFloat = 10
    @ScaledMetric(relativeTo: .caption2) private var compactHeight: CGFloat = 14
    @ScaledMetric(relativeTo: .caption2) private var expandedHeight: CGFloat = 16

    let snapshot: LiveActivityRideSnapshot
    let compact: Bool

    public init(snapshot: LiveActivityRideSnapshot, compact: Bool) {
        self.snapshot = snapshot
        self.compact = compact
    }

    private var headroomPresentation: (systemName: String, tint: Color) {
        switch snapshot.headroomSeverity {
        case .reduceAcceleration:
            ("exclamationmark.triangle.fill", PevLiveActivityPalette.warning)
        case .nominal:
            ("checkmark.circle.fill", PevLiveActivityPalette.connected)
        case .notApplicable:
            ("minus.circle", PevLiveActivityPalette.secondaryText)
        case .unavailable, nil:
            ("questionmark.circle", PevLiveActivityPalette.secondaryText)
        }
    }

    public var body: some View {
        HStack(spacing: compact ? 6 : 8) {
            PevLiveActivityFooterChip(
                systemName: headroomPresentation.systemName,
                value: snapshot.headroom,
                tint: headroomPresentation.tint
            )
            .accessibilitySortPriority(
                PevLiveActivityMetricRole.headroom.accessibilitySortPriority(for: snapshot.headroomSeverity)
            )
            Divider().overlay(PevLiveActivityPalette.border)
                .accessibilityHidden(true)
            PevLiveActivityFooterChip(systemName: "speaker.wave.2.fill", value: snapshot.beeps, tint: PevLiveActivityPalette.accent)
            Divider().overlay(PevLiveActivityPalette.border)
                .accessibilityHidden(true)
            PevLiveActivityFooterChip(systemName: "thermometer.medium", value: snapshot.temperature, tint: PevLiveActivityPalette.primaryText)
        }
        .font(.system(size: compact ? compactFontSize : expandedFontSize, weight: .medium))
        .frame(height: compact ? compactHeight : expandedHeight)
    }
}

public struct PevLiveActivityFooterChip: View {
    @ScaledMetric(relativeTo: .caption2) private var iconSize: CGFloat = 10

    let systemName: String
    let value: LiveActivityRideValue
    let tint: Color

    public init(systemName: String, value: LiveActivityRideValue, tint: Color) {
        self.systemName = systemName
        self.value = value
        self.tint = tint
    }

    public var body: some View {
        HStack(spacing: 4) {
            Image(systemName: systemName)
                .font(.system(size: iconSize, weight: .semibold))
                .foregroundStyle(tint)
                .accessibilityHidden(true)
            Text(value.displayValue)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
                .foregroundStyle(value.state == .available ? PevLiveActivityPalette.primaryText : PevLiveActivityPalette.secondaryText)
            if let unit = value.unit {
                Text(unit)
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                    .foregroundStyle(PevLiveActivityPalette.secondaryText)
            }
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(value.label)
        .accessibilityValue(value.accessibilityValue)
    }
}

public struct PevLiveActivityGlyph: View {
    let snapshot: LiveActivityRideSnapshot

    public init(snapshot: LiveActivityRideSnapshot) {
        self.snapshot = snapshot
    }

    public var body: some View {
        Image(assetName)
            .resizable()
            .renderingMode(.original)
            .interpolation(.high)
            .scaledToFit()
            .accessibilityHidden(true)
    }

    private var assetName: String {
        switch snapshot.glyph {
        case .electricUnicycle:
            "EucGlyph"
        case .floatwheelAtom:
            "AtomGlyph"
        }
    }
}

public enum PevLiveActivityPalette {
    public static let background = PevLiveActivitySystemColors.background
    public static let cellBackground = Color.secondary.opacity(0.08)
    public static let border = Color.secondary.opacity(0.25)
    public static let track = Color.secondary.opacity(0.30)
    public static let primaryText = Color.primary
    public static let secondaryText = Color.secondary
    public static let accent = PevLiveActivitySystemColors.accent
    public static let accent2 = PevLiveActivitySystemColors.accent2
    public static let connected = PevLiveActivitySystemColors.connected
    public static let warning = PevLiveActivitySystemColors.warning
    public static let orange = PevLiveActivitySystemColors.orange
}
