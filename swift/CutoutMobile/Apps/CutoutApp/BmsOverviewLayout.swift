import CutoutMobile
import SwiftUI

struct BmsOverviewLayout: View {
    let content: PevBmsContent

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            summaryCard

            if overviewPresentation.averageGroupVoltage != nil || overviewPresentation.lowestGroupVoltage != nil {
                PevDashboardGrid(
                    adaptiveMinimumColumnWidth: 140,
                    columnSpacing: 14,
                    spacing: 14
                ) {
                    if let averageGroupVoltage = overviewPresentation.averageGroupVoltage {
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.overview.average_group"),
                            metricValue: bmsVoltageMetricValue(averageGroupVoltage),
                            unit: RideUnits.voltageUnit,
                            detail: ""
                        )
                    }
                    if let lowestGroupVoltage = overviewPresentation.lowestGroupVoltage {
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.overview.lowest_group"),
                            metricValue: bmsVoltageMetricValue(lowestGroupVoltage),
                            unit: RideUnits.voltageUnit,
                            detail: overviewPresentation.lowestGroupLabel
                        )
                    }
                }
            }

            if let highestTemperature = overviewPresentation.highestTemperature {
                PevDashboardMetricTile(
                    label: localizedAppText("bms.overview.highest_temperature"),
                    metricValue: bmsTemperatureMetricValue(highestTemperature),
                    unit: RideUnits.temperatureUnit,
                    detail: overviewPresentation.highestTemperatureLabel
                )
            }

            if snapshot.balancingSummary != nil {
                PevDashboardWideCard(
                    title: localizedAppText("bms.overview.balancing"),
                    metricValue: snapshot.balancingMetricValue,
                    detail: snapshot.balancingMetricDetail
                )
            }

            if snapshot.faultSummary != nil {
                PevDashboardWideCard(
                    title: localizedAppText("bms.overview.fault_state"),
                    metricValue: snapshot.faultMetricValue,
                    detail: snapshot.faultMetricDetail,
                    stroke: PevColors.red
                )
            }
        }
    }

    @ViewBuilder
    private var summaryCard: some View {
        if let energyProgress = snapshot.energyProgress,
           snapshot.energyPercent != nil {
            PevDashboardHeroCard(
                eyebrow: localizedAppText("bms.overview.usable_energy"),
                metricValue: snapshot.energyMetricValue,
                unit: snapshot.availability.displayText,
                detail: snapshot.topology.layoutLabel,
                progress: energyProgress
            )
        } else {
            PevDashboardWideCard(
                title: localizedAppText("bms.overview.pack_telemetry"),
                metricValue: snapshot.voltageMetricValue,
                detail: snapshot.topology.layoutLabel
            )
        }
    }

    private var overviewPresentation: BmsOverviewPresentation {
        snapshot.overviewPresentation
    }
}
