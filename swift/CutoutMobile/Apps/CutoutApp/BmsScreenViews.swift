import CutoutMobile
import SwiftUI

struct BmsScreenView: View {
    let screen: PevScreen
    let rideState: EucRideScreenState?
    let bmsSnapshot: BmsSnapshot?
    let selectScreen: (PevScreenID) -> Void
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
            let designWidth = min(proxy.size.width, 390)
            let scale = min(1, designWidth / 390.0, proxy.size.height / 844.0)

            Group {
                if content.kind == .noData {
                    BmsNoDataLayout(
                        screen: screen,
                        content: content,
                        rideState: rideState,
                        liveSnapshot: bmsSnapshot,
                        scale: scale,
                        selectScreen: selectScreen
                    )
                } else {
                    VStack(spacing: 0) {
                        ScrollView(.vertical, showsIndicators: false) {
                            VStack(alignment: .leading, spacing: 14 * scale) {
                                header(scale: scale)
                                chipRow(scale: scale, contentWidth: designWidth - (46 * scale))
                                contentSection(scale: scale)
                                if let bmsSnapshot, bmsSnapshot.shouldRenderReadback {
                                    liveReadbackSection(snapshot: bmsSnapshot, scale: scale)
                                }
                            }
                            .padding(.horizontal, 23 * scale)
                            .padding(.top, 31 * scale)
                            .padding(.bottom, 18 * scale)
                        }

                        bottomTabs(scale: scale)
                            .padding(.horizontal, 23 * scale)
                            .padding(.top, 12 * scale)
                            .padding(.bottom, 18 * scale)
                            .background(PevColors.pageBackground)
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
        VStack(alignment: .leading, spacing: 2 * scale) {
            Text("CutOut · BMS")
                .font(.system(size: 15 * scale, weight: .medium))
                .foregroundStyle(PevColors.muted)
            Text(screen.title)
                .font(.system(size: 32 * scale, weight: .black))
                .lineLimit(1)
                .minimumScaleFactor(0.82)
        }
    }

    private func chipRow(scale: CGFloat, contentWidth: CGFloat) -> some View {
        let chipWidths: [CGFloat?]
        if content.chips.count == 3 {
            let availableWidth = max(contentWidth - (20 * scale), 0)
            chipWidths = [availableWidth * 0.38, availableWidth * 0.22, availableWidth * 0.40]
        } else {
            chipWidths = Array(repeating: nil, count: content.chips.count)
        }

        return HStack(spacing: 10 * scale) {
            ForEach(Array(content.chips.enumerated()), id: \.offset) { index, chip in
                BmsChip(
                    title: chip.title,
                    accent: chip.accent,
                    scale: scale,
                    maxWidth: chipWidths[index]
                )
            }
        }
    }

    private func bottomTabs(scale: CGFloat) -> some View {
        HStack {
            BmsBottomTab(title: "Ride", isSelected: false, scale: scale) {
                selectScreen(.eucRide)
            }
            Spacer()
            BmsBottomTab(title: "Pack", isSelected: true, scale: scale, action: nil)
            Spacer()
            BmsBottomTab(
                title: "Cells",
                isSelected: [.cellMapInline, .cellMapScrollable, .cellDetail].contains(content.kind),
                scale: scale,
                action: nil
            )
            Spacer()
            BmsBottomTab(title: "Faults", isSelected: content.kind == .unknownTopology, scale: scale, action: nil)
        }
        .padding(.horizontal, 18 * scale)
    }

    private func liveReadbackSection(snapshot: BmsSnapshot, scale: CGFloat) -> some View {
        BmsDiagnosticsSection(
            snapshot: snapshot,
            scale: scale,
            isExpanded: $showsDiagnostics
        )
    }
}
