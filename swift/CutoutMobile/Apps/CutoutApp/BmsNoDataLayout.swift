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
    private var controllerConfidence: ControllerOnlyEstimateConfidence {
        rideState?.controllerOnlyConfidence ?? fallbackConfidence
    }
    private var controllerConfidenceTitle: String {
        switch controllerConfidence {
        case .medium:
            localizedAppText("bms.no_data.confidence.medium")
        case .low:
            localizedAppText("bms.no_data.confidence.low")
        case .unknown:
            localizedAppText("bms.no_data.confidence.unknown")
        }
    }
    private var controllerConfidenceDetail: String {
        switch controllerConfidence {
        case .medium, .low:
            localizedAppText("bms.no_data.confidence_detail.not_cell_safe")
        case .unknown:
            localizedAppText("bms.no_data.confidence_detail.telemetry_unavailable")
        }
    }
    private var controllerRidingRuleProgress: Double {
        controllerConfidence.ridingRuleProgress
    }
    private var packVoltage: Voltage? {
        rideState?.telemetry?.voltage ?? snapshot.voltage
    }
    private var packCurrent: BatteryCurrent? {
        rideState?.telemetry?.batteryCurrent ?? snapshot.current
    }
    private var packVoltageMetricValue: PevDashboardMetricValue {
        packVoltage.map { voltage in
            let value = RideUnits.voltageText(millivolts: voltage.value)
            return .available(display: value, accessibility: value)
        } ?? .unavailable
    }
    private var rideSagMetricValue: PevDashboardMetricValue {
        rideState?.voltageSag.map { voltageSag in
            let value = decimalString(abs(Double(voltageSag.value)) / 1_000.0, fractionDigits: 1)
            return .available(display: value, accessibility: value)
        } ?? .unavailable
    }
    private var loadMetricValue: PevDashboardMetricValue {
        packCurrent.map { current in
            let value = decimalString(Double(current.value) / 1_000.0, fractionDigits: 0)
            return .available(display: value, accessibility: value)
        } ?? .unavailable
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
    private var fallbackConfidence: ControllerOnlyEstimateConfidence {
        if snapshot.voltage != nil, snapshot.current != nil {
            return .medium
        }
        if snapshot.voltage != nil || snapshot.energyPercent != nil {
            return .low
        }
        return .unknown
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
                        voltageMetricValue: packVoltageMetricValue,
                        rideSagMetricValue: rideSagMetricValue,
                        loadMetricValue: loadMetricValue
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

}
