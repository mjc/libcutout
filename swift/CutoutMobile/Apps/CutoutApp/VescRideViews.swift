import CutoutMobile
import SwiftUI

struct VescOnewheelRideMockupView: View {
    let screen: MockupScreen
    let liveSnapshot: VescRideSnapshot?
    let allowsFixtureFallback: Bool
    let disconnect: () -> Void

    private var title: String {
        liveSnapshot?.title ?? (allowsFixtureFallback ? screen.title : "VESC Onewheel")
    }

    private var subtitle: String {
        liveSnapshot.map { "\($0.vehicleKind.displayName) · \($0.subProtocol.displayName)" }
            ?? (allowsFixtureFallback ? screen.subtitle : "VESC · live")
    }

    private var speedText: String {
        liveSnapshot?.boardSpeed.map { SpeedReadout(millimetersPerSecond: $0.value).displayValue }
            ?? (allowsFixtureFallback ? screen.primaryValue : "--")
    }

    private var speedUnit: String {
        liveSnapshot?.boardSpeed == nil && !allowsFixtureFallback ? "" : "mph"
    }

    private var safetyBars: [MockupSafetyBar] {
        guard let liveSnapshot else {
            return allowsFixtureFallback ? screen.safetyBars : unavailableSafetyBars(from: screen.safetyBars)
        }
        guard let dutyHeadroom = liveSnapshot.displayedDutyHeadroom else {
            return unavailableSafetyBars(from: screen.safetyBars)
        }
        return [
            MockupSafetyBar(
                label: "Duty headroom",
                value: percentText(dutyHeadroom),
                progress: Double(dutyHeadroom.value) / 100.0,
                accent: .orange
            ),
        ]
    }

    private var warningCard: MockupWarningCard? {
        guard let liveSnapshot else {
            return allowsFixtureFallback
                ? screen.warningCard
                : MockupWarningCard(title: "Telemetry pending", detail: "Waiting for live VESC values.")
        }
        switch liveSnapshot.warning {
        case .pushbackSoon:
            return MockupWarningCard(title: "Pushback soon", detail: footpadText ?? "Live VESC/ReFloat warning.")
        case .none, .unknown:
            return nil
        }
    }

    private var footpadText: String? {
        guard let footpad = liveSnapshot?.footpad else { return nil }
        let hasLeft = footpad.adc1Milliunits != nil
        let hasRight = footpad.adc2Milliunits != nil
        guard let values = footpadValues else { return "footpad \(footpad.state)" }
        switch (hasLeft, hasRight) {
        case (true, true):
            return "footpad \(footpad.state) · L \(values.left) / R \(values.right)"
        case (true, false):
            return "footpad \(footpad.state) · L \(values.left)"
        case (false, true):
            return "footpad \(footpad.state) · R \(values.right)"
        case (false, false):
            return "footpad \(footpad.state)"
        }
    }

    private var footpadValues: (left: String, right: String, detail: String)? {
        guard let footpad = liveSnapshot?.footpad else {
            return nil
        }
        let left = footpad.adc1Milliunits.map(formatMilliunits) ?? "--"
        let right = footpad.adc2Milliunits.map(formatMilliunits) ?? "--"
        return (left, right, "state \(footpad.state)")
    }

    private var dashboardTiles: [MockupDashboardTile] {
        guard let liveSnapshot else {
            return allowsFixtureFallback ? screen.dashboardTiles : unavailableDashboardTiles(from: screen.dashboardTiles)
        }
        return screen.dashboardTiles.map { tile in
            switch tile.kind {
            case .batteryCurrent:
                return tile.replacing(
                    label: "battery voltage",
                    value: voltageText(liveSnapshot.batteryVoltage),
                    unit: "V",
                    detail: batteryCurrentDetail(liveSnapshot.batteryCurrent)
                )
            case .motorCurrent:
                return tile.replacing(
                    value: phaseCurrentText(liveSnapshot.motorCurrent),
                    detail: energyFlowText(liveSnapshot.powerFlow) ?? (liveSnapshot.motorCurrent == nil ? "unavailable" : "live VESC")
                )
            case .boardAngle:
                return tile.replacing(
                    value: angleText(liveSnapshot.boardAngle),
                    detail: liveSnapshot.boardAngle == nil ? "unavailable" : "live pitch"
                )
            case .controller:
                return tile.replacing(
                    value: temperatureText(liveSnapshot.controllerTemperature),
                    detail: liveSnapshot.motorTemperature.map { "motor \(temperatureText($0)) \(RideUnits.temperatureUnit)" } ?? "motor unavailable"
                )
            default:
                return tile
            }
        }
    }

