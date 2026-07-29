import CutoutMobile
import SwiftUI

struct BmsOverviewLayout: View {
    let content: PevBmsContent

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            summaryCard

            if overviewPresentation.averageGroupMetricValue != nil || overviewPresentation.lowestGroupMetricValue != nil {
                PevDashboardGrid(
                    adaptiveMinimumColumnWidth: 140,
                    columnSpacing: 14,
                    spacing: 14
                ) {
                    if let averageGroupMetricValue = overviewPresentation.averageGroupMetricValue {
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.overview.average_group"),
                            metricValue: averageGroupMetricValue,
                            unit: RideUnits.voltageUnit,
                            detail: ""
                        )
                    }
                    if let lowestGroupMetricValue = overviewPresentation.lowestGroupMetricValue {
                        PevDashboardMetricTile(
                            label: localizedAppText("bms.overview.lowest_group"),
                            metricValue: lowestGroupMetricValue,
                            unit: RideUnits.voltageUnit,
                            detail: overviewPresentation.lowestGroupLabel
                        )
                    }
                }
            }

            if let highestTemperatureMetricValue = overviewPresentation.highestTemperatureMetricValue {
                PevDashboardMetricTile(
                    label: localizedAppText("bms.overview.highest_temperature"),
                    metricValue: highestTemperatureMetricValue,
                    unit: RideUnits.temperatureUnit,
                    detail: overviewPresentation.highestTemperatureLabel
                )
            }

            if overviewPresentation.shouldShowBalancingSummary {
                PevDashboardWideCard(
                    title: localizedAppText("bms.overview.balancing"),
                    metricValue: snapshot.balancingMetricValue,
                    detail: snapshot.balancingMetricDetail
                )
            }

            if overviewPresentation.shouldShowFaultSummary {
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
        if overviewPresentation.shouldShowEnergyHero,
           let energyProgress = snapshot.energyProgress {
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
