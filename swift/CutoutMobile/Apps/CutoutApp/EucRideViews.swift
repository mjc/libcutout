import CutoutMobile
import SwiftUI

struct EucRideScreenView: View {
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let captureStatusText: String?
    let disconnect: () -> Void
    let selectScreen: (PevScreenID) -> Void

    private var speedParts: (value: String, unit: String) {
        if let rideState {
            return (rideState.speedText, rideState.speedUnit)
        }
        return ("--", "")
    }

    private var phaseText: String {
        rideState?.statusText ?? "Connecting"
    }

    private var titleText: String {
        rideTitle ?? "EUC"
    }

    private var warningCard: PevWarningCard? {
        if let warningState {
            return PevWarningCard(title: warningState.title, detail: warningState.detail)
        }
        return nil
    }

    private var warningSeverity: EucRideWarningSeverity {
        warningState?.severity ?? .reduceAcceleration
    }

    private var warningState: EucRideWarningState? {
        guard let rideState else {
            return nil
        }
        guard let now = rideState.displayState.lastUpdate else {
            return rideState.warningState
        }
        return rideState.warningState(at: now, staleAfter: MonotonicMilliseconds(2_000))
    }

    private var safetyBars: [PevSafetyBar] {
        if let rideState {
            if rideState.telemetry != nil {
                return liveSafetyBars(for: rideState)
            }
            return []
        }
        return []
    }

    private var dashboardTiles: [PevDashboardTile] {
        if let rideState {
            if let telemetry = rideState.telemetry {
                return liveDashboardTiles(from: rideState, telemetry: telemetry)
            }
            return []
        }
        return []
    }

    var body: some View {
        PevRideDashboardShell(
            sectionTitle: "EUC ride",
            title: titleText,
            subtitle: phaseText,
            statusFill: PevColors.green,
            captureStatusText: captureStatusText,
            speedValue: speedParts.value,
            speedUnit: speedParts.unit,
            speedCaption: "speed",
        ) { scale, columns in
            VStack(spacing: 10 * scale) {
                ForEach(safetyBars, id: \.label) { bar in
                    PevDashboardProgressBar(
                        label: bar.label,
                        value: bar.value,
                        progress: bar.progress,
                        accent: bar.accent.color,
                        track: PevColors.cardFill,
                        labelColor: PevColors.muted,
                        valueColor: bar.accent.color,
                        scale: scale
                    )
                }
            }

            if let warningCard {
                PevDashboardWarningCard(
                    title: warningCard.title,
                    detail: warningCard.detail,
                    accent: eucWarningAccent(for: warningSeverity),
                    detailColor: PevColors.warningText,
                    fill: PevColors.warningFill,
                    stroke: PevColors.warningStroke,
                    scale: scale,
                    height: 76
                )
                    .padding(.top, 14 * scale)
            }

            PevDashboardGrid(columns: columns, spacing: 12 * scale) {
                ForEach(dashboardTiles) { tile in
                    PevDashboardMetricTile(tile, scale: scale, cornerRadius: 16, minHeight: 104)
                }
            }
            .padding(.top, 12 * scale)
        }
    }
}

private func eucWarningAccent(for severity: EucRideWarningSeverity) -> Color {
    switch severity {
    case .normal:
        PevColors.green
    case .caution, .reduceAcceleration:
        PevColors.orange
    case .limpHome, .failed:
        PevColors.red
    case .unavailable:
        PevColors.muted
    }
}
