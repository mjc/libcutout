import Charts
import CutoutMobile
import SwiftUI

struct BmsPackLayout: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    let snapshot: BmsSnapshot
    let showGroupDetail: (Int) -> Void
    @State private var showsAllCells = false
    @State private var readingQuery = ""
    @FocusState private var searchFocused: Bool

    private var readings: [BmsGroupSnapshot] {
        snapshot.groups.filter { $0.voltage != nil }
    }

    private var lowest: BmsGroupSnapshot? {
        snapshot.groups.first { $0.index == snapshot.lowestGroupIndex }
    }

    private var highest: BmsGroupSnapshot? {
        snapshot.groups.first { $0.index == snapshot.highestGroupIndex }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            if let lowest, let highest,
                let lowVoltage = lowest.voltage, let highVoltage = highest.voltage,
                let spread = snapshot.cellDelta
            {
                voltageSummary(
                    lowest: lowest, highest: highest, low: Double(lowVoltage.value), high: Double(highVoltage.value),
                    spread: spread)
            }

            if snapshot.energyPercent != nil || snapshot.voltage != nil || snapshot.current != nil {
                packMeasurements
            }

            ForEach(snapshot.groups.filter { $0.alertLevel == .warning || $0.alertLevel == .critical }) { group in
                Button {
                    showGroupDetail(group.index)
                } label: {
                    Label(
                        localizedAppText("bms.pack.reading_warning", group.index),
                        systemImage: "exclamationmark.triangle"
                    )
                    .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                }
                .foregroundStyle(group.alertLevel == .critical ? PevColors.red : PevColors.orange)
                .buttonStyle(.plain)
            }

            if !snapshot.groups.isEmpty {
                if snapshot.groups.count > 24 {
                    DisclosureGroup(isExpanded: $showsAllCells) {
                        TextField(localizedAppText("bms.pack.find_reading"), text: $readingQuery)
                            .textFieldStyle(.roundedBorder)
                            .focused($searchFocused)
                            .onSubmit { searchFocused = false }
                            .accessibilityIdentifier("bms.pack.search")
                            .padding(.top, 12)
                        cellGrid.padding(.top, 16)
                    } label: {
                        Text(localizedAppText("bms.pack.all_cells"))
                            .font(.headline)
                            .padding(.vertical, 8)
                    }
                    .tint(.primary)
                } else {
                    cellGrid
                }
            }

            if !snapshot.temperatureReadings.isEmpty {
                VStack(alignment: .leading, spacing: 14) {
                    Text(localizedAppText("bms.pack.temperatures"))
                        .font(.headline)
                        .accessibilityHeading(.h2)
                    PevDashboardGrid(adaptiveMinimumColumnWidth: 90, columnSpacing: 12, spacing: 16) {
                        // Sensor position is its identity; temperatures may repeat or change.
                        ForEach(Array(zip(1..., snapshot.temperatureReadings)), id: \.0) { sensor, temperature in
                            BmsReadingMetric(
                                title: localizedAppText("bms.pack.sensor", sensor),
                                value: RideUnits.temperatureText(millicelsius: temperature.value, fractionDigits: 1),
                                unit: RideUnits.temperatureUnit
                            )
                        }
                    }
                }
                .bmsPanel()
            } else if let temperature = snapshot.highestTemperature {
                BmsReadingMetric(
                    title: localizedAppText("bms.pack.temperature"),
                    value: RideUnits.temperatureText(millicelsius: temperature.value, fractionDigits: 1),
                    unit: RideUnits.temperatureUnit
                )
                .bmsPanel()
            }

            ForEach(snapshot.faults) { fault in
                Label(fault.label, systemImage: "exclamationmark.triangle")
                    .font(.body)
                    .foregroundStyle(fault.level == .critical ? PevColors.red : PevColors.orange)
                    .bmsPanel()
            }
        }
    }

    private func voltageSummary(
        lowest: BmsGroupSnapshot, highest: BmsGroupSnapshot, low: Double, high: Double, spread: VoltageDelta
    ) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            VStack(alignment: .leading, spacing: 6) {
                Text(localizedAppText("bms.pack.difference"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(spread.value, format: .number.precision(.fractionLength(0)))
                        .font(.largeTitle.weight(.semibold))
                    Text("mV")
                        .font(.title3)
                        .foregroundStyle(.secondary)
                }
                .monospacedDigit()
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("bms.pack.difference")
            }

            Chart(readings) { group in
                if let voltage = group.voltage {
                    BarMark(
                        x: .value(localizedAppText("bms.pack.reading_axis"), group.index),
                        yStart: .value("V", (low - 5) / 1_000),
                        yEnd: .value("V", Double(voltage.value) / 1_000)
                    )
                    .foregroundStyle(Color.accentColor.opacity(0.75))
                    .cornerRadius(2)
                }
            }
            .chartYScale(domain: (low - 5) / 1_000...(high + 5) / 1_000)
            .chartYAxis {
                AxisMarks(position: .leading, values: .automatic(desiredCount: 3)) { value in
                    AxisGridLine().foregroundStyle(.secondary.opacity(0.15))
                    AxisValueLabel {
                        if let volts = value.as(Double.self) {
                            Text(volts, format: .number.precision(.fractionLength(3)))
                        }
                    }
                }
            }
            .chartXAxis {
                AxisMarks(values: .automatic(desiredCount: 4))
            }
            .frame(height: 110)
            .accessibilityLabel(localizedAppText("bms.pack.chart"))

            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 16) {
                    extreme(lowest, title: "bms.pack.lowest")
                    extreme(highest, title: "bms.pack.highest")
                }
            } else {
                HStack(spacing: 16) {
                    extreme(lowest, title: "bms.pack.lowest")
                    extreme(highest, title: "bms.pack.highest")
                }
            }
            Text(localizedAppText("bms.pack.readings", snapshot.observedGroupCount ?? readings.count))
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .bmsPanel()
    }

    private func extreme(_ group: BmsGroupSnapshot, title: String) -> some View {
        Button {
            showGroupDetail(group.index)
        } label: {
            HStack(alignment: .center, spacing: 8) {
                BmsReadingMetric(title: localizedAppText(title), value: group.voltageMetricValue.displayText, unit: "V")
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(title)
        .accessibilityHint(localizedAppText("bms.pack.open_reading", group.index))
    }

    private var filteredGroups: [BmsGroupSnapshot] {
        snapshot.groups.filter { group in
            readingQuery.isEmpty
                || String(group.index).contains(readingQuery)
                || group.label?.localizedCaseInsensitiveContains(readingQuery) == true
        }
    }

    private var packGroups: [(number: Int, groups: [BmsGroupSnapshot])] {
        Dictionary(
            grouping: filteredGroups.compactMap { group in
                group.packNumber.map { ($0, group) }
            }, by: \.0
        )
        .map { number, entries in (number, entries.map(\.1)) }
        .sorted { $0.number < $1.number }
    }

    @ViewBuilder
    private var cellGrid: some View {
        if !packGroups.isEmpty, packGroups.flatMap(\.groups).count == filteredGroups.count {
            VStack(alignment: .leading, spacing: 18) {
                ForEach(packGroups, id: \.number) { pack in
                    Text(localizedAppText("bms.pack.pack_number", pack.number))
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.secondary)
                    groupGrid(pack.groups)
                }
            }
        } else {
            groupGrid(filteredGroups)
        }
    }

    private func groupGrid(_ groups: [BmsGroupSnapshot]) -> some View {
        PevDashboardGrid(
            adaptiveMinimumColumnWidth: 80, accessibilityMinimumColumnWidth: 240, columnSpacing: 8, spacing: 8
        ) {
            ForEach(groups) { group in
                BmsStripCell(group: group, isHighlighted: false) {
                    showGroupDetail(group.index)
                }
            }
        }
    }

    private var packMeasurements: some View {
        PevDashboardGrid(adaptiveMinimumColumnWidth: 100, columnSpacing: 16, spacing: 16) {
            if snapshot.energyPercent != nil {
                BmsReadingMetric(
                    title: localizedAppText("bms.pack.charge"), value: snapshot.energyMetricValue.displayText, unit: ""
                )
                .accessibilityIdentifier("bms.pack.charge")
            }
            if snapshot.voltage != nil {
                BmsReadingMetric(
                    title: localizedAppText("bms.pack.voltage"), value: snapshot.voltageMetricValue.displayText,
                    unit: "V"
                )
                .accessibilityIdentifier("bms.pack.voltage")
            }
            if let current = snapshot.current {
                BmsReadingMetric(
                    title: localizedAppText("bms.pack.current"),
                    value: RideUnits.decimalString(Double(current.value) / 1_000, fractionDigits: 1), unit: "A")
            }
        }
        .bmsPanel()
    }
}

struct BmsReadingMetric: View {
    let title: String
    let value: String
    let unit: String

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title).font(.subheadline).foregroundStyle(.secondary)
            Text(unit.isEmpty ? value : "\(value) \(unit)")
                .font(.title3.weight(.semibold))
                .monospacedDigit()
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(unit.isEmpty ? value : "\(value) \(unit)")
    }
}

extension View {
    func bmsPanel() -> some View {
        padding(18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 20))
    }
}
