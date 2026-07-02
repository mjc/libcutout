import Foundation
import CutoutMobile
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: LiveSpeedModel
    @State private var selectedScreenID: MockupScreenID
    @State private var pairedDestinationScreenID: MockupScreenID?

    private let catalog = MockupScreenCatalog.v2

    init(model: LiveSpeedModel) {
        self.model = model
        _selectedScreenID = State(initialValue: Self.initialScreenID())
    }

    var body: some View {
        ZStack {
            MockupColors.pageBackground
                .ignoresSafeArea()

            TabView(selection: $selectedScreenID) {
                ForEach(catalog.screens) { screen in
                    MockupScreenContainer(
                        screen: screen,
                        devicePickerScanState: model.devicePickerScanState,
                        rideState: model.selectedRideTitle == nil && model.phase == .starting && model.displayState.notificationCount == 0
                            ? nil
                            : model.rideState,
                        rideTitle: model.selectedRideTitle,
                        disconnect: {
                            model.disconnectAndSearch()
                            pairedDestinationScreenID = nil
                            selectedScreenID = .devicePicker
                        },
                        pair: pair,
                        selectScreen: { selectedScreenID = $0 }
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .tag(screen.id)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(MockupColors.pageBackground.ignoresSafeArea())
        .onChange(of: model.phase) { _, phase in
            openRideScreen(ifNeededFor: phase)
        }
    }

    private func pair(_ row: MockupPickerRow) {
        guard model.pair(platformIdentifier: row.id) else { return }
        guard let screenID = Self.destinationScreenID(for: row) else { return }
        pairedDestinationScreenID = screenID
        selectedScreenID = screenID
    }

    private func openRideScreen(ifNeededFor phase: LiveSpeedConnectionPhase) {
        guard phase.opensRideScreen else { return }
        selectedScreenID = pairedDestinationScreenID ?? .eucRide
    }

    private static func initialScreenID() -> MockupScreenID {
        let arguments = CommandLine.arguments
        if let index = arguments.firstIndex(of: "--mockup-screen"),
           arguments.indices.contains(index + 1),
           let id = MockupScreenID(rawValue: arguments[index + 1]) {
            return id
        }

        if let value = ProcessInfo.processInfo.environment["CUTOUT_MOCKUP_SCREEN"],
           let id = MockupScreenID(rawValue: value) {
            return id
        }

        return .devicePicker
    }

    private static func destinationScreenID(for row: MockupPickerRow) -> MockupScreenID? {
        row.connectionRoute?.destinationScreenID
    }
}

private struct MockupScreenContainer: View {
    let screen: MockupScreen
    let devicePickerScanState: DevicePickerScanState?
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let disconnect: () -> Void
    let pair: (MockupPickerRow) -> Void
    let selectScreen: (MockupScreenID) -> Void

    var body: some View {
        switch screen.id {
        case .devicePicker:
            DevicePickerMockupView(screen: screen, scanState: devicePickerScanState, pair: pair)
        case .eucRide:
            EucRideMockupView(screen: screen, rideState: rideState, rideTitle: rideTitle, disconnect: disconnect)
        case .eucGarage:
            EucGarageMockupView(screen: screen)
        case .vescOnewheelRide:
            VescOnewheelRideMockupView(screen: screen)
        case .vescDebug:
            VescDebugMockupView(screen: screen)
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData:
            BmsMockupView(screen: screen, selectScreen: selectScreen)
        default:
            GenericMockupView(screen: screen, liveSpeed: liveSpeed)
        }
    }
}

private struct EucGarageMockupView: View {
    let screen: MockupScreen

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
                ForEach(screen.dashboardTiles) { tile in
                    EucDashboardTile(tile: tile, scale: scale)
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

            if let faultCard = screen.faultCard {
                Text(faultCard.title)
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 12 * scale)

                EucFaultStatusCard(card: faultCard, scale: scale)
            }
        }
    }
}

private struct MockupScreenScaffold<Content: View>: View {
    let sectionTitle: String
    let bottomPadding: CGFloat
    let allowsVerticalScroll: Bool
    let columnSpacing: CGFloat
    let contentSpacing: CGFloat
    let horizontalPadding: CGFloat
    private let content: (CGFloat, [GridItem]) -> Content

    init(
        sectionTitle: String,
        bottomPadding: CGFloat,
        allowsVerticalScroll: Bool = true,
        columnSpacing: CGFloat = 26,
        contentSpacing: CGFloat = 16,
        horizontalPadding: CGFloat = 24,
        @ViewBuilder content: @escaping (CGFloat, [GridItem]) -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.bottomPadding = bottomPadding
        self.allowsVerticalScroll = allowsVerticalScroll
        self.columnSpacing = columnSpacing
        self.contentSpacing = contentSpacing
        self.horizontalPadding = horizontalPadding
        self.content = content
    }

    var body: some View {
        GeometryReader { proxy in
            let scale = min(proxy.size.width / 390.0, proxy.size.height / 844.0)
            let columns = [
                GridItem(.flexible(), spacing: columnSpacing * scale),
                GridItem(.flexible(), spacing: columnSpacing * scale),
            ]

            Group {
                if allowsVerticalScroll {
                    ScrollView(.vertical, showsIndicators: false) {
                        scaffoldContent(scale: scale, columns: columns, width: proxy.size.width)
                    }
                } else {
                    scaffoldContent(scale: scale, columns: columns, width: proxy.size.width)
                        .frame(height: proxy.size.height, alignment: .topLeading)
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height, alignment: .top)
            .background(MockupColors.pageBackground)
            .foregroundStyle(MockupColors.primaryText)
        }
    }

    private func scaffoldContent(scale: CGFloat, columns: [GridItem], width: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: contentSpacing * scale) {
            MockupScreenHeader(sectionTitle: sectionTitle, scale: scale)

            content(scale, columns)
        }
        .padding(.horizontal, horizontalPadding * scale)
        .padding(.bottom, bottomPadding * scale)
        .frame(width: width, alignment: .topLeading)
    }
}

private struct MockupScreenHeader: View {
    let sectionTitle: String
    let scale: CGFloat

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text("CutOut")
                .font(.system(size: 18 * scale, weight: .bold))
                .foregroundStyle(MockupColors.yellow)
            Spacer()
            Text(sectionTitle)
                .font(.system(size: 15 * scale, weight: .semibold))
                .foregroundStyle(MockupColors.muted)
        }
        .padding(.top, 10 * scale)
    }
}

private struct EucDeviceStatusCard: View {
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

private struct VescDebugMockupView: View {
    let screen: MockupScreen

