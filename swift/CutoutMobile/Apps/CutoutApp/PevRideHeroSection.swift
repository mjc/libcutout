import CutoutMobile
import SwiftUI

enum PevRideHeroStyle {
    case electricUnicycle
    case vescOnewheel

    static let electricUnicycleSpeedPointSize: CGFloat = 138
    static let vescOnewheelSpeedPointSize: CGFloat = 124
    static let unitPointSize: CGFloat = 24

    var speedPointSize: CGFloat {
        switch self {
        case .electricUnicycle: Self.electricUnicycleSpeedPointSize
        case .vescOnewheel: Self.vescOnewheelSpeedPointSize
        }
    }
}

enum PevRideMetricProvenance: Equatable {
    case vehicleTelemetry

    var accessibilityText: String {
        switch self {
        case .vehicleTelemetry: "vehicle telemetry"
        }
    }
}

enum PevRideMetricSeverity: Equatable {
    case nominal
    case caution
    case critical
    case unavailable

    init(_ severity: EucRideWarningSeverity) {
        switch severity {
        case .normal: self = .nominal
        case .caution, .reduceAcceleration: self = .caution
        case .limpHome, .failed: self = .critical
        case .unavailable: self = .unavailable
        }
    }

    init(_ warning: VescRideWarning) {
        switch warning {
        case .none: self = .nominal
        case .pushbackSoon: self = .caution
        case .unknown: self = .unavailable
        }
    }

    var accessibilityText: String {
        switch self {
        case .nominal: "nominal"
        case .caution: "caution"
        case .critical: "critical"
        case .unavailable: "severity unavailable"
        }
    }
}

enum PevRideHeroReadout: Equatable {
    case available(
        value: String,
        unit: String,
        provenance: PevRideMetricProvenance,
        freshness: EucRideUpdateFreshness,
        severity: PevRideMetricSeverity
    )
    case unavailable(
        provenance: PevRideMetricProvenance,
        freshness: EucRideUpdateFreshness,
        severity: PevRideMetricSeverity
    )

    var displayValue: String {
        switch self {
        case .available(let value, _, _, _, _): value
        case .unavailable: "Unavailable"
        }
    }

    var displayUnit: String {
        switch self {
        case .available(_, let unit, _, _, _): unit
        case .unavailable: ""
        }
    }

    var accessibilityValue: String {
        switch self {
        case .available(let value, let unit, let provenance, let freshness, let severity):
            [
                value,
                unit,
                "available",
                provenance.accessibilityText,
                freshness.accessibilityText,
                severity.accessibilityText,
            ]
            .filter { !$0.isEmpty }
            .joined(separator: ", ")
        case .unavailable(let provenance, let freshness, let severity):
            [
                "unavailable",
                provenance.accessibilityText,
                freshness.accessibilityText,
                severity.accessibilityText,
            ]
            .joined(separator: ", ")
        }
    }

    var isAvailable: Bool {
        if case .available = self { true } else { false }
    }

    static func euc(
        state: EucRideScreenState?,
        now: MonotonicMilliseconds
    ) -> Self {
        let freshness = state?.updateAge(
            at: now,
            staleAfter: MonotonicMilliseconds(2_000)
        ).freshness ?? .unavailable
        let severity = PevRideMetricSeverity(
            state?.warningState(
                at: now,
                staleAfter: MonotonicMilliseconds(2_000)
            ).severity ?? .unavailable
        )
        guard let state, state.telemetry?.speed != nil else {
            return .unavailable(
                provenance: .vehicleTelemetry,
                freshness: freshness,
                severity: severity
            )
        }
        return .available(
            value: state.speedText,
            unit: state.speedUnit,
            provenance: .vehicleTelemetry,
            freshness: freshness,
            severity: severity
        )
    }

