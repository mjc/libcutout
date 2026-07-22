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
            PevDashboardTile(
                kind: .dutyCycle,
                label: localizedAppText("vesc.debug.metric.duty"),
                value: snapshot.dutyCycle.map { RideUnits.percentText(abs(Int($0.permille)) / 10) } ?? "--",
                unit: "%",
                detail: localizedAppText("vesc.debug.detail.motor_duty"),
                accent: .orange
            ),
            PevDashboardTile(
                kind: .headroom,
                label: localizedAppText("vesc.debug.metric.headroom"),
                value: percentText(snapshot.dutyHeadroom),
                unit: "%",
                detail: localizedAppText("vesc.debug.detail.remaining_duty"),
                accent: .yellow
            ),
            PevDashboardTile(
                kind: .boardAngle,
                label: localizedAppText("vesc.debug.metric.board"),
                value: angleText(snapshot.boardAngle),
                unit: "°",
                detail: localizedAppText("vesc.debug.detail.balance", angleText(snapshot.balanceAngle)),
                accent: .cyan
            ),
            PevDashboardTile(
                kind: .controller,
                label: localizedAppText("vesc.debug.metric.controller"),
                value: temperatureText(snapshot.controllerTemperature),
                unit: "°C",
                detail: localizedAppText("vesc.debug.detail.motor_temperature", temperatureText(snapshot.motorTemperature)),
                accent: .green
            ),
        ]
    }

    private var rows: [PevDashboardKeyValueRow] {
        guard let snapshot else {
            return [PevDashboardKeyValueRow(id: "phase", label: localizedAppText("vesc.debug.row.session"), value: phase.displayText)]
        }
        return [
            PevDashboardKeyValueRow(id: "phase", label: localizedAppText("vesc.debug.row.session"), value: phase.displayText),
            PevDashboardKeyValueRow(id: "protocol", label: localizedAppText("vesc.debug.row.protocol"), value: String(describing: snapshot.subProtocol)),
            PevDashboardKeyValueRow(id: "state", label: localizedAppText("vesc.debug.row.state"), value: String(describing: snapshot.operatingState)),
            PevDashboardKeyValueRow(id: "notifications", label: localizedAppText("vesc.debug.row.notifications"), value: String(notificationCount)),
            PevDashboardKeyValueRow(id: "voltage", label: localizedAppText("vesc.debug.row.pack_voltage"), value: localizedAppText("vesc.debug.value.voltage", voltageText(snapshot.batteryVoltage))),
            PevDashboardKeyValueRow(id: "battery-current", label: localizedAppText("vesc.debug.row.battery_current"), value: localizedAppText("vesc.debug.value.current", currentText(snapshot.batteryCurrent))),
            PevDashboardKeyValueRow(id: "motor-current", label: localizedAppText("vesc.debug.row.motor_current"), value: localizedAppText("vesc.debug.value.current", phaseCurrentText(snapshot.motorCurrent))),
            PevDashboardKeyValueRow(id: "footpad", label: localizedAppText("vesc.debug.row.footpad"), value: snapshot.footpad?.stateDisplayText ?? "--"),
        ]
    }

    var body: some View {
        PevDashboardScaffold(sectionTitle: localizedAppText("vesc.debug.section"), bottomPadding: 20, showsHeader: false) {
            PevScreenTitleBlock(
                title: snapshot?.title ?? localizedAppText("vesc.debug.title"),
                subtitle: captureStatusText ?? phase.displayText
            )

            PevDashboardGrid(spacing: 20) {
                ForEach(tiles) { tile in
                    PevDashboardMetricTile(tile, cornerRadius: 16, minHeight: 104)
                }
            }
            .padding(.top, 8)

            PevDashboardKeyValueRows(
                rows: rows,
                fill: PevColors.cardFill,
                stroke: PevColors.cardStroke,
                labelColor: PevColors.muted,
                valueColor: PevColors.primaryText
            )
        }
        .accessibilityElement(children: .contain)
    }
}
