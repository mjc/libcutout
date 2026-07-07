import CutoutMobile
import SwiftUI

struct BmsUnknownLayout: View {
    let content: MockupBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16 * scale) {
            PevDashboardWideCard(
                title: "do not pretend certainty",
                value: snapshot.faultSummary ?? "--",
                detail: snapshot.faultDetail ?? "",
                accent: MockupColors.orange,
                stroke: MockupColors.orange,
                scale: scale
            )

            HStack(spacing: 14 * scale) {
                PevDashboardMetricTile(
                    label: "reported voltage",
                    value: voltageText(snapshot.voltage),
                    unit: "V",
                    detail: snapshot.unknownTopologyVoltageDetail,
                    accent: MockupColors.yellow,
                    scale: scale,
                    detailColor: MockupColors.yellow
                )
                PevDashboardMetricTile(
                    label: "cell count",
                    value: snapshot.unknownTopologyCellCountValue,
                    unit: "",
                    detail: snapshot.unknownTopologyCellCountDetail,
                    accent: MockupColors.orange,
                    scale: scale,
                    detailColor: MockupColors.orange
                )
            }

            HStack(spacing: 14 * scale) {
                PevDashboardMetricTile(
                    label: "temps",
                    value: snapshot.unknownTopologyTemperatureValue,
                    unit: "sensors",
                    detail: snapshot.unknownTopologyTemperatureDetail,
                    accent: MockupColors.green,
                    scale: scale,
                    detailColor: MockupColors.green
                )
                PevDashboardMetricTile(
                    label: "fault bits",
                    value: snapshot.faults.first?.code ?? "--",
                    unit: "",
                    detail: snapshot.faults.first?.label ?? "",
                    accent: MockupColors.orange,
                    scale: scale,
                    detailColor: MockupColors.orange
                )
            }

            VStack(alignment: .leading, spacing: 10 * scale) {
                Text("next capture flow")
                    .font(.system(size: 15 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                Text(snapshot.captureActionTitle ?? "--")
                    .font(.system(size: 25 * scale, weight: .black))
                    .lineLimit(1)
                    .minimumScaleFactor(0.84)
                Text(snapshot.unknownTopologyCaptureDetail)
                    .font(.system(size: 13 * scale, weight: .semibold))
                    .foregroundStyle(MockupColors.muted)
                Text(snapshot.captureActionState ?? "")
                    .font(.system(size: 15 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.primaryText.opacity(0.82))
                    .padding(.horizontal, 18 * scale)
                    .frame(height: 34 * scale)
                    .background(Capsule().fill(MockupColors.muted.opacity(0.33)))
            }
            .padding(.horizontal, 18 * scale)
            .padding(.vertical, 18 * scale)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
        }
    }
}
