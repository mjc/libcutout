import Accessibility
import CutoutMobile
import SwiftUI

struct EucRideScreenView: View {
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let now: MonotonicMilliseconds
    let captureStatusText: String?
    let connectionStatusText: String
    let phoneLocationReadback: PhoneLocationReadback

    private var speedReadout: RideHeroReadout {
        .euc(state: rideState, now: now)
    }

    var phaseText: String {
        guard rideState?.phase == .live else { return connectionStatusText }
        guard let warningState, warningState.severity != .normal else {
            return rideState?.statusText ?? connectionStatusText
        }
        return warningState.title
    }

    private var titleText: String {
        rideTitle ?? localizedAppText("euc.ride.untitled")
    }

    private var sectionTitle: String {
        PevScreenCatalog.live.screen(id: .eucRide)!.title
    }

    var statusTone: PevDashboardStatusPillTone {
        guard rideState?.phase == .live,
              let warningState,
              warningState.severity == .normal
        else { return .warning }
        return .eucRide
    }

    private var warningCard: PevWarningCard? {
        warningState.map { PevWarningCard(title: $0.title, detail: $0.detail) }
    }

    private var warningSeverity: EucRideWarningSeverity {
        warningState?.severity ?? .reduceAcceleration
    }

    private var warningState: EucRideWarningState? {
        guard let rideState else {
            return nil
        }
        return rideState.warningState(at: now, staleAfter: RideTelemetryFreshnessPolicy.staleAfter)
    }

    private var safetyBars: [PevSafetyBar] {
        guard let rideState, rideState.telemetry != nil else { return [] }
        return liveSafetyBars(for: rideState)
    }

    private var dashboardTiles: [PevDashboardTile] {
        guard let rideState, let telemetry = rideState.telemetry else { return [] }
        return liveDashboardTiles(from: rideState, telemetry: telemetry)
    }

    private var gpsSpeedTile: PevDashboardTile {
        eucGpsSpeedTile(from: phoneLocationReadback, at: now)
    }

    var body: some View {
        PevRideDashboardShell(
            sectionTitle: sectionTitle,
            heroStyle: .electricUnicycle,
            title: titleText,
            subtitle: phaseText,
            statusTone: statusTone,
            captureStatusText: captureStatusText,
            speedReadout: speedReadout,
            speedCaption: localizedAppText("euc.speed.caption"),
        ) {
            VStack(spacing: 10) {
                ForEach(safetyBars) { bar in
                    PevDashboardProgressBar(
                        label: bar.label,
                        metricValue: bar.metricValue,
                        progress: bar.progress
                    )
                }
            }

            if let warningCard {
                PevDashboardWarningCard(
                    title: warningCard.title,
                    detail: warningCard.detail,
                    accent: eucWarningAccent(for: warningSeverity),
                    detailColor: PevColors.primaryText,
                    fill: PevColors.warningFill,
                    stroke: PevColors.warningStroke
                )
                    .accessibilityIdentifier("euc.warning")
                    .padding(.top, 14)
            }

            PevDashboardGrid(columnSpacing: 12, spacing: 12) {
                ForEach(dashboardTiles) { tile in
                    PevDashboardMetricTile(tile, prominence: .dashboard)
                }
                PevDashboardMetricTile(gpsSpeedTile, prominence: .dashboard)
            }
            .padding(.top, 12)
        }
        .accessibilityElement(children: .contain)
        .onChange(of: warningState?.severity) { _, severity in
            if let announcement = severity?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
    }
}

func eucGpsSpeedTile(
    from readback: PhoneLocationReadback,
    at now: MonotonicMilliseconds
) -> PevDashboardTile {
    return PevDashboardTile(
        kind: .gpsSpeed,
        label: localizedAppText("euc.metric.gps_speed"),
        metricValue: readback.speedMetricValue,
        unit: readback.speedUnit,
        detail: readback.detail(at: now),
        accent: readback.freshness(at: now) == .fresh ? .cyan : .yellow
    )
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
