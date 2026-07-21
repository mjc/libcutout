import CutoutMobile
import SwiftUI

struct EucGarageScreenView: View {
    let screen: PevScreen
    let settingsReadback: SettingsReadback?
    let faultHistoryReadback: FaultHistoryReadback?
    let bmsSnapshot: BmsSnapshot?

    private var dashboardTiles: [PevDashboardTile] {
        let settings = settingsReadback?.eucGarageSettings ?? EucGarageSettingsSnapshot()
        return [
            settingsSpeedTile(
                tile: PevDashboardTile(
                    kind: .beepMargin,
                    label: "beep margin",
                    value: "--",
                    unit: "mph",
                    detail: "read-only setting",
                    accent: .yellow
                ),
                readback: settings.beepMargin
            ),
            settingsSpeedTile(
                tile: PevDashboardTile(
                    kind: .tiltback,
                    label: "tiltback",
                    value: "--",
                    unit: "mph",
                    detail: "read-only setting",
                    accent: .orange
                ),
                readback: settings.tiltback
            ),
            settingsPedalTile(
                tile: PevDashboardTile(
                    kind: .pedalMode,
                    label: "pedal mode",
                    value: "--",
                    unit: "%",
                    detail: "read-only setting",
                    accent: .purple
                ),
                readback: settings.pedalMode
            )
        ]
    }

    var body: some View {
        PevDashboardScaffold(sectionTitle: "EUC pack", bottomPadding: 24, showsHeader: false) { columns in
            PevScreenTitleBlock(
                title: screen.title,
                subtitle: screen.subtitle
            )

            PevDashboardGrid(columns: columns, spacing: 16) {
                ForEach(dashboardTiles) { tile in
                    PevDashboardMetricTile(
                        label: tile.label,
                        value: tile.value,
                        unit: tile.unit,
                        detail: tile.detail,
                        accent: tile.accent.color,
                        cornerRadius: 16,
                        minHeight: 104
                    )
                }
            }
            .padding(.top, 6)

            if let bmsSnapshot, bmsSnapshot.shouldRenderReadback {
                Text("Read-only pack health")
                    .font(.headline)
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 12)
                    .accessibilityHeading(.h2)

                PevDashboardKeyValueRows(
                    rows: bmsSnapshot.readbackRows
                        .filter { $0.label != "page" && $0.label != "page verification" }
                        .map { row in
                            PevDashboardKeyValueRow(id: row.label, label: row.label, value: row.value)
                        },
                    fill: PevColors.cardFill,
                    stroke: PevColors.cardStroke,
                    labelColor: PevColors.muted,
                    valueColor: PevColors.primaryText,
                    verticalPadding: 6
                )
            }

            if let settingsReadback, settingsReadback.shouldRender {
                Text("Read-only settings")
                    .font(.headline)
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 12)
                    .accessibilityHeading(.h2)

                SettingsReadbackRows(readback: settingsReadback)
            }

            if let faultHistoryReadback, faultHistoryReadback.shouldRender {
                Text("Read-only fault history")
                    .font(.headline)
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 12)
                    .accessibilityHeading(.h2)

                FaultHistoryReadbackRows(readback: faultHistoryReadback)
            }

        }
    }

    private func settingsSpeedTile(
        tile: PevDashboardTile,
        readback: ReadbackValue<Speed>
    ) -> PevDashboardTile {
        guard let speed = readback.value else {
            return unavailableTile(tile, availability: readback.availability)
        }

        let readout = SpeedReadout(millimetersPerSecond: speed.value)
        return PevDashboardTile(
            kind: tile.kind,
            label: tile.label,
            value: readout.displayValue,
            unit: readout.displayUnit,
            detail: "read-only setting",
            accent: tile.accent
        )
    }

    private func settingsPedalTile(
        tile: PevDashboardTile,
        readback: ReadbackValue<PedalMode>
    ) -> PevDashboardTile {
        guard let mode = readback.value else {
            return unavailableTile(tile, availability: readback.availability)
        }

        let value: String
        let unit: String
        switch mode.value {
        case let .hardnessPercent(percent):
            value = "\(percent)"
            unit = "%"
        case let .rawMode(rawMode):
            value = "\(rawMode)"
            unit = "raw"
        }

        return PevDashboardTile(
            kind: tile.kind,
            label: tile.label,
            value: value,
            unit: unit,
            detail: "read-only setting",
            accent: tile.accent
        )
    }

    private func unavailableTile(
        _ tile: PevDashboardTile,
        availability: ReadbackAvailability
    ) -> PevDashboardTile {
        PevDashboardTile(
            kind: tile.kind,
            label: tile.label,
            value: "--",
            unit: tile.unit,
            detail: availability.displayText,
            accent: tile.accent
        )
    }
}