    var body: some View {
        MockupScreenScaffold(sectionTitle: "VESC debug", bottomPadding: 20) { scale, columns in
            VStack(alignment: .leading, spacing: 8 * scale) {
                Text(screen.title)
                    .font(.system(size: 29 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                Text(screen.subtitle)
                    .font(.system(size: 14 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                    .lineLimit(2)
            }

            if let profile = screen.deviceCard {
                VescProfileCard(card: profile, scale: scale)
                    .padding(.top, 10 * scale)
            }

            LazyVGrid(columns: columns, spacing: 20 * scale) {
                ForEach(screen.dashboardTiles) { tile in
                    EucDashboardTile(tile: tile, scale: scale)
                }
            }
            .padding(.top, 8 * scale)

            if let summaryTitle = screen.summaryTitle {
                Text(summaryTitle)
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 0)
            }

            if !screen.summaryRows.isEmpty {
                EucSummaryRows(rows: screen.summaryRows, scale: scale)
            }

            if let guardrail = screen.faultCard {
                Text(guardrail.title)
                    .font(.system(size: 16 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.top, 6 * scale)

                VescGuardrailCard(card: guardrail, scale: scale)
            }
        }
    }
}

private struct VescProfileCard: View {
    let card: MockupDeviceCard
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            Text(card.title)
                .font(.system(size: 22 * scale, weight: .black))
                .foregroundStyle(MockupColors.primaryText)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(card.detail)
                .font(.system(size: 13 * scale, weight: .bold))
                .foregroundStyle(MockupColors.muted)
                .lineLimit(1)
                .minimumScaleFactor(0.72)
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 87 * scale, alignment: .center)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(CardBackground(cornerRadius: 25 * scale))
    }
}

private struct VescGuardrailCard: View {
    let card: MockupFaultCard
    let scale: CGFloat

    var body: some View {
        MockupFaultDetailCard(
            card: card,
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

private struct VescOnewheelRideMockupView: View {
    let screen: MockupScreen

    var body: some View {
        MockupScreenScaffold(sectionTitle: "OW ride", bottomPadding: 20, allowsVerticalScroll: false) { scale, columns in
            HStack(alignment: .center, spacing: 12 * scale) {
                Text(screen.title)
                    .font(.system(size: 18 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.primaryText)
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                Spacer(minLength: 8 * scale)
                VescArmedBadge(title: screen.subtitle, scale: scale)
            }

            VStack(alignment: .center, spacing: 2 * scale) {
                HStack(alignment: .firstTextBaseline, spacing: 9 * scale) {
                    Text(screen.primaryValue)
                        .font(.system(size: 92 * scale, weight: .black))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.75)
                    Text("mph")
                        .font(.system(size: 22 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.muted)
                }
                Text(screen.secondaryValue)
                    .font(.system(size: 13 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
            }
            .frame(maxWidth: .infinity)
            .foregroundStyle(MockupColors.primaryText)

            if let duty = screen.safetyBars.first {
                VescDutyHeadroomCard(bar: duty, scale: scale)
                    .padding(.top, 16 * scale)
            }

            if let warningCard = screen.warningCard {
                VescPushbackWarningCard(card: warningCard, scale: scale)
                    .padding(.top, 6 * scale)
            }

            LazyVGrid(columns: columns, spacing: 20 * scale) {
                ForEach(screen.dashboardTiles) { tile in
                    EucDashboardTile(tile: tile, scale: scale)
                }
            }
            .padding(.top, 6 * scale)

            EucRideTabs(tabs: screen.tabs, scale: scale)
                .padding(.top, 14 * scale)
        }
    }
}

private struct VescArmedBadge: View {
    let title: String
    let scale: CGFloat

    var body: some View {
        Text(title)
            .font(.system(size: 13 * scale, weight: .black))
            .foregroundStyle(.black)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .padding(.horizontal, 14 * scale)
            .frame(height: 31 * scale)
            .background(Capsule().fill(MockupColors.purple))
    }
}

private struct VescDutyHeadroomCard: View {
    let bar: MockupSafetyBar
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 11 * scale) {
            HStack(alignment: .firstTextBaseline) {
                Text(bar.label)
                    .font(.system(size: 14 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                Spacer()
                Text(bar.value)
                    .font(.system(size: 25 * scale, weight: .black))
                    .foregroundStyle(MockupColors.yellow)
                    .monospacedDigit()
            }

            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(MockupColors.cardStroke)
                    Capsule()
                        .fill(bar.accent.color)
                        .frame(width: max(0, min(1, bar.progress)) * proxy.size.width)
                }
            }
            .frame(height: 17 * scale)

            Text("Nose authority is the ride-critical value here.")
                .font(.system(size: 13 * scale, weight: .bold))
                .foregroundStyle(MockupColors.muted)
                .lineLimit(2)
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 17 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 25 * scale))
    }
}

private struct VescPushbackWarningCard: View {
    let card: MockupWarningCard
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            Text(card.title)
                .font(.system(size: 20 * scale, weight: .black))
                .foregroundStyle(MockupColors.purple)
            Text(card.detail)
                .font(.system(size: 13 * scale, weight: .bold))
                .foregroundStyle(MockupColors.primaryText)
                .lineLimit(2)
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 16 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 24 * scale, style: .continuous)
                .fill(MockupColors.purple.opacity(0.18))
                .overlay(
                    RoundedRectangle(cornerRadius: 24 * scale, style: .continuous)
                        .stroke(MockupColors.purple.opacity(0.55), lineWidth: 1)
                )
        )
    }
}

private struct EucRideMockupView: View {
    let screen: MockupScreen
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let disconnect: () -> Void

    private var speedParts: (value: String, unit: String) {
        if let rideState {
            return (rideState.speedText, rideState.speedUnit)
        }
        let parts = screen.primaryValue.split(separator: " ", maxSplits: 1).map(String.init)
        return (parts.first ?? screen.primaryValue, parts.dropFirst().first ?? "")
    }

    private var phaseText: String {
        rideState?.phaseText ?? screen.displaySubtitle
    }

    private var titleText: String {
        rideTitle ?? screen.title
    }

    private var warningCard: MockupWarningCard? {
        if let rideState {
            return liveWarningCard(for: rideState)
        }
        return screen.warningCard
    }

    private var safetyBars: [MockupSafetyBar] {
        if let rideState {
            if let telemetry = rideState.telemetry {
                return liveSafetyBars(from: telemetry)
            }
            return unavailableSafetyBars(from: screen.safetyBars)
        }
        return screen.safetyBars
    }

    private var dashboardTiles: [MockupDashboardTile] {
        if let rideState {
            if let telemetry = rideState.telemetry {
                return liveDashboardTiles(from: telemetry)
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
                EucStatusBadge(title: phaseText, scale: scale)
            }
            .padding(.top, 8 * scale)

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
                    EucSafetyBar(bar: bar, scale: scale)
                }
            }

            if let warningCard {
                EucWarningCard(card: warningCard, scale: scale)
                    .padding(.top, 14 * scale)
            }

            LazyVGrid(columns: columns, spacing: 12 * scale) {
                ForEach(dashboardTiles) { tile in
                    EucDashboardTile(tile: tile, scale: scale)
                }
            }
            .padding(.top, 12 * scale)

            EucRideTabs(tabs: screen.tabs, scale: scale)
                .padding(.top, 48 * scale)
        }
    }
}

private struct EucStatusBadge: View {
    let title: String
    let scale: CGFloat

    var body: some View {
        Text(title)
            .font(.system(size: 14 * scale, weight: .black))
            .foregroundStyle(.black)
            .lineLimit(1)
            .minimumScaleFactor(0.75)
            .padding(.horizontal, 12 * scale)
            .frame(height: 30 * scale)
            .background(Capsule().fill(MockupColors.green))
    }
}

private struct EucSafetyBar: View {
    let bar: MockupSafetyBar
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 7 * scale) {
            HStack {
                Text(bar.label)
                    .font(.system(size: 15 * scale, weight: .semibold))
                    .foregroundStyle(MockupColors.muted)
                Spacer()
                Text(bar.value)
                    .font(.system(size: 14 * scale, weight: .black))
                    .foregroundStyle(bar.accent.color)
                    .monospacedDigit()
            }

            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(MockupColors.cardFill)
                    Capsule()
                        .fill(bar.accent.color)
                        .frame(width: max(0, min(1, bar.progress)) * proxy.size.width)
                }
            }
            .frame(height: 17 * scale)
        }
    }
}

private struct EucWarningCard: View {
    let card: MockupWarningCard
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 5 * scale) {
            Text(card.title)
                .font(.system(size: 20 * scale, weight: .black))
                .foregroundStyle(MockupColors.orange)
            Text(card.detail)
                .font(.system(size: 13 * scale, weight: .black))
                .foregroundStyle(MockupColors.warningText)
        }
        .padding(.horizontal, 22 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: 76 * scale)
        .background(
            RoundedRectangle(cornerRadius: 23 * scale, style: .continuous)
                .fill(MockupColors.warningFill)
                .overlay(
                    RoundedRectangle(cornerRadius: 23 * scale, style: .continuous)
                        .stroke(MockupColors.warningStroke, lineWidth: 1)
                )
        )
    }
}

private struct EucDashboardTile: View {
    let tile: MockupDashboardTile
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            Text(tile.label)
                .font(.system(size: 13 * scale, weight: .bold))
                .foregroundStyle(MockupColors.muted)

            HStack(alignment: .firstTextBaseline, spacing: 4 * scale) {
                Text(tile.value)
                    .font(.system(size: 31 * scale, weight: .black))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.72)
                Spacer(minLength: 4 * scale)
                Text(tile.unit)
                    .font(.system(size: 15 * scale, weight: .black))
                    .foregroundStyle(tile.accent.color)
            }
            VStack(alignment: .leading, spacing: 4 * scale) {
                Text(tile.detail)
                    .font(.system(size: 12 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                    .lineLimit(1)
                    .minimumScaleFactor(0.68)
            }
        }
        .padding(.horizontal, 16 * scale)
        .padding(.vertical, 14 * scale)
        .frame(maxWidth: .infinity, minHeight: 104 * scale, alignment: .topLeading)
        .background(CardBackground(cornerRadius: 16 * scale))
    }
}

private struct EucSummaryRows: View {
    let rows: [MockupSummaryRow]
    let scale: CGFloat

    var body: some View {
        VStack(spacing: 0) {
            ForEach(rows) { row in
                HStack {
                    Text(row.label)
                        .font(.system(size: 14 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.muted)
                    Spacer()
                    Text(row.value)
                        .font(.system(size: 15 * scale, weight: .black))
                        .monospacedDigit()
                        .foregroundStyle(row.accent?.color ?? MockupColors.primaryText)
                }
                .frame(height: 31 * scale)

                if row.id != rows.last?.id {
                    Rectangle()
                        .fill(MockupColors.cardStroke)
                        .frame(height: 1)
                }
            }
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 12 * scale)
        .background(CardBackground(cornerRadius: 22 * scale))
    }
}

private struct EucFaultStatusCard: View {
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

private struct MockupFaultDetailCard: View {
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

private struct EucRideTabs: View {
    let tabs: [MockupScreenTab]
    let scale: CGFloat

