import CutoutMobile
import SwiftUI

struct VescDebugMockupView: View {
    let screen: MockupScreen

    var body: some View {
        MockupScreenScaffold(sectionTitle: "VESC debug", bottomPadding: 20) { scale, columns in
            VStack(alignment: .leading, spacing: 8 * scale) {
                Text(screen.title)
                    .font(.system(size: 29 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                Text(screen.subtitle)
                    .font(.system(size: 14 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                    .lineLimit(2)
            }

            if let profile = screen.deviceCard {
                VescProfileCard(card: profile, scale: scale)
                    .padding(.top, 10 * scale)
            }

            LazyVGrid(columns: columns, spacing: 20 * scale) {
                ForEach(screen.dashboardTiles) { tile in
                    PevDashboardMetricTile(
                        label: tile.label,
                        value: tile.value,
                        unit: tile.unit,
                        detail: tile.detail,
                        accent: tile.accent.color,
                        scale: scale,
                        cornerRadius: 16,
                        minHeight: 104
                    )
                }
            }
            .padding(.top, 8 * scale)

            if let summaryTitle = screen.summaryTitle {
                Text(summaryTitle)
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 0)
            }

            if !screen.summaryRows.isEmpty {
                EucSummaryRows(rows: screen.summaryRows, scale: scale)
            }

            if let guardrail = screen.faultCard {
                Text(guardrail.title)
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 6 * scale)

                VescGuardrailCard(card: guardrail, scale: scale)
            }
        }
    }
}

struct VescProfileCard: View {
    let card: MockupDeviceCard
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            Text(card.title)
                .font(.system(size: 22 * scale, weight: .black))
                .foregroundStyle(MockupColors.primaryText)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(card.detail)
                .font(.system(size: 13 * scale, weight: .bold))
                .foregroundStyle(MockupColors.muted)
                .lineLimit(1)
                .minimumScaleFactor(0.72)
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 87 * scale, alignment: .center)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(CardBackground(cornerRadius: 25 * scale))
    }
}

struct VescGuardrailCard: View {
    let card: MockupFaultCard
    let scale: CGFloat

    var body: some View {
        MockupFaultDetailCard(
            card: card,
            scale: scale,
            fontSize: 13,
            horizontalAlignment: .center,
            horizontalPadding: 20,
            height: 57,
            cornerRadius: 18,
            minimumScaleFactor: 0.72
        )
    }
}
