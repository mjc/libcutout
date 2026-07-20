import CutoutMobile
import SwiftUI

struct BmsNoDataHeader: View {
    let screen: PevScreen
    let scale: CGFloat

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: 12 * scale) {
                titleBlock
                Spacer(minLength: 0)
                statusPill
            }
            VStack(alignment: .leading, spacing: 10 * scale) {
                titleBlock
                statusPill
            }
        }
    }

    private var titleBlock: some View {
        PevScreenTitleBlock(title: screen.title, subtitle: screen.subtitle)
    }

    private var statusPill: some View {
        PevDashboardStatusPill(title: screen.secondaryValue, scale: scale, fill: PevColors.yellow)
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
                    .foregroundStyle(PevColors.yellow)
                    .frame(width: 28 * scale, height: 28 * scale)
                    .accessibilityHidden(true)

                Text(snapshot.noDataWarningTitle)
                    .font(.headline)
                    .foregroundStyle(PevColors.yellow)
            }
            ForEach(snapshot.noDataWarningLines) { line in
                Text(line.text)
                    .font(.body)
                    .foregroundStyle(PevColors.primaryText.opacity(0.9))
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
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(snapshot.noDataWarningTitle)
        .accessibilityValue(snapshot.noDataWarningLines.map(\.text).joined(separator: ". "))
        .accessibilityIdentifier("bms.no-data.warning")
    }
}

struct BmsNoDataPackEstimateCard: View {
    let percentText: String
    let detail: String
    let confidenceTitle: String
    let confidenceDetail: String
    let scale: CGFloat

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: 14 * scale) {
                estimate
                confidence
            }
            VStack(alignment: .leading, spacing: 14 * scale) {
                estimate
                confidence
            }
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 28 * scale))
    }

    private var estimate: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            PevDashboardSectionLabel(title: "PACK ESTIMATE", font: .caption.weight(.bold))
            HStack(alignment: .firstTextBaseline, spacing: 4 * scale) {
                Text(percentText)
                    .font(.largeTitle.weight(.black))
                    .monospacedDigit()
                Text("%")
                    .font(.headline.weight(.bold))
                    .foregroundStyle(PevColors.muted)
            }
            Text(detail)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Pack estimate")
        .accessibilityValue("\(percentText) percent. \(detail)")
    }

    private var confidence: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            PevDashboardSectionLabel(title: "CONFIDENCE", font: .caption.weight(.bold))
            Text(confidenceTitle)
                .font(.title2.weight(.black))
            Text(confidenceDetail)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
        }
        .padding(.horizontal, 10 * scale)
        .padding(.vertical, 14 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 18 * scale, style: .continuous)
                .fill(PevColors.cardFill)
                .overlay(
                    RoundedRectangle(cornerRadius: 18 * scale, style: .continuous)
                        .stroke(style: StrokeStyle(lineWidth: 1.2, dash: [5 * scale, 5 * scale]))
                        .foregroundStyle(PevColors.cardStroke)
                )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Confidence")
        .accessibilityValue("\(confidenceTitle). \(confidenceDetail)")
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
            PevDashboardSectionLabel(title: "WHAT WE CAN SEE", font: .caption.weight(.bold))
            PevDashboardGrid(
                columns: [GridItem(.adaptive(minimum: 100 * scale), spacing: 18 * scale)],
                spacing: 18 * scale
            ) {
                BmsNoDataMetric(value: voltageValue, unit: "V", label: "pack voltage")
                BmsNoDataMetric(value: rideSagValue, unit: rideSagUnit, label: "ride sag")
                BmsNoDataMetric(value: loadValue, unit: loadUnit, label: "load now")
            }
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
    }
}

struct BmsNoDataUnknownsCard: View {
    let rows: [BmsNoDataTextRow]
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 10 * scale) {
            PevDashboardSectionLabel(title: "WHAT IS UNKNOWN", font: .caption.weight(.bold))
            ForEach(rows) { row in
                Text(row.text)
                    .font(.body)
                    .foregroundStyle(PevColors.primaryText.opacity(0.92))
                    .padding(.horizontal, 12 * scale)
                    .frame(maxWidth: .infinity, minHeight: 30 * scale, alignment: .leading)
                    .background(
                        RoundedRectangle(cornerRadius: 10 * scale, style: .continuous)
                            .fill(PevColors.cardFill)
                            .overlay(
                                RoundedRectangle(cornerRadius: 10 * scale, style: .continuous)
                                    .stroke(style: StrokeStyle(lineWidth: 1.2, dash: [5 * scale, 5 * scale]))
                                    .foregroundStyle(PevColors.cardStroke)
                            )
                    )
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
            PevDashboardSectionLabel(title: "RIDING RULE", font: .caption.weight(.bold))
            Text(title)
                .font(.body)
                .foregroundStyle(PevColors.primaryText.opacity(0.9))
            ProgressView(value: clampedProgress)
                .tint(PevColors.yellow)
                .accessibilityLabel("Riding rule progress")
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
    }
}