    var body: some View {
        MockupScreenScaffold(
            sectionTitle: "OW ride",
            bottomPadding: 20,
            allowsVerticalScroll: false,
            columnSpacing: 12
        ) { scale, columns in
            HStack(alignment: .firstTextBaseline) {
                if allowsFixtureFallback {
                    Text("CutOut")
                        .font(.system(size: 18 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.yellow)
                } else {
                    Button("Disconnect", action: disconnect)
                        .font(.system(size: 18 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.yellow)
                }
                Spacer()
            }

            HStack(alignment: .center, spacing: 12 * scale) {
                Text(title)
                    .font(.system(size: 18 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.primaryText)
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                Spacer(minLength: 8 * scale)
                VescArmedBadge(title: subtitle, scale: scale)
            }
            .padding(.top, 8 * scale)

            VStack(alignment: .center, spacing: 2 * scale) {
                HStack(alignment: .firstTextBaseline, spacing: 9 * scale) {
                    Text(speedText)
                        .font(.system(size: 104 * scale, weight: .black))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.72)
                    if !speedUnit.isEmpty {
                        Text(speedUnit)
                            .font(.system(size: 27 * scale, weight: .bold))
                            .foregroundStyle(MockupColors.muted)
                    }
                }
                Text(screen.secondaryValue)
                    .font(.system(size: 13 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
            }
            .frame(maxWidth: .infinity)
            .foregroundStyle(MockupColors.primaryText)

            if let duty = safetyBars.first {
                PevDashboardProgressCard(
                    label: duty.label,
                    value: duty.value,
                    detail: "Nose authority is the ride-critical value here.",
                    progress: duty.progress,
                    accent: duty.accent.color,
                    fill: MockupColors.cardFill,
                    stroke: MockupColors.cardStroke,
                    track: MockupColors.cardStroke,
                    labelColor: MockupColors.muted,
                    valueColor: MockupColors.yellow,
                    detailColor: MockupColors.muted,
                    scale: scale
                )
                    .padding(.top, 16 * scale)
            }

            if let warningCard {
                PevDashboardWarningCard(
                    title: warningCard.title,
                    detail: warningCard.detail,
                    accent: MockupColors.purple,
                    detailColor: MockupColors.primaryText,
                    fill: MockupColors.purple.opacity(0.18),
                    stroke: MockupColors.purple.opacity(0.55),
                    scale: scale,
                    cornerRadius: 24
                )
                    .padding(.top, 14 * scale)
            }

            if let footpadValues {
                PevDashboardFootpadReadout(
                    leftValue: footpadValues.left,
                    rightValue: footpadValues.right,
                    detail: footpadValues.detail,
                    accent: MockupColors.cyan,
                    fill: MockupColors.cardFill,
                    stroke: MockupColors.cardStroke,
                    textColor: MockupColors.primaryText,
                    secondaryTextColor: MockupColors.muted,
                    scale: scale
                )
                .padding(.top, 12 * scale)
            }

            LazyVGrid(columns: columns, spacing: 12 * scale) {
                ForEach(dashboardTiles) { tile in
                    PevDashboardMetricTile(
                        label: tile.label,
                        value: tile.value,
                        unit: tile.unit,
                        detail: tile.detail,
                        accent: tile.accent.color,
                        scale: scale,
                        cornerRadius: 16,
                        minHeight: 104
                    )
                }
            }
            .padding(.top, 12 * scale)

            EucRideTabs(tabs: screen.tabs, scale: scale)
                .padding(.top, 48 * scale)
        }
    }
}

private func formatMilliunits(_ value: Int32) -> String {
    String(format: "%.2f", Double(value) / 1_000)
}

struct VescArmedBadge: View {
    let title: String
    let scale: CGFloat

    var body: some View {
        PevDashboardStatusPill(
            title: title,
            scale: scale,
            fill: MockupColors.purple,
            fontSize: 13,
            horizontalPadding: 14,
            height: 31,
            fixedHorizontal: true
        )
    }
}
