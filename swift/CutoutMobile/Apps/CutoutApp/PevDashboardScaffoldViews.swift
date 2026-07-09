import CutoutMobile
import SwiftUI

struct PevDashboardScaffold<Content: View>: View {
    let sectionTitle: String
    let headerLeadingAccessory: ((CGFloat) -> AnyView)?
    let bottomPadding: CGFloat
    let allowsVerticalScroll: Bool
    let columnSpacing: CGFloat
    let contentSpacing: CGFloat
    let horizontalPadding: CGFloat
    let showsHeader: Bool
    private let content: (CGFloat, [GridItem]) -> Content

    init(
        sectionTitle: String,
        headerLeadingAccessory: ((CGFloat) -> AnyView)? = nil,
        bottomPadding: CGFloat,
        allowsVerticalScroll: Bool = true,
        columnSpacing: CGFloat = 26,
        contentSpacing: CGFloat = 16,
        horizontalPadding: CGFloat = 24,
        showsHeader: Bool = true,
        @ViewBuilder content: @escaping (CGFloat, [GridItem]) -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.headerLeadingAccessory = headerLeadingAccessory
        self.bottomPadding = bottomPadding
        self.allowsVerticalScroll = allowsVerticalScroll
        self.columnSpacing = columnSpacing
        self.contentSpacing = contentSpacing
        self.horizontalPadding = horizontalPadding
        self.showsHeader = showsHeader
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
            .background(PevColors.pageBackground)
            .foregroundStyle(PevColors.primaryText)
        }
    }

    private func scaffoldContent(scale: CGFloat, columns: [GridItem], width: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: contentSpacing * scale) {
            if showsHeader {
                PevDashboardHeader(sectionTitle: sectionTitle, scale: scale, leadingAccessory: headerLeadingAccessory.map { $0(scale) })
            }

            content(scale, columns)
        }
        .padding(.horizontal, horizontalPadding * scale)
        .padding(.bottom, bottomPadding * scale)
        .frame(width: width, alignment: .topLeading)
    }
}

struct PevAppShell<Content: View>: View {
    let sectionTitle: String
    let tabs: [PevScreenTab]
    let connectionPhase: SessionConnectionPhase
    let selectedColor: Color
    let unselectedColor: Color
    let disconnect: () -> Void
    let selectTarget: (PevNavigationTarget) -> Void
    let content: Content

    init(
        sectionTitle: String,
        tabs: [PevScreenTab],
        connectionPhase: SessionConnectionPhase,
        selectedColor: Color,
        unselectedColor: Color,
        disconnect: @escaping () -> Void,
        selectTarget: @escaping (PevNavigationTarget) -> Void,
        @ViewBuilder content: () -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.tabs = tabs
        self.connectionPhase = connectionPhase
        self.selectedColor = selectedColor
        self.unselectedColor = unselectedColor
        self.disconnect = disconnect
        self.selectTarget = selectTarget
        self.content = content()
    }

