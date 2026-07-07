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
                    .font(.system(size: 15 * scale, weight: .bold))
                    .foregroundStyle(PevColors.muted)
                Text(snapshot.captureActionTitle ?? "--")
                    .font(.system(size: 25 * scale, weight: .black))
                    .lineLimit(1)
                    .minimumScaleFactor(0.84)
                Text(snapshot.unknownTopologyCaptureDetail)
                    .font(.system(size: 13 * scale, weight: .semibold))
                    .foregroundStyle(PevColors.muted)
                Text(snapshot.captureActionState ?? "")
                    .font(.system(size: 15 * scale, weight: .bold))
                    .foregroundStyle(PevColors.primaryText.opacity(0.82))
                    .padding(.horizontal, 18 * scale)
                    .frame(height: 34 * scale)
                    .background(Capsule().fill(PevColors.muted.opacity(0.33)))
            }
            .padding(.horizontal, 18 * scale)
            .padding(.vertical, 18 * scale)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
        }
    }
}