    var body: some View {
        VStack(spacing: 12 * scale) {
            Rectangle()
                .fill(MockupColors.cardStroke)
                .frame(width: 254 * scale, height: 1)

            HStack(spacing: 0) {
                ForEach(tabs) { tab in
                    VStack(spacing: 8 * scale) {
                        Text(tab.title)
                            .font(.system(size: 14 * scale, weight: tab.isSelected ? .black : .semibold))
                            .foregroundStyle(tab.isSelected ? MockupColors.yellow : MockupColors.muted)
                            .lineLimit(1)
                            .minimumScaleFactor(0.8)
                        Capsule()
                            .fill(tab.isSelected ? MockupColors.yellow : Color.clear)
                            .frame(width: 28 * scale, height: 4 * scale)
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .frame(width: 254 * scale)
        }
        .frame(height: 58 * scale, alignment: .top)
        .frame(maxWidth: .infinity)
    }
}

private struct DevicePickerMockupView: View {
    let screen: MockupScreen
    let scanState: DevicePickerScanState?
    let pair: (MockupPickerRow) -> Void

    private var renderedScanState: DevicePickerScanState {
        scanState ?? DevicePickerScanState(status: .scanning, rows: screen.pickerRows)
    }

    private var sections: MockupPickerSections {
        renderedScanState.sections
    }

    var body: some View {
        MockupScreenScaffold(
            sectionTitle: "setup",
            bottomPadding: 24,
            allowsVerticalScroll: false,
            contentSpacing: 18,
            horizontalPadding: 18
        ) { scale, _ in
            VStack(alignment: .leading, spacing: 7 * scale) {
                Text("Pick your device(s)")
                    .font(.system(size: 34 * scale, weight: .bold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.78)
                Text("Nearby devices that look rideable. Pair supported ones.")
                    .font(.system(size: 15 * scale, weight: .semibold))
                    .foregroundStyle(MockupColors.muted)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }

            ScanStatusPill(
                text: renderedScanState.statusText,
                isScanning: renderedScanState.status == .scanning,
                scale: scale
            )
                .padding(.top, 4 * scale)

            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 18 * scale) {
                    if !sections.supported.isEmpty {
                        SectionLabel("Supported now", scale: scale)
                            .padding(.top, 8 * scale)
                        VStack(spacing: 12 * scale) {
                            ForEach(sections.supported) { row in
                                Button {
                                    pair(row)
                                } label: {
                                    PickerDeviceRow(row: row, scale: scale)
                                }
                                .buttonStyle(.plain)
                                .contentShape(Rectangle())
                            }
                        }
                    }

                    if !sections.unsupported.isEmpty {
                        SectionLabel("Looks like a PEV, unsupported for launch", scale: scale)
                            .padding(.top, 8 * scale)
                        VStack(spacing: 12 * scale) {
                            ForEach(sections.unsupported) { row in
                                PickerDeviceRow(row: row, scale: scale)
                            }
                        }
                    }

                    if let manualRow = sections.manual {
                        ManualPickerRow(row: manualRow, scale: scale)
                            .padding(.top, 32 * scale)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .foregroundStyle(.white)
    }
}

private struct ScanStatusPill: View {
    let text: String
    let isScanning: Bool
    let scale: CGFloat
    @State private var phase = 0

    var body: some View {
        HStack {
            Text(text)
                .font(.system(size: 18 * scale, weight: .bold))
            Spacer()
            HStack(spacing: 9 * scale) {
                ForEach(0..<3, id: \.self) { index in
                    Circle()
                        .frame(width: 13 * scale, height: 13 * scale)
                        .opacity(!isScanning || index == phase ? 1 : 0.32)
                }
            }
            .foregroundStyle(MockupColors.yellow)
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 64 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 28 * scale))
        .task(id: isScanning) {
            guard isScanning else { return }
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(260))
                phase = (phase + 1) % 3
            }
        }
    }
}

private struct SectionLabel: View {
    let title: String
    let scale: CGFloat

    init(_ title: String, scale: CGFloat) {
        self.title = title
        self.scale = scale
    }

    var body: some View {
        Text(title)
            .font(.system(size: 15 * scale, weight: .semibold))
            .foregroundStyle(MockupColors.muted)
    }
}

private struct PickerDeviceRow: View {
    let row: MockupPickerRow
    let scale: CGFloat

    var body: some View {
        HStack(spacing: 14 * scale) {
            DeviceGlyph(row: row)
                .frame(width: 56 * scale, height: 56 * scale)

            VStack(alignment: .leading, spacing: 4 * scale) {
                Text(row.title)
                    .font(.system(size: 20 * scale, weight: .bold))
                    .foregroundStyle(row.titleColor)
                    .lineLimit(1)
                    .minimumScaleFactor(0.68)
                Text(row.subtitle)
                    .font(.system(size: 11.5 * scale, weight: .semibold))
                    .foregroundStyle(row.secondaryTextColor)
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
                Text(row.detail)
                    .font(.system(size: 12.5 * scale, weight: .bold))
                    .foregroundStyle(row.secondaryTextColor)
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
            }
            .layoutPriority(1)

            Spacer(minLength: 6 * scale)

            ActionBadge(state: row.state, scale: scale)
        }
        .padding(.horizontal, 18 * scale)
        .frame(height: 92 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 26 * scale))
    }
}

private struct ManualPickerRow: View {
    let row: MockupPickerRow
    let scale: CGFloat

    var body: some View {
        HStack {
            Text(row.title)
                .font(.system(size: 15 * scale, weight: .semibold))
                .foregroundStyle(MockupColors.muted)
                .lineLimit(1)
                .minimumScaleFactor(0.7)
            Spacer()
            ActionBadge(state: row.state, scale: scale)
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 64 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 24 * scale))
        .padding(.top, 2 * scale)
    }
}

private struct ActionBadge: View {
    let state: MockupPickerRowState
    let scale: CGFloat

    var body: some View {
        Text(state.actionTitle)
            .font(.system(size: 15 * scale, weight: .bold))
            .foregroundStyle(state.isSupported ? .black : MockupColors.muted)
            .frame(width: state.isSupported ? 76 * scale : 64 * scale)
            .frame(height: state.isSupported ? 38 * scale : 30 * scale)
            .background(
                Capsule()
                    .fill(state.isSupported ? MockupColors.yellow : MockupColors.disabledFill)
            )
            .overlay(
                Capsule()
                    .stroke(MockupColors.cardStroke, lineWidth: state.isSupported ? 0 : 1)
            )
    }
}

private struct DeviceGlyph: View {
    let row: MockupPickerRow

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            let line = max(2, side * 0.08)

            ZStack {
                switch row.title {
                case "Aero-126V":
                    EucGlyph(color: row.glyphColor, lineWidth: line)
                case "Little FOCer BT":
                    OnewheelGlyph(color: row.glyphColor, accent: MockupColors.purple, lineWidth: line)
                case "NINEBOT-7A31":
                    ScooterGlyph(color: row.glyphColor, lineWidth: line)
                case "HX Hoverboard":
                    HoverboardGlyph(color: row.glyphColor, lineWidth: line)
                default:
                    Circle()
                        .fill(row.glyphBackground)
                    Image(systemName: row.symbolName)
                        .font(.system(size: side * 0.42, weight: .bold))
                        .foregroundStyle(row.glyphColor)
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
    }
}

private struct EucGlyph: View {
    let color: Color
    let lineWidth: CGFloat

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            ZStack {
                Circle()
                    .fill(MockupColors.iconFill)
                Circle()
                    .stroke(color, lineWidth: lineWidth)
                Circle()
                    .fill(MockupColors.cardFill)
                    .frame(width: side * 0.42, height: side * 0.42)
                ForEach(0..<8, id: \.self) { index in
                    Circle()
                        .fill(color)
                        .frame(width: side * 0.085, height: side * 0.085)
                        .offset(y: -side * 0.16)
                        .rotationEffect(.degrees(Double(index) * 45))
                }
                Circle()
                    .fill(color)
                    .frame(width: side * 0.10, height: side * 0.10)
            }
        }
    }
}

private struct OnewheelGlyph: View {
    let color: Color
    let accent: Color
    let lineWidth: CGFloat

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            ZStack {
                Capsule()
                    .stroke(accent, lineWidth: lineWidth * 0.8)
                    .frame(width: side * 0.92, height: side * 0.34)
                Circle()
                    .fill(MockupColors.iconFill)
                    .frame(width: side * 0.46, height: side * 0.46)
                Circle()
                    .stroke(color, lineWidth: lineWidth)
                    .frame(width: side * 0.46, height: side * 0.46)
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
    }
}

private struct ScooterGlyph: View {
    let color: Color
    let lineWidth: CGFloat

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            ZStack {
                Circle()
                    .stroke(color, lineWidth: lineWidth * 0.85)
                    .frame(width: side * 0.23, height: side * 0.23)
                    .offset(x: -side * 0.30, y: side * 0.22)
                Circle()
                    .stroke(color, lineWidth: lineWidth * 0.85)
                    .frame(width: side * 0.23, height: side * 0.23)
                    .offset(x: side * 0.32, y: side * 0.22)
                Path { path in
                    path.move(to: CGPoint(x: side * 0.20, y: side * 0.29))
                    path.addLine(to: CGPoint(x: side * 0.48, y: side * 0.74))
                    path.addLine(to: CGPoint(x: side * 0.75, y: side * 0.74))
                    path.move(to: CGPoint(x: side * 0.48, y: side * 0.74))
                    path.addLine(to: CGPoint(x: side * 0.26, y: side * 0.74))
                    path.move(to: CGPoint(x: side * 0.20, y: side * 0.29))
                    path.addLine(to: CGPoint(x: side * 0.34, y: side * 0.29))
                }
                .stroke(color, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round, lineJoin: .round))
                .frame(width: side, height: side)
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
    }
}

private struct HoverboardGlyph: View {
    let color: Color
    let lineWidth: CGFloat

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            ZStack {
                Capsule()
                    .stroke(color, lineWidth: lineWidth)
                    .frame(width: side * 0.56, height: side * 0.26)
                    .offset(x: -side * 0.18)
                Capsule()
                    .stroke(color, lineWidth: lineWidth)
                    .frame(width: side * 0.56, height: side * 0.26)
                    .offset(x: side * 0.18)
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
    }
}

private struct CardBackground: View {
    let cornerRadius: CGFloat

