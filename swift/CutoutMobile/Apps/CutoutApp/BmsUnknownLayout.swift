import CutoutMobile
import SwiftUI

struct BmsUnknownLayout: View {
    let content: PevBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16 * scale) {
            PevDashboardWideCard(
                title: "do not pretend certainty",
                value: snapshot.faultSummary ?? "--",
                detail: snapshot.faultDetail ?? "",
                accent: PevColors.orange,
                stroke: PevColors.orange,
                scale: scale
            )

            PevDashboardGrid(
                columns: [
                    GridItem(.flexible(), spacing: 14 * scale),
                    GridItem(.flexible(), spacing: 14 * scale),
                ],
                spacing: 14 * scale
            ) {
                PevDashboardMetricTile(
                    label: "reported voltage",
                    value: voltageText(snapshot.voltage),
                    unit: "V",
                    detail: snapshot.unknownTopologyVoltageDetail,
                    accent: PevColors.yellow,
                    scale: scale,
                    detailColor: PevColors.yellow
                )
                PevDashboardMetricTile(
                    label: "cell count",
                    value: snapshot.unknownTopologyCellCountValue,
                    unit: "",
                    detail: snapshot.unknownTopologyCellCountDetail,
                    accent: PevColors.orange,
                    scale: scale,
                    detailColor: PevColors.orange
                )
                PevDashboardMetricTile(
                    label: "temps",
                    value: snapshot.unknownTopologyTemperatureValue,
                    unit: "sensors",
                    detail: snapshot.unknownTopologyTemperatureDetail,
                    accent: PevColors.green,
                    scale: scale,
                    detailColor: PevColors.green
                )
                PevDashboardMetricTile(
                    label: "fault bits",
                    value: snapshot.faults.first?.code ?? "--",
                    unit: "",
                    detail: snapshot.faults.first?.label ?? "",
                    accent: PevColors.orange,
                    scale: scale,
                    detailColor: PevColors.orange
                )
            }

            VStack(alignment: .leading, spacing: 10 * scale) {
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
                    .padding(.horizontal, 18 * scale)
                    .frame(minHeight: 44 * scale)
                    .background(Capsule().fill(PevColors.muted.opacity(0.33)))
            }
            .padding(.horizontal, 18 * scale)
            .padding(.vertical, 18 * scale)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("bms.unknown.capture-flow")
        }
    }
}
