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
                            label: "average group",
                            value: groupVoltageText(averageGroupVoltage),
                            unit: "V",
                            detail: "",
                            detailColor: PevColors.primaryText
                        )
                    }
                    if let lowestGroupVoltage {
                        PevDashboardMetricTile(
                            label: "lowest group",
                            value: groupVoltageText(lowestGroupVoltage),
                            unit: "V",
                            detail: snapshot.lowestGroupLabel ?? "",
                            detailColor: PevColors.primaryText
                        )
                    }
                }
            }

            if hasTemperatureEvidence {
                PevDashboardMetricTile(
                    label: "highest temp",
                    value: temperatureText(snapshot.highestTemperature),
                    unit: "°C",
                    detail: snapshot.highestTemperatureLabel ?? "",
                    detailColor: PevColors.primaryText
                )
            }

            if let balancingSummary = snapshot.balancingSummary {
                PevDashboardWideCard(
                    title: "balancing",
                    value: balancingSummary,
                    detail: snapshot.balancingDetail ?? ""
                )
            }

            if let faultSummary = snapshot.faultSummary {
                PevDashboardWideCard(
                    title: "fault state",
                    value: faultSummary,
                    detail: snapshot.faultDetail ?? "",
                    stroke: PevColors.red
                )
            }
        }
    }

    @ViewBuilder
    private var summaryCard: some View {
        if let energyProgress = snapshot.energyProgress {
            PevDashboardHeroCard(
                eyebrow: "usable energy",
                value: percentText(snapshot.energyPercent),
                unit: snapshot.availability.displayText,
                detail: snapshot.topology.layoutLabel,
                progress: energyProgress,
                accent: PevColors.primaryText
            )
        } else {
            PevDashboardWideCard(
                title: "pack telemetry",
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
