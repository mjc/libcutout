import CutoutMobile
import SwiftUI

struct BmsScreenView: View {
    let screen: PevScreen
    let rideState: EucRideScreenState?
    let bmsSnapshot: BmsSnapshot?
    let selectedGroupIndex: Int?
    let showGroupDetail: (Int) -> Void
    let showCellMap: () -> Void

    private var content: PevBmsContent {
        screen.bmsContent ?? PevBmsContent(
            kind: .unknownTopology,
            snapshot: BmsSnapshot(
                topology: BmsTopology(
                    layoutLabel: "missing data",
                    seriesGroupCount: nil,
                    parallelCount: nil,
                    packCount: 0,
                    bmsCount: 0,
                    confidence: .unverified
                )
            )
        )
    }

    var body: some View {
        Group {
            if content.kind == .noData {
                BmsNoDataLayout(
                    screen: screen,
                    content: content,
                    rideState: rideState,
                    liveSnapshot: bmsSnapshot
                )
            } else {
                VStack(spacing: 0) {
                    ScrollView(.vertical, showsIndicators: false) {
                        VStack(alignment: .leading, spacing: 14) {
                            PevScreenTitleBlock(
                                title: screen.title,
                                subtitle: localizedAppText("bms.screen.subtitle")
                            )
                            chipRow
                            contentSection()
                            if let bmsSnapshot, bmsSnapshot.shouldRenderReadback {
                                BmsDiagnosticsSection(snapshot: bmsSnapshot)
                            }
                        }
                        .padding(.horizontal, 23)
                        .padding(.top, 31)
                        .padding(.bottom, 18)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(PevColors.pageBackground)
        .foregroundStyle(PevColors.primaryText)
    }

    @ViewBuilder
    private func contentSection() -> some View {
        switch content.kind {
        case .overview:
            BmsOverviewLayout(content: content)
        case .cellMapInline:
            BmsInlineLayout(content: content, showGroupDetail: showGroupDetail)
        case .cellMapScrollable:
            BmsScrollableLayout(content: content, showGroupDetail: showGroupDetail)
        case .cellDetail:
            BmsDetailLayout(
                content: content,
                selectedGroupIndex: selectedGroupIndex,
                showGroupDetail: showGroupDetail,
                showCellMap: showCellMap
            )
        case .unknownTopology:
            BmsUnknownLayout(content: content)
        case .noData:
            EmptyView()
        }
    }

    private var chipRow: some View {
        PevDashboardGrid(
            columns: [GridItem(.adaptive(minimum: 100), spacing: 10)],
            spacing: 10
        ) {
            ForEach(content.chips) { chip in
                BmsChip(
                    title: chip.title,
                    accent: chip.accent
                )
            }
        }
    }

}

struct BmsDiagnosticsSection: View {
    let snapshot: BmsSnapshot
    @State private var isExpanded = false

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            PevDashboardKeyValueRows(
                rows: snapshot.readbackRows
                    .filter { $0.role == .data }
                    .map { row in
                        PevDashboardKeyValueRow(
                            id: row.id,
                            label: row.label,
                            metricValue: row.metricValue
                        )
                    },
                verticalPadding: 6
            )
            .padding(.top, 8)
        } label: {
            VStack(alignment: .leading, spacing: 3) {
                Text(localizedAppText("bms.diagnostics.title"))
                    .font(.headline)
                    .foregroundStyle(PevColors.primaryText)
                Text(localizedAppText("bms.diagnostics.detail"))
                    .font(.caption.weight(.bold))
                    .foregroundStyle(PevColors.muted)
            }
        }
        .tint(PevColors.muted)
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .background(PevDashboardCardBackground(cornerRadius: 20))
        .accessibilityIdentifier("bms.diagnostics")
    }
}
