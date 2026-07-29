import CutoutMobile
import SwiftUI

enum PevRideHeroStyle {
    case electricUnicycle
    case vescOnewheel

    static let electricUnicycleSpeedPointSize: CGFloat = 138
    static let vescOnewheelSpeedPointSize: CGFloat = 124
    static let unitPointSize: CGFloat = 24

}

extension RideHeroReadout {
    var accessibilityValue: String {
        switch self {
        case .available(let value, let unit, let freshness, let severity):
            localizedAppText(
                "ride.hero.accessibility.available",
                value,
                unit,
                localizedAppText("ride.hero.value.available"),
                localizedAppText("ride.hero.provenance.vehicle_telemetry"),
                freshness.accessibilityText,
                severity.accessibilityText
            )
        case .unavailable(let freshness, let severity):
            localizedAppText(
                "ride.hero.accessibility.unavailable",
                localizedAppText("ride.hero.value.unavailable_accessibility"),
                localizedAppText("ride.hero.provenance.vehicle_telemetry"),
                freshness.accessibilityText,
                severity.accessibilityText
            )
        }
    }
}

private extension EucRideUpdateFreshness {
    var accessibilityText: String {
        switch self {
        case .fresh: localizedAppText("ride.hero.freshness.fresh")
        case .stale: localizedAppText("ride.hero.freshness.stale")
        case .unavailable: localizedAppText("ride.hero.freshness.unavailable")
        }
    }
}

private extension RideHeroSeverity {
    var accessibilityText: String {
        switch self {
        case .nominal: localizedAppText("ride.hero.severity.nominal")
        case .caution: localizedAppText("ride.hero.severity.caution")
        case .critical: localizedAppText("ride.hero.severity.critical")
        case .unavailable: localizedAppText("ride.hero.severity.unavailable")
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
    let statusTone: PevDashboardStatusPillTone
    let captureStatusText: String?
    let speedReadout: RideHeroReadout
    let speedCaption: String

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 8) {
                    titleText
                    PevDashboardStatusPill(title: subtitle, tone: statusTone)
                }
            } else {
                HStack(alignment: .center, spacing: 12) {
                    titleText
                    Spacer(minLength: 8)
                    PevDashboardStatusPill(title: subtitle, tone: statusTone)
                }
            }
        }
        .padding(.top, 8)
        .accessibilityElement(children: .combine)
        .accessibilityHeading(.h1)
        .accessibilityIdentifier("ride.hero.status")

        if let captureStatusText {
            PevStatusStrip(
                text: captureStatusText
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
        .accessibilityElement(children: .combine)
        .accessibilityLabel(speedCaption)
        .accessibilityValue(speedReadout.accessibilityValue)
        .accessibilityIdentifier("ride.hero.speed")
    }

    private var titleText: some View {
        Text(title)
            .font(.system(.headline, design: .default, weight: .semibold))
            .foregroundStyle(PevColors.primaryText)
    }

    @ViewBuilder
    private var speed: some View {
        if let displayValue = speedReadout.displayValue {
            Text(displayValue)
                .font(speedFont)
                .monospacedDigit()
        } else {
            Text(localizedAppText("ride.hero.value.unavailable"))
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
        if let unit = speedReadout.displayUnit, !unit.isEmpty {
            Text(unit)
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
