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
        if let liveSnapshot {
            return vescRideSubtitle(liveSnapshot)
        }
        return phase == .live
            ? localizedAppText("vesc.subtitle.telemetry_pending")
            : connectionStatusText ?? phase.displayText
    }

    private var speedReadout: PevRideHeroReadout {
        .vesc(snapshot: liveSnapshot, now: now)
    }

    private var warningCard: PevWarningCard? {
        guard let liveSnapshot else { return nil }
        switch liveSnapshot.warning {
        case .pushbackSoon:
            return PevWarningCard(
                title: localizedAppText("vesc.warning.pushback_soon"),
                detail: footpadText ?? localizedAppText("vesc.warning.live_telemetry")
            )
        case .none, .unknown:
            return nil
        }
    }

    private var footpadText: String? {
        liveSnapshot?.footpad?.summaryText
    }

    private var telemetryAge: EucRideUpdateAge? {
        liveSnapshot?.updateAge(
            at: now,
            staleAfter: RideTelemetryFreshnessPolicy.staleAfter
        )
    }

    private var batteryDetail: String {
        guard let liveSnapshot else { return "" }
        if let battery = liveSnapshot.batteryLevelReported {
            if let current = liveSnapshot.batteryCurrent {
                return localizedAppText(
                    "vesc.battery_detail.reported_current",
                    percentText(battery),
                    currentText(current)
                )
            }
            return localizedAppText(
                "vesc.battery_detail.reported_unavailable",
                percentText(battery)
            )
        }
        if let battery = liveSnapshot.batteryLevelEstimated {
            if let current = liveSnapshot.batteryCurrent {
                return localizedAppText(
                    "vesc.battery_detail.estimated_current",
                    percentText(battery),
                    currentText(current)
                )
            }
            return localizedAppText(
                "vesc.battery_detail.estimated_unavailable",
                percentText(battery)
            )
        }
        if let current = liveSnapshot.batteryCurrent {
            return localizedAppText(
                "vesc.battery_detail.unavailable_current",
                currentText(current)
            )
        }
        return localizedAppText("vesc.battery_detail.unavailable_unavailable")
    }

    private var motorCurrentDetail: String {
        guard let liveSnapshot, liveSnapshot.motorCurrent != nil else {
            return localizedAppText("vesc.current.unavailable")
        }
        return powerFlowDetail(
            liveSnapshot.powerFlow,
            fallback: localizedAppText("vesc.phase_current")
        )
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
                detail: liveSnapshot.motorTemperature.map {
                    localizedAppText(
                        "vesc.motor_temperature.available",
                        temperatureText($0),
                        RideUnits.temperatureUnit
                    )
                } ?? localizedAppText("vesc.motor_temperature.unavailable"),
                accent: .green
            ),
        ]
    }

    private var prioritizesMetrics: Bool {
        dynamicTypeSize.isAccessibilitySize && verticalSizeClass == .compact
    }

    private var boardAngleDetail: String? {
        guard let angle = liveSnapshot?.boardAngle else { return nil }
        let direction: String
        if angle.value < 0 {
            direction = "nose_down"
        } else if angle.value > 0 {
            direction = "nose_up"
        } else {
            direction = "level"
        }
        if let balance = liveSnapshot?.balanceAngle {
            return localizedAppText(
                "vesc.board_angle.\(direction)_with_balance",
                angleText(balance)
            )
        }
        return localizedAppText("vesc.board_angle.\(direction)")
    }

    var body: some View {
        PevRideDashboardShell(
            sectionTitle: localizedAppText("navigation.section.ride"),
            heroStyle: .vescOnewheel,
            title: title,
            subtitle: subtitle,
            statusTone: .vescRide,
            captureStatusText: captureStatusText,
            speedReadout: speedReadout,
            speedCaption: localizedAppText("vesc.speed.caption"),
            allowsVerticalScroll: true,
        ) {

            if prioritizesMetrics {
                metricsGrid
            }

            if let age = telemetryAge, age.freshness == .stale, let elapsed = age.elapsed {
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
                .padding(.top, 12)
            } else if let liveSnapshot, liveSnapshot.dutyHeadroomProgressMetricValue != .unavailable {
                PevDashboardProgressCard(
                    label: localizedAppText("vesc.duty_headroom.label"),
                    metricValue: liveSnapshot.dutyHeadroomProgressMetricValue,
                    detail: localizedAppText("vesc.duty_headroom.detail"),
                    progress: liveSnapshot.dutyHeadroomProgress
                )
                    .padding(.top, 12)
            } else if liveSnapshot == nil && phase == .live {
                PevDashboardWarningCard(
                    title: localizedAppText("vesc.subtitle.telemetry_pending"),
                    detail: localizedAppText("vesc.telemetry_pending.detail"),
                    tone: .vesc
                )
                    .padding(.top, 12)
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
                    leftValue: footpad.adc1DisplayText,
                    rightLabel: localizedAppText("vesc.footpad.right"),
                    rightValue: footpad.adc2DisplayText,
                    detail: footpad.stateDisplayText,
                    accessibilityValue: footpad.accessibilityValue
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
