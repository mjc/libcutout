import Accessibility
import CutoutMobile
import SwiftUI

struct EucRideScreenView: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let now: MonotonicMilliseconds
    let captureStatusText: String?
    let connectionStatusText: String
    let phoneLocationReadback: PhoneLocationReadback
    var disconnect: (() -> Void)? = nil

    private var speedReadout: RideHeroReadout {
        .euc(state: rideState, now: now)
    }

    var phaseText: String {
        guard let rideState, rideState.phase == .live else { return connectionStatusText }
        if rideState.operatingState == .charging {
            switch rideState.chargeEstimate.kind {
            case .full, .balancing: return rideState.chargeEstimate.displayValue
            default: return pevLocalizedText("euc.status.charging")
            }
        }
        if rideState.warningState.severity == .reduceAcceleration {
            return rideState.warningState.title
        }
        return rideState.operatingState == .unknown
            ? pevLocalizedText("euc.status.connected") : rideState.statusText
    }

    private var titleText: String {
        rideTitle ?? localizedAppText("euc.ride.untitled")
    }

    var statusTone: PevDashboardStatusPillTone {
        guard let rideState, rideState.phase == .live else { return .warning }
        return rideState.warningState.severity == .reduceAcceleration ? .warning : .eucRide
    }

    private var warningState: EucRideWarningState? {
        guard let rideState else {
            return nil
        }
        return rideState.warningState
    }

    private var currentTelemetry: Bool {
        guard let rideState, rideState.phase == .live else { return false }
        return rideState.updateAge(at: now, staleAfter: RideTelemetryFreshnessPolicy.staleAfter).freshness == .fresh
    }

    var safetyBars: [PevSafetyBar] {
        guard currentTelemetry, let rideState else { return [] }
        return liveSafetyBars(for: rideState)
    }

    private var dashboardTiles: [PevDashboardTile] {
        let state = rideState ?? EucRideScreenState(phase: .starting, displayState: RideDisplayState())
        return liveDashboardTiles(from: state, telemetry: state.telemetry ?? TelemetrySnapshot())
    }

    private var gpsSpeedTile: PevDashboardTile {
        eucGpsSpeedTile(from: phoneLocationReadback, at: now)
    }

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                accessibleDashboard
            } else {
                EucRideInstruments(
                    title: titleText, subtitle: phaseText, statusTone: statusTone,
                    captureStatusText: captureStatusText, speed: speedReadout,
                    headroom: safetyBars.first,
                    headroomNumber: safetyBars.first == nil
                        ? nil : rideState?.pwmHeadroomPermille.map(RideUnits.permillePercentText),
                    batteryProgress: rideState?.controllerOnlyEstimatePercent.map { Double($0.value) / 100 },
                    tiles: dashboardTiles, gpsSpeed: gpsSpeedTile, disconnect: disconnect
                )
            }
        }
        .accessibilityElement(children: .contain)
        .onChange(of: warningState?.severity) { _, severity in
            if let announcement = severity?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
    }

    private var accessibleDashboard: some View {
        PevRideDashboardShell(
            heroStyle: .electricUnicycle,
            title: titleText,
            subtitle: phaseText,
            statusTone: statusTone,
            captureStatusText: captureStatusText,
            speedReadout: speedReadout,
            speedCaption: localizedAppText("euc.speed.caption"),
            gpsSpeed: gpsSpeedTile
        ) {
            EucRideReadingsSection(safetyBars: safetyBars, tiles: dashboardTiles, gpsSpeed: gpsSpeedTile)
        }
    }
}

private struct EucRideReadingsSection: View {
    let safetyBars: [PevSafetyBar]
    let tiles: [PevDashboardTile]
    let gpsSpeed: PevDashboardTile

    var body: some View {
        PevRideAccessibleReadings(
            headroomLabel: localizedAppText("ride.safety.pwm_headroom"),
            headroom: safetyBars.first?.metricValue ?? .unavailable,
            headroomProgress: safetyBars.first?.progress,
            tiles: tiles + [gpsSpeed]
        )
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
