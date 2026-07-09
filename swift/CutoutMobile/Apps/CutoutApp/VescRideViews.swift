import CutoutMobile
import SwiftUI

struct VescRideScreenView: View {
    let liveSnapshot: VescRideSnapshot?
    let now: MonotonicMilliseconds
    let captureStatusText: String?
    let disconnect: () -> Void
    let selectScreen: (PevScreenID) -> Void

    private var title: String {
        liveSnapshot?.title ?? "Refloat"
    }

    private var subtitle: String {
        liveSnapshot?.screenSubtitle ?? "live"
    }

    private var speedParts: (value: String, unit: String) {
        guard let boardSpeed = liveSnapshot?.boardSpeed else {
            return ("--", "")
        }
        let readout = SpeedReadout(millimetersPerSecond: boardSpeed.value)
        return (readout.displayValue, readout.displayUnit)
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
        let details = [battery, liveSnapshot.batteryCurrent.map { "current \(currentText($0)) A" }]
            .compactMap { $0 }
        return details.joined(separator: " · ")
    }

    private var dashboardTiles: [PevDashboardTile] {
        guard let liveSnapshot else { return [] }
        return [
            PevDashboardTile(
                label: "voltage",
                value: voltageText(liveSnapshot.batteryVoltage),
                unit: "V",
                detail: batteryDetail,
                accent: .yellow
            ),
            PevDashboardTile(
                label: "motor current",
                value: phaseCurrentText(liveSnapshot.motorCurrent),
                unit: "A",
                detail: powerFlowDetail(liveSnapshot.powerFlow, fallback: "phase estimate"),
                accent: .orange
            ),
            PevDashboardTile(
                label: "board angle",
                value: angleText(liveSnapshot.boardAngle),
                unit: "°",
                detail: boardAngleDetail ?? "board angle",
                accent: .cyan
            ),
            PevDashboardTile(
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
        if angle.value < 0 {
            return "nose down"
        }
        if angle.value > 0 {
            return "nose up"
        }
        return "level"
    }

    var body: some View {
        PevRideDashboardShell(
            sectionTitle: "Ride",
            headerLeadingAccessory: { scale in AnyView(PevRideDisconnectButton(scale: scale, action: disconnect)) },
            title: title,
            subtitle: subtitle,
            statusFill: PevColors.purple,
            captureStatusText: captureStatusText,
            speedValue: speedParts.value,
            speedUnit: speedParts.unit,
            speedCaption: "board speed",
            allowsVerticalScroll: false,
            topLeadingAccessory: { _ in EmptyView() }
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
            } else if liveSnapshot == nil {
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
                    scale: scale
                )
                .padding(.top, 8 * scale)
            }

            PevDashboardGrid(columns: columns, spacing: 12 * scale) {
                ForEach(dashboardTiles) { tile in
                    PevDashboardMetricTile(tile, scale: scale, cornerRadius: 16, minHeight: 96)
                }
            }
            .padding(.top, 8 * scale)

            PevDashboardTabStrip(
                tabs: PevRideTabs.vescRideTabs(),
                scale: scale,
                selectedColor: PevColors.purple,
                unselectedColor: PevColors.muted,
                selectScreen: selectScreen
            )
                .padding(.top, 24 * scale)
        }
    }
}
