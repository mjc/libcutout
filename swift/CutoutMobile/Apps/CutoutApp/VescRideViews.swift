import Accessibility
import CutoutMobile
import SwiftUI

func vescFootpadAccessibilityValue(_ footpad: FootpadTelemetry) -> String {
    [
        "left / adc1",
        footpad.adc1Milliunits == nil ? "unavailable" : footpad.adc1DisplayText,
        footpad.adc1Milliunits == nil ? "" : "available",
        "right / adc2",
        footpad.adc2Milliunits == nil ? "unavailable" : footpad.adc2DisplayText,
        footpad.adc2Milliunits == nil ? "" : "available",
        footpad.stateDisplayText,
    ]
    .filter { !$0.isEmpty }
    .joined(separator: ", ")
}

struct VescRideScreenView: View {
    let liveSnapshot: VescRideSnapshot?
    let phase: SessionConnectionPhase
    let now: MonotonicMilliseconds
    let captureStatusText: String?
    let disconnect: () -> Void

    private var title: String {
        liveSnapshot?.title ?? VescRideSnapshot.defaultTitle
    }

    private var subtitle: String {
        if let liveSnapshot {
            return liveSnapshot.screenSubtitle
        }
        return phase == .live ? "Telemetry pending" : phase.displayText
    }

    private var speedReadout: PevRideHeroReadout {
        guard let boardSpeed = liveSnapshot?.boardSpeed else {
            return .unavailable
        }
        let readout = SpeedReadout(millimetersPerSecond: boardSpeed.value)
        return .available(value: readout.displayValue, unit: readout.displayUnit)
    }

    private var warningCard: PevWarningCard? {
        guard let liveSnapshot else { return nil }
        switch liveSnapshot.warning {
        case .pushbackSoon:
            return PevWarningCard(title: "Pushback soon", detail: footpadText ?? "Live telemetry.")
        case .none, .unknown:
            return nil
        }
    }

    private var footpadText: String? {
        liveSnapshot?.footpad.map {
            "footpad \($0.stateDisplayText) · adc1 left \($0.adc1DisplayText) · adc2 right \($0.adc2DisplayText)"
        }
    }

    private var dutyHeadroom: BatteryLevel? {
        liveSnapshot?.displayedDutyHeadroom
    }

    private var telemetryAge: EucRideUpdateAge? {
        liveSnapshot?.updateAge(
            at: now,
            staleAfter: MonotonicMilliseconds(2_000)
        )
    }

    private var batteryDetail: String {
        guard let liveSnapshot else { return "" }
        let battery = liveSnapshot.batteryLevelReported.map {
            "battery \(percentText($0)) reported"
        } ?? liveSnapshot.batteryLevelEstimated.map {
            "battery \(percentText($0)) estimated"
        }
        let details = [
            battery ?? "battery level unavailable",
            liveSnapshot.batteryCurrent.map { "current \(currentText($0)) A" } ?? "current unavailable",
        ]
        return details.joined(separator: " · ")
    }

    private var motorCurrentDetail: String {
        guard let liveSnapshot, liveSnapshot.motorCurrent != nil else {
            return "current unavailable"
        }
        return powerFlowDetail(
            liveSnapshot.powerFlow,
            fallback: "phase current"
        )
    }

    private var dashboardTiles: [PevDashboardTile] {
        guard let liveSnapshot else { return [] }
        return [
            PevDashboardTile(
                kind: .batteryVoltage,
                label: "voltage",
                value: voltageText(liveSnapshot.batteryVoltage),
                unit: "V",
                detail: batteryDetail,
                accent: .yellow
            ),
            PevDashboardTile(
                kind: .motorCurrent,
                label: "motor current",
                value: phaseCurrentText(liveSnapshot.motorCurrent),
                unit: "A",
                detail: motorCurrentDetail,
                accent: .orange
            ),
            PevDashboardTile(
                kind: .boardAngle,
                label: "board angle",
                value: angleText(liveSnapshot.boardAngle),
                unit: "°",
                detail: boardAngleDetail ?? "angle unavailable",
                accent: .cyan
            ),
            PevDashboardTile(
                kind: .controller,
                label: "controller",
                value: temperatureText(liveSnapshot.controllerTemperature),
                unit: "°C",
                detail: liveSnapshot.motorTemperature.map { "motor \(temperatureText($0)) \(RideUnits.temperatureUnit)" } ?? "motor unavailable",
                accent: .green
            ),
        ]
    }

