import CutoutMobile
import SwiftUI

struct PevPlaceholderScreenView: View {
    let screen: PevScreen

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: screen.title,
            bottomPadding: 20,
            allowsVerticalScroll: false,
            columnSpacing: 12,
            contentSpacing: 14,
            showsHeader: false
        ) { scale, columns in
            VStack(alignment: .leading, spacing: 12 * scale) {
                HStack(spacing: 10 * scale) {
                    PevDashboardStatusPill(
                        title: screen.primaryValue,
                        scale: scale,
                        fill: PevColors.yellow,
                        foreground: .black,
                        fontSize: 14,
                        horizontalPadding: 14,
                        height: 30
                    )
                    PevDashboardStatusPill(
                        title: screen.secondaryValue,
                        scale: scale,
                        fill: PevColors.cardStroke,
                        foreground: PevColors.primaryText,
                        fontSize: 12,
                        horizontalPadding: 14,
                        height: 30
                    )
                    Spacer(minLength: 0)
                }

                PevDashboardWarningCard(
                    title: screen.title,
                    detail: screen.warning ?? screen.subtitle,
                    accent: PevColors.orange,
                    detailColor: PevColors.primaryText,
                    fill: PevColors.cardFill,
                    stroke: PevColors.cardStroke,
                    scale: scale,
                    cornerRadius: 24
                )

                PevDashboardKeyValueRows(
                    rows: screen.metrics.enumerated().map { index, metric in
                        PevDashboardKeyValueRow(
                            id: "\(screen.id.rawValue)-\(index)",
                            label: metric.label,
                            value: metric.value
                        )
                    },
                    scale: scale
                )
            }
        }
    }
}
