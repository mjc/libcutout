import CutoutMobile
import SwiftUI

struct BmsInlineLayout: View {
    let content: PevBmsContent
    let showGroupDetail: (Int) -> Void

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            PevDashboardWideCard(
                title: "topology fits inline",
                value: snapshot.cellMapVisibilitySummary,
                detail: snapshot.topology.layoutLabel,
                accent: PevColors.green
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
                title: "range of interest",
                value: snapshot.cellMapSpreadSummary,
                detail: snapshot.cellMapFocusSummary,
                accent: PevColors.cyan
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
                title: "large packs use grouped overview first",
                value: snapshot.cellMapVisibilitySummary,
                detail: snapshot.topology.layoutLabel,
                accent: PevColors.cyan
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
                title: "interesting groups",
                value: snapshot.cellMapFocusSummary,
                detail: snapshot.cellMapFocusDetail ?? snapshot.cellMapSpreadSummary,
                accent: PevColors.orange,
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
    let showCellMap: () -> Void
    @State private var selectedGroupIndex: Int?

    init(
        content: PevBmsContent,
        selectedGroupIndex: Int?,
        showCellMap: @escaping () -> Void
    ) {
        self.content = content
        self.showCellMap = showCellMap
        _selectedGroupIndex = State(initialValue: selectedGroupIndex)
    }

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
                Label("Back to cell map", systemImage: "chevron.left")
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
                        action: { selectedGroupIndex = group.index }
                    )
                }
            }

            if let selectedGroup {
                VStack(alignment: .leading, spacing: 15) {
                    Text("group \(selectedGroup.index)")
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
                            label: "temp",
                            value: temperatureText(selectedGroup.temperature),
                            unit: "°C",
                            detail: "",
                            accent: PevColors.green,
                            detailColor: PevColors.primaryText
                        )
                        PevDashboardMetricTile(
                            label: "IR est.",
                            value: selectedGroup.resistance.map { String($0.value) } ?? "--",
                            unit: "mΩ",
                            detail: "",
                            accent: PevColors.green,
                            detailColor: PevColors.primaryText
                        )
                    }

                    PevDashboardWideCard(
                        title: nil,
                        value: "trend: \(snapshot.detailGroupTrend(for: selectedGroup.index))",
                        detail: snapshot.detailGroupTrendDetail(for: selectedGroup.index),
                        accent: PevColors.yellow
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
