import CutoutMobile
import SwiftUI

struct BmsScreenView: View {
    let screen: PevScreen
    let rideState: EucRideScreenState?
    let bmsSnapshot: BmsSnapshot?
    @State private var showsDiagnostics = false

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
        let scale: CGFloat = 1

        Group {
            if content.kind == .noData {
                BmsNoDataLayout(
                    screen: screen,
                    content: content,
                    rideState: rideState,
                    liveSnapshot: bmsSnapshot,
                    scale: scale,
                )
            } else {
                VStack(spacing: 0) {
                    ScrollView(.vertical, showsIndicators: false) {
                        VStack(alignment: .leading, spacing: 14 * scale) {
                            header
                            chipRow(scale: scale)
                            contentSection()
                            if let bmsSnapshot, bmsSnapshot.shouldRenderReadback {
                                liveReadbackSection(snapshot: bmsSnapshot, scale: scale)
                            }
                        }
                        .padding(.horizontal, 23 * scale)
                        .padding(.top, 31 * scale)
                        .padding(.bottom, 18 * scale)
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
            BmsInlineLayout(content: content)
        case .cellMapScrollable:
            BmsScrollableLayout(content: content)
        case .cellDetail:
            BmsDetailLayout(content: content)
        case .unknownTopology:
            BmsUnknownLayout(content: content)
        case .noData:
            EmptyView()
        }
    }

    private var header: some View {
        PevScreenTitleBlock(title: screen.title, subtitle: "CutOut · BMS")
    }

    private func chipRow(scale: CGFloat) -> some View {
        PevDashboardGrid(
            columns: [GridItem(.adaptive(minimum: 100 * scale), spacing: 10 * scale)],
            spacing: 10 * scale
        ) {
            ForEach(content.chips) { chip in
                BmsChip(
                    title: chip.title,
                    accent: chip.accent
                )
            }
        }
    }

    private func liveReadbackSection(snapshot: BmsSnapshot, scale: CGFloat) -> some View {
        BmsDiagnosticsSection(
            snapshot: snapshot,
            scale: scale,
            isExpanded: $showsDiagnostics
        )
    }
}
