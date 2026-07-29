import CutoutMobile
import SwiftUI

struct BmsUnknownLayout: View {
    let content: PevBmsContent

    private var snapshot: BmsSnapshot { content.snapshot }
    private var capturePresentation: BmsUnknownTopologyCapturePresentation {
        snapshot.unknownTopologyCapturePresentation
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            PevDashboardWideCard(
                title: localizedAppText("bms.unknown.title"),
                metricValue: snapshot.unknownTopologySummaryMetricValue,
                detail: snapshot.unknownTopologySummaryDetail,
                stroke: PevColors.orange
            )

            PevDashboardGrid(
                columns: [
                    GridItem(.flexible(), spacing: 14),
                    GridItem(.flexible(), spacing: 14),
                ],
                spacing: 14
            ) {
                PevDashboardMetricTile(
                    label: localizedAppText("bms.unknown.reported_voltage"),
                    metricValue: snapshot.unknownTopologyVoltageMetricValue,
                    unit: RideUnits.voltageUnit,
                    detail: snapshot.unknownTopologyVoltageDetail
                )
                PevDashboardMetricTile(
                    label: localizedAppText("bms.unknown.cell_count"),
                    metricValue: snapshot.unknownTopologyCellCountMetricValue,
                    unit: "",
                    detail: snapshot.unknownTopologyCellCountDetail
                )
                PevDashboardMetricTile(
                    label: localizedAppText("bms.unknown.temperatures"),
                    metricValue: snapshot.unknownTopologyTemperatureSensorCountMetricValue,
                    unit: localizedAppText("bms.unknown.temperature_sensors"),
                    detail: snapshot.unknownTopologyTemperatureDetail
                )
                PevDashboardMetricTile(
                    label: localizedAppText("bms.unknown.fault_bits"),
                    metricValue: snapshot.unknownTopologyFaultMetricValue,
                    unit: "",
                    detail: snapshot.unknownTopologyFaultDetail
                )
            }

            VStack(alignment: .leading, spacing: 10) {
                Text(localizedAppText("bms.unknown.next_capture_flow"))
                    .font(.headline)
                    .foregroundStyle(PevColors.muted)
                    .accessibilityHeading(.h2)
                Text(capturePresentation.title)
                    .font(.title2.weight(.black))
                Text(capturePresentation.detail)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
                Text(capturePresentation.state)
                    .font(.headline)
                    .foregroundStyle(PevColors.primaryText.opacity(0.82))
                    .padding(.horizontal, 18)
                    .frame(minHeight: 44)
                    .background(Capsule().fill(PevColors.muted.opacity(0.33)))
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 24))
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("bms.unknown.capture-flow")
        }
    }

}
