import Accessibility
import CutoutMobile
import SwiftUI

struct VescRideScreenView: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.verticalSizeClass) private var verticalSizeClass

    let liveSnapshot: VescRideSnapshot?
    let phase: SessionConnectionPhase
    let now: MonotonicMilliseconds
    let captureStatusText: String?
    let connectionStatusText: String?

    private var title: String {
        liveSnapshot?.title ?? VescRideSnapshot.defaultTitle
    }

    private var subtitle: String {
        if phase != .live {
            return connectionStatusText ?? phase.displayText
        }
        switch dashboardSupport {
        case .telemetryStale:
            return localizedAppText("vesc.warning.telemetry_stale")
        case .telemetryPending:
            return localizedAppText("vesc.subtitle.telemetry_pending")
        case .dutyHeadroom, .none:
            break
        }
        if let liveSnapshot {
            return vescRideSubtitle(liveSnapshot)
        }
        return ""
    }

    private var statusTone: PevDashboardStatusPillTone {
        guard phase == .live else { return .warning }
        switch dashboardSupport {
        case .telemetryStale, .telemetryPending:
            return .warning
        case .dutyHeadroom, .none:
            return .vescRide
        }
    }

    private var speedReadout: RideHeroReadout {
        .vesc(snapshot: liveSnapshot, now: now)
    }

    private var warningCard: PevWarningCard? {
        guard let liveSnapshot else { return nil }
        switch liveSnapshot.warning {
        case .lowVoltage:
            return warningCard("vesc.warning.low_voltage")
        case .highVoltage:
            return warningCard("vesc.warning.high_voltage")
        case .mosfetTemperature:
            return warningCard("vesc.warning.mosfet_temperature")
        case .motorTemperature:
            return warningCard("vesc.warning.motor_temperature")
        case .current:
            return warningCard("vesc.warning.current")
        case .dutyPushback:
            return warningCard("vesc.warning.duty_pushback", showsFootpad: true)
        case .sensors:
            return warningCard("vesc.warning.sensors", showsFootpad: true)
        case .lowBattery:
            return warningCard("vesc.warning.low_battery")
        case .error:
            return warningCard("vesc.warning.error")
        case .none, .unknown:
            return nil
        }
    }

    private func warningCard(_ titleKey: String, showsFootpad: Bool = false) -> PevWarningCard {
        PevWarningCard(
            title: localizedAppText(titleKey),
            detail: showsFootpad
                ? footpadText ?? localizedAppText("vesc.warning.live_telemetry")
                : localizedAppText("vesc.warning.live_telemetry")
        )
    }

    private var footpadText: String? {
        liveSnapshot?.footpad?.summaryText
    }

    private var dashboardSupport: VescRideDashboardSupport {
        VescRideSnapshot.dashboardSupport(
            for: liveSnapshot,
            phase: phase,
            at: now,
            staleAfter: RideTelemetryFreshnessPolicy.staleAfter
        )
    }

    private var batteryDetail: String {
        guard let liveSnapshot else { return "" }
        switch liveSnapshot.batteryReadback {
        case let .reported(level, current):
            let level = localizedAppText("ride.value.percent", level)
            if let current {
                return localizedAppText(
                    "vesc.battery_detail.reported_current",
                    level,
                    current
                )
            }
            return localizedAppText(
                "vesc.battery_detail.reported_unavailable",
                level
            )
        case let .estimated(level, current):
            let level = localizedAppText("ride.value.percent", level)
            if let current {
                return localizedAppText(
                    "vesc.battery_detail.estimated_current",
                    level,
                    current
                )
            }
            return localizedAppText(
                "vesc.battery_detail.estimated_unavailable",
                level
            )
        case let .unavailable(current):
            if let current {
                return localizedAppText(
                    "vesc.battery_detail.unavailable_current",
                    current
                )
            }
            return localizedAppText("vesc.battery_detail.unavailable_unavailable")
        }
    }

    private var motorCurrentDetail: String {
        guard let liveSnapshot else {
            return localizedAppText("vesc.current.unavailable")
        }
        switch liveSnapshot.motorCurrentDetail {
        case let .available(powerFlow):
            return powerFlowDetail(powerFlow, fallback: localizedAppText("vesc.phase_current"))
        case .unavailable:
            return localizedAppText("vesc.current.unavailable")
        }
    }

    var dashboardTiles: [PevDashboardTile] {
        guard let liveSnapshot else { return [] }
        return [
            PevDashboardTile(
                kind: .batteryVoltage,
                label: localizedAppText("vesc.metric.battery_voltage"),
                metricValue: liveSnapshot.batteryVoltageMetricValue,
                unit: RideUnits.voltageUnit,
                detail: batteryDetail,
                accent: .yellow
            ),
            PevDashboardTile(
                kind: .motorCurrent,
                label: localizedAppText("vesc.metric.motor_current"),
                metricValue: liveSnapshot.motorCurrentMetricValue,
                unit: RideUnits.currentUnit,
                detail: motorCurrentDetail,
                accent: .orange
            ),
            PevDashboardTile(
                kind: .boardAngle,
                label: localizedAppText("vesc.metric.board_angle"),
                metricValue: liveSnapshot.boardAngleMetricValue,
                unit: RideUnits.angleUnit,
                detail: boardAngleDetail ?? localizedAppText("vesc.board_angle.unavailable"),
                accent: .cyan
            ),
            PevDashboardTile(
                kind: .controller,
                label: localizedAppText("vesc.metric.controller"),
                metricValue: liveSnapshot.controllerTemperatureMetricValue,
                unit: RideUnits.temperatureUnit,
                detail: controllerTemperatureDetail(for: liveSnapshot),
                accent: .green
            ),
        ]
    }

    private var prioritizesMetrics: Bool {
        guard dynamicTypeSize.isAccessibilitySize && verticalSizeClass == .compact else {
            return false
        }
        switch dashboardSupport {
        case .telemetryStale, .telemetryPending:
            return false
        case .dutyHeadroom, .none:
            return warningCard == nil
        }
    }

    private var boardAngleDetail: String? {
        guard let liveSnapshot else { return nil }
        switch liveSnapshot.boardAngleReadback {
        case let .available(orientation, balanceAngle):
            let direction = switch orientation {
            case .noseDown: "nose_down"
            case .level: "level"
            case .noseUp: "nose_up"
            }
            if let balanceAngle {
                return localizedAppText(
                    "vesc.board_angle.\(direction)_with_balance",
                    balanceAngle
                )
            }
            return localizedAppText("vesc.board_angle.\(direction)")
        case .unavailable:
            return nil
        }
    }

    private func controllerTemperatureDetail(for snapshot: VescRideSnapshot) -> String {
        switch snapshot.controllerTemperatureReadback {
        case let .available(motorTemperature):
            return localizedAppText(
                "vesc.motor_temperature.available",
                motorTemperature,
                RideUnits.temperatureUnit
            )
        case .unavailable:
            return localizedAppText("vesc.motor_temperature.unavailable")
        }
    }

    var body: some View {
        PevRideDashboardShell(
            sectionTitle: localizedAppText("navigation.section.ride"),
            heroStyle: .vescOnewheel,
            title: title,
            subtitle: subtitle,
            statusTone: statusTone,
            captureStatusText: captureStatusText,
            speedReadout: speedReadout,
            speedCaption: localizedAppText("vesc.speed.caption"),
            allowsVerticalScroll: true,
        ) {

            if prioritizesMetrics {
                metricsGrid
            }

            switch dashboardSupport {
            case let .telemetryStale(elapsed):
                PevDashboardWarningCard(
                    title: localizedAppText("vesc.warning.telemetry_stale"),
                    detail: localizedAppText(
                        "vesc.warning.telemetry_stale_detail",
                        Int64(elapsed.rawValue)
                    ),
                    accent: PevColors.primaryText,
                    detailColor: PevColors.primaryText,
                    fill: PevColors.cardFill,
                    stroke: PevColors.primaryText,
                    cornerRadius: 24
                )
                .accessibilityIdentifier("vesc.warning.telemetry-stale")
                .padding(.top, 12)
            case let .dutyHeadroom(metricValue, progress):
                PevDashboardProgressCard(
                    label: localizedAppText("vesc.duty_headroom.label"),
                    metricValue: metricValue,
                    detail: localizedAppText("vesc.duty_headroom.detail"),
                    progress: progress
                )
                .padding(.top, 12)
            case .telemetryPending:
                PevDashboardWarningCard(
                    title: localizedAppText("vesc.subtitle.telemetry_pending"),
                    detail: localizedAppText("vesc.telemetry_pending.detail"),
                    tone: .vesc
                )
                .accessibilityIdentifier("vesc.warning.telemetry-pending")
                .padding(.top, 12)
            case .none:
                EmptyView()
            }

            if let warningCard {
                PevDashboardWarningCard(
                    title: warningCard.title,
                    detail: warningCard.detail,
                    tone: .vesc
                )
                    .padding(.top, 10)
            }

            if let footpad = liveSnapshot?.footpad {
                PevDashboardFootpadReadout(
                    leftLabel: localizedAppText("vesc.footpad.left"),
                    leftMetricValue: footpad.adc1MetricValue,
                    rightLabel: localizedAppText("vesc.footpad.right"),
                    rightMetricValue: footpad.adc2MetricValue,
                    detail: footpad.stateDisplayText
                )
                .padding(.top, 8)
            }

            if !prioritizesMetrics {
                metricsGrid
            }
        }
        .accessibilityElement(children: .contain)
        .onChange(of: liveSnapshot?.warning) { _, warning in
            if let announcement = warning?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
    }

    private var metricsGrid: some View {
        PevDashboardGrid(columnSpacing: 12, spacing: 12) {
            ForEach(dashboardTiles) { tile in
                PevDashboardMetricTile(tile, prominence: .compactDashboard)
            }
        }
        .padding(.top, 8)
    }
}
