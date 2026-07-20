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
        GeometryReader { proxy in
            let designWidth = proxy.size.width
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
                                header(scale: scale)
                                chipRow(scale: scale)
                                contentSection(scale: scale)
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
            .frame(width: designWidth, height: proxy.size.height, alignment: .topLeading)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(PevColors.pageBackground)
            .foregroundStyle(PevColors.primaryText)
        }
    }

    @ViewBuilder
    private func contentSection(scale: CGFloat) -> some View {
        switch content.kind {
        case .overview:
            BmsOverviewLayout(content: content, scale: scale)
        case .cellMapInline:
            BmsInlineLayout(content: content, scale: scale)
        case .cellMapScrollable:
            BmsScrollableLayout(content: content, scale: scale)
        case .cellDetail:
            BmsDetailLayout(content: content, scale: scale)
        case .unknownTopology:
            BmsUnknownLayout(content: content, scale: scale)
        case .noData:
            EmptyView()
        }
    }

    private func header(scale: CGFloat) -> some View {
        PevScreenTitleBlock(title: screen.title, subtitle: "CutOut · BMS", scale: scale)
    }

    private func chipRow(scale: CGFloat) -> some View {
        PevDashboardGrid(
            columns: [GridItem(.adaptive(minimum: 100 * scale), spacing: 10 * scale)],
            spacing: 10 * scale
        ) {
            ForEach(content.chips, id: \.self) { chip in
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