    init(cornerRadius: CGFloat = 22) {
        self.cornerRadius = cornerRadius
    }

    var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(MockupColors.cardFill)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(MockupColors.cardStroke, lineWidth: 1)
            )
    }
}

private extension MockupPickerRow {
    var glyphColor: Color {
        switch title {
        case "NINEBOT-7A31":
            MockupColors.teal
        case "HX Hoverboard":
            MockupColors.brown
        default:
            MockupColors.yellow
        }
    }

    var glyphBackground: Color {
        glyphColor.opacity(isSupported ? 0.12 : 0.16)
    }

    var titleColor: Color {
        isSupported ? MockupColors.primaryText : MockupColors.disabledText
    }

    var secondaryTextColor: Color {
        isSupported ? MockupColors.muted : MockupColors.disabledSecondaryText
    }
}

private enum MockupColors {
    static let pageBackground = Color(red: 0.027, green: 0.031, blue: 0.043)
    static let cardFill = Color(red: 0.067, green: 0.078, blue: 0.106)
    static let cardStroke = Color(red: 0.165, green: 0.188, blue: 0.239)
    static let disabledFill = Color(red: 0.067, green: 0.078, blue: 0.106)
    static let primaryText = Color(red: 0.969, green: 0.953, blue: 0.918)
    static let disabledText = Color(red: 0.455, green: 0.475, blue: 0.514)
    static let disabledSecondaryText = Color(red: 0.36, green: 0.38, blue: 0.42)
    static let muted = Color(red: 0.561, green: 0.596, blue: 0.659)
    static let yellow = Color(red: 1.0, green: 0.827, blue: 0.302)
    static let cyan = Color(red: 0.278, green: 0.824, blue: 0.933)
    static let green = Color(red: 0.376, green: 0.906, blue: 0.553)
    static let orange = Color(red: 1.0, green: 0.486, blue: 0.188)
    static let warningText = Color(red: 1.0, green: 0.667, blue: 0.345)
    static let warningFill = Color(red: 0.173, green: 0.087, blue: 0.040)
    static let warningStroke = Color(red: 0.443, green: 0.216, blue: 0.102)
    static let teal = Color(red: 0.180, green: 0.384, blue: 0.459)
    static let brown = Color(red: 0.443, green: 0.259, blue: 0.141)
    static let purple = Color(red: 0.635, green: 0.459, blue: 0.918)
    static let iconFill = Color(red: 0.043, green: 0.051, blue: 0.071)
}

private extension MockupAccent {
    var color: Color {
        switch self {
        case .cyan:
            MockupColors.cyan
        case .green:
            MockupColors.green
        case .orange:
            MockupColors.orange
        case .purple:
            MockupColors.purple
        case .yellow:
            MockupColors.yellow
        }
    }
}

private extension MockupPickerRowState {
    var actionTitle: String {
        switch self {
        case .supported(let action), .unsupported(let action), .manual(let action):
            action
        }
    }

    var isSupported: Bool {
        if case .supported = self { true } else { false }
    }
}

private func liveWarningCard(for state: EucRideScreenState) -> MockupWarningCard {
    switch state.phase {
    case .failed(let failure):
        MockupWarningCard(title: "Connection failed", detail: failure.displayText)
    case .live where state.telemetry != nil:
        MockupWarningCard(
            title: "Telemetry live",
            detail: state.telemetry?.speed?.provenanceText ?? "Live telemetry from typed Rust/FFI state"
        )
    case .live:
        MockupWarningCard(title: "Telemetry unavailable", detail: "No live snapshot yet")
    case .connecting, .discoveringServices, .subscribing:
        MockupWarningCard(title: state.phaseText, detail: "Waiting for live telemetry")
    case .starting, .bluetoothUnavailable, .scanning:
        MockupWarningCard(title: state.phaseText, detail: "Ride screen is not active yet")
    }
}

private func liveSafetyBars(from telemetry: TelemetrySnapshot) -> [MockupSafetyBar] {
    [
        telemetry.pwm.map { pwm in
            let usedPermille = min(1_000, abs(Int(pwm.value.permille)))
            let headroomPermille = max(0, 1_000 - usedPermille)
            return MockupSafetyBar(
                label: "PWM headroom",
                value: percentageString(fromPermille: headroomPermille),
                progress: Double(headroomPermille) / 1_000.0,
                accent: .yellow
            )
        } ?? MockupSafetyBar(label: "PWM headroom", value: "Unavailable", progress: 0, accent: .yellow),
        telemetry.batteryLevelEstimated.map { batteryLevel in
            MockupSafetyBar(
                label: "estimated energy",
                value: percentageString(fromPercent: Int(batteryLevel.value)),
                progress: Double(batteryLevel.value) / 100.0,
                accent: .cyan
            )
        } ?? MockupSafetyBar(label: "sag-adjusted energy", value: "Unavailable", progress: 0, accent: .cyan),
    ]
}

private func liveDashboardTiles(from telemetry: TelemetrySnapshot) -> [MockupDashboardTile] {
    [
        telemetry.voltage.map { voltage in
            MockupDashboardTile(
                label: "pack",
                value: decimalString(fromMillivolts: voltage.value, fractionDigits: 1),
                unit: "V",
                detail: voltage.provenanceText,
                accent: .cyan
            )
        } ?? MockupDashboardTile(label: "pack", value: "--", unit: "V", detail: "unavailable", accent: .cyan),
        livePowerTile(from: telemetry),
        (telemetry.controllerTemperature != nil || telemetry.motorTemperature != nil || telemetry.batteryTemperature != nil)
            ? MockupDashboardTile(
                label: "thermal",
                value: liveThermalValue(telemetry: telemetry),
                unit: "°C",
                detail: liveThermalDetail(telemetry: telemetry),
                accent: .green
            )
            : MockupDashboardTile(label: "thermal", value: "--", unit: "°C", detail: "unavailable", accent: .green),
        telemetry.batteryLevelEstimated.map { batteryLevel in
            MockupDashboardTile(
                label: "energy",
                value: percentageString(fromPercent: Int(batteryLevel.value)),
                unit: "%",
                detail: batteryLevel.provenanceText,
                accent: .cyan
            )
        } ?? MockupDashboardTile(label: "limp-home", value: "--", unit: "%", detail: "unavailable", accent: .cyan),
    ]
}

private func livePowerTile(from telemetry: TelemetrySnapshot) -> MockupDashboardTile {
    if let voltage = telemetry.voltage,
       let current = telemetry.batteryCurrent,
       current.value != 0 {
        let milliwatts = Int64(voltage.value) * Int64(current.value) / 1_000
        return MockupDashboardTile(
            label: "power",
            value: decimalString(
                fromMilliwatts: milliwatts,
                fractionDigits: powerFractionDigits(fromMilliwatts: milliwatts)
            ),
            unit: "kW",
            detail: "calculated from pack current",
            accent: .yellow
        )
    }

    if let power = telemetry.power {
        return MockupDashboardTile(
            label: "power",
            value: decimalString(
                fromMilliwatts: power.value,
                fractionDigits: powerFractionDigits(fromMilliwatts: power.value)
            ),
            unit: "kW",
            detail: power.provenanceText,
            accent: .yellow
        )
    }

    return MockupDashboardTile(label: "power", value: "--", unit: "kW", detail: "unavailable", accent: .yellow)
}

private func unavailableSafetyBars(from bars: [MockupSafetyBar]) -> [MockupSafetyBar] {
    bars.map {
        MockupSafetyBar(label: $0.label, value: "Unavailable", progress: 0, accent: $0.accent)
    }
}

private func unavailableDashboardTiles(from tiles: [MockupDashboardTile]) -> [MockupDashboardTile] {
    tiles.map {
        MockupDashboardTile(label: $0.label, value: "--", unit: $0.unit, detail: "unavailable", accent: $0.accent)
    }
}

