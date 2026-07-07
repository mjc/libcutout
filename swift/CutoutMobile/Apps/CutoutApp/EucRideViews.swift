import CutoutMobile
import SwiftUI

struct EucRideScreenView: View {
    let screen: MockupScreen
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let captureStatusText: String?
    let disconnect: () -> Void
    let selectScreen: (MockupScreenID) -> Void

    private var speedParts: (value: String, unit: String) {
        if let rideState {
            return (rideState.speedText, rideState.speedUnit)
        }
        let parts = screen.primaryValue.split(separator: " ", maxSplits: 1).map(String.init)
        return (parts.first ?? screen.primaryValue, parts.dropFirst().first ?? "")
    }

    private var phaseText: String {
        rideState?.statusText ?? screen.displaySubtitle
    }

    private var titleText: String {
        rideTitle ?? screen.title
    }

    private var warningCard: MockupWarningCard? {
        if let warningState {
            return MockupWarningCard(title: warningState.title, detail: warningState.detail)
        }
        return screen.warningCard
    }

    private var warningSeverity: EucRideWarningSeverity {
        warningState?.severity ?? .reduceAcceleration
    }

    private var warningState: EucRideWarningState? {
        guard let rideState else {
            return nil
        }
        guard let now = rideState.displayState.lastUpdate else {
            return rideState.warningState
        }
        return rideState.warningState(at: now, staleAfter: MonotonicMilliseconds(2_000))
    }

    private var safetyBars: [MockupSafetyBar] {
        if let rideState {
            if rideState.telemetry != nil {
                return liveSafetyBars(for: rideState)
            }
            return unavailableSafetyBars(from: screen.safetyBars)
        }
        return screen.safetyBars
    }

    private var dashboardTiles: [MockupDashboardTile] {
        if let rideState {
            if let telemetry = rideState.telemetry {
                return liveDashboardTiles(from: rideState, telemetry: telemetry)
            }
            return unavailableDashboardTiles(from: screen.dashboardTiles)
        }
        return screen.dashboardTiles
    }

    var body: some View {
        MockupScreenScaffold(
            sectionTitle: "EUC ride",
            bottomPadding: 20,
            allowsVerticalScroll: false,
            columnSpacing: 12
        ) { scale, columns in
            HStack(alignment: .firstTextBaseline) {
                if rideState == nil {
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
                Text(titleText)
                    .font(.system(size: 18 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.primaryText)
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                Spacer(minLength: 8 * scale)
                PevDashboardStatusPill(
                    title: phaseText,
                    scale: scale,
                    fill: MockupColors.green
                )
            }
            .padding(.top, 8 * scale)

            if let captureStatusText {
                CaptureStatusPill(text: captureStatusText, scale: scale)
            }

            VStack(alignment: .center, spacing: 2 * scale) {
                HStack(alignment: .firstTextBaseline, spacing: 9 * scale) {
                    Text(speedParts.value)
                        .font(.system(size: 104 * scale, weight: .black))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.72)
                    Text(speedParts.unit)
                        .font(.system(size: 27 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.muted)
                }
                Text("speed")
                    .font(.system(size: 13 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
            }
            .frame(maxWidth: .infinity)
            .foregroundStyle(MockupColors.primaryText)

            VStack(spacing: 10 * scale) {
                ForEach(safetyBars, id: \.label) { bar in
                    PevDashboardProgressBar(
                        label: bar.label,
                        value: bar.value,
                        progress: bar.progress,
                        accent: bar.accent.color,
                        track: MockupColors.cardFill,
                        labelColor: MockupColors.muted,
                        valueColor: bar.accent.color,
                        scale: scale
                    )
                }
            }

            if let warningCard {
                PevDashboardWarningCard(
                    title: warningCard.title,
                    detail: warningCard.detail,
                    accent: eucWarningAccent(for: warningSeverity),
                    detailColor: MockupColors.warningText,
                    fill: MockupColors.warningFill,
                    stroke: MockupColors.warningStroke,
                    scale: scale,
                    height: 76
                )
                    .padding(.top, 14 * scale)
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

            EucRideTabs(tabs: screen.tabs, scale: scale, selectScreen: selectScreen)
                .padding(.top, 48 * scale)
        }
    }
}

struct EucSummaryRows: View {
    let rows: [MockupSummaryRow]
    let scale: CGFloat

    var body: some View {
        PevDashboardKeyValueRows(
            rows: rows.map { row in
                PevDashboardKeyValueRow(
                    id: row.id,
                    label: row.label,
                    value: row.value,
                    valueColor: row.accent?.color
                )
            },
            scale: scale,
            fill: MockupColors.cardFill,
            stroke: MockupColors.cardStroke,
            labelColor: MockupColors.muted,
            valueColor: MockupColors.primaryText
        )
    }
}

struct EucFaultStatusCard: View {
    let card: MockupFaultCard
    let scale: CGFloat

    var body: some View {
        MockupFaultDetailCard(
            card: card,
            scale: scale,
            fontSize: 15,
            horizontalAlignment: .leading,
            horizontalPadding: 22,
            height: 54,
            cornerRadius: 19
        )
    }
}

struct MockupFaultDetailCard: View {
    let card: MockupFaultCard
    let scale: CGFloat
    let fontSize: CGFloat
    let horizontalAlignment: Alignment
    let horizontalPadding: CGFloat
    let height: CGFloat
    let cornerRadius: CGFloat
    var minimumScaleFactor: CGFloat = 1

    var body: some View {
        Text(card.detail)
            .font(.system(size: fontSize * scale, weight: .black))
            .foregroundStyle(card.accent.color)
            .lineLimit(1)
            .minimumScaleFactor(minimumScaleFactor)
            .frame(maxWidth: .infinity, alignment: horizontalAlignment)
            .padding(.horizontal, horizontalPadding * scale)
            .frame(height: height * scale)
            .background(CardBackground(cornerRadius: cornerRadius * scale))
    }
}

struct EucRideTabs: View {
    let tabs: [MockupScreenTab]
    let scale: CGFloat
    var selectScreen: ((MockupScreenID) -> Void)? = nil

    var body: some View {
        VStack(spacing: 12 * scale) {
            Rectangle()
                .fill(MockupColors.cardStroke)
                .frame(width: 254 * scale, height: 1)

            HStack(spacing: 0) {
                ForEach(tabs) { tab in
                    tabContent(tab)
                }
            }
            .frame(width: 254 * scale)
        }
        .frame(height: 58 * scale, alignment: .top)
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private func tabContent(_ tab: MockupScreenTab) -> some View {
        if let destination = tab.destinationScreenID, let selectScreen {
            Button {
                selectScreen(destination)
            } label: {
                tabLabel(tab)
            }
            .buttonStyle(.plain)
        } else {
            tabLabel(tab)
        }
    }

    private func tabLabel(_ tab: MockupScreenTab) -> some View {
        PevDashboardTabLabel(
            title: tab.title,
            isSelected: tab.isSelected,
            scale: scale,
            selectedColor: MockupColors.yellow,
            unselectedColor: MockupColors.muted
        )
        .frame(maxWidth: .infinity)
    }
}

private func eucWarningAccent(for severity: EucRideWarningSeverity) -> Color {
    switch severity {
    case .normal:
        MockupColors.green
    case .caution, .reduceAcceleration:
        MockupColors.orange
    case .limpHome, .failed:
        MockupColors.red
    case .unavailable:
        MockupColors.muted
    }
}
