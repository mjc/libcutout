import CutoutMobile
import SwiftUI

struct EucRideScreenView: View {
    let screen: PevScreen
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let captureStatusText: String?
    let disconnect: () -> Void
    let selectScreen: (PevScreenID) -> Void

    private var speedParts: (value: String, unit: String) {
        if let rideState {
            return (rideState.speedText, rideState.speedUnit)
        }
        let parts = screen.primaryValue.split(separator: " ", maxSplits: 1).map(String.init)
        return (parts.first ?? screen.primaryValue, parts.dropFirst().first ?? "")
    }

    private var phaseText: String {
        rideState?.statusText ?? screen.displaySubtitle
    }

    private var titleText: String {
        rideTitle ?? screen.title
    }

    private var warningCard: PevWarningCard? {
        if let warningState {
            return PevWarningCard(title: warningState.title, detail: warningState.detail)
        }
        return screen.warningCard
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
            return unavailableSafetyBars(from: screen.safetyBars)
        }
        return screen.safetyBars
    }

    private var dashboardTiles: [PevDashboardTile] {
        if let rideState {
            if let telemetry = rideState.telemetry {
                return liveDashboardTiles(from: rideState, telemetry: telemetry)
            }
            return unavailableDashboardTiles(from: screen.dashboardTiles)
        }
        return screen.dashboardTiles
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
            topLeadingAccessory: { scale in
                if rideState == nil {
                    Text("CutOut")
                        .font(.system(size: 18 * scale, weight: .bold))
                        .foregroundStyle(PevColors.yellow)
                } else {
                    PevRideDisconnectButton(scale: scale, action: disconnect)
                }
            }
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

            PevDashboardTabStrip(
                tabs: PevRideTabs.eucRideTabs(),
                scale: scale,
                selectedColor: PevColors.yellow,
                unselectedColor: PevColors.muted,
                selectScreen: selectScreen
            )
                .padding(.top, 48 * scale)
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
