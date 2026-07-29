import CutoutMobile
import SwiftUI

struct BmsNoDataLayout: View {
    let screen: PevScreen
    let content: PevBmsContent
    let rideState: EucRideScreenState?
    let liveSnapshot: BmsSnapshot?

    private var snapshot: BmsSnapshot { content.snapshot }
    private var presentation: BmsNoDataPresentation {
        snapshot.noDataPresentation(rideState: rideState)
    }
    private var controllerEstimateDetail: String {
        switch presentation.controllerEstimateDetail {
        case .recentSag:
            localizedAppText("bms.no_data.estimate_detail.recent_sag")
        case .voltageCurve:
            localizedAppText("bms.no_data.estimate_detail.voltage_curve")
        case .unavailable:
            localizedAppText("bms.no_data.estimate_detail.unavailable")
        }
    }
    private var controllerConfidenceTitle: String {
        switch presentation.controllerConfidence {
        case .medium:
            localizedAppText("bms.no_data.confidence.medium")
        case .low:
            localizedAppText("bms.no_data.confidence.low")
        case .unknown:
            localizedAppText("bms.no_data.confidence.unknown")
        }
    }
    private var controllerConfidenceDetail: String {
        switch presentation.controllerConfidence {
        case .medium, .low:
            localizedAppText("bms.no_data.confidence_detail.not_cell_safe")
        case .unknown:
            localizedAppText("bms.no_data.confidence_detail.telemetry_unavailable")
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 14) {
                    BmsNoDataHeader(screen: screen)

                    BmsNoDataWarningCard(snapshot: snapshot)

                    BmsNoDataPackEstimateCard(
                        metricValue: presentation.controllerEstimateMetricValue,
                        detail: controllerEstimateDetail,
                        confidenceTitle: controllerConfidenceTitle,
                        confidenceDetail: controllerConfidenceDetail
                    )

                    BmsNoDataTelemetryCard(
                        voltageMetricValue: presentation.packVoltageMetricValue,
                        rideSagMetricValue: presentation.rideSagMetricValue,
                        loadMetricValue: presentation.loadMetricValue
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
