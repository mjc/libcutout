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
    static let critical = Color(uiColor: .systemRed)
    static let orange = Color(uiColor: .systemOrange)
    #elseif os(macOS)
    static let background = Color(nsColor: .windowBackgroundColor)
    static let accent = Color(nsColor: .systemPurple)
    static let accent2 = Color(nsColor: .systemPink)
    static let connected = Color(nsColor: .systemGreen)
    static let warning = Color(nsColor: .systemOrange)
    static let critical = Color(nsColor: .systemRed)
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
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 8) {
                    PevLiveActivityBrandMark(size: compact ? 16 : 18)
                    Text("CUTOUT")
                        .font(.system(size: compact ? compactWordmarkSize : expandedWordmarkSize, weight: .bold))
                }
                PevLiveActivityBrandMark(size: compact ? 16 : 18)
            }
            Spacer(minLength: 8)
            Text(snapshot.identity.label)
                .font(.system(size: compact ? compactIdentitySize : expandedIdentitySize, weight: .medium))
                .foregroundStyle(PevLiveActivityPalette.secondaryText)
                .lineLimit(1)
                .layoutPriority(1)
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
                    .font(.system(size: diameter * 0.35, weight: .bold, design: .rounded))
                    .lineLimit(1)
                if let unit = speed.unit {
                    Text(unit)
                        .font(.system(size: diameter * 0.13, weight: .medium))
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

public struct PevLiveActivityCompactPwmBar: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    let snapshot: LiveActivityRideSnapshot

    public init(snapshot: LiveActivityRideSnapshot) {
        self.snapshot = snapshot
    }

    nonisolated static func shouldPulse(severity: LiveActivityRidePwmSeverity, reduceMotion: Bool) -> Bool {
        severity == .critical && !reduceMotion
    }

    public var body: some View {
        let speed = LiveActivityRideValue.speedPresentation(for: snapshot.speed)
        let pwm = snapshot.pwm
        let severity = snapshot.pwmSeverity

        GeometryReader { proxy in
            ZStack {
                Capsule()
                    .fill(PevLiveActivityPalette.track)
                pwmFill(width: proxy.size.width, pwm: pwm, severity: severity)
                ViewThatFits(in: .horizontal) {
                    compactSpeedLabel(speed: speed, severity: severity, includesUnit: true)
                    compactSpeedLabel(speed: speed, severity: severity, includesUnit: false)
                }
                .foregroundStyle(.white)
            }
        }
        .frame(width: 86, height: 28)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(speed.label)
        .accessibilityValue(
            [speed.accessibilityValue, pwm.label, pwm.accessibilityValue, severity.accessibilityDescription]
                .compactMap { $0 }
                .joined(separator: ", ")
        )
    }

    @ViewBuilder
    private func compactSpeedLabel(
        speed: LiveActivityRideValue,
        severity: LiveActivityRidePwmSeverity,
        includesUnit: Bool
    ) -> some View {
        HStack(spacing: 3) {
            Text(speed.displayValue)
                .font(.caption.weight(.bold))
                .lineLimit(1)
            if includesUnit, let unit = speed.unit {
                Text(unit)
                    .font(.caption2.weight(.semibold))
                    .lineLimit(1)
            }
            if severity == .critical {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.caption2.weight(.bold))
                    .accessibilityHidden(true)
            }
        }
    }

    @ViewBuilder
    private func pwmFill(
        width: CGFloat,
        pwm: LiveActivityRideValue,
        severity: LiveActivityRidePwmSeverity
    ) -> some View {
        let fill = Capsule()
            .fill(severity == .critical ? PevLiveActivityPalette.critical : PevLiveActivityPalette.accent)
            .frame(width: width * (pwm.progressValue ?? 0), alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .leading)
            .clipShape(.capsule)

        if Self.shouldPulse(severity: severity, reduceMotion: reduceMotion) {
            PhaseAnimator([0, 1]) { phase in
                fill.opacity(phase == 0 ? 1 : 0.55)
            } animation: { _ in
                .easeInOut(duration: 0.5)
            }
        } else {
            fill
        }
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
                if snapshot.headroomSeverity == .nominal {
                    metricCell(role: .headroom, value: snapshot.headroom, tint: PevLiveActivityPalette.orange)
                } else {
                    Color.clear
                        .accessibilityHidden(true)
                }
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
        .accessibilityHidden(
            role == .headroom || (role == .temperature && snapshot.showsSecondarySafetyMetrics)
        )
    }
}

public struct PevLiveActivitySafetyFooter: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @ScaledMetric(relativeTo: .caption2) private var compactFontSize: CGFloat = 9
    @ScaledMetric(relativeTo: .caption2) private var expandedFontSize: CGFloat = 10

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
            headroomChip
            if snapshot.showsSecondarySafetyMetrics && !dynamicTypeSize.isAccessibilitySize {
                Divider().overlay(PevLiveActivityPalette.border)
                    .accessibilityHidden(true)
                PevLiveActivityFooterChip(
                    systemName: "speaker.wave.2.fill",
                    value: snapshot.beeps,
                    tint: PevLiveActivityPalette.accent
                )
                Divider().overlay(PevLiveActivityPalette.border)
                    .accessibilityHidden(true)
                PevLiveActivityFooterChip(
                    systemName: "thermometer.medium",
                    value: snapshot.temperature,
                    tint: PevLiveActivityPalette.primaryText
                )
            }
        }
        .font(.system(size: compact ? compactFontSize : expandedFontSize, weight: .medium))
    }

    private var headroomChip: some View {
        PevLiveActivityFooterChip(
            systemName: headroomPresentation.systemName,
            value: snapshot.headroom,
            tint: headroomPresentation.tint,
            lineLimit: snapshot.headroomSeverity == .reduceAcceleration ? 2 : 1,
            emphasizesValue: snapshot.headroomSeverity == .reduceAcceleration
        )
        .accessibilitySortPriority(
            PevLiveActivityMetricRole.headroom.accessibilitySortPriority(for: snapshot.headroomSeverity)
        )
    }
}

public struct PevLiveActivityFooterChip: View {
    @ScaledMetric(relativeTo: .caption2) private var iconSize: CGFloat = 10

    let systemName: String
    let value: LiveActivityRideValue
    let tint: Color
    let lineLimit: Int
    let emphasizesValue: Bool

    public init(
        systemName: String,
        value: LiveActivityRideValue,
        tint: Color,
        lineLimit: Int = 1,
        emphasizesValue: Bool = false
    ) {
        self.systemName = systemName
        self.value = value
        self.tint = tint
        self.lineLimit = lineLimit
        self.emphasizesValue = emphasizesValue
    }

    public var body: some View {
        HStack(spacing: 4) {
            Image(systemName: systemName)
                .font(.system(size: iconSize, weight: .semibold))
                .foregroundStyle(tint)
                .accessibilityHidden(true)
            Text(value.displayValue)
                .lineLimit(lineLimit)
                .foregroundStyle(
                    value.state == .available || emphasizesValue
                        ? PevLiveActivityPalette.primaryText
                        : PevLiveActivityPalette.secondaryText
                )
            if let unit = value.unit {
                Text(unit)
                    .lineLimit(1)
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
    public static let critical = PevLiveActivitySystemColors.critical
    public static let orange = PevLiveActivitySystemColors.orange
}
