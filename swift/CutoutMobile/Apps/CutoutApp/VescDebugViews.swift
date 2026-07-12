import CutoutMobile
import SwiftUI

struct VescDebugScreenView: View {
    let snapshot: VescRideSnapshot?
    let phase: SessionConnectionPhase
    let notificationCount: UInt64
    let captureStatusText: String?

    private var tiles: [PevDashboardTile] {
        guard let snapshot else { return [] }
        return [
            PevDashboardTile(label: "duty", value: snapshot.dutyCycle.map { RideUnits.percentText(abs(Int($0.permille)) / 10) } ?? "--", unit: "%", detail: "motor duty cycle", accent: .orange),
            PevDashboardTile(label: "headroom", value: percentText(snapshot.dutyHeadroom), unit: "%", detail: "remaining duty", accent: .yellow),
            PevDashboardTile(label: "board", value: angleText(snapshot.boardAngle), unit: "°", detail: "balance \(angleText(snapshot.balanceAngle))°", accent: .cyan),
            PevDashboardTile(label: "controller", value: temperatureText(snapshot.controllerTemperature), unit: "°C", detail: "motor \(temperatureText(snapshot.motorTemperature)) °C", accent: .green),
        ]
    }

    private var rows: [PevDashboardKeyValueRow] {
        guard let snapshot else {
            return [PevDashboardKeyValueRow(id: "phase", label: "Session", value: phase.displayText)]
        }
        return [
            PevDashboardKeyValueRow(id: "phase", label: "Session", value: phase.displayText),
            PevDashboardKeyValueRow(id: "protocol", label: "Protocol", value: String(describing: snapshot.subProtocol)),
            PevDashboardKeyValueRow(id: "state", label: "State", value: String(describing: snapshot.operatingState)),
            PevDashboardKeyValueRow(id: "notifications", label: "Notifications", value: String(notificationCount)),
            PevDashboardKeyValueRow(id: "voltage", label: "Pack voltage", value: "\(voltageText(snapshot.batteryVoltage)) V"),
            PevDashboardKeyValueRow(id: "battery-current", label: "Battery current", value: "\(currentText(snapshot.batteryCurrent)) A"),
            PevDashboardKeyValueRow(id: "motor-current", label: "Motor current", value: "\(phaseCurrentText(snapshot.motorCurrent)) A"),
            PevDashboardKeyValueRow(id: "footpad", label: "Footpad", value: snapshot.footpad?.stateDisplayText ?? "--"),
        ]
    }

    var body: some View {
        PevDashboardScaffold(sectionTitle: "VESC debug", bottomPadding: 20, showsHeader: false) { scale, columns in
            PevScreenTitleBlock(
                title: snapshot?.title ?? "VESC Debug",
                subtitle: captureStatusText ?? phase.displayText,
                scale: scale,
                titleFontSize: 29,
                subtitleFontSize: 14,
                titleMinimumScaleFactor: 0.8,
                subtitleLineLimit: 2
            )

            PevDashboardGrid(columns: columns, spacing: 20 * scale) {
                ForEach(tiles) { tile in
                    PevDashboardMetricTile(tile, scale: scale, cornerRadius: 16, minHeight: 104)
                }
            }
            .padding(.top, 8 * scale)

            PevDashboardKeyValueRows(
                rows: rows,
                scale: scale,
                fill: PevColors.cardFill,
                stroke: PevColors.cardStroke,
                labelColor: PevColors.muted,
                valueColor: PevColors.primaryText
            )
        }
    }
}
