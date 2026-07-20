import SwiftUI

public struct PevLiveActivityBrandMark: View {
    let size: CGFloat

    public init(size: CGFloat) {
        self.size = size
    }

    public var body: some View {
        Text("C")
            .font(.system(size: size, weight: .black, design: .rounded))
            .italic()
            .foregroundStyle(
                LinearGradient(
                    colors: [PevLiveActivityPalette.accent, PevLiveActivityPalette.accent2],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
            .accessibilityLabel("Cutout")
    }
}

public struct PevLiveActivityHeader: View {
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
                .font(.system(size: compact ? 12 : 13, weight: .bold))
            Spacer(minLength: 8)
            Text(snapshot.identity.displayLabel)
                .font(.system(size: compact ? 10 : 12, weight: .medium))
                .foregroundStyle(PevLiveActivityPalette.secondaryText)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Circle()
                .fill(snapshot.connectionState == .connected ? PevLiveActivityPalette.connected : PevLiveActivityPalette.warning)
                .frame(width: 7, height: 7)
                .accessibilityHidden(true)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Cutout ride")
        .accessibilityValue(
            "\(snapshot.identity.displayLabel), \(snapshot.connectionState.accessibilityValue)"
        )
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
        let gaugeEnd = 0.12 + (0.76 * (snapshot.speed.speedGaugeProgressValue ?? 0.0))

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
                Text(snapshot.speed.displayValue)
                    .font(.system(size: diameter * 0.35, weight: .bold, design: .rounded))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                Text(snapshot.speed.unit ?? "mph")
                    .font(.system(size: diameter * 0.13, weight: .medium))
                    .foregroundStyle(PevLiveActivityPalette.secondaryText)
            }
        }
        .frame(width: diameter, height: diameter)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(snapshot.speed.label)
        .accessibilityValue(snapshot.speed.accessibilityValue)
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
                metricCell(value: snapshot.battery, tint: PevLiveActivityPalette.connected, showProgress: true)
                metricCell(value: snapshot.packVoltage, tint: PevLiveActivityPalette.primaryText)
                metricCell(value: snapshot.pwm, tint: PevLiveActivityPalette.accent2, showProgress: true)
            }
            GridRow {
                metricCell(value: snapshot.mode, tint: PevLiveActivityPalette.orange)
                metricCell(value: snapshot.duration, tint: PevLiveActivityPalette.primaryText)
                metricCell(value: snapshot.distance, tint: PevLiveActivityPalette.primaryText)
            }
            GridRow {
                metricCell(value: snapshot.chargeEstimate, tint: PevLiveActivityPalette.connected)
                metricCell(value: snapshot.headroom, tint: PevLiveActivityPalette.orange)
                metricCell(value: snapshot.temperature, tint: PevLiveActivityPalette.primaryText)
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
        value: LiveActivityRideValue,
        tint: Color,
        showProgress: Bool = false
    ) -> some View {
        PevLiveActivityValueCell(
            value: value,
            tint: tint,
            textColor: PevLiveActivityPalette.primaryText,
            secondaryTextColor: PevLiveActivityPalette.secondaryText,
            background: PevLiveActivityPalette.cellBackground,
            compact: compact,
            showProgress: showProgress
        )
    }
}

public struct PevLiveActivitySafetyFooter: View {
    let snapshot: LiveActivityRideSnapshot
    let compact: Bool

    public init(snapshot: LiveActivityRideSnapshot, compact: Bool) {
        self.snapshot = snapshot
        self.compact = compact
    }

    private var headroomIsWarning: Bool {
        snapshot.headroom.value == "Reduce acceleration"
    }

    public var body: some View {
        HStack(spacing: compact ? 6 : 8) {
            PevLiveActivityFooterChip(
                systemName: headroomIsWarning ? "exclamationmark.triangle.fill" : "checkmark.circle.fill",
                value: snapshot.headroom,
                tint: headroomIsWarning ? PevLiveActivityPalette.warning : PevLiveActivityPalette.connected
            )
            Divider().overlay(PevLiveActivityPalette.border)
                .accessibilityHidden(true)
            PevLiveActivityFooterChip(systemName: "speaker.wave.2.fill", value: snapshot.beeps, tint: PevLiveActivityPalette.accent)
            Divider().overlay(PevLiveActivityPalette.border)
                .accessibilityHidden(true)
            PevLiveActivityFooterChip(systemName: "thermometer.medium", value: snapshot.temperature, tint: PevLiveActivityPalette.primaryText)
        }
        .font(.system(size: compact ? 9 : 10, weight: .medium))
        .frame(height: compact ? 14 : 16)
    }
}

public struct PevLiveActivityFooterChip: View {
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
                .font(.system(size: 10, weight: .semibold))
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
    public static let background = Color(red: 0.02, green: 0.03, blue: 0.06)
    public static let cellBackground = Color.white.opacity(0.025)
    public static let border = Color.white.opacity(0.10)
    public static let track = Color.white.opacity(0.14)
    public static let primaryText = Color.white
    public static let secondaryText = Color.white.opacity(0.62)
    public static let accent = Color(red: 0.47, green: 0.23, blue: 1.0)
    public static let accent2 = Color(red: 0.82, green: 0.32, blue: 1.0)
    public static let connected = Color(red: 0.24, green: 0.90, blue: 0.35)
    public static let warning = Color(red: 1.0, green: 0.66, blue: 0.22)
    public static let orange = Color(red: 1.0, green: 0.55, blue: 0.22)
}
