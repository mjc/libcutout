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
                            accent: PevColors.green,
                            detailColor: PevColors.green
                        )
                    }
                    if let lowestGroupVoltage {
                        PevDashboardMetricTile(
                            label: "lowest group",
                            value: groupVoltageText(lowestGroupVoltage),
                            unit: "V",
                            detail: snapshot.lowestGroupLabel ?? "",
                            accent: PevColors.orange,
                            detailColor: PevColors.orange
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
                    accent: PevColors.green,
                    detailColor: PevColors.green
                )
            }

            if let balancingSummary = snapshot.balancingSummary {
                PevDashboardWideCard(
                    title: "balancing",
                    value: balancingSummary,
                    detail: snapshot.balancingDetail ?? "",
                    accent: PevColors.orange
                )
            }

            if let faultSummary = snapshot.faultSummary {
                PevDashboardWideCard(
                    title: "fault state",
                    value: faultSummary,
                    detail: snapshot.faultDetail ?? "",
                    accent: PevColors.orange,
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
                accent: PevColors.yellow
            )
        } else {
            PevDashboardWideCard(
                title: "pack telemetry",
                value: voltageText(snapshot.voltage) == "--" ? snapshot.availability.displayText : "\(voltageText(snapshot.voltage)) V",
                detail: snapshot.topology.layoutLabel,
                accent: PevColors.green
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