    private var boardAngleDetail: String? {
        guard let angle = liveSnapshot?.boardAngle else { return nil }
        let balance = liveSnapshot?.balanceAngle.map { " · balance \(angleText($0))°" } ?? ""
        if angle.value < 0 {
            return "nose down\(balance)"
        }
        if angle.value > 0 {
            return "nose up\(balance)"
        }
        return "level\(balance)"
    }

    var body: some View {
        PevRideDashboardShell(
            sectionTitle: "Ride",
            heroStyle: .vescOnewheel,
            title: title,
            subtitle: subtitle,
            statusFill: PevColors.purple,
            captureStatusText: captureStatusText,
            speedReadout: speedReadout,
            speedCaption: "board speed",
            allowsVerticalScroll: true,
        ) { scale, columns in

            if let age = telemetryAge, age.freshness == .stale, let elapsed = age.elapsed {
                PevDashboardWarningCard(
                    title: "Telemetry stale",
                    detail: "Last update \(elapsed.rawValue) ms ago.",
                    accent: PevColors.orange,
                    detailColor: PevColors.primaryText,
                    fill: PevColors.cardFill,
                    stroke: PevColors.cardStroke,
                    scale: scale,
                    cornerRadius: 24
                )
                .padding(.top, 12 * scale)
            } else if let dutyHeadroom {
                PevDashboardProgressCard(
                    label: "Duty headroom",
                    value: percentText(dutyHeadroom),
                    detail: "Nose authority is the ride-critical value here.",
                    progress: Double(dutyHeadroom.value) / 100.0,
                    accent: PevColors.orange,
                    fill: PevColors.cardFill,
                    stroke: PevColors.cardStroke,
                    track: PevColors.cardStroke,
                    labelColor: PevColors.muted,
                    valueColor: PevColors.yellow,
                    detailColor: PevColors.muted,
                    scale: scale
                )
                    .padding(.top, 12 * scale)
            } else if liveSnapshot == nil && phase == .live {
                PevDashboardWarningCard(
                    title: "Telemetry pending",
                    detail: "Waiting for live values.",
                    accent: PevColors.purple,
                    detailColor: PevColors.primaryText,
                    fill: PevColors.purple.opacity(0.18),
                    stroke: PevColors.purple.opacity(0.55),
                    scale: scale,
                    cornerRadius: 24
                )
                    .padding(.top, 12 * scale)
            }

            if let warningCard {
                PevDashboardWarningCard(
                    title: warningCard.title,
                    detail: warningCard.detail,
                    accent: PevColors.purple,
                    detailColor: PevColors.primaryText,
                    fill: PevColors.purple.opacity(0.18),
                    stroke: PevColors.purple.opacity(0.55),
                    scale: scale,
                    cornerRadius: 24
                )
                    .padding(.top, 10 * scale)
            }

            if let footpad = liveSnapshot?.footpad {
                PevDashboardFootpadReadout(
                    leftLabel: "left / adc1",
                    leftValue: footpad.adc1DisplayText,
                    rightLabel: "right / adc2",
                    rightValue: footpad.adc2DisplayText,
                    detail: footpad.stateDisplayText,
                    accent: PevColors.cyan,
                    fill: PevColors.cardFill,
                    stroke: PevColors.cardStroke,
                    textColor: PevColors.primaryText,
                    secondaryTextColor: PevColors.muted,
                    scale: scale,
                    accessibilityValue: vescFootpadAccessibilityValue(footpad)
                )
                .padding(.top, 8 * scale)
            }

            PevDashboardGrid(columns: columns, spacing: 12 * scale) {
                ForEach(dashboardTiles) { tile in
                    PevDashboardMetricTile(tile, scale: scale, cornerRadius: 16, minHeight: 96)
                }
            }
            .padding(.top, 8 * scale)
        }
        .accessibilityElement(children: .contain)
        .onChange(of: liveSnapshot?.warning) { _, warning in
            if let announcement = warning?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
    }
}
