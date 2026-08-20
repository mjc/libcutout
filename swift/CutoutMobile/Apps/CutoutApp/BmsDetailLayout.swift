import CutoutMobile
import SwiftUI

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

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Button(action: showCellMap) {
                Label(localizedAppText("bms.detail.back_to_cell_map"), systemImage: "chevron.left")
                    .font(.body.weight(.semibold))
            }
            .buttonStyle(.bordered)
            .frame(minHeight: 44)
            .accessibilityIdentifier("bms.detail.back")

            PevDashboardGrid(
                adaptiveMinimumColumnWidth: 52,
                accessibilityMinimumColumnWidth: 240,
                columnSpacing: 10,
                spacing: 10
            ) {
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
                    Text(selectedGroup.accessibilityLabel)
                        .font(.headline)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityHeading(.h2)
                        .accessibilityIdentifier("bms.detail.selected-group")
                    Text(selectedGroup.voltageMetricValue.displayText)
                        .font(.largeTitle.weight(.black))
                        .monospacedDigit()
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityLabel(localizedAppText("bms.detail.voltage"))
                        .accessibilityValue(selectedGroup.accessibilityValue)
                        .accessibilityIdentifier("bms.detail.voltage")
                    Text(snapshot.detailGroupStatus(for: selectedGroup.index))
                        .font(.headline.weight(.black))
                        .foregroundStyle(PevColors.primaryText)

                    PevDashboardGrid(
                        adaptiveMinimumColumnWidth: 140,
                        columnSpacing: 14,
                        spacing: 14
                    ) {
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.detail.temperature"),
                            metricValue: selectedGroup.temperatureMetricValue,
                            unit: RideUnits.temperatureUnit,
                            detail: ""
                        )
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.detail.resistance"),
                            metricValue: selectedGroup.resistanceMetricValue,
                            unit: "mΩ",
                            detail: ""
                        )
                    }

                    PevDashboardWideCard(
                        title: nil,
                        metricValue: snapshot.detailGroupTrendMetricValue(for: selectedGroup.index),
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
