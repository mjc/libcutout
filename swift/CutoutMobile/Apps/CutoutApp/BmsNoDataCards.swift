import CutoutMobile
import SwiftUI

struct BmsNoDataHeader: View {
    let screen: MockupScreen
    let scale: CGFloat

    var body: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 6 * scale) {
                Text(screen.title)
                    .font(.system(size: 24 * scale, weight: .bold))
                Text(screen.subtitle)
                    .font(.system(size: 11 * scale, weight: .medium))
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                    .foregroundStyle(MockupColors.muted)
            }

            Spacer(minLength: 12 * scale)

            HStack(spacing: 10 * scale) {
                Circle()
                    .fill(MockupColors.yellow)
                    .frame(width: 10 * scale, height: 10 * scale)
                Text(screen.secondaryValue)
                    .font(.system(size: 11 * scale, weight: .medium))
                    .foregroundStyle(MockupColors.primaryText.opacity(0.92))
            }
            .padding(.horizontal, 12 * scale)
            .frame(height: 30 * scale)
            .background(
                Capsule(style: .continuous)
                    .fill(MockupColors.cardFill)
                    .overlay(
                        Capsule(style: .continuous)
                            .stroke(MockupColors.cardStroke, lineWidth: 1)
                    )
            )
        }
    }
}

struct BmsNoDataWarningCard: View {
    let snapshot: BmsSnapshot
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            HStack(spacing: 14 * scale) {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 28 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.yellow)
                    .frame(width: 28 * scale, height: 28 * scale)

                Text(snapshot.noDataWarningTitle)
                    .font(.system(size: 15 * scale, weight: .black))
                    .foregroundStyle(MockupColors.yellow)
            }
            ForEach(snapshot.noDataWarningLines, id: \.self) { line in
                Text(line)
                    .font(.system(size: 14 * scale, weight: .medium))
                    .foregroundStyle(MockupColors.primaryText.opacity(0.9))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 18 * scale)
        .padding(.vertical, 16 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 22 * scale, style: .continuous)
                .fill(Color(red: 0.145, green: 0.094, blue: 0.102))
                .overlay(
                    RoundedRectangle(cornerRadius: 22 * scale, style: .continuous)
                        .stroke(Color(red: 0.318, green: 0.188, blue: 0.208), lineWidth: 1.2)
                )
        )
    }
}

struct BmsNoDataPackEstimateCard: View {
    let percentText: String
    let detail: String
    let confidenceTitle: String
    let confidenceDetail: String
    let scale: CGFloat

    var body: some View {
        HStack(alignment: .top, spacing: 14 * scale) {
            VStack(alignment: .leading, spacing: 8 * scale) {
                BmsNoDataLabel(text: "PACK ESTIMATE", scale: scale)
                HStack(alignment: .firstTextBaseline, spacing: 4 * scale) {
                    Text(percentText)
                        .font(.system(size: 64 * scale, weight: .black))
                        .monospacedDigit()
                    Text("%")
                        .font(.system(size: 18 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.muted)
                }
                Text(detail)
                    .font(.system(size: 10 * scale, weight: .medium))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .foregroundStyle(MockupColors.muted)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            VStack(alignment: .leading, spacing: 8 * scale) {
                BmsNoDataLabel(text: "CONFIDENCE", scale: scale)
                Text(confidenceTitle)
                    .font(.system(size: 22 * scale, weight: .black))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                Text(confidenceDetail)
                    .font(.system(size: 11 * scale, weight: .medium))
                    .foregroundStyle(MockupColors.muted)
            }
            .padding(.horizontal, 10 * scale)
            .padding(.vertical, 14 * scale)
            .frame(width: 112 * scale, alignment: .leading)
            .background(BmsNoDataDashedCard(cornerRadius: 18 * scale, scale: scale))
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 28 * scale))
    }
}

struct BmsNoDataTelemetryCard: View {
    let voltageValue: String
    let rideSagValue: String
    let rideSagUnit: String
    let loadValue: String
    let loadUnit: String
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 14 * scale) {
            BmsNoDataLabel(text: "WHAT WE CAN SEE", scale: scale)
            HStack(alignment: .top, spacing: 18 * scale) {
                BmsNoDataMetric(value: voltageValue, unit: "V", label: "pack voltage", scale: scale)
                BmsNoDataMetric(value: rideSagValue, unit: rideSagUnit, label: "ride sag", scale: scale)
                BmsNoDataMetric(value: loadValue, unit: loadUnit, label: "load now", scale: scale)
            }
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
    }
}

struct BmsNoDataUnknownsCard: View {
    let rows: [String]
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 10 * scale) {
            BmsNoDataLabel(text: "WHAT IS UNKNOWN", scale: scale)
            ForEach(rows, id: \.self) { row in
                Text(row)
                    .font(.system(size: 14 * scale, weight: .medium))
                    .foregroundStyle(MockupColors.primaryText.opacity(0.92))
                    .lineLimit(1)
                    .minimumScaleFactor(0.82)
                    .padding(.horizontal, 12 * scale)
                    .frame(maxWidth: .infinity, minHeight: 30 * scale, alignment: .leading)
                    .background(BmsNoDataDashedCard(cornerRadius: 10 * scale, scale: scale))
            }
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
    }
}

struct BmsNoDataRidingRuleCard: View {
    let title: String
    let progress: Double
    let scale: CGFloat

    private var clampedProgress: Double {
        min(max(progress, 0), 1)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10 * scale) {
            BmsNoDataLabel(text: "RIDING RULE", scale: scale)
            Text(title)
                .font(.system(size: 13 * scale, weight: .medium))
                .lineLimit(2)
                .minimumScaleFactor(0.84)
                .foregroundStyle(MockupColors.primaryText.opacity(0.9))
            Capsule()
                .fill(MockupColors.cardStroke)
                .frame(height: 6 * scale)
                .overlay(alignment: .leading) {
                    Capsule()
                        .fill(
                            LinearGradient(
                                colors: [MockupColors.yellow, MockupColors.orange],
                                startPoint: .leading,
                                endPoint: .trailing
                            )
                        )
                        .frame(width: 300 * scale * clampedProgress)
                }
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
    }
}
