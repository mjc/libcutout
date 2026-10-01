import Charts
import CutoutMobile
import SwiftUI

struct BmsDetailLayout: View {
    let content: PevBmsContent
    let selectedGroupIndex: Int?
    let showGroupDetail: (Int) -> Void
    let showCellMap: () -> Void

    private var snapshot: BmsSnapshot { content.snapshot }
    private var selectedGroup: BmsGroupSnapshot? {
        snapshot.groups.first { $0.index == (selectedGroupIndex ?? content.selectedGroupIndex) }
            ?? snapshot.groups.first
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Button(action: showCellMap) {
                Label(localizedAppText("bms.pack.back"), systemImage: "chevron.left")
                    .font(.body.weight(.semibold))
                    .frame(minHeight: 44)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("bms.detail.back")

            if let group = selectedGroup {
                VStack(alignment: .leading, spacing: 16) {
                    Text(group.label ?? localizedAppText("bms.pack.reading", group.index))
                        .font(.headline)
                        .foregroundStyle(.secondary)
                        .accessibilityHeading(.h2)
                        .accessibilityIdentifier("bms.detail.selected-group")
                    Text("\(group.voltageMetricValue.displayText) V")
                        .font(.largeTitle.weight(.semibold))
                        .monospacedDigit()
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityLabel(localizedAppText("bms.detail.voltage"))
                        .accessibilityValue(group.accessibilityValue)
                        .accessibilityIdentifier("bms.detail.voltage")

                    if group.recentVoltages.count > 1 {
                        recentHistory(group)
                    }

                    if group.alertLevel == .critical || group.alertLevel == .warning {
                        Label(localizedAppText("bms.pack.cell_warning"), systemImage: "exclamationmark.triangle")
                            .foregroundStyle(group.alertLevel == .critical ? PevColors.red : PevColors.orange)
                    }
                    if group.isBalancing == true {
                        Label(localizedAppText("bms.pack.balancing"), systemImage: "equal")
                    }
                    if group.temperature != nil {
                        BmsReadingMetric(
                            title: localizedAppText("bms.pack.temperature"),
                            value: group.temperatureMetricValue.displayText, unit: RideUnits.temperatureUnit)
                    }
                    if group.resistance != nil {
                        BmsReadingMetric(
                            title: localizedAppText("bms.detail.resistance"),
                            value: group.resistanceMetricValue.displayText, unit: "mΩ")
                    }
                }
                .bmsPanel()

                HStack {
                    if let previous = snapshot.groups.last(where: { $0.index < group.index }) {
                        Button {
                            showGroupDetail(previous.index)
                        } label: {
                            Label(localizedAppText("bms.pack.previous"), systemImage: "chevron.left")
                                .frame(minHeight: 44)
                        }
                        .accessibilityIdentifier("bms.detail.previous")
                    }
                    Spacer()
                    if let next = snapshot.groups.first(where: { $0.index > group.index }) {
                        Button {
                            showGroupDetail(next.index)
                        } label: {
                            Label(localizedAppText("bms.pack.next"), systemImage: "chevron.right")
                                .frame(minHeight: 44)
                        }
                        .accessibilityIdentifier("bms.detail.next")
                    }
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func recentHistory(_ group: BmsGroupSnapshot) -> some View {
        let values = group.recentVoltages.map { Double($0.value) / 1_000 }
        let low = values.min() ?? 0
        let high = values.max() ?? low
        return VStack(alignment: .leading, spacing: 14) {
            Text(localizedAppText("bms.pack.recent_history"))
                .font(.headline)
                .accessibilityHeading(.h3)
            Chart(Array(values.enumerated()), id: \.offset) { sample in
                LineMark(
                    x: .value(localizedAppText("bms.pack.sample"), sample.offset + 1),
                    y: .value("V", sample.element)
                )
                .foregroundStyle(Color.accentColor)
                PointMark(
                    x: .value(localizedAppText("bms.pack.sample"), sample.offset + 1),
                    y: .value("V", sample.element)
                )
                .foregroundStyle(Color.accentColor)
            }
            .chartYScale(domain: (low - 0.005)...(high + 0.005))
            .chartXAxis(.hidden)
            .frame(height: 120)
            .accessibilityLabel(localizedAppText("bms.pack.recent_history_accessibility", group.recentVoltages.count))

            PevDashboardGrid(adaptiveMinimumColumnWidth: 120, columnSpacing: 16, spacing: 16) {
                BmsReadingMetric(
                    title: localizedAppText("bms.pack.recent_range"),
                    value:
                        "\(RideUnits.decimalString(low, fractionDigits: 3))–\(RideUnits.decimalString(high, fractionDigits: 3))",
                    unit: "V"
                )
                if let latest = group.latestVoltage, latest != group.voltage {
                    BmsReadingMetric(
                        title: localizedAppText("bms.pack.latest_raw"),
                        value: RideUnits.decimalString(Double(latest.value) / 1_000, fractionDigits: 3),
                        unit: "V"
                    )
                }
            }
            Text(localizedAppText("bms.pack.stabilized_explanation"))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}
