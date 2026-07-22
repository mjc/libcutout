import CutoutMobile
import SwiftUI

struct BmsNoDataMetric: View {
    let metricValue: PevDashboardMetricValue
    let unit: String
    let label: String

    var accessibilityValueText: String {
        metricValue.accessibilityValue(unit: unit, detail: "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 3) {
                Text(metricValue.displayText)
                    .font(.title3.weight(.black))
                    .monospacedDigit()
                Text(unit)
                    .font(.headline.weight(.bold))
                    .foregroundStyle(PevColors.muted)
            }
            Text(label)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityValue(accessibilityValueText)
    }
}
