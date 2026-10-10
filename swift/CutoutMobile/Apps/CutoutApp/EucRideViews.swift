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
        .accessibilityElement(children: .contain)
        .onChange(of: warningState?.severity) { _, severity in
            if let announcement = severity?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
    }
}

private struct EucRideReadingsSection: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @ScaledMetric(relativeTo: .headline) private var headroomLineHeight: CGFloat = 21
    @ScaledMetric(relativeTo: .caption) private var footerHeight: CGFloat = 42
    let safetyBars: [PevSafetyBar]
    let tiles: [PevDashboardTile]
    let gpsSpeed: PevDashboardTile

    private var metrics: [PevDashboardTile] {
        let source =
            tiles.isEmpty
            ? liveDashboardTiles(
                from: EucRideScreenState(phase: .live, displayState: RideDisplayState(telemetry: TelemetrySnapshot())),
                telemetry: TelemetrySnapshot())
            : tiles
        return source.filter { $0.kind != .chargeEstimate && $0.kind != .limpHomeRange }
    }

    var body: some View {
        if dynamicTypeSize.isAccessibilitySize {
            PevRideAccessibleReadings(
                headroomLabel: localizedAppText("ride.safety.pwm_headroom"),
                headroom: safetyBars.first?.metricValue ?? .unavailable,
                headroomProgress: safetyBars.first?.progress,
                tiles: (tiles.isEmpty ? metrics : tiles) + [gpsSpeed]
            )
        } else {
            GeometryReader { geometry in
                let rowHeight = min(
                    108, max(0, (geometry.size.height - headroomLineHeight - 17 - footerHeight - 32) / 2))
                VStack(spacing: 8) {
                    PevDashboardProgressBar(
                        label: localizedAppText("ride.safety.pwm_headroom"),
                        metricValue: safetyBars.first?.metricValue ?? .unavailable,
                        progress: safetyBars.first?.progress,
                        height: 10
                    )
                    .fixedSize(horizontal: false, vertical: true)
                    Grid(horizontalSpacing: 8, verticalSpacing: 8) {
                        GridRow {
                            metric(metrics[0], height: rowHeight)
                            metric(metrics[1], height: rowHeight)
                        }
                        GridRow {
                            metric(metrics[2], height: rowHeight)
                            metric(metrics[3], height: rowHeight)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    compactMetric(tiles.first { $0.kind == .chargeEstimate })
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(minHeight: 26)
                    compactMetric(tiles.first { $0.kind == .limpHomeRange })
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(minHeight: 16)
                }
            }
        }
    }

    private func metric(_ tile: PevDashboardTile, height: CGFloat) -> some View {
        PevDashboardMetricTile(tile, prominence: .ride)
            .frame(height: height)
    }

    @ViewBuilder
    private func compactMetric(_ tile: PevDashboardTile?) -> some View {
        if let tile {
            HStack {
                Text(tile.label)
                Spacer(minLength: 4)
                Text(tile.value)
                    .monospacedDigit()
            }
            .font(.caption.weight(.semibold))
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .accessibilityElement(children: .combine)
        } else {
            Color.clear
                .accessibilityHidden(true)
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
