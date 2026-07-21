import CutoutMobile
import SwiftUI

struct BmsUnknownLayout: View {
    let content: PevBmsContent

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            PevDashboardWideCard(
                title: "do not pretend certainty",
                value: snapshot.faultSummary ?? "--",
                detail: snapshot.faultDetail ?? "",
                accent: PevColors.orange,
                stroke: PevColors.orange,
                scale: 1
            )

            PevDashboardGrid(
                columns: [
                    GridItem(.flexible(), spacing: 14),
                    GridItem(.flexible(), spacing: 14),
                ],
                spacing: 14
            ) {
                PevDashboardMetricTile(
                    label: "reported voltage",
                    value: voltageText(snapshot.voltage),
                    unit: "V",
                    detail: snapshot.unknownTopologyVoltageDetail,
                    accent: PevColors.yellow,
                    detailColor: PevColors.yellow
                )
                PevDashboardMetricTile(
                    label: "cell count",
                    value: snapshot.unknownTopologyCellCountValue,
                    unit: "",
                    detail: snapshot.unknownTopologyCellCountDetail,
                    accent: PevColors.orange,
                    detailColor: PevColors.orange
                )
                PevDashboardMetricTile(
                    label: "temps",
                    value: snapshot.unknownTopologyTemperatureValue,
                    unit: "sensors",
                    detail: snapshot.unknownTopologyTemperatureDetail,
                    accent: PevColors.green,
                    detailColor: PevColors.green
                )
                PevDashboardMetricTile(
                    label: "fault bits",
                    value: snapshot.faults.first?.code ?? "--",
                    unit: "",
                    detail: snapshot.faults.first?.label ?? "",
                    accent: PevColors.orange,
                    detailColor: PevColors.orange
                )
            }

            VStack(alignment: .leading, spacing: 10) {
                Text("next capture flow")
                    .font(.headline)
                    .foregroundStyle(PevColors.muted)
                    .accessibilityHeading(.h2)
                Text(snapshot.captureActionTitle ?? "--")
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
}
