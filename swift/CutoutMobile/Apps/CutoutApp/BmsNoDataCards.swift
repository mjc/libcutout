import CutoutMobile
import SwiftUI

struct BmsNoDataHeader: View {
    let screen: PevScreen

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: 12) {
                titleBlock
                Spacer(minLength: 0)
                statusPill
            }
            VStack(alignment: .leading, spacing: 10) {
                titleBlock
                statusPill
            }
        }
    }

    private var titleBlock: some View {
        PevScreenTitleBlock(title: screen.title, subtitle: screen.subtitle)
    }

    private var statusPill: some View {
        PevDashboardStatusPill(title: screen.secondaryValue, fill: PevColors.yellow)
    }
}

struct BmsNoDataWarningCard: View {
    let snapshot: BmsSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 14) {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 28, weight: .bold))
                    .foregroundStyle(PevColors.yellow)
                    .frame(width: 28, height: 28)
                    .accessibilityHidden(true)

                Text(snapshot.noDataWarningTitle)
                    .font(.headline)
                    .foregroundStyle(PevColors.primaryText)
            }
            ForEach(snapshot.noDataWarningLines) { line in
                Text(line.text)
                    .font(.body)
                    .foregroundStyle(PevColors.primaryText.opacity(0.9))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 22, style: .continuous)
                .fill(PevColors.warningFill)
                .overlay(
                    RoundedRectangle(cornerRadius: 22, style: .continuous)
                        .stroke(PevColors.warningStroke, lineWidth: 1.2)
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

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: 14) {
                estimate
                confidence
            }
            VStack(alignment: .leading, spacing: 14) {
                estimate
                confidence
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 28))
    }

    private var estimate: some View {
        VStack(alignment: .leading, spacing: 8) {
            PevDashboardSectionLabel(title: "PACK ESTIMATE", font: .caption.weight(.bold))
            HStack(alignment: .firstTextBaseline, spacing: 4) {
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
        VStack(alignment: .leading, spacing: 8) {
            PevDashboardSectionLabel(title: "CONFIDENCE", font: .caption.weight(.bold))
            Text(confidenceTitle)
                .font(.title2.weight(.black))
            Text(confidenceDetail)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(PevColors.cardFill)
                .overlay(
                    RoundedRectangle(cornerRadius: 18, style: .continuous)
                        .stroke(style: StrokeStyle(lineWidth: 1.2, dash: [5, 5]))
                        .foregroundStyle(PevColors.cardStroke)
                )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Confidence")
        .accessibilityValue("\(confidenceTitle). \(confidenceDetail)")
    }
}

struct BmsNoDataTelemetryCard: View {
    let voltageMetricValue: PevDashboardMetricValue
    let rideSagMetricValue: PevDashboardMetricValue
    let loadMetricValue: PevDashboardMetricValue

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            PevDashboardSectionLabel(title: "WHAT WE CAN SEE", font: .caption.weight(.bold))
            PevDashboardGrid(
                columns: [GridItem(.adaptive(minimum: 100), spacing: 18)],
                spacing: 18
            ) {
                BmsNoDataMetric(metricValue: voltageMetricValue, unit: "V", label: "pack voltage")
                BmsNoDataMetric(metricValue: rideSagMetricValue, unit: "V", label: "ride sag")
                BmsNoDataMetric(metricValue: loadMetricValue, unit: "A", label: "load now")
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24))
    }
}

struct BmsNoDataUnknownsCard: View {
    let rows: [BmsNoDataTextRow]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            PevDashboardSectionLabel(title: "WHAT IS UNKNOWN", font: .caption.weight(.bold))
            ForEach(rows) { row in
                Text(row.text)
                    .font(.body)
                    .foregroundStyle(PevColors.primaryText.opacity(0.92))
                    .padding(.horizontal, 12)
                    .frame(maxWidth: .infinity, minHeight: 30, alignment: .leading)
                    .background(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .fill(PevColors.cardFill)
                            .overlay(
                                RoundedRectangle(cornerRadius: 10, style: .continuous)
                                    .stroke(style: StrokeStyle(lineWidth: 1.2, dash: [5, 5]))
                                    .foregroundStyle(PevColors.cardStroke)
                            )
                    )
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24))
    }
}

struct BmsNoDataRidingRuleCard: View {
    let title: String
    let progress: Double

    private var clampedProgress: Double {
        min(max(progress, 0), 1)
    }

    var progressAccessibilityValue: String {
        "\(Int((clampedProgress * 100).rounded())) percent"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            PevDashboardSectionLabel(title: "RIDING RULE", font: .caption.weight(.bold))
            Text(title)
                .font(.body)
                .foregroundStyle(PevColors.primaryText.opacity(0.9))
            ProgressView(value: clampedProgress)
                .tint(PevColors.yellow)
                .accessibilityLabel("Riding rule progress")
                .accessibilityValue(progressAccessibilityValue)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24))
    }
}
