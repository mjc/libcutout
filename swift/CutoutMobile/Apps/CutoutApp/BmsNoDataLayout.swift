import CutoutMobile
import SwiftUI

struct BmsNoDataLayout: View {
    let screen: PevScreen
    let content: PevBmsContent
    let rideState: EucRideScreenState?
    let liveSnapshot: BmsSnapshot?

    private var snapshot: BmsSnapshot { content.snapshot }
    private var controllerEstimateMetricValue: PevDashboardMetricValue {
        snapshot.noDataPackEstimateMetricValue(
            controllerEstimatePercent: rideState?.controllerOnlyEstimatePercent
        )
    }
    private var controllerEstimateDetail: String {
        switch rideState?.controllerOnlyEstimateDetail ?? fallbackEstimateDetail {
        case .recentSag:
            localizedAppText("bms.no_data.estimate_detail.recent_sag")
        case .voltageCurve:
            localizedAppText("bms.no_data.estimate_detail.voltage_curve")
        case .unavailable:
            localizedAppText("bms.no_data.estimate_detail.unavailable")
        }
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
    private var packVoltage: Voltage? {
        rideState?.telemetry?.voltage ?? snapshot.voltage
    }
    private var packCurrent: BatteryCurrent? {
        rideState?.telemetry?.batteryCurrent ?? snapshot.current
    }
    private var packVoltageMetricValue: PevDashboardMetricValue {
        bmsPackVoltageMetricValue(packVoltage)
    }
    private var rideSagMetricValue: PevDashboardMetricValue {
        bmsVoltageSagMetricValue(rideState?.voltageSag)
    }
    private var loadMetricValue: PevDashboardMetricValue {
        bmsBatteryCurrentMetricValue(packCurrent)
    }
    private var fallbackEstimateDetail: ControllerOnlyEstimateDetail {
        if snapshot.voltage != nil, snapshot.current != nil {
            return .recentSag
        }
        if snapshot.voltage != nil {
            return .voltageCurve
        }
        return .unavailable
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
                        metricValue: controllerEstimateMetricValue,
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

                    if let liveSnapshot, liveSnapshot.shouldRenderReadback {
                        BmsDiagnosticsSection(snapshot: liveSnapshot)
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
