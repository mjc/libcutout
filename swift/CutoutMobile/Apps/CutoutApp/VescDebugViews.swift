import CutoutMobile
import SwiftUI

struct VescDebugScreenView: View {
    let screen: PevScreen

    var body: some View {
        PevDashboardScaffold(sectionTitle: "VESC debug", bottomPadding: 20) { scale, columns in
            PevScreenTitleBlock(
                title: screen.title,
                subtitle: screen.subtitle,
                scale: scale,
                titleFontSize: 29,
                subtitleFontSize: 14,
                titleMinimumScaleFactor: 0.8,
                subtitleLineLimit: 2
            )

            if let profile = screen.deviceCard {
                PevDashboardIdentityCard(
                    title: profile.title,
                    detail: profile.detail,
                    scale: scale,
                    titleFontSize: 22,
                    detailFontSize: 13,
                    titleMinimumScaleFactor: 0.75,
                    detailMinimumScaleFactor: 0.72,
                    trailingStatus: nil,
                    trailingStatusFill: PevColors.cardStroke,
                    trailingStatusForeground: PevColors.primaryText,
                    trailingStatusWidth: 18,
                    trailingStatusHeight: 32,
                    cornerRadius: 25,
                    height: 87
                )
                    .padding(.top, 10 * scale)
            }

            PevDashboardGrid(columns: columns, spacing: 20 * scale) {
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
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 0)
            }

            if !screen.summaryRows.isEmpty {
                PevDashboardKeyValueRows(
                    rows: screen.summaryRows.map { row in
                        PevDashboardKeyValueRow(
                            id: row.id,
                            label: row.label,
                            value: row.value,
                            valueColor: row.accent?.color
                        )
                    },
                    scale: scale,
                    fill: PevColors.cardFill,
                    stroke: PevColors.cardStroke,
                    labelColor: PevColors.muted,
                    valueColor: PevColors.primaryText
                )
            }

            if let guardrail = screen.faultCard {
                Text(guardrail.title)
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 6 * scale)

                PevDashboardFaultDetailCard(
                    detail: guardrail.detail,
                    accent: guardrail.accent.color,
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
    }
}
