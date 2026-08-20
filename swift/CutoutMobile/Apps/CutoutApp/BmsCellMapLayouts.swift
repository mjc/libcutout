import CutoutMobile
import SwiftUI

struct BmsInlineLayout: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let content: PevBmsContent
    let showGroupDetail: (Int) -> Void

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            PevDashboardWideCard(
                title: localizedAppText("bms.inline.topology_fits"),
                metricValue: snapshot.cellMapVisibilityMetricValue,
                detail: snapshot.topology.layoutLabel
            )

            groupGrid

            PevDashboardWideCard(
                title: localizedAppText("bms.inline.range_of_interest"),
                metricValue: snapshot.cellMapSpreadMetricValue,
                detail: snapshot.cellMapFocusSummary
            )

            BmsDisplayModesCard(modes: content.modes) {
                Text(snapshot.cellMapInteractionHint)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
            }
        }
    }

    @ViewBuilder
    private var groupGrid: some View {
        if dynamicTypeSize.isAccessibilitySize {
            VStack(spacing: 14) {
                groupCells
            }
        } else {
            PevDashboardGrid(
                adaptiveMinimumColumnWidth: 96,
                accessibilityMinimumColumnWidth: 240,
                columnSpacing: 12,
                spacing: 14
            ) {
                groupCells
            }
        }
    }

    @ViewBuilder
    private var groupCells: some View {
        ForEach(snapshot.groups) { group in
            BmsGroupCell(
                group: group,
                isHighlighted: content.highlightedGroupIndices.contains(group.index),
                action: { showGroupDetail(group.index) }
            )
        }
    }
}

struct BmsScrollableLayout: View {
    let content: PevBmsContent
    let showGroupDetail: (Int) -> Void

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            PevDashboardWideCard(
                title: localizedAppText("bms.scroll.large_packs"),
                metricValue: snapshot.cellMapVisibilityMetricValue,
                detail: snapshot.topology.layoutLabel
            )

            PevDashboardGrid(
                adaptiveMinimumColumnWidth: 72,
                accessibilityMinimumColumnWidth: 240,
                columnSpacing: 8,
                spacing: 8
            ) {
                ForEach(snapshot.groups) { group in
                    BmsStripCell(
                        group: group,
                        isHighlighted: content.highlightedGroupIndices.contains(group.index),
                        action: { showGroupDetail(group.index) }
                    )
                }
            }

            PevDashboardWideCard(
                title: localizedAppText("bms.scroll.interesting_groups"),
                metricValue: snapshot.cellMapFocusMetricValue,
                detail: snapshot.scrollableCellMapFocusDetail,
                stroke: PevColors.orange
            )

            BmsDisplayModesCard(modes: content.modes, spacing: 10) {
                Text(snapshot.scrollableCellMapRule)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
                Text(snapshot.scrollableCellMapFocusHint)
                    .font(.subheadline.weight(.black))
                    .foregroundStyle(PevColors.primaryText)
            }
        }
    }
}

private struct BmsDisplayModesCard<Details: View>: View {
    let modes: [PevBmsMode]
    let spacing: CGFloat
    let details: Details

    init(
        modes: [PevBmsMode],
        spacing: CGFloat = 14,
        @ViewBuilder details: () -> Details
    ) {
        self.modes = modes
        self.spacing = spacing
        self.details = details()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: spacing) {
            Text(localizedAppText("bms.display_modes"))
                .font(.headline)
                .foregroundStyle(PevColors.muted)
                .accessibilityHeading(.h2)
            BmsModeGrid(modes: modes)
            details
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24))
    }
}
