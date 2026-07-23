import CutoutMobile
import SwiftUI

struct BmsUnknownLayout: View {
    let content: PevBmsContent

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            PevDashboardWideCard(
                title: localizedAppText("bms.unknown.title"),
                metricValue: faultMetricValue,
                detail: snapshot.faultDetail ?? "",
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
                    metricValue: bmsVoltageMetricValue(snapshot.voltage),
                    unit: "V",
                    detail: snapshot.unknownTopologyVoltageDetail,
                    detailColor: PevColors.primaryText
                )
                PevDashboardMetricTile(
                    label: localizedAppText("bms.unknown.cell_count"),
                    metricValue: .available(
                        display: snapshot.unknownTopologyCellCountValue,
                        accessibility: snapshot.unknownTopologyCellCountValue
                    ),
                    unit: "",
                    detail: snapshot.unknownTopologyCellCountDetail,
                    detailColor: PevColors.primaryText
                )
                PevDashboardMetricTile(
                    label: localizedAppText("bms.unknown.temperatures"),
                    metricValue: snapshot.unknownTopologyTemperatureSensorCount.map {
                        let value = RideUnits.decimalString(Double($0), fractionDigits: 0)
                        return .available(display: value, accessibility: value)
                    } ?? .unavailable,
                    unit: localizedAppText("bms.unknown.temperature_sensors"),
                    detail: snapshot.unknownTopologyTemperatureDetail,
                    detailColor: PevColors.primaryText
                )
                PevDashboardMetricTile(
                    label: localizedAppText("bms.unknown.fault_bits"),
                    metricValue: snapshot.faults.first.map {
                        .available(display: $0.code, accessibility: $0.code)
                    } ?? .unavailable,
                    unit: "",
                    detail: snapshot.faults.first?.label ?? "",
                    detailColor: PevColors.primaryText
                )
            }

            VStack(alignment: .leading, spacing: 10) {
                Text(localizedAppText("bms.unknown.next_capture_flow"))
                    .font(.headline)
                    .foregroundStyle(PevColors.muted)
                    .accessibilityHeading(.h2)
                Text(snapshot.captureActionTitle ?? localizedAppText("bms.unknown.capture_unavailable"))
                    .font(.title2.weight(.black))
                Text(snapshot.unknownTopologyCaptureDetail)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
                Text(snapshot.captureActionState ?? "")
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

    private var faultMetricValue: PevDashboardMetricValue {
        guard let faultSummary = snapshot.faultSummary else { return .unavailable }
        return .available(display: faultSummary, accessibility: faultSummary)
    }
}
