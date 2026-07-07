import CutoutMobile
import SwiftUI

struct EucGarageMockupView: View {
    let screen: MockupScreen
    let settingsReadback: SettingsReadback?
    let faultHistoryReadback: FaultHistoryReadback?
    let bmsSnapshot: BmsSnapshot?

    private var dashboardTiles: [MockupDashboardTile] {
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
        MockupScreenScaffold(sectionTitle: "EUC pack", bottomPadding: 24) { scale, columns in
            VStack(alignment: .leading, spacing: 8 * scale) {
                Text(screen.title)
                    .font(.system(size: 31 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                Text(screen.subtitle)
                    .font(.system(size: 14 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if let deviceCard = screen.deviceCard {
                EucDeviceStatusCard(card: deviceCard, scale: scale)
                    .padding(.top, 10 * scale)
            }

            LazyVGrid(columns: columns, spacing: 16 * scale) {
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
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 2 * scale)
            }

            if !screen.summaryRows.isEmpty {
                EucSummaryRows(rows: screen.summaryRows, scale: scale)
            }

            if let bmsSnapshot, bmsSnapshot.shouldRenderReadback {
                Text("Read-only pack health")
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 12 * scale)

                BmsReadbackRows(snapshot: bmsSnapshot, scale: scale)
            }

            if let settingsReadback, settingsReadback.shouldRender {
                Text("Read-only settings")
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 12 * scale)

                SettingsReadbackRows(readback: settingsReadback, scale: scale)
            }

            if let faultHistoryReadback, faultHistoryReadback.shouldRender {
                Text("Read-only fault history")
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 12 * scale)

                FaultHistoryReadbackRows(readback: faultHistoryReadback, scale: scale)
            }

            if let faultCard = screen.faultCard {
                Text(faultCard.title)
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 12 * scale)

                EucFaultStatusCard(card: faultCard, scale: scale)
            }
        }
    }

    private func settingsSpeedTile(
        tile: MockupDashboardTile,
        readback: ReadbackValue<Speed>
    ) -> MockupDashboardTile {
        guard let speed = readback.value else {
            return unavailableTile(tile, availability: readback.availability)
        }

        let readout = SpeedReadout(millimetersPerSecond: speed.value)
        return MockupDashboardTile(
            label: tile.label,
            value: readout.displayValue,
            unit: readout.displayUnit,
            detail: "read-only setting",
            accent: tile.accent
        )
    }

    private func settingsPedalTile(
        tile: MockupDashboardTile,
        readback: ReadbackValue<PedalMode>
    ) -> MockupDashboardTile {
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

        return MockupDashboardTile(
            label: tile.label,
            value: value,
            unit: unit,
            detail: "read-only setting",
            accent: tile.accent
        )
    }

    private func unavailableTile(
        _ tile: MockupDashboardTile,
        availability: ReadbackAvailability
    ) -> MockupDashboardTile {
        MockupDashboardTile(
            label: tile.label,
            value: "--",
            unit: tile.unit,
            detail: availability.displayText,
            accent: tile.accent
        )
    }
}

struct EucDeviceStatusCard: View {
    let card: MockupDeviceCard
    let scale: CGFloat

    var body: some View {
        HStack(alignment: .center, spacing: 12 * scale) {
            VStack(alignment: .leading, spacing: 8 * scale) {
                Text(card.title)
                    .font(.system(size: 22 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                Text(card.detail)
                    .font(.system(size: 13 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                    .lineLimit(1)
                    .minimumScaleFactor(0.62)
            }
            .layoutPriority(1)

            Text(card.status)
                .font(.system(size: 14 * scale, weight: .black))
                .foregroundStyle(.black)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .padding(.horizontal, 18 * scale)
                .frame(minWidth: 58 * scale, minHeight: 32 * scale)
                .background(Capsule().fill(card.accent.color))
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 104 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 26 * scale))
    }
}
