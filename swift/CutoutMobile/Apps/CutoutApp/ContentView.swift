import CutoutMobile
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: LiveSpeedModel
    @State private var selectedScreenID: MockupScreenID

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
                        liveSpeed: model.speed.displayValue,
                        devicePickerScanState: model.devicePickerScanState,
                        pair: { platformIdentifier in
                            if model.pair(platformIdentifier: platformIdentifier) {
                                selectedScreenID = .eucRide
                            }
                        }
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
            if phase.opensRideScreen {
                selectedScreenID = .eucRide
            }
        }
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
}

private struct MockupScreenContainer: View {
    let screen: MockupScreen
    let liveSpeed: String
    let devicePickerScanState: DevicePickerScanState?
    let pair: (String) -> Void

    var body: some View {
        switch screen.id {
        case .devicePicker:
            DevicePickerMockupView(screen: screen, scanState: devicePickerScanState, pair: pair)
        case .eucRide:
            EucRideMockupView(screen: screen)
        case .eucGarage:
            EucGarageMockupView(screen: screen)
        case .vescOnewheelRide:
            VescOnewheelRideMockupView(screen: screen)
        case .vescDebug:
            VescDebugMockupView(screen: screen)
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
    private let content: (CGFloat, [GridItem]) -> Content

    init(
        sectionTitle: String,
        bottomPadding: CGFloat,
        allowsVerticalScroll: Bool = true,
        @ViewBuilder content: @escaping (CGFloat, [GridItem]) -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.bottomPadding = bottomPadding
        self.allowsVerticalScroll = allowsVerticalScroll
        self.content = content
    }

    var body: some View {
        GeometryReader { proxy in
            let scale = min(proxy.size.width / 390.0, proxy.size.height / 844.0)
            let columns = [
                GridItem(.flexible(), spacing: 26 * scale),
                GridItem(.flexible(), spacing: 26 * scale),
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
        VStack(alignment: .leading, spacing: 16 * scale) {
            MockupScreenHeader(sectionTitle: sectionTitle, scale: scale)

            content(scale, columns)
        }
        .padding(.horizontal, 24 * scale)
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

    private var speedParts: (value: String, unit: String) {
        let parts = screen.primaryValue.split(separator: " ", maxSplits: 1).map(String.init)
        return (parts.first ?? screen.primaryValue, parts.dropFirst().first ?? "")
    }

    var body: some View {
        GeometryReader { proxy in
            let scale = min(proxy.size.width / 390.0, proxy.size.height / 844.0)
            let columns = [
                GridItem(.flexible(), spacing: 12 * scale),
                GridItem(.flexible(), spacing: 12 * scale),
            ]

            VStack(alignment: .leading, spacing: 14 * scale) {
                HStack(alignment: .firstTextBaseline) {
                    Text("CutOut")
                        .font(.system(size: 18 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.yellow)
                    Spacer()
                    Text("EUC ride")
                        .font(.system(size: 15 * scale, weight: .semibold))
                        .foregroundStyle(MockupColors.muted)
                }
                .padding(.top, 10 * scale)

                HStack(alignment: .center, spacing: 12 * scale) {
                    Text(screen.title)
                        .font(.system(size: 18 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.primaryText)
                        .lineLimit(1)
                        .minimumScaleFactor(0.85)
                    Spacer(minLength: 8 * scale)
                    EucStatusBadge(title: screen.displaySubtitle, scale: scale)
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
                .padding(.top, 0)

                VStack(spacing: 10 * scale) {
                    ForEach(screen.safetyBars, id: \.label) { bar in
                        EucSafetyBar(bar: bar, scale: scale)
                    }
                }
                .padding(.top, 0)

                if let warningCard = screen.warningCard {
                    EucWarningCard(card: warningCard, scale: scale)
                        .padding(.top, 14 * scale)
                }

                LazyVGrid(columns: columns, spacing: 12 * scale) {
                    ForEach(screen.dashboardTiles) { tile in
                        EucDashboardTile(tile: tile, scale: scale)
                    }
                }
                .padding(.top, 12 * scale)

                EucRideTabs(tabs: screen.tabs, scale: scale)
                    .padding(.top, 48 * scale)
            }
            .padding(.horizontal, 24 * scale)
            .padding(.bottom, 20 * scale)
            .frame(width: proxy.size.width, height: proxy.size.height, alignment: .top)
            .background(MockupColors.pageBackground)
            .foregroundStyle(MockupColors.primaryText)
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
            ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
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

                if index < rows.count - 1 {
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
    let pair: (String) -> Void

    private var renderedScanState: DevicePickerScanState {
        if let scanState {
            return scanState
        }
        return DevicePickerScanState(status: .scanning, rows: [])
    }

    private var sections: MockupPickerSections {
        renderedScanState.sections
    }

    var body: some View {
        GeometryReader { proxy in
            let scale = min(proxy.size.width / 390.0, proxy.size.height / 844.0)
            VStack(alignment: .leading, spacing: 18 * scale) {
                MockupScreenHeader(sectionTitle: "setup", scale: scale)

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
                            ForEach(Array(sections.supported.enumerated()), id: \.offset) { _, row in
                                Button {
                                    pair(row.id)
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
                            ForEach(Array(sections.unsupported.enumerated()), id: \.offset) { _, row in
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
            .padding(.horizontal, 18 * scale)
            .padding(.bottom, 24 * scale)
            .frame(width: proxy.size.width, height: proxy.size.height, alignment: .top)
            .background(MockupColors.pageBackground)
            .foregroundStyle(.white)
        }
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
