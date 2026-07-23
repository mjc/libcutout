import CutoutMobile
import SwiftUI

struct VescDebugScreenView: View {
    let snapshot: VescRideSnapshot?
    let phase: SessionConnectionPhase
    let notificationCount: UInt64
    let captureStatusText: String?
    let connectionStatusText: String?

    private var tiles: [PevDashboardTile] {
        snapshot.map(vescDebugTiles) ?? []
    }

    private var rows: [PevDashboardKeyValueRow] {
        vescDebugRows(snapshot, phase: phase, notificationCount: notificationCount)
    }

    var body: some View {
        PevDashboardScaffold(sectionTitle: localizedAppText("vesc.debug.section"), bottomPadding: 20, showsHeader: false) {
            PevScreenTitleBlock(
                title: snapshot?.title ?? localizedAppText("vesc.debug.title"),
                subtitle: captureStatusText ?? connectionStatusText ?? phase.displayText
            )

            PevDashboardGrid(spacing: 20) {
                ForEach(tiles) { tile in
                    PevDashboardMetricTile(tile, cornerRadius: 16, minHeight: 104)
                }
            }
            .padding(.top, 8)

            PevDashboardKeyValueRows(
                rows: rows,
                fill: PevColors.cardFill,
                stroke: PevColors.cardStroke,
                labelColor: PevColors.muted,
                valueColor: PevColors.primaryText
            )
        }
        .accessibilityElement(children: .contain)
    }
}
