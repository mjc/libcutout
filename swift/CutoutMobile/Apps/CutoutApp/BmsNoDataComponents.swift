import SwiftUI

struct BmsNoDataLabel: View {
    let text: String
    let scale: CGFloat

    var body: some View {
        Text(text)
            .font(.system(size: 12 * scale, weight: .bold))
            .foregroundStyle(MockupColors.muted)
    }
}

struct BmsNoDataMetric: View {
    let value: String
    let unit: String
    let label: String
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 6 * scale) {
            HStack(alignment: .firstTextBaseline, spacing: 3 * scale) {
                Text(value)
                    .font(.system(size: 24 * scale, weight: .black))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                Text(unit)
                    .font(.system(size: 18 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
            }
            Text(label)
                .font(.system(size: 12 * scale, weight: .medium))
                .foregroundStyle(MockupColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct BmsNoDataDashedCard: View {
    let cornerRadius: CGFloat
    let scale: CGFloat

    var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(MockupColors.cardFill)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(style: StrokeStyle(lineWidth: 1.2, dash: [5 * scale, 5 * scale]))
                    .foregroundStyle(MockupColors.cardStroke)
            )
    }
}
