import CutoutMobile
import SwiftUI

struct PevRideHeroSection: View {
    let title: String
    let subtitle: String
    let statusFill: Color
    let captureStatusText: String?
    let speedValue: String
    let speedUnit: String
    let speedCaption: String
    let scale: CGFloat

    var body: some View {
        HStack(alignment: .center, spacing: 12 * scale) {
            Text(title)
                .font(.system(size: 18 * scale, weight: .bold))
                .foregroundStyle(PevColors.primaryText)
                .lineLimit(1)
                .minimumScaleFactor(0.85)
            Spacer(minLength: 8 * scale)
            PevDashboardStatusPill(
                title: subtitle,
                scale: scale,
                fill: statusFill
            )
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
                    .font(.system(size: 104 * scale, weight: .black))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.72)
                if !speedUnit.isEmpty {
                    Text(speedUnit)
                        .font(.system(size: 27 * scale, weight: .bold))
                        .foregroundStyle(PevColors.muted)
                }
            }
            Text(speedCaption)
                .font(.system(size: 13 * scale, weight: .bold))
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity)
        .foregroundStyle(PevColors.primaryText)
    }
}
