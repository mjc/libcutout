import CutoutMobile
import SwiftUI

struct BmsNoDataLayout: View {
    let screen: PevScreen
    let content: PevBmsContent
    let rideState: EucRideScreenState?
    let liveSnapshot: BmsSnapshot?
    let scale: CGFloat
    let selectScreen: (PevScreenID) -> Void
    @State private var showsDiagnostics = false

    private var snapshot: BmsSnapshot { content.snapshot }
    private var rideSagMetric: PevMetric? {
        screen.metrics.first { $0.label == "ride sag" }
    }
    private var loadNowMetric: PevMetric? {
        screen.metrics.first { $0.label == "load now" }
    }
    private var controllerEstimatePercentText: String {
        if let percent = rideState?.controllerOnlyEstimatePercent ?? snapshot.energyPercent {
            return String(percent.value)
        }
        return screen.primaryValue.replacingOccurrences(of: "%", with: "")
    }
    private var controllerEstimateDetail: String {
        rideState?.controllerOnlyEstimateDetail ?? fallbackEstimateDetail
    }
    private var controllerConfidenceTitle: String {
        rideState?.controllerOnlyConfidenceTitle ?? fallbackConfidenceTitle
    }
    private var controllerConfidenceDetail: String {
        rideState?.controllerOnlyConfidenceDetail ?? (controllerConfidenceTitle == "unknown" ? "telemetry unavailable" : "not cell-safe")
    }
    private var controllerRidingRuleProgress: Double {
        rideState?.controllerOnlyRidingRuleProgress ?? fallbackRidingRuleProgress
    }
    private var packVoltage: Voltage? {
        rideState?.telemetry?.voltage ?? snapshot.voltage
    }
    private var packCurrent: BatteryCurrent? {
        rideState?.telemetry?.batteryCurrent ?? snapshot.current
    }
    private var rideSagText: String? {
        rideState?.voltageSag.map { String(format: "%.1f", abs(Double($0.value)) / 1_000.0) }
    }
    private var fallbackEstimateDetail: String {
        if snapshot.voltage != nil, snapshot.current != nil || rideSagMetric != nil {
            return "derived from voltage curve + recent sag"
        }
        if snapshot.voltage != nil || !screen.primaryValue.isEmpty {
            return "derived from voltage curve only"
        }
        return "estimate unavailable"
    }
    private var fallbackConfidenceTitle: String {
        if snapshot.voltage != nil, snapshot.current != nil || rideSagMetric != nil {
            return "medium"
        }
        if snapshot.voltage != nil || snapshot.energyPercent != nil || !screen.primaryValue.isEmpty {
            return "low"
        }
        return "unknown"
    }
    private var fallbackRidingRuleProgress: Double {
        switch fallbackConfidenceTitle {
        case "medium":
            0.62
        case "low":
            0.35
        default:
            0.15
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 14 * scale) {
                    BmsNoDataHeader(screen: screen, scale: scale)

                    BmsNoDataWarningCard(snapshot: snapshot, scale: scale)

                    BmsNoDataPackEstimateCard(
                        percentText: controllerEstimatePercentText,
                        detail: controllerEstimateDetail,
                        confidenceTitle: controllerConfidenceTitle,
                        confidenceDetail: controllerConfidenceDetail,
                        scale: scale
                    )

                    BmsNoDataTelemetryCard(
                        voltageValue: voltageText(packVoltage),
                        rideSagValue: rideSagText ?? rideSagMetric.map(metricValueText) ?? "--",
                        rideSagUnit: rideSagText == nil ? rideSagMetric.map(metricUnitText) ?? "" : "V",
                        loadValue: currentText(packCurrent) ?? loadNowMetric.map(metricValueText) ?? "--",
                        loadUnit: currentUnitText(packCurrent) ?? loadNowMetric.map(metricUnitText) ?? "",
                        scale: scale
                    )

                    BmsNoDataUnknownsCard(rows: snapshot.noDataUnknownRows, scale: scale)

                    BmsNoDataRidingRuleCard(
                        title: snapshot.captureActionTitle ?? "--",
                        progress: controllerRidingRuleProgress,
                        scale: scale
                    )

                    if let liveSnapshot, liveSnapshot.shouldRenderReadback {
                        BmsDiagnosticsSection(
                            snapshot: liveSnapshot,
                            scale: scale,
                            isExpanded: $showsDiagnostics
                        )
                    }
                }
                .padding(.horizontal, 24 * scale)
                .padding(.top, 44 * scale)
                .padding(.bottom, 20 * scale)
            }

            HStack {
                BmsBottomTab(title: "Ride", isSelected: false, scale: scale) {
                    selectScreen(.eucRide)
                }
                Spacer()
                BmsBottomTab(title: "Pack", isSelected: true, scale: scale, action: nil)
            }
            .padding(.horizontal, 24 * scale)
            .padding(.top, 12 * scale)
            .padding(.bottom, 20 * scale)
            .background(PevColors.pageBackground)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(PevColors.pageBackground)
        .foregroundStyle(PevColors.primaryText)
    }

    private func currentText(_ value: BatteryCurrent?) -> String? {
        value.map { decimalString(Double($0.value) / 1_000.0, fractionDigits: 0) }
    }

    private func currentUnitText(_ value: BatteryCurrent?) -> String? {
        value.map { _ in "A" }
    }

    private func metricUnitText(_ metric: PevMetric) -> String {
        metric.value.split(separator: " ").dropFirst().first.map(String.init) ?? ""
    }

    private func metricValueText(_ metric: PevMetric) -> String {
        metric.value.split(separator: " ").first.map(String.init) ?? metric.value
    }
}