private func liveThermalValue(telemetry: TelemetrySnapshot) -> String {
    let values = [telemetry.controllerTemperature, telemetry.motorTemperature, telemetry.batteryTemperature]
        .compactMap { $0?.value }
    guard let maxValue = values.max() else {
        return "--"
    }
    return decimalString(fromMillicelsius: maxValue, fractionDigits: 0)
}

private func liveThermalDetail(telemetry: TelemetrySnapshot) -> String {
    let parts = [
        telemetry.controllerTemperature.map { "ESC " + decimalString(fromMillicelsius: $0.value, fractionDigits: 0) },
        telemetry.motorTemperature.map { "motor " + decimalString(fromMillicelsius: $0.value, fractionDigits: 0) },
        telemetry.batteryTemperature.map { "battery " + decimalString(fromMillicelsius: $0.value, fractionDigits: 0) },
    ].compactMap { $0 }
    return parts.isEmpty ? "typed telemetry" : parts.joined(separator: " · ")
}

private func percentageString<T: BinaryInteger>(fromPercent percent: T) -> String {
    "\(percent)%"
}

private func percentageString<T: BinaryInteger>(fromPermille permille: T) -> String {
    "\(permille / 10)%"
}

private func decimalString<T: BinaryInteger>(fromMillivolts value: T, fractionDigits: Int) -> String {
    decimalString(Double(value) / 1_000.0, fractionDigits: fractionDigits)
}

private func decimalString<T: BinaryInteger>(fromMilliwatts value: T, fractionDigits: Int) -> String {
    decimalString(Double(value) / 1_000_000.0, fractionDigits: fractionDigits)
}

private func powerFractionDigits<T: BinaryInteger>(fromMilliwatts value: T) -> Int {
    abs(Int64(value)) < 1_000_000 ? 2 : 1
}

private func decimalString<T: BinaryInteger>(fromMillicelsius value: T, fractionDigits: Int) -> String {
    decimalString(Double(value) / 1_000.0, fractionDigits: fractionDigits)
}

private func decimalString(_ value: Double, fractionDigits: Int) -> String {
    String(format: "%.\(fractionDigits)f", value)
}

private struct GenericMockupView: View {
    let screen: MockupScreen
    let liveSpeed: String

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text(screen.title)
                    .font(.largeTitle.weight(.bold))
                Text(screen.subtitle)
                    .font(.headline)
                    .foregroundStyle(.secondary)
                Text(screen.primaryValue)
                    .font(.system(size: 58, weight: .bold, design: .rounded))
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
                Text(screen.secondaryValue)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(.secondary)

                if let warning = screen.warning {
                    Text(warning)
                        .font(.headline.weight(.semibold))
                        .foregroundStyle(.orange)
                }

                ForEach(screen.metrics, id: \.label) { metric in
                    HStack {
                        Text(metric.label).foregroundStyle(.secondary)
                        Spacer()
                        Text(metric.value).monospacedDigit()
                    }
                }

                Divider()
                HStack {
                    Text("Live speed").foregroundStyle(.secondary)
                    Spacer()
                    Text("\(liveSpeed) mph").monospacedDigit()
                }
            }
            .padding(24)
        }
        .background(Color.black)
        .foregroundStyle(.white)
    }
}

private struct BmsMockupView: View {
    let screen: MockupScreen
    let selectScreen: (MockupScreenID) -> Void

    private var content: MockupBmsContent {
        screen.bmsContent ?? MockupBmsContent(
            kind: .unknownTopology,
            snapshot: BmsSnapshot(
                topology: BmsTopology(
                    layoutLabel: "missing fixture",
                    seriesGroupCount: nil,
                    parallelCount: nil,
                    packCount: 0,
                    bmsCount: 0,
                    confidence: .unverified
                )
            )
        )
    }

    var body: some View {
        GeometryReader { proxy in
            let designWidth = min(proxy.size.width, 390)
            let scale = min(1, designWidth / 390.0, proxy.size.height / 844.0)

            Group {
                if content.kind == .noData {
                    BmsNoDataLayout(screen: screen, content: content, scale: scale)
                } else {
                    VStack(alignment: .leading, spacing: 14 * scale) {
                        header(scale: scale)
                        chipRow(scale: scale, contentWidth: designWidth - (46 * scale))
                        contentSection(scale: scale)
                        Spacer(minLength: 0)
                        bottomTabs(scale: scale)
                    }
                    .padding(.horizontal, 23 * scale)
                    .padding(.top, 31 * scale)
                    .padding(.bottom, 18 * scale)
                }
            }
            .frame(width: designWidth, height: proxy.size.height, alignment: .topLeading)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(MockupColors.pageBackground)
            .foregroundStyle(MockupColors.primaryText)
        }
    }

    @ViewBuilder
    private func contentSection(scale: CGFloat) -> some View {
        switch content.kind {
        case .overview:
            BmsOverviewLayout(content: content, scale: scale)
        case .cellMapInline:
            BmsInlineLayout(content: content, scale: scale)
        case .cellMapScrollable:
            BmsScrollableLayout(content: content, scale: scale)
        case .cellDetail:
            BmsDetailLayout(content: content, scale: scale)
        case .unknownTopology:
            BmsUnknownLayout(content: content, scale: scale)
        case .noData:
            EmptyView()
        }
    }

    private func header(scale: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: 2 * scale) {
            Text("CutOut · BMS")
                .font(.system(size: 15 * scale, weight: .medium))
                .foregroundStyle(MockupColors.muted)
            Text(screen.title)
                .font(.system(size: 32 * scale, weight: .black))
                .lineLimit(1)
                .minimumScaleFactor(0.82)
        }
    }

    private func chipRow(scale: CGFloat, contentWidth: CGFloat) -> some View {
        let chipWidths: [CGFloat?]
        if content.chips.count == 3 {
            let availableWidth = max(contentWidth - (20 * scale), 0)
            chipWidths = [availableWidth * 0.38, availableWidth * 0.22, availableWidth * 0.40]
        } else {
            chipWidths = Array(repeating: nil, count: content.chips.count)
        }

        return HStack(spacing: 10 * scale) {
            ForEach(Array(content.chips.enumerated()), id: \.element.id) { index, chip in
                BmsChip(
                    title: chip.title,
                    accent: chip.accent,
                    scale: scale,
                    maxWidth: chipWidths[index]
                )
            }
        }
    }

    private func bottomTabs(scale: CGFloat) -> some View {
        HStack {
            BmsBottomTab(title: "Ride", isSelected: false, scale: scale) {
                selectScreen(.eucRide)
            }
            Spacer()
            BmsBottomTab(title: "Pack", isSelected: false, scale: scale) {
                selectScreen(.bmsOverview)
            }
            Spacer()
            BmsBottomTab(
                title: "Cells",
                isSelected: true,
                scale: scale
            ) {
                selectScreen(.bmsCellMap40S)
            }
            Spacer()
            BmsBottomTab(title: "Faults", isSelected: false, scale: scale) {
                selectScreen(.bmsUnknownTopology)
            }
        }
        .padding(.horizontal, 18 * scale)
    }
}

private struct BmsOverviewLayout: View {
    let content: MockupBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 14 * scale) {
            BmsHeroCard(
                eyebrow: "usable energy",
                title: percentText(snapshot.energyPercent),
                trailing: "sag adjusted",
                detail: "15.8 mi confident · 21.4 mi gentle",
                accent: .yellow,
                scale: scale
            )

            HStack(spacing: 14 * scale) {
                BmsMetricCard(
                    title: "pack voltage",
                    value: voltageText(snapshot.voltage),
                    unit: "V",
                    detail: "4.08 V avg",
                    accent: .green,
                    scale: scale
                )
                BmsMetricCard(
                    title: "cell delta",
                    value: millivoltsText(snapshot.cellDeltaMillivolts),
                    unit: "mV",
                    detail: "balanced enough",
                    accent: .green,
                    scale: scale
                )
            }

            HStack(spacing: 14 * scale) {
                BmsMetricCard(
                    title: "lowest group",
                    value: groupVoltageText(snapshot, index: snapshot.lowestGroupIndex),
                    unit: "V",
                    detail: "group 17",
                    accent: .orange,
                    scale: scale
                )
                BmsMetricCard(
                    title: "highest temp",
                    value: temperatureText(snapshot.highestTemperature),
                    unit: "°C",
                    detail: snapshot.highestTemperatureLabel ?? "",
                    accent: .green,
                    scale: scale
                )
            }

            BmsWideCard(
                title: "balancing",
                value: snapshot.balancingSummary ?? "--",
                detail: snapshot.balancingDetail ?? "",
                accent: .orange,
                border: .normal,
                scale: scale
            )

            BmsWideCard(
                title: "fault state",
                value: snapshot.faultSummary ?? "--",
                detail: snapshot.faultDetail ?? "",
                accent: .orange,
                border: .critical,
                scale: scale
            )
        }
    }
}

