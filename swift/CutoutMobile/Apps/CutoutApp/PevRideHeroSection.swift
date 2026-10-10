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

extension EucRideUpdateFreshness {
    fileprivate var accessibilityText: String {
        switch self {
        case .fresh: localizedAppText("ride.hero.freshness.fresh")
        case .stale: localizedAppText("ride.hero.freshness.stale")
        case .unavailable: localizedAppText("ride.hero.freshness.unavailable")
        }
    }
}

extension RideHeroSeverity {
    fileprivate var accessibilityText: String {
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

    let style: PevRideHeroStyle
    let title: String
    let subtitle: String
    let statusTone: PevDashboardStatusPillTone
    let captureStatusText: String?
    let speedReadout: RideHeroReadout
    let speedCaption: String
    var speedPointSize: CGFloat = 124
    var gpsSpeed: PevDashboardTile? = nil
    var compactLayout = false

    var body: some View {
        VStack(spacing: 6) {
            let headerLayout =
                dynamicTypeSize.isAccessibilitySize
                ? AnyLayout(VStackLayout(alignment: .leading, spacing: 6))
                : AnyLayout(HStackLayout(spacing: 8))
            headerLayout {
                HStack(spacing: 8) {
                    Text(title)
                        .font(
                            dynamicTypeSize.isAccessibilitySize
                                ? .caption.weight(.semibold) : .headline.weight(.semibold)
                        )
                        .fixedSize(horizontal: false, vertical: true)
                    Circle()
                        .fill(PevColors.red)
                        .frame(width: 8, height: 8)
                        .opacity(captureStatusText == nil ? 0 : 1)
                        .accessibilityLabel(captureStatusText ?? "")
                        .accessibilityHidden(captureStatusText == nil)
                        .accessibilityIdentifier("ride.recording.indicator")
                    Spacer(minLength: 0)
                }
                .layoutPriority(1)
                PevDashboardStatusPill(title: subtitle, tone: statusTone)
                    .lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 1)
                    .minimumScaleFactor(dynamicTypeSize.isAccessibilitySize ? 1 : 0.6)
                    .layoutPriority(1)
            }
            .frame(maxWidth: .infinity, minHeight: 34, alignment: .leading)
            .accessibilityElement(children: .contain)
            .accessibilityHeading(.h1)
            .accessibilityLabel("\(title), \(subtitle)")
            .accessibilityValue(statusTone == .warning ? pevLocalizedText("status.accessibility.warning") : "")
            .accessibilityIdentifier("ride.hero.status")

            HStack(alignment: .center, spacing: 8) {
                Text(speedReadout.displayValue ?? localizedAppText("ride.hero.value.unavailable"))
                    .font(
                        dynamicTypeSize.isAccessibilitySize
                            ? .title2.weight(.black) : .system(size: speedPointSize, weight: .black)
                    )
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(dynamicTypeSize.isAccessibilitySize ? 1 : 0.4)
                    .accessibilityLabel(speedCaption)
                    .accessibilityValue(speedReadout.accessibilityValue)
                    .accessibilityIdentifier("ride.hero.speed")
                VStack(alignment: .leading, spacing: 8) {
                    Text(speedReadout.displayUnit ?? RideUnits.speedUnit)
                        .font(dynamicTypeSize.isAccessibilitySize ? .caption.weight(.bold) : .headline.weight(.bold))
                        .lineLimit(1)
                        .minimumScaleFactor(dynamicTypeSize.isAccessibilitySize ? 1 : 0.6)
                        .foregroundStyle(PevColors.muted)
                    if let gpsSpeed, !dynamicTypeSize.isAccessibilitySize || !compactLayout {
                        Text("GPS \(gpsSpeed.value)")
                            .font(.caption.weight(.semibold))
                            .monospacedDigit()
                            .lineLimit(1)
                            .minimumScaleFactor(dynamicTypeSize.isAccessibilitySize ? 1 : 0.6)
                            .foregroundStyle(PevColors.muted)
                            .accessibilityLabel(gpsSpeed.label)
                            .accessibilityValue(
                                gpsSpeed.metricValue.accessibilityValue(unit: gpsSpeed.unit, detail: gpsSpeed.detail)
                            )
                            .accessibilityIdentifier("ride.gps.speed")
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .foregroundStyle(PevColors.primaryText)
    }
}
