import CutoutMobile
import SwiftUI

struct BmsOverviewLayout: View {
    let content: PevBmsContent

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            summaryCard

            if hasCellVoltageEvidence {
                PevDashboardGrid(
                    columns: [GridItem(.adaptive(minimum: 140), spacing: 14)],
                    spacing: 14
                ) {
                    if let averageGroupVoltage {
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.overview.average_group"),
                            metricValue: bmsVoltageMetricValue(averageGroupVoltage),
                            unit: RideUnits.voltageUnit,
                            detail: ""
                        )
                    }
                    if let lowestGroupVoltage {
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.overview.lowest_group"),
                            metricValue: bmsVoltageMetricValue(lowestGroupVoltage),
                            unit: RideUnits.voltageUnit,
                            detail: snapshot.lowestGroupLabel ?? ""
                        )
                    }
                }
            }

            if hasTemperatureEvidence {
                PevDashboardMetricTile(
                    label: localizedAppText("bms.overview.highest_temperature"),
                    metricValue: bmsTemperatureMetricValue(snapshot.highestTemperature),
                    unit: RideUnits.temperatureUnit,
                    detail: snapshot.highestTemperatureLabel ?? ""
                )
            }

            if let balancingSummary = snapshot.balancingSummary {
                PevDashboardWideCard(
                    title: localizedAppText("bms.overview.balancing"),
                    metricValue: .available(display: balancingSummary, accessibility: balancingSummary),
                    detail: snapshot.balancingDetail ?? ""
                )
            }

            if let faultSummary = snapshot.faultSummary {
                PevDashboardWideCard(
                    title: localizedAppText("bms.overview.fault_state"),
                    metricValue: .available(display: faultSummary, accessibility: faultSummary),
                    detail: snapshot.faultDetail ?? "",
                    stroke: PevColors.red
                )
            }
        }
    }

    @ViewBuilder
    private var summaryCard: some View {
        if let energyProgress = snapshot.energyProgress,
           let energyPercent = snapshot.energyPercent {
            let energyText = percentText(energyPercent)
            PevDashboardHeroCard(
                eyebrow: localizedAppText("bms.overview.usable_energy"),
                metricValue: .available(
                    display: energyText,
                    accessibility: energyText
                ),
                unit: snapshot.availability.displayText,
                detail: snapshot.topology.layoutLabel,
                progress: energyProgress
            )
        } else {
            PevDashboardWideCard(
                title: localizedAppText("bms.overview.pack_telemetry"),
                metricValue: telemetryMetricValue,
                detail: snapshot.topology.layoutLabel
            )
        }
    }

    private var lowestGroupVoltage: Voltage? {
        guard let lowestGroupIndex = snapshot.lowestGroupIndex else { return nil }
        return snapshot.groups.first { $0.index == lowestGroupIndex }?.voltage.flatMap(nonZeroVoltage)
    }

    private var averageGroupVoltage: Voltage? {
        guard hasCellVoltageEvidence else { return nil }
        return snapshot.averageGroupVoltage.flatMap(nonZeroVoltage)
    }

    private var telemetryMetricValue: PevDashboardMetricValue {
        guard let voltage = snapshot.voltage else { return .unavailable }
        let display = "\(voltageText(voltage)) V"
        return .available(display: display, accessibility: display)
    }

    private var hasCellVoltageEvidence: Bool {
        snapshot.groups.contains { group in
            group.voltage.map { $0.value > 0 } ?? false
        }
    }

    private func nonZeroVoltage(_ voltage: Voltage) -> Voltage? {
        voltage.value > 0 ? voltage : nil
    }

    private var hasTemperatureEvidence: Bool {
        snapshot.highestTemperature != nil && (!snapshot.temperatureReadings.isEmpty || snapshot.highestTemperatureLabel != nil)
    }
}
