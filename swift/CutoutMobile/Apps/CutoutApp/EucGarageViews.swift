import CutoutMobile
import SwiftUI

struct EucGarageScreenView: View {
    let screen: PevScreen
    let settingsReadback: SettingsReadback?
    let faultHistoryReadback: FaultHistoryReadback?
    let bmsSnapshot: BmsSnapshot?

    private var dashboardTiles: [PevDashboardTile] {
        guard let settingsReadback else {
            return screen.dashboardTiles
        }

        let settings = settingsReadback.eucGarageSettings
        return screen.dashboardTiles.map { tile in
            switch tile.kind {
            case .beepMargin:
                return settingsSpeedTile(tile: tile, readback: settings.beepMargin)
            case .tiltback:
                return settingsSpeedTile(tile: tile, readback: settings.tiltback)
            case .pedalMode:
                return settingsPedalTile(tile: tile, readback: settings.pedalMode)
            case .metric, .batteryCurrent, .motorCurrent, .boardAngle, .controller:
                return tile
            }
        }
    }

    var body: some View {
        PevDashboardScaffold(sectionTitle: "EUC pack", bottomPadding: 24, showsHeader: false) { scale, columns in
            PevScreenTitleBlock(
                title: screen.title,
                subtitle: screen.subtitle,
                scale: scale,
                titleFontSize: 31,
                subtitleFontSize: 14,
                titleMinimumScaleFactor: 0.76,
                subtitleLineLimit: 2
            )

            if let deviceCard = screen.deviceCard {
                PevDashboardIdentityCard(
                    title: deviceCard.title,
                    detail: deviceCard.detail,
                    scale: scale,
                    titleFontSize: 22,
                    detailFontSize: 13,
                    titleMinimumScaleFactor: 0.75,
                    detailMinimumScaleFactor: 0.62,
                    trailingStatus: deviceCard.status,
                    trailingStatusFill: deviceCard.accent.color,
                    trailingStatusForeground: .black,
                    trailingStatusWidth: 18,
                    trailingStatusHeight: 32,
                    cornerRadius: 26,
                    height: 104
                )
                    .padding(.top, 10 * scale)
            }

            PevDashboardGrid(columns: columns, spacing: 16 * scale) {
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
            .padding(.top, 6 * scale)

            if let summaryTitle = screen.summaryTitle {
                Text(summaryTitle)
                    .font(.system(size: 18 * scale, weight: .black))
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 2 * scale)
            }

            if !screen.summaryRows.isEmpty {
                PevDashboardKeyValueRows(
                    rows: screen.summaryRows.map { row in
                        PevDashboardKeyValueRow(
                            id: row.id,
                            label: row.label,
                            value: row.value,
                            valueColor: row.accent?.color
                        )
                    },
                    scale: scale,
                    fill: PevColors.cardFill,
                    stroke: PevColors.cardStroke,
                    labelColor: PevColors.muted,
                    valueColor: PevColors.primaryText
                )
            }

            if let bmsSnapshot, bmsSnapshot.shouldRenderReadback {
                Text("Read-only pack health")
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 12 * scale)

                PevDashboardKeyValueRows(
                    rows: bmsSnapshot.readbackRows
                        .filter { $0.label != "page" && $0.label != "page verification" }
                        .enumerated()
                        .map { offset, row in
                            PevDashboardKeyValueRow(id: "\(offset)-\(row.label)", label: row.label, value: row.value)
                        },
                    scale: scale,
                    fill: PevColors.cardFill,
                    stroke: PevColors.cardStroke,
                    labelColor: PevColors.muted,
                    valueColor: PevColors.primaryText,
                    verticalPadding: 6
                )
            }

            if let settingsReadback, settingsReadback.shouldRender {
                Text("Read-only settings")
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 12 * scale)

                SettingsReadbackRows(readback: settingsReadback, scale: scale)
            }

            if let faultHistoryReadback, faultHistoryReadback.shouldRender {
                Text("Read-only fault history")
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 12 * scale)

                FaultHistoryReadbackRows(readback: faultHistoryReadback, scale: scale)
            }

            if let faultCard = screen.faultCard {
                Text(faultCard.title)
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.top, 12 * scale)

                PevDashboardFaultDetailCard(
                    detail: faultCard.detail,
                    accent: faultCard.accent.color,
                    scale: scale,
                    fontSize: 13,
                    horizontalAlignment: .center,
                    horizontalPadding: 20,
                    height: 57,
                    cornerRadius: 18,
                    minimumScaleFactor: 0.72
                )
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
            label: tile.label,
            value: "--",
            unit: tile.unit,
            detail: availability.displayText,
            accent: tile.accent
        )
    }
}