    var body: some View {
        GeometryReader { proxy in
            let scale = min(proxy.size.width / 390.0, proxy.size.height / 844.0)

            VStack(spacing: 0) {
                PevDashboardHeader(
                    sectionTitle: sectionTitle,
                    scale: scale,
                    leadingAccessory: AnyView(PevRideDisconnectButton(scale: scale, action: disconnect))
                )
                .padding(.horizontal, 24 * scale)

                if let connectionStatus = connectionStatus {
                    PevDashboardWarningCard(
                        title: connectionStatus.title,
                        detail: connectionStatus.detail,
                        accent: connectionStatus.accent,
                        detailColor: PevColors.primaryText,
                        fill: connectionStatus.fill,
                        stroke: connectionStatus.stroke,
                        scale: scale,
                        cornerRadius: 20
                    )
                    .padding(.horizontal, 24 * scale)
                    .padding(.top, 8 * scale)
                }

                content
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

                PevDashboardTabStrip(
                    tabs: tabs,
                    scale: scale,
                    selectedColor: selectedColor,
                    unselectedColor: unselectedColor,
                    selectTarget: selectTarget
                )
                .padding(.horizontal, 24 * scale)
                .padding(.top, 8 * scale)
                .padding(.bottom, 8 * scale)
                .background(PevColors.pageBackground)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(PevColors.pageBackground)
        }
    }

    private var connectionStatus: (title: String, detail: String, accent: Color, fill: Color, stroke: Color)? {
        switch connectionPhase {
        case .starting:
            ("Starting", "Preparing Bluetooth.", PevColors.muted, PevColors.cardFill, PevColors.cardStroke)
        case .bluetoothUnavailable:
            ("Bluetooth unavailable", "Turn on Bluetooth to reconnect.", PevColors.orange, PevColors.cardFill, PevColors.cardStroke)
        case .scanning:
            ("Retrying connection", "Searching for the selected device.", PevColors.orange, PevColors.cardFill, PevColors.cardStroke)
        case .connecting, .discoveringServices, .subscribing:
            ("Connecting", connectionPhase.displayText, PevColors.purple, PevColors.purple.opacity(0.18), PevColors.purple.opacity(0.55))
        case .failed:
            ("Connection lost", "Retrying the selected device.", PevColors.orange, PevColors.cardFill, PevColors.cardStroke)
        case .live:
            nil
        }
    }
}

struct PevDashboardHeader: View {
    let sectionTitle: String
    let scale: CGFloat
    let leadingAccessory: AnyView?

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            if let leadingAccessory {
                leadingAccessory
            } else {
                Text("CutOut")
                    .font(.system(size: 18 * scale, weight: .bold))
                    .foregroundStyle(PevColors.yellow)
            }
            Spacer()
            Text(sectionTitle)
                .font(.system(size: 15 * scale, weight: .semibold))
                .foregroundStyle(PevColors.muted)
        }
        .padding(.top, 10 * scale)
    }
}

struct PevDashboardIdentityTextBlock: View {
    let title: String
    let detail: String
    let scale: CGFloat
    let titleFontSize: CGFloat
    let detailFontSize: CGFloat
    let titleMinimumScaleFactor: CGFloat
    let detailMinimumScaleFactor: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 8 * scale) {
            Text(title)
                .font(.system(size: titleFontSize * scale, weight: .black))
                .foregroundStyle(PevColors.primaryText)
                .lineLimit(1)
                .minimumScaleFactor(titleMinimumScaleFactor)
            Text(detail)
                .font(.system(size: detailFontSize * scale, weight: .bold))
                .foregroundStyle(PevColors.muted)
                .lineLimit(1)
                .minimumScaleFactor(detailMinimumScaleFactor)
        }
    }
}

struct PevDashboardIdentityCard: View {
    let title: String
    let detail: String
    let scale: CGFloat
    let titleFontSize: CGFloat
    let detailFontSize: CGFloat
    let titleMinimumScaleFactor: CGFloat
    let detailMinimumScaleFactor: CGFloat
    let trailingStatus: String?
    let trailingStatusFill: Color
    let trailingStatusForeground: Color
    let trailingStatusWidth: CGFloat
    let trailingStatusHeight: CGFloat
    let cornerRadius: CGFloat
    let height: CGFloat

    var body: some View {
        HStack(alignment: .center, spacing: 12 * scale) {
            PevDashboardIdentityTextBlock(
                title: title,
                detail: detail,
                scale: scale,
                titleFontSize: titleFontSize,
                detailFontSize: detailFontSize,
                titleMinimumScaleFactor: titleMinimumScaleFactor,
                detailMinimumScaleFactor: detailMinimumScaleFactor
            )
            .layoutPriority(1)

            if let trailingStatus {
                PevDashboardStatusPill(
                    title: trailingStatus,
                    scale: scale,
                    fill: trailingStatusFill,
                    foreground: trailingStatusForeground,
                    fontSize: 14,
                    horizontalPadding: trailingStatusWidth,
                    height: trailingStatusHeight,
                    fixedHorizontal: true
                )
            }
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: height * scale)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: cornerRadius * scale))
    }
}