private struct BmsInlineLayout: View {
    let content: MockupBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16 * scale) {
            BmsWideCard(
                title: "topology fits inline",
                value: "no scrolling needed",
                detail: "show every group with readable numbers",
                accent: .green,
                border: .normal,
                scale: scale
            )

            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 12 * scale), count: 3), spacing: 14 * scale) {
                ForEach(snapshot.groups) { group in
                    BmsGroupCell(
                        group: group,
                        isHighlighted: content.highlightedGroupIndices.contains(group.index),
                        isSelected: false,
                        scale: scale
                    )
                }
            }

            BmsWideCard(
                title: "range of interest",
                value: "12 mV spread",
                detail: "tiny packs can expose exact state immediately",
                accent: .cyan,
                border: .normal,
                scale: scale
            )

            VStack(alignment: .leading, spacing: 14 * scale) {
                Text("controls")
                    .font(.system(size: 15 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                HStack(spacing: 10 * scale) {
                    ForEach(content.modeTitles, id: \.self) { title in
                        BmsModeChip(title: title, isSelected: title == content.modeTitles.first, scale: scale)
                    }
                }
                Text("tap a cell for history, IR estimate, and BMS raw fields")
                    .font(.system(size: 13 * scale, weight: .semibold))
                    .foregroundStyle(MockupColors.muted)
            }
            .padding(.horizontal, 18 * scale)
            .padding(.vertical, 18 * scale)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(BmsCardBackground(cornerRadius: 24 * scale))
        }
    }
}

private struct BmsScrollableLayout: View {
    let content: MockupBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }

    private let columns = Array(repeating: GridItem(.fixed(31), spacing: 5), count: 10)

    var body: some View {
        VStack(alignment: .leading, spacing: 16 * scale) {
            BmsWideCard(
                title: "large packs use grouped overview first",
                value: "tap / horizontal-scroll for exact cells",
                detail: nil,
                accent: .cyan,
                border: .normal,
                scale: scale
            )

            LazyVGrid(columns: columns, spacing: 8 * scale) {
                ForEach(snapshot.groups) { group in
                    BmsStripCell(
                        group: group,
                        isHighlighted: content.highlightedGroupIndices.contains(group.index),
                        scale: scale
                    )
                }
            }

            BmsWideCard(
                title: "interesting groups",
                value: "17–19 sagging under load",
                detail: "31 has temp sensor mismatch",
                accent: .orange,
                border: .warning,
                scale: scale
            )

            VStack(alignment: .leading, spacing: 10 * scale) {
                Text("display modes")
                    .font(.system(size: 15 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                Text(content.modeTitles.joined(separator: " • "))
                    .font(.system(size: 19 * scale, weight: .black))
                    .lineLimit(2)
                    .minimumScaleFactor(0.8)
                Text("Rule: never render 40+ cells as a tiny unreadable grid.")
                    .font(.system(size: 13 * scale, weight: .semibold))
                    .foregroundStyle(MockupColors.muted)
                Text("Show anomalies first, raw table second.")
                    .font(.system(size: 13 * scale, weight: .black))
                    .foregroundStyle(MockupColors.yellow)
            }
            .padding(.horizontal, 18 * scale)
            .padding(.vertical, 18 * scale)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(BmsCardBackground(cornerRadius: 24 * scale))
        }
    }
}

private struct BmsDetailLayout: View {
    let content: MockupBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }
    private var selectedGroup: BmsGroupSnapshot? {
        snapshot.groups.first { $0.index == content.selectedGroupIndex }
    }

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 10), count: 5)

    var body: some View {
        VStack(alignment: .leading, spacing: 12 * scale) {
            LazyVGrid(columns: columns, spacing: 10 * scale) {
                ForEach(snapshot.groups) { group in
                    BmsGroupIndexCell(
                        group: group,
                        isSelected: group.index == content.selectedGroupIndex,
                        scale: scale
                    )
                }
            }

            if let selectedGroup {
                VStack(alignment: .leading, spacing: 15 * scale) {
                    Text("group \(selectedGroup.index)")
                        .font(.system(size: 15 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.muted)
                    Text(groupVoltageText(selectedGroup))
                        .font(.system(size: 58 * scale, weight: .black))
                        .monospacedDigit()
                    Text("lowest group · 18 mV below pack avg")
                        .font(.system(size: 14 * scale, weight: .black))
                        .foregroundStyle(MockupColors.orange)

                    HStack(spacing: 14 * scale) {
                        BmsMetricCard(
                            title: "temp",
                            value: temperatureText(selectedGroup.temperature),
                            unit: "°C",
                            detail: "",
                            accent: .green,
                            scale: scale
                        )
                        BmsMetricCard(
                            title: "IR est.",
                            value: selectedGroup.resistanceMilliohms.map(String.init) ?? "--",
                            unit: "mΩ",
                            detail: "",
                            accent: .green,
                            scale: scale
                        )
                    }

                    BmsWideCard(
                        title: nil,
                        value: "trend: \(selectedGroup.detail ?? "not enough history")",
                        detail: "actions: mark, compare neighbors, export raw sample",
                        accent: .yellow,
                        border: .normal,
                        scale: scale
                    )
                }
                .padding(.horizontal, 18 * scale)
                .padding(.vertical, 20 * scale)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(BmsOutlinedCardBackground(cornerRadius: 34 * scale, stroke: MockupColors.yellow))
            }
        }
    }
}

