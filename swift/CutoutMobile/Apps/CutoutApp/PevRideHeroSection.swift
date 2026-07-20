import CutoutMobile
import SwiftUI

enum PevRideHeroStyle {
    case electricUnicycle
    case vescOnewheel

    var speedPointSize: CGFloat {
        switch self {
        case .electricUnicycle: 138
        case .vescOnewheel: 124
        }
    }

    var unitPointSize: CGFloat { 24 }
}

struct PevRideHeroSection: View {
    @ScaledMetric(relativeTo: .largeTitle) private var speedFontSize: CGFloat = 0
    @ScaledMetric(relativeTo: .title2) private var speedUnitFontSize: CGFloat = 0

    let title: String
    let subtitle: String
    let statusFill: Color
    let captureStatusText: String?
    let speedValue: String
    let speedUnit: String
    let speedCaption: String
    let scale: CGFloat

    init(
        style: PevRideHeroStyle,
        title: String,
        subtitle: String,
        statusFill: Color,
        captureStatusText: String?,
        speedValue: String,
        speedUnit: String,
        speedCaption: String,
        scale: CGFloat
    ) {
        _speedFontSize = ScaledMetric(
            wrappedValue: style.speedPointSize,
            relativeTo: .largeTitle
        )
        _speedUnitFontSize = ScaledMetric(
            wrappedValue: style.unitPointSize,
            relativeTo: .title2
        )
        self.title = title
        self.subtitle = subtitle
        self.statusFill = statusFill
        self.captureStatusText = captureStatusText
        self.speedValue = speedValue
        self.speedUnit = speedUnit
        self.speedCaption = speedCaption
        self.scale = scale
    }

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .center, spacing: 12 * scale) {
                titleText
                Spacer(minLength: 8 * scale)
                statusPill
            }
            VStack(alignment: .leading, spacing: 8 * scale) {
                titleText
                statusPill
            }
        }
        .padding(.top, 8 * scale)

        if let captureStatusText {
            PevStatusStrip(
                text: captureStatusText,
                scale: scale,
                indicatorColor: PevColors.green,
                background: PevColors.cardFill,
                foreground: PevColors.primaryText,
                cornerRadius: 18
            )
        }

        VStack(alignment: .center, spacing: 2 * scale) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline, spacing: 9 * scale) {
                    speed
                    unit
                }
                VStack(spacing: 2 * scale) {
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
            scale: scale,
            fill: statusFill
        )
    }

    private var speed: some View {
        Text(speedValue)
            .font(.system(size: speedFontSize, weight: .black))
            .monospacedDigit()
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
