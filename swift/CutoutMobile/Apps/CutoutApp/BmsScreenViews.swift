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
        screen.bmsContentOrUnavailable
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
                    .safeAreaPadding(.bottom)
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
            adaptiveMinimumColumnWidth: 100,
            columnSpacing: 10,
            spacing: 10
        ) {
            ForEach(content.chips) { chip in
                BmsChip(
                    id: chip.id,
                    title: chip.title,
                    accent: chip.accent
                )
            }
        }
    }
}
