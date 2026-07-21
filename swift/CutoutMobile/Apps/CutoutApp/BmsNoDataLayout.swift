import CutoutMobile
import SwiftUI

struct BmsNoDataLayout: View {
    let screen: PevScreen
    let content: PevBmsContent
    let rideState: EucRideScreenState?
    let liveSnapshot: BmsSnapshot?
    @State private var showsDiagnostics = false

    private var snapshot: BmsSnapshot { content.snapshot }
    private var controllerEstimatePercentText: String {
        if let percent = rideState?.controllerOnlyEstimatePercent ?? snapshot.energyPercent {
            return String(percent.value)
        }
        return "--"
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
        if snapshot.voltage != nil, snapshot.current != nil {
            return "derived from voltage curve + recent sag"
        }
        if snapshot.voltage != nil {
            return "derived from voltage curve only"
        }
        return "estimate unavailable"
    }
    private var fallbackConfidenceTitle: String {
        if snapshot.voltage != nil, snapshot.current != nil {
            return "medium"
        }
        if snapshot.voltage != nil || snapshot.energyPercent != nil {
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
                VStack(alignment: .leading, spacing: 14) {
                    BmsNoDataHeader(screen: screen)

                    BmsNoDataWarningCard(snapshot: snapshot)

                    BmsNoDataPackEstimateCard(
                        percentText: controllerEstimatePercentText,
                        detail: controllerEstimateDetail,
                        confidenceTitle: controllerConfidenceTitle,
                        confidenceDetail: controllerConfidenceDetail
                    )

                    BmsNoDataTelemetryCard(
                        voltageValue: voltageText(packVoltage),
                        rideSagValue: rideSagText ?? "--",
                        rideSagUnit: rideSagText == nil ? "" : "V",
                        loadValue: currentText(packCurrent) ?? "--",
                        loadUnit: currentUnitText(packCurrent) ?? ""
                    )

                    BmsNoDataUnknownsCard(rows: snapshot.noDataUnknownRows)

                    BmsNoDataRidingRuleCard(
                        title: snapshot.captureActionTitle ?? "--",
                        progress: controllerRidingRuleProgress
                    )

                    if let liveSnapshot, liveSnapshot.shouldRenderReadback {
                        BmsDiagnosticsSection(
                            snapshot: liveSnapshot,
                            isExpanded: $showsDiagnostics
                        )
                    }
                }
                .padding(.horizontal, 24)
                .padding(.top, 44)
                .padding(.bottom, 20)
            }

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

}
