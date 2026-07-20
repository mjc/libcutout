import CutoutMobile
import SwiftUI

struct BmsInlineLayout: View {
    let content: PevBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16 * scale) {
            PevDashboardWideCard(
                title: "topology fits inline",
                value: snapshot.cellMapVisibilitySummary,
                detail: snapshot.topology.layoutLabel,
                accent: PevColors.green,
                scale: scale
            )

            PevDashboardGrid(
                columns: [GridItem(.adaptive(minimum: 96), spacing: 12 * scale)],
                spacing: 14 * scale
            ) {
                ForEach(snapshot.groups) { group in
                    BmsGroupCell(
                        group: group,
                        isHighlighted: content.highlightedGroupIndices.contains(group.index),
                        isSelected: false,
                        scale: scale
                    )
                }
            }

            PevDashboardWideCard(
                title: "range of interest",
                value: snapshot.cellMapSpreadSummary,
                detail: snapshot.cellMapFocusSummary,
                accent: PevColors.cyan,
                scale: scale
            )

            VStack(alignment: .leading, spacing: 14 * scale) {
                Text("display modes")
                    .font(.headline)
                    .foregroundStyle(PevColors.muted)
                    .accessibilityAddTraits(.isHeader)
                PevDashboardGrid(
                    columns: [GridItem(.adaptive(minimum: 100), spacing: 10 * scale)],
                    spacing: 10 * scale
                ) {
                    ForEach(content.modeTitles, id: \.self) { title in
                        BmsModeChip(title: title)
                    }
                }
                Text(snapshot.cellMapInteractionHint)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
            }
            .padding(.horizontal, 18 * scale)
            .padding(.vertical, 18 * scale)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
        }
    }
}

struct BmsScrollableLayout: View {
    let content: PevBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }

    private var columns: [GridItem] {
        [GridItem(.adaptive(minimum: 72), spacing: 8 * scale)]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16 * scale) {
            PevDashboardWideCard(
                title: "large packs use grouped overview first",
                value: snapshot.cellMapVisibilitySummary,
                detail: snapshot.topology.layoutLabel,
                accent: PevColors.cyan,
                scale: scale
            )

            PevDashboardGrid(columns: columns, spacing: 8 * scale) {
                ForEach(snapshot.groups) { group in
                    BmsStripCell(
                        group: group,
                        isHighlighted: content.highlightedGroupIndices.contains(group.index),
                        scale: scale
                    )
                }
            }

            PevDashboardWideCard(
                title: "interesting groups",
                value: snapshot.cellMapFocusSummary,
                detail: snapshot.cellMapFocusDetail ?? snapshot.cellMapSpreadSummary,
                accent: PevColors.orange,
                stroke: PevColors.orange,
                scale: scale
            )

            VStack(alignment: .leading, spacing: 10 * scale) {
                Text("display modes")
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
                    .foregroundStyle(PevColors.yellow)
            }
            .padding(.horizontal, 18 * scale)
            .padding(.vertical, 18 * scale)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
        }
    }
}

struct BmsDetailLayout: View {
    let content: PevBmsContent
    let scale: CGFloat
    @State private var selectedGroupIndex: Int?

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
        [GridItem(.adaptive(minimum: 52), spacing: 10 * scale)]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12 * scale) {
            PevDashboardGrid(columns: columns, spacing: 10 * scale) {
                ForEach(snapshot.groups) { group in
                    BmsGroupIndexCell(
                        group: group,
                        isSelected: group.index == selectedGroup?.index,
                        scale: scale,
                        action: { selectedGroupIndex = group.index }
                    )
                }
            }

            if let selectedGroup {
                VStack(alignment: .leading, spacing: 15 * scale) {
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
                        .foregroundStyle(PevColors.orange)

                    PevDashboardGrid(
                        columns: [
                            GridItem(.adaptive(minimum: 140), spacing: 14 * scale),
                        ],
                        spacing: 14 * scale
                    ) {
                        PevDashboardMetricTile(
                            label: "temp",
                            value: temperatureText(selectedGroup.temperature),
                            unit: "°C",
                            detail: "",
                            accent: PevColors.green,
                            scale: scale,
                            detailColor: PevColors.green
                        )
                        PevDashboardMetricTile(
                            label: "IR est.",
                            value: selectedGroup.resistance.map { String($0.value) } ?? "--",
                            unit: "mΩ",
                            detail: "",
                            accent: PevColors.green,
                            scale: scale,
                            detailColor: PevColors.green
                        )
                    }

                    PevDashboardWideCard(
                        title: nil,
                        value: "trend: \(snapshot.detailGroupTrend(for: selectedGroup.index))",
                        detail: snapshot.detailGroupTrendDetail(for: selectedGroup.index),
                        accent: PevColors.yellow,
                        scale: scale
                    )
                }
                .padding(.horizontal, 18 * scale)
                .padding(.vertical, 20 * scale)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(PevDashboardCardBackground(cornerRadius: 34 * scale, stroke: PevColors.yellow, lineWidth: 1.2))
            }
        }
    }
}
