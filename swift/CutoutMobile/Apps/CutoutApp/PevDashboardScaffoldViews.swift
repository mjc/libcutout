import CutoutMobile
import SwiftUI

struct PevDashboardScaffold<Content: View>: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let sectionTitle: String
    let bottomPadding: CGFloat
    let allowsVerticalScroll: Bool
    let columnSpacing: CGFloat
    let contentSpacing: CGFloat
    let horizontalPadding: CGFloat
    let showsHeader: Bool
    private let content: ([GridItem]) -> Content

    init(
        sectionTitle: String,
        bottomPadding: CGFloat,
        allowsVerticalScroll: Bool = true,
        columnSpacing: CGFloat = 26,
        contentSpacing: CGFloat = 16,
        horizontalPadding: CGFloat = 24,
        showsHeader: Bool = true,
        @ViewBuilder content: @escaping ([GridItem]) -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.bottomPadding = bottomPadding
        self.allowsVerticalScroll = allowsVerticalScroll
        self.columnSpacing = columnSpacing
        self.contentSpacing = contentSpacing
        self.horizontalPadding = horizontalPadding
        self.showsHeader = showsHeader
        self.content = content
    }

    var body: some View {
        let minimumColumnWidth: CGFloat = dynamicTypeSize.isAccessibilitySize ? 240 : 150
        let columns = [
            GridItem(.adaptive(minimum: minimumColumnWidth), spacing: columnSpacing),
        ]

        Group {
            if allowsVerticalScroll {
                ScrollView(.vertical, showsIndicators: false) {
                    scaffoldContent(columns: columns)
                }
            } else {
                scaffoldContent(columns: columns)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(PevColors.pageBackground)
        .foregroundStyle(PevColors.primaryText)
    }

    private func scaffoldContent(columns: [GridItem]) -> some View {
        VStack(alignment: .leading, spacing: contentSpacing) {
            if showsHeader {
                PevDashboardHeader(sectionTitle: sectionTitle)
            }

            content(columns)
        }
        .padding(.horizontal, horizontalPadding)
        .padding(.bottom, bottomPadding)
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }
}

struct PevAppShell<Content: View>: View {
    let sectionTitle: String
    let disconnect: () -> Void
    let content: Content

    init(
        sectionTitle: String,
        disconnect: @escaping () -> Void,
        @ViewBuilder content: () -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.disconnect = disconnect
        self.content = content()
    }

    var body: some View {
        VStack(spacing: 0) {
            PevDashboardHeader(
                sectionTitle: sectionTitle
            ) {
                PevRideDisconnectButton(action: disconnect)
            }
            .padding(.horizontal, 24)

            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(PevColors.pageBackground)
    }
}

struct PevDashboardHeader<LeadingAccessory: View>: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let sectionTitle: String
    let leadingAccessory: LeadingAccessory

    init(
        sectionTitle: String,
        @ViewBuilder leadingAccessory: () -> LeadingAccessory
    ) {
        self.sectionTitle = sectionTitle
        self.leadingAccessory = leadingAccessory()
    }

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 8) {
                    leading
                    section
                }
            } else {
                HStack(alignment: .firstTextBaseline) {
                    leading
                    Spacer()
                    section
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("dashboard.top.navigation")
    }

    @ViewBuilder
    private var leading: some View {
        leadingAccessory
    }

    private var section: some View {
        Text(sectionTitle)
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(PevColors.muted)
    }
}

extension PevDashboardHeader where LeadingAccessory == PevDashboardBrand {
    init(sectionTitle: String) {
        self.init(sectionTitle: sectionTitle) {
            PevDashboardBrand()
        }
    }
}

struct PevDashboardBrand: View {
    var body: some View {
        Text("CutOut")
            .font(.headline.weight(.bold))
            .foregroundStyle(PevColors.yellow)
    }
}