    static func vesc(
        snapshot: VescRideSnapshot?,
        now: MonotonicMilliseconds
    ) -> Self {
        let freshness = snapshot?.updateAge(
            at: now,
            staleAfter: MonotonicMilliseconds(2_000)
        ).freshness ?? .unavailable
        let severity = PevRideMetricSeverity(snapshot?.warning ?? .unknown)
        guard let boardSpeed = snapshot?.boardSpeed else {
            return .unavailable(
                provenance: .vehicleTelemetry,
                freshness: freshness,
                severity: severity
            )
        }
        let readout = SpeedReadout(millimetersPerSecond: boardSpeed.value)
        return .available(
            value: readout.displayValue,
            unit: readout.displayUnit,
            provenance: .vehicleTelemetry,
            freshness: freshness,
            severity: severity
        )
    }
}

private extension EucRideUpdateFreshness {
    var accessibilityText: String {
        switch self {
        case .fresh: "fresh"
        case .stale: "stale"
        case .unavailable: "freshness unavailable"
        }
    }
}

struct PevRideHeroSection: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @ScaledMetric(relativeTo: .largeTitle) private var eucSpeedFontSize = PevRideHeroStyle.electricUnicycleSpeedPointSize
    @ScaledMetric(relativeTo: .largeTitle) private var vescSpeedFontSize = PevRideHeroStyle.vescOnewheelSpeedPointSize
    @ScaledMetric(relativeTo: .title2) private var speedUnitFontSize = PevRideHeroStyle.unitPointSize

    let style: PevRideHeroStyle
    let title: String
    let subtitle: String
    let statusFill: Color
    let captureStatusText: String?
    let speedReadout: PevRideHeroReadout
    let speedCaption: String

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .center, spacing: 12) {
                titleText
                Spacer(minLength: 8)
                statusPill
            }
            VStack(alignment: .leading, spacing: 8) {
                titleText
                statusPill
            }
        }
        .padding(.top, 8)

        if let captureStatusText {
            PevStatusStrip(
                text: captureStatusText,
                indicatorColor: PevColors.green,
                background: PevColors.cardFill,
                foreground: PevColors.primaryText,
                cornerRadius: 18
            )
        }

        VStack(alignment: .center, spacing: 2) {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(spacing: 2) {
                    speed
                    unit
                }
            } else {
                HStack(alignment: .firstTextBaseline, spacing: 9) {
                    speed
                    unit
                }
            }
            Text(speedCaption)
                .font(.caption.weight(.bold))
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity)
        .foregroundStyle(PevColors.primaryText)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(speedCaption)
        .accessibilityValue(speedReadout.accessibilityValue)
        .accessibilityIdentifier("ride.hero.speed")
    }

    private var titleText: some View {
        Text(title)
            .font(.headline)
            .foregroundStyle(PevColors.primaryText)
            .accessibilityHeading(.h1)
    }

    private var statusPill: some View {
        PevDashboardStatusPill(
            title: subtitle,
            fill: statusFill
        )
    }

    @ViewBuilder
    private var speed: some View {
        if speedReadout.isAvailable {
            Text(speedReadout.displayValue)
                .font(speedFont)
                .monospacedDigit()
        } else {
            Text(speedReadout.displayValue)
                .font(.title2.weight(.semibold))
        }
    }

    private var speedFontSize: CGFloat {
        switch style {
        case .electricUnicycle: eucSpeedFontSize
        case .vescOnewheel: vescSpeedFontSize
        }
    }

    private var speedFont: Font {
        dynamicTypeSize.isAccessibilitySize
            ? .largeTitle.weight(.black)
            : .system(size: speedFontSize, weight: .black)
    }

    @ViewBuilder
    private var unit: some View {
        if !speedReadout.displayUnit.isEmpty {
            Text(speedReadout.displayUnit)
                .font(unitFont)
                .foregroundStyle(PevColors.muted)
        }
    }

    private var unitFont: Font {
        dynamicTypeSize.isAccessibilitySize
            ? .title2.weight(.bold)
            : .system(size: speedUnitFontSize, weight: .bold)
    }
}