private struct BmsUnknownLayout: View {
    let content: MockupBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 16 * scale) {
            BmsWideCard(
                title: "do not pretend certainty",
                value: snapshot.faultSummary ?? "--",
                detail: snapshot.faultDetail ?? "",
                accent: .orange,
                border: .warning,
                scale: scale
            )

            HStack(spacing: 14 * scale) {
                BmsMetricCard(
                    title: "reported voltage",
                    value: voltageText(snapshot.voltage),
                    unit: "V",
                    detail: "source: BMS",
                    accent: .yellow,
                    scale: scale
                )
                BmsMetricCard(
                    title: "cell count",
                    value: "?",
                    unit: "",
                    detail: "advertised 18–24S?",
                    accent: .orange,
                    scale: scale
                )
            }

            HStack(spacing: 14 * scale) {
                BmsMetricCard(
                    title: "temps",
                    value: "3",
                    unit: "sensors",
                    detail: "names unknown",
                    accent: .green,
                    scale: scale
                )
                BmsMetricCard(
                    title: "fault bits",
                    value: snapshot.faults.first?.code ?? "--",
                    unit: "",
                    detail: snapshot.faults.first?.label ?? "",
                    accent: .orange,
                    scale: scale
                )
            }

            VStack(alignment: .leading, spacing: 10 * scale) {
                Text("next capture flow")
                    .font(.system(size: 15 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
                Text(snapshot.captureActionTitle ?? "--")
                    .font(.system(size: 25 * scale, weight: .black))
                    .lineLimit(1)
                    .minimumScaleFactor(0.84)
                Text("capture BLE services, characteristic samples, vendor strings")
                    .font(.system(size: 13 * scale, weight: .semibold))
                    .foregroundStyle(MockupColors.muted)
                Text(snapshot.captureActionState ?? "")
                    .font(.system(size: 15 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.primaryText.opacity(0.82))
                    .padding(.horizontal, 18 * scale)
                    .frame(height: 34 * scale)
                    .background(Capsule().fill(MockupColors.muted.opacity(0.33)))
            }
            .padding(.horizontal, 18 * scale)
            .padding(.vertical, 18 * scale)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(BmsCardBackground(cornerRadius: 24 * scale))
        }
    }
}

private struct BmsNoDataLayout: View {
    let screen: MockupScreen
    let content: MockupBmsContent
    let scale: CGFloat

    private var snapshot: BmsSnapshot { content.snapshot }

    var body: some View {
        VStack(alignment: .leading, spacing: 14 * scale) {
            header

            warningCard

            packEstimateCard

            telemetryCard

            unknownsCard

            ridingRuleCard
        }
        .padding(.horizontal, 24 * scale)
        .padding(.top, 44 * scale)
        .padding(.bottom, 20 * scale)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(MockupColors.pageBackground)
        .foregroundStyle(MockupColors.primaryText)
    }

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 6 * scale) {
                Text(screen.title)
                    .font(.system(size: 24 * scale, weight: .bold))
                Text(screen.subtitle)
                    .font(.system(size: 11 * scale, weight: .medium))
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                    .foregroundStyle(MockupColors.muted)
            }

            Spacer(minLength: 12 * scale)

            HStack(spacing: 10 * scale) {
                Circle()
                    .fill(MockupColors.yellow)
                    .frame(width: 10 * scale, height: 10 * scale)
                Text("limited data")
                    .font(.system(size: 11 * scale, weight: .medium))
                    .foregroundStyle(MockupColors.primaryText.opacity(0.92))
            }
            .padding(.horizontal, 12 * scale)
            .frame(height: 30 * scale)
            .background(
                Capsule(style: .continuous)
                    .fill(MockupColors.cardFill)
                    .overlay(
                        Capsule(style: .continuous)
                            .stroke(MockupColors.cardStroke, lineWidth: 1)
                    )
            )
        }
    }

    private var warningCard: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            HStack(spacing: 14 * scale) {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 28 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.yellow)
                    .frame(width: 28 * scale, height: 28 * scale)

                Text("No cell-level BMS data")
                    .font(.system(size: 15 * scale, weight: .black))
                    .foregroundStyle(MockupColors.yellow)
            }
            Text("CutOut can’t see cell balance, weak groups,")
                .font(.system(size: 14 * scale, weight: .medium))
                .foregroundStyle(MockupColors.primaryText.opacity(0.9))
                .fixedSize(horizontal: false, vertical: true)
            Text("BMS faults, or pack temperature from this wheel.")
                .font(.system(size: 14 * scale, weight: .medium))
                .foregroundStyle(MockupColors.primaryText.opacity(0.9))
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 18 * scale)
        .padding(.vertical, 16 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 22 * scale, style: .continuous)
                .fill(Color(red: 0.145, green: 0.094, blue: 0.102))
                .overlay(
                    RoundedRectangle(cornerRadius: 22 * scale, style: .continuous)
                        .stroke(Color(red: 0.318, green: 0.188, blue: 0.208), lineWidth: 1.2)
                )
        )
    }

    private var packEstimateCard: some View {
        HStack(alignment: .top, spacing: 14 * scale) {
            VStack(alignment: .leading, spacing: 8 * scale) {
                cardLabel("PACK ESTIMATE")
                HStack(alignment: .firstTextBaseline, spacing: 4 * scale) {
                    Text("71")
                        .font(.system(size: 64 * scale, weight: .black))
                        .monospacedDigit()
                    Text("%")
                        .font(.system(size: 18 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.muted)
                }
                Text("derived from voltage curve + recent sag")
                    .font(.system(size: 10 * scale, weight: .medium))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .foregroundStyle(MockupColors.muted)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            VStack(alignment: .leading, spacing: 8 * scale) {
                cardLabel("CONFIDENCE")
                Text("medium")
                    .font(.system(size: 22 * scale, weight: .black))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                Text("not cell-safe")
                    .font(.system(size: 11 * scale, weight: .medium))
                    .foregroundStyle(MockupColors.muted)
            }
            .padding(.horizontal, 10 * scale)
            .padding(.vertical, 14 * scale)
            .frame(width: 112 * scale, alignment: .leading)
            .background(dashedCard(cornerRadius: 18 * scale))
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(BmsCardBackground(cornerRadius: 28 * scale))
    }

    private var telemetryCard: some View {
        VStack(alignment: .leading, spacing: 14 * scale) {
            cardLabel("WHAT WE CAN SEE")
            HStack(alignment: .top, spacing: 18 * scale) {
                noDataMetric(value: "117.6", unit: "V", label: "pack voltage")
                noDataMetric(value: "4.8", unit: "V", label: "ride sag")
                noDataMetric(value: "38", unit: "A", label: "load now")
            }
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(BmsCardBackground(cornerRadius: 24 * scale))
    }

    private var unknownsCard: some View {
        VStack(alignment: .leading, spacing: 10 * scale) {
            cardLabel("WHAT IS UNKNOWN")
            noDataUnknownRow("individual cell/group voltages")
            noDataUnknownRow("cell balance / weak parallel group")
            noDataUnknownRow("BMS temperature, faults, and cutout reason")
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(BmsCardBackground(cornerRadius: 24 * scale))
    }

    private var ridingRuleCard: some View {
        VStack(alignment: .leading, spacing: 10 * scale) {
            cardLabel("RIDING RULE")
            Text(snapshot.captureActionTitle ?? "--")
                .font(.system(size: 13 * scale, weight: .medium))
                .lineLimit(2)
                .minimumScaleFactor(0.84)
                .foregroundStyle(MockupColors.primaryText.opacity(0.9))
            Capsule()
                .fill(MockupColors.cardStroke)
                .frame(height: 6 * scale)
                .overlay(alignment: .leading) {
                    Capsule()
                        .fill(
                            LinearGradient(
                                colors: [MockupColors.yellow, MockupColors.orange],
                                startPoint: .leading,
                                endPoint: .trailing
                            )
                        )
                        .frame(width: 186 * scale)
                }
        }
        .padding(.horizontal, 20 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(BmsCardBackground(cornerRadius: 24 * scale))
    }

    private func cardLabel(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 12 * scale, weight: .bold))
            .foregroundStyle(MockupColors.muted)
    }

    private func noDataMetric(value: String, unit: String, label: String) -> some View {
        VStack(alignment: .leading, spacing: 6 * scale) {
            HStack(alignment: .firstTextBaseline, spacing: 3 * scale) {
                Text(value)
                    .font(.system(size: 24 * scale, weight: .black))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                Text(unit)
                    .font(.system(size: 18 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
            }
            Text(label)
                .font(.system(size: 12 * scale, weight: .medium))
                .foregroundStyle(MockupColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func noDataUnknownRow(_ title: String) -> some View {
        Text(title)
            .font(.system(size: 14 * scale, weight: .medium))
            .foregroundStyle(MockupColors.primaryText.opacity(0.92))
            .lineLimit(1)
            .minimumScaleFactor(0.82)
            .padding(.horizontal, 12 * scale)
            .frame(maxWidth: .infinity, minHeight: 30 * scale, alignment: .leading)
            .background(dashedCard(cornerRadius: 10 * scale))
    }

    private func dashedCard(cornerRadius: CGFloat) -> some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(MockupColors.cardFill)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(style: StrokeStyle(lineWidth: 1.2, dash: [5 * scale, 5 * scale]))
                    .foregroundStyle(MockupColors.cardStroke)
            )
    }
}

private struct BmsChip: View {
    let title: String
    let accent: MockupAccent
    let scale: CGFloat
    let maxWidth: CGFloat?

    var body: some View {
        Text(title)
            .font(.system(size: 15 * scale, weight: .bold))
            .foregroundStyle(.black.opacity(accent == .green ? 0.82 : 0.92))
            .lineLimit(1)
            .minimumScaleFactor(0.72)
            .padding(.horizontal, 16 * scale)
            .frame(maxWidth: maxWidth, minHeight: 30 * scale)
            .background(chipBackground)
    }

    @ViewBuilder
    private var chipBackground: some View {
        if #available(iOS 26, macOS 26, *) {
            Capsule()
                .fill(accent.color)
                .glassEffect(.regular.tint(accent.color.opacity(0.78)), in: .capsule)
        } else {
            Capsule().fill(accent.color)
        }
    }
}

private struct BmsBottomTab: View {
    let title: String
    let isSelected: Bool
    let scale: CGFloat
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 7 * scale) {
                Text(title)
                    .font(.system(size: 15 * scale, weight: isSelected ? .black : .medium))
                    .foregroundStyle(isSelected ? MockupColors.yellow : MockupColors.muted)
                Capsule()
                    .fill(isSelected ? MockupColors.yellow : Color.clear)
                    .frame(width: 24 * scale, height: 3 * scale)
            }
        }
        .buttonStyle(.plain)
    }
}

private struct BmsHeroCard: View {
    let eyebrow: String
    let title: String
    let trailing: String
    let detail: String
    let accent: MockupAccent
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 2 * scale) {
            Text(eyebrow)
                .font(.system(size: 14 * scale, weight: .bold))
                .foregroundStyle(MockupColors.muted)
            HStack(alignment: .firstTextBaseline, spacing: 8 * scale) {
                Text(title)
                    .font(.system(size: 58 * scale, weight: .black))
                    .monospacedDigit()
                Text(trailing)
                    .font(.system(size: 15 * scale, weight: .black))
                    .foregroundStyle(accent.color)
            }
            Text(detail)
                .font(.system(size: 13 * scale, weight: .bold))
                .foregroundStyle(MockupColors.muted)
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(MockupColors.cardStroke)
                    Capsule()
                        .fill(accent.color)
                        .frame(width: proxy.size.width * 0.72)
                }
            }
            .frame(height: 12 * scale)
        }
        .padding(.horizontal, 18 * scale)
        .padding(.vertical, 16 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(BmsCardBackground(cornerRadius: 24 * scale))
    }
}

private struct BmsMetricCard: View {
    let title: String
    let value: String
    let unit: String
    let detail: String
    let accent: MockupAccent
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 5 * scale) {
            Text(title)
                .font(.system(size: 14 * scale, weight: .bold))
                .foregroundStyle(MockupColors.muted)
            HStack(alignment: .firstTextBaseline, spacing: 4 * scale) {
                Text(value)
                    .font(.system(size: 25 * scale, weight: .black))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.82)
                if !unit.isEmpty {
                    Text(unit)
                        .font(.system(size: 13 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.muted)
                }
            }
            if !detail.isEmpty {
                Text(detail)
                    .font(.system(size: 13 * scale, weight: .black))
                    .foregroundStyle(accent.color)
            }
        }
        .padding(.horizontal, 16 * scale)
        .padding(.vertical, 16 * scale)
        .frame(maxWidth: .infinity, minHeight: 106 * scale, alignment: .topLeading)
        .background(BmsCardBackground(cornerRadius: 20 * scale))
    }
}

