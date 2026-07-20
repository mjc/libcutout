import CutoutMobile
import SwiftUI

struct PevRideHeroSection: View {
    @ScaledMetric(relativeTo: .largeTitle) private var speedFontSize: CGFloat = 104
    @ScaledMetric(relativeTo: .title2) private var speedUnitFontSize: CGFloat = 27

    let title: String
    let subtitle: String
    let statusFill: Color
    let captureStatusText: String?
    let speedValue: String
    let speedUnit: String
    let speedCaption: String
    let scale: CGFloat

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
            HStack(alignment: .firstTextBaseline, spacing: 9 * scale) {
                Text(speedValue)
                    .font(.system(size: speedFontSize * scale, weight: .black))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.72)
                if !speedUnit.isEmpty {
                    Text(speedUnit)
                        .font(.system(size: speedUnitFontSize * scale, weight: .bold))
                        .foregroundStyle(PevColors.muted)
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
}
