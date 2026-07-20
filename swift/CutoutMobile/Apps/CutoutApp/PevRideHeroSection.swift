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

struct PevRideHeroSection: View {
    @ScaledMetric(relativeTo: .largeTitle) private var eucSpeedFontSize = PevRideHeroStyle.electricUnicycleSpeedPointSize
    @ScaledMetric(relativeTo: .largeTitle) private var vescSpeedFontSize = PevRideHeroStyle.vescOnewheelSpeedPointSize
    @ScaledMetric(relativeTo: .title2) private var speedUnitFontSize = PevRideHeroStyle.unitPointSize

    let style: PevRideHeroStyle
    let title: String
    let subtitle: String
    let statusFill: Color
    let captureStatusText: String?
    let speedValue: String
    let speedUnit: String
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
                scale: 1,
                indicatorColor: PevColors.green,
                background: PevColors.cardFill,
                foreground: PevColors.primaryText,
                cornerRadius: 18
            )
        }

        VStack(alignment: .center, spacing: 2) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline, spacing: 9) {
                    speed
                    unit
                }
                VStack(spacing: 2) {
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
        .accessibilityValue([speedValue, speedUnit].filter { !$0.isEmpty }.joined(separator: " "))
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
            scale: 1,
            fill: statusFill
        )
    }

    private var speed: some View {
        Text(speedValue)
            .font(.system(size: speedFontSize, weight: .black))
            .monospacedDigit()
    }

    private var speedFontSize: CGFloat {
        switch style {
        case .electricUnicycle: eucSpeedFontSize
        case .vescOnewheel: vescSpeedFontSize
        }
    }

    @ViewBuilder
    private var unit: some View {
        if !speedUnit.isEmpty {
            Text(speedUnit)
                .font(.system(size: speedUnitFontSize, weight: .bold))
                .foregroundStyle(PevColors.muted)
        }
    }
}