private struct BmsWideCard: View {
    let title: String?
    let value: String
    let detail: String?
    let accent: MockupAccent
    let border: BmsCardBorder
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 7 * scale) {
            if let title {
                Text(title)
                    .font(.system(size: 14 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
            }
            Text(value)
                .font(.system(size: 25 * scale, weight: .black))
                .lineLimit(2)
                .minimumScaleFactor(0.74)
            if let detail, !detail.isEmpty {
                Text(detail)
                    .font(.system(size: 13 * scale, weight: .black))
                    .foregroundStyle(accent.color)
                    .lineLimit(2)
                    .minimumScaleFactor(0.72)
            }
        }
        .padding(.horizontal, 18 * scale)
        .padding(.vertical, 18 * scale)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(BmsOutlinedCardBackground(cornerRadius: 24 * scale, stroke: border.strokeColor))
    }
}

private struct BmsGroupCell: View {
    let group: BmsGroupSnapshot
    let isHighlighted: Bool
    let isSelected: Bool
    let scale: CGFloat

    var body: some View {
        VStack(spacing: 8 * scale) {
            Text("\(group.index)")
                .font(.system(size: 14 * scale, weight: .medium))
                .foregroundStyle(MockupColors.muted)
            Text(groupVoltageText(group))
                .font(.system(size: 20 * scale, weight: .black))
                .monospacedDigit()
                .minimumScaleFactor(0.84)
        }
        .frame(maxWidth: .infinity)
        .frame(height: 70 * scale)
        .background(BmsOutlinedCardBackground(cornerRadius: 10 * scale, stroke: strokeColor))
    }

    private var strokeColor: Color {
        if isSelected {
            return MockupColors.yellow
        }
        if isHighlighted {
            return MockupColors.orange
        }
        return MockupColors.green
    }
}

private struct BmsStripCell: View {
    let group: BmsGroupSnapshot
    let isHighlighted: Bool
    let scale: CGFloat

    var body: some View {
        VStack(spacing: 2 * scale) {
            Text(String(format: "%02d", group.index))
                .font(.system(size: 8 * scale, weight: .medium))
                .foregroundStyle(MockupColors.muted)
            Text(groupVoltageText(group))
                .font(.system(size: 9 * scale, weight: .black))
                .monospacedDigit()
                .minimumScaleFactor(0.7)
        }
        .frame(width: 31 * scale, height: 44 * scale)
        .background(BmsOutlinedCardBackground(cornerRadius: 8 * scale, stroke: strokeColor))
    }

    private var strokeColor: Color {
        switch group.alertLevel {
        case .critical:
            MockupColors.warningStroke
        case .warning:
            MockupColors.orange
        case .nominal, .unknown:
            isHighlighted ? MockupColors.orange : MockupColors.green
        }
    }
}

private struct BmsGroupIndexCell: View {
    let group: BmsGroupSnapshot
    let isSelected: Bool
    let scale: CGFloat

    var body: some View {
        Text("\(group.index)")
            .font(.system(size: 14 * scale, weight: .medium))
            .foregroundStyle(MockupColors.muted)
            .frame(maxWidth: .infinity)
            .frame(height: 34 * scale)
            .background(BmsOutlinedCardBackground(cornerRadius: 8 * scale, stroke: isSelected ? MockupColors.orange : MockupColors.green))
    }
}

private struct BmsModeChip: View {
    let title: String
    let isSelected: Bool
    let scale: CGFloat

    var body: some View {
        Text(title)
            .font(.system(size: 15 * scale, weight: .bold))
            .foregroundStyle(isSelected ? .black : MockupColors.primaryText)
            .padding(.horizontal, 16 * scale)
            .frame(height: 32 * scale)
            .background(Capsule().fill(isSelected ? MockupColors.yellow : MockupColors.iconFill))
    }
}

private struct BmsCardBackground: View {
    let cornerRadius: CGFloat

    var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(MockupColors.cardFill)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(MockupColors.cardStroke, lineWidth: 1)
            )
    }
}

private struct BmsOutlinedCardBackground: View {
    let cornerRadius: CGFloat
    let stroke: Color

    var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(MockupColors.cardFill)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(stroke, lineWidth: 1.2)
            )
    }
}

private enum BmsCardBorder {
    case normal
    case warning
    case critical

    var strokeColor: Color {
        switch self {
        case .normal:
            MockupColors.cardStroke
        case .warning:
            MockupColors.orange
        case .critical:
            Color(red: 0.92, green: 0.33, blue: 0.35)
        }
    }
}

private func percentText(_ reading: TelemetryReading<UInt8>?) -> String {
    guard let reading else { return "--" }
    return "\(reading.value)%"
}

private func voltageText(_ reading: TelemetryReading<Int32>?) -> String {
    reading.map { String(format: "%.1f", Double($0.value) / 1_000.0) } ?? "--"
}

private func millivoltsText(_ reading: TelemetryReading<Int32>?) -> String {
    reading.map { String($0.value) } ?? "--"
}

private func temperatureText(_ reading: TelemetryReading<Int32>?) -> String {
    reading.map { String(format: "%.1f", Double($0.value) / 1_000.0) } ?? "--"
}

private func groupVoltageText(_ snapshot: BmsSnapshot, index: Int?) -> String {
    guard let index else { return "--" }
    return groupVoltageText(snapshot.groups.first { $0.index == index })
}

private func groupVoltageText(_ group: BmsGroupSnapshot?) -> String {
    guard let value = group?.voltage?.value else { return "--" }
    return String(format: "%.3f", Double(value) / 1_000.0)
}

private extension MockupPickerRow {
    var glyphColor: Color {
        switch title {
        case "NINEBOT-7A31":
            MockupColors.teal
        case "HX Hoverboard":
            MockupColors.brown
        default:
            MockupColors.yellow
        }
    }

    var glyphBackground: Color {
        glyphColor.opacity(isSupported ? 0.12 : 0.16)
    }

    var titleColor: Color {
        isSupported ? MockupColors.primaryText : MockupColors.disabledText
    }

    var secondaryTextColor: Color {
        isSupported ? MockupColors.muted : MockupColors.disabledSecondaryText
    }
}

enum MockupColors {
    static let pageBackground = Color(red: 0.027, green: 0.031, blue: 0.043)
    static let cardFill = Color(red: 0.067, green: 0.078, blue: 0.106)
    static let cardStroke = Color(red: 0.165, green: 0.188, blue: 0.239)
    static let disabledFill = Color(red: 0.067, green: 0.078, blue: 0.106)
    static let primaryText = Color(red: 0.969, green: 0.953, blue: 0.918)
    static let disabledText = Color(red: 0.455, green: 0.475, blue: 0.514)
    static let disabledSecondaryText = Color(red: 0.36, green: 0.38, blue: 0.42)
    static let muted = Color(red: 0.561, green: 0.596, blue: 0.659)
    static let yellow = Color(red: 1.0, green: 0.827, blue: 0.302)
    static let cyan = Color(red: 0.278, green: 0.824, blue: 0.933)
    static let green = Color(red: 0.376, green: 0.906, blue: 0.553)
    static let orange = Color(red: 1.0, green: 0.486, blue: 0.188)
    static let warningText = Color(red: 1.0, green: 0.667, blue: 0.345)
    static let warningFill = Color(red: 0.173, green: 0.087, blue: 0.040)
    static let warningStroke = Color(red: 0.443, green: 0.216, blue: 0.102)
    static let teal = Color(red: 0.180, green: 0.384, blue: 0.459)
    static let brown = Color(red: 0.443, green: 0.259, blue: 0.141)
    static let purple = Color(red: 0.635, green: 0.459, blue: 0.918)
    static let iconFill = Color(red: 0.043, green: 0.051, blue: 0.071)
}

extension MockupAccent {
    var color: Color {
        switch self {
        case .cyan:
            MockupColors.cyan
        case .green:
            MockupColors.green
        case .orange:
            MockupColors.orange
        case .yellow:
            MockupColors.yellow
        }
    }
}

private extension MockupPickerRowState {
    var actionTitle: String {
        switch self {
        case .supported(let action), .unsupported(let action), .manual(let action):
            action
        }
    }

    var isSupported: Bool {
        if case .supported = self { true } else { false }
    }
}

private extension MockupScreen {
    var displaySubtitle: String {
        subtitle.replacingOccurrences(of: " - ", with: " · ")
    }

    var tabTitle: String {
        switch id {
        case .devicePicker:
            "Picker"
        case .eucRide:
            "EUC"
        case .bmsOverview:
            "BMS"
        case .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsNoData:
            "Cells"
        case .bmsUnknownTopology:
            "Faults"
        case .eucGarage:
            "Pack"
        case .vescOnewheelRide:
            "OW"
        case .vescDebug:
            "VESC"
        }
    }
}

private extension LiveSpeedConnectionPhase {
    var opensRideScreen: Bool {
        switch self {
        case .connecting, .discoveringServices, .subscribing, .live:
            true
        case .starting, .bluetoothUnavailable, .scanning, .failed:
            false
        }
    }
}
