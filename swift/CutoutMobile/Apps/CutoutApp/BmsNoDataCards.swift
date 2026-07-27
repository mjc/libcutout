import CutoutMobile
import SwiftUI

struct BmsNoDataHeader: View {
    let screen: PevScreen

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: 12) {
                PevScreenTitleBlock(title: screen.title, subtitle: screen.subtitle)
                Spacer(minLength: 0)
                PevDashboardStatusPill(title: screen.secondaryValue, tone: .warning)
            }
            VStack(alignment: .leading, spacing: 10) {
                PevScreenTitleBlock(title: screen.title, subtitle: screen.subtitle)
                PevDashboardStatusPill(title: screen.secondaryValue, tone: .warning)
            }
        }
    }
}

struct BmsNoDataWarningCard: View {
    @ScaledMetric(relativeTo: .headline) private var warningIconSize = 28.0

    let snapshot: BmsSnapshot

    var accessibilityValueText: String {
        pevDashboardAccessibilityValue(snapshot.noDataWarningLines.map(\.text))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 14) {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: warningIconSize, weight: .bold))
                    .foregroundStyle(PevColors.yellow)
                    .frame(width: warningIconSize, height: warningIconSize)
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
            PevDashboardCardBackground(
                cornerRadius: 22,
                fill: PevColors.warningFill,
                stroke: PevColors.warningStroke,
                lineWidth: 1.2
            )
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(snapshot.noDataWarningTitle)
        .accessibilityValue(accessibilityValueText)
        .accessibilityIdentifier("bms.no-data.warning")
    }
}

struct BmsNoDataPackEstimateCard: View {
    let metricValue: PevDashboardMetricValue
    let detail: String
    let confidenceTitle: String
    let confidenceDetail: String

    var accessibilityValueText: String {
        localizedAppText(
            "bms.no_data.pack_estimate_accessibility_value",
            metricValue.accessibilityText,
            detail
        )
    }

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
            PevDashboardSectionLabel(title: localizedAppText("bms.no_data.pack_estimate"), font: .caption.weight(.bold))
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text(metricValue.displayText)
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
        .accessibilityLabel(localizedAppText("bms.no_data.pack_estimate_accessibility"))
        .accessibilityValue(accessibilityValueText)
    }

    private var confidence: some View {
        VStack(alignment: .leading, spacing: 8) {
            PevDashboardSectionLabel(title: localizedAppText("bms.no_data.confidence"), font: .caption.weight(.bold))
            Text(confidenceTitle)
                .font(.title2.weight(.black))
            Text(confidenceDetail)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 18, lineWidth: 1.2, dashPattern: [5, 5]))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(localizedAppText("bms.no_data.confidence_accessibility"))
        .accessibilityValue(
            localizedAppText("bms.no_data.confidence_accessibility_value", confidenceTitle, confidenceDetail)
        )
    }
}

struct BmsNoDataTelemetryCard: View {
    let voltageMetricValue: PevDashboardMetricValue
    let rideSagMetricValue: PevDashboardMetricValue
    let loadMetricValue: PevDashboardMetricValue

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            PevDashboardSectionLabel(title: localizedAppText("bms.no_data.what_we_can_see"), font: .caption.weight(.bold))
            PevDashboardGrid(
                columns: [GridItem(.adaptive(minimum: 100), spacing: 18)],
                spacing: 18
            ) {
                BmsNoDataMetric(metricValue: voltageMetricValue, unit: "V", label: localizedAppText("bms.no_data.pack_voltage"))
                BmsNoDataMetric(metricValue: rideSagMetricValue, unit: "V", label: localizedAppText("bms.no_data.ride_sag"))
                BmsNoDataMetric(metricValue: loadMetricValue, unit: "A", label: localizedAppText("bms.no_data.load_now"))
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
            PevDashboardSectionLabel(title: localizedAppText("bms.no_data.what_is_unknown"), font: .caption.weight(.bold))
            ForEach(rows) { row in
                Text(row.text)
                    .font(.body)
                    .foregroundStyle(PevColors.primaryText.opacity(0.92))
                    .padding(.horizontal, 12)
                    .frame(maxWidth: .infinity, minHeight: 30, alignment: .leading)
                    .background(PevDashboardCardBackground(cornerRadius: 10, lineWidth: 1.2, dashPattern: [5, 5]))
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24))
    }
}

struct BmsNoDataRidingRuleCard: View {
    let metricValue: PevDashboardMetricValue
    let progress: Double

    var titleAccessibilityText: String {
        metricValue.accessibilityText
    }

    private var clampedProgress: Double {
        min(max(progress, 0), 1)
    }

    var progressAccessibilityValue: String {
        localizedAppText(
            "bms.no_data.riding_rule_progress_value",
            Int64((clampedProgress * 100).rounded())
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            PevDashboardSectionLabel(title: localizedAppText("bms.no_data.riding_rule"), font: .caption.weight(.bold))
            Text(metricValue.displayText)
                .font(.body)
                .foregroundStyle(PevColors.primaryText.opacity(0.9))
                .accessibilityLabel(titleAccessibilityText)
            ProgressView(value: clampedProgress)
                .tint(PevColors.yellow)
                .accessibilityLabel(localizedAppText("bms.no_data.riding_rule_progress_accessibility"))
                .accessibilityValue(progressAccessibilityValue)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24))
    }
}
