import CutoutMobile
import SwiftUI

struct BmsInlineLayout: View {
    let content: PevBmsContent
    let showGroupDetail: (Int) -> Void

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            PevDashboardWideCard(
                title: localizedAppText("bms.inline.topology_fits"),
                metricValue: .available(
                    display: snapshot.cellMapVisibilitySummary,
                    accessibility: snapshot.cellMapVisibilitySummary
                ),
                detail: snapshot.topology.layoutLabel
            )

            PevDashboardGrid(
                columns: [GridItem(.adaptive(minimum: 96), spacing: 12)],
                spacing: 14
            ) {
                ForEach(snapshot.groups) { group in
                    BmsGroupCell(
                        group: group,
                        isHighlighted: content.highlightedGroupIndices.contains(group.index),
                        action: { showGroupDetail(group.index) }
                    )
                }
            }

            PevDashboardWideCard(
                title: localizedAppText("bms.inline.range_of_interest"),
                metricValue: .available(
                    display: snapshot.cellMapSpreadSummary,
                    accessibility: snapshot.cellMapSpreadSummary
                ),
                detail: snapshot.cellMapFocusSummary
            )

            VStack(alignment: .leading, spacing: 14) {
                Text(String(localized: "bms.display_modes"))
                    .font(.headline)
                    .foregroundStyle(PevColors.muted)
                    .accessibilityAddTraits(.isHeader)
                PevDashboardGrid(
                    columns: [GridItem(.adaptive(minimum: 100), spacing: 10)],
                    spacing: 10
                ) {
                    ForEach(content.modes) { mode in
                        BmsModeChip(title: mode.title)
                    }
                }
                Text(snapshot.cellMapInteractionHint)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 24))
        }
    }
}

struct BmsScrollableLayout: View {
    let content: PevBmsContent
    let showGroupDetail: (Int) -> Void

    private var snapshot: BmsSnapshot { content.snapshot }

    private var columns: [GridItem] {
        [GridItem(.adaptive(minimum: 72), spacing: 8)]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            PevDashboardWideCard(
                title: localizedAppText("bms.scroll.large_packs"),
                metricValue: .available(
                    display: snapshot.cellMapVisibilitySummary,
                    accessibility: snapshot.cellMapVisibilitySummary
                ),
                detail: snapshot.topology.layoutLabel
            )

            PevDashboardGrid(columns: columns, spacing: 8) {
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
                metricValue: .available(
                    display: snapshot.cellMapFocusSummary,
                    accessibility: snapshot.cellMapFocusSummary
                ),
                detail: snapshot.cellMapFocusDetail ?? snapshot.cellMapSpreadSummary,
                stroke: PevColors.orange
            )

            VStack(alignment: .leading, spacing: 10) {
                Text(String(localized: "bms.display_modes"))
                    .font(.headline)
                    .foregroundStyle(PevColors.muted)
                    .accessibilityAddTraits(.isHeader)
                Text(content.modeTitles.joined(separator: " • "))
                    .font(.headline.weight(.black))
                Text(snapshot.scrollableCellMapRule)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
                Text(snapshot.scrollableCellMapFocusHint)
                    .font(.subheadline.weight(.black))
                    .foregroundStyle(PevColors.primaryText)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 24))
        }
    }
}

struct BmsDetailLayout: View {
    let content: PevBmsContent
    let selectedGroupIndex: Int?
    let showGroupDetail: (Int) -> Void
    let showCellMap: () -> Void

    private var snapshot: BmsSnapshot { content.snapshot }
    private var selectedGroup: BmsGroupSnapshot? {
        if let selectedGroupIndex,
           let selectedGroup = snapshot.groups.first(where: { $0.index == selectedGroupIndex }) {
            return selectedGroup
        }
        if let contentGroupIndex = content.selectedGroupIndex,
           let contentGroup = snapshot.groups.first(where: { $0.index == contentGroupIndex }) {
            return contentGroup
        }
        return snapshot.groups.first
    }

    private var columns: [GridItem] {
        [GridItem(.adaptive(minimum: 52), spacing: 10)]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Button(action: showCellMap) {
                Label(localizedAppText("bms.detail.back_to_cell_map"), systemImage: "chevron.left")
                    .font(.body.weight(.semibold))
            }
            .buttonStyle(.bordered)
            .frame(minHeight: 44)
            .accessibilityIdentifier("bms.detail.back")

            PevDashboardGrid(columns: columns, spacing: 10) {
                ForEach(snapshot.groups) { group in
                    BmsGroupIndexCell(
                        group: group,
                        isSelected: group.index == selectedGroup?.index,
                        action: { showGroupDetail(group.index) }
                    )
                }
            }

            if let selectedGroup {
                VStack(alignment: .leading, spacing: 15) {
                    Text(localizedAppText("bms.detail.group", Int64(selectedGroup.index)))
                        .font(.headline)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityAddTraits(.isHeader)
                    Text(groupVoltageText(selectedGroup))
                        .font(.largeTitle.weight(.black))
                        .monospacedDigit()
                        .accessibilityLabel(selectedGroup.accessibilityLabel)
                        .accessibilityValue(selectedGroup.accessibilityValue)
                    Text(snapshot.detailGroupStatus(for: selectedGroup.index))
                        .font(.headline.weight(.black))
                        .foregroundStyle(PevColors.primaryText)

                    PevDashboardGrid(
                        columns: [
                            GridItem(.adaptive(minimum: 140), spacing: 14),
                        ],
                        spacing: 14
                    ) {
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.detail.temperature"),
                            metricValue: selectedGroup.temperature.map {
                                .available(display: temperatureText($0), accessibility: temperatureText($0))
                            } ?? .unavailable,
                            unit: "°C",
                            detail: "",
                            detailColor: PevColors.primaryText
                        )
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.detail.resistance"),
                            metricValue: selectedGroup.resistance.map {
                                let value = RideUnits.decimalString(Double($0.value), fractionDigits: 0)
                                return .available(display: value, accessibility: value)
                            } ?? .unavailable,
                            unit: "mΩ",
                            detail: "",
                            detailColor: PevColors.primaryText
                        )
                    }

                    PevDashboardWideCard(
                        title: nil,
                        metricValue: {
                            let value = localizedAppText("bms.detail.trend", snapshot.detailGroupTrend(for: selectedGroup.index))
                            return .available(display: value, accessibility: value)
                        }(),
                        detail: snapshot.detailGroupTrendDetail(for: selectedGroup.index)
                    )
                }
                .padding(.horizontal, 18)
                .padding(.vertical, 20)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(PevDashboardCardBackground(cornerRadius: 34, stroke: PevColors.yellow, lineWidth: 1.2))
            }
        }
    }
}
