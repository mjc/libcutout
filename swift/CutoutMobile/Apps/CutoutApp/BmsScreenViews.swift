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
                            Text(localizedAppText("bms.pack.title"))
                                .font(.largeTitle.bold())
                                .accessibilityHeading(.h1)
                            contentSection()
                        }
                        .padding(.horizontal, 23)
                        .padding(.top, 20)
                        .padding(.bottom, 32)
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
        case .overview, .cellMapInline, .cellMapScrollable, .unknownTopology:
            BmsPackLayout(snapshot: content.snapshot, showGroupDetail: showGroupDetail)
        case .cellDetail:
            BmsDetailLayout(
                content: content,
                selectedGroupIndex: selectedGroupIndex,
                showGroupDetail: showGroupDetail,
                showCellMap: showCellMap
            )
        case .noData:
            EmptyView()
        }
    }

}
