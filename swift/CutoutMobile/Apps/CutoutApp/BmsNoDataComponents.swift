import SwiftUI

struct BmsNoDataMetric: View {
    let value: String
    let unit: String
    let label: String
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 6 * scale) {
            HStack(alignment: .firstTextBaseline, spacing: 3 * scale) {
                Text(value)
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
        .accessibilityValue([value, unit].filter { !$0.isEmpty }.joined(separator: " "))
    }
}
