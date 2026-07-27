import CutoutMobile
import SwiftUI

struct PevDashboardScaffold<Content: View>: View {
    let sectionTitle: String
    let bottomPadding: CGFloat
    let allowsVerticalScroll: Bool
    let contentSpacing: CGFloat
    let horizontalPadding: CGFloat
    let showsHeader: Bool
    private let content: Content

    init(
        sectionTitle: String,
        bottomPadding: CGFloat,
        allowsVerticalScroll: Bool = true,
        contentSpacing: CGFloat = 16,
        horizontalPadding: CGFloat = 24,
        showsHeader: Bool = true,
        @ViewBuilder content: () -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.bottomPadding = bottomPadding
        self.allowsVerticalScroll = allowsVerticalScroll
        self.contentSpacing = contentSpacing
        self.horizontalPadding = horizontalPadding
        self.showsHeader = showsHeader
        self.content = content()
    }

    var body: some View {
        Group {
            if allowsVerticalScroll {
                ScrollView(.vertical, showsIndicators: false) {
                    scaffoldContent
                }
            } else {
                scaffoldContent
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(PevColors.pageBackground)
        .foregroundStyle(PevColors.primaryText)
    }

    private var scaffoldContent: some View {
        VStack(alignment: .leading, spacing: contentSpacing) {
            if showsHeader {
                PevDashboardHeader(sectionTitle: sectionTitle)
            }

            content
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
                Button(action: disconnect) {
                    Text(localizedAppText("ride.action.disconnect"))
                }
                .font(.callout.weight(.bold))
                .foregroundStyle(PevDashboardColors.primaryText)
                .padding(.horizontal, 12)
                .frame(minWidth: 44, minHeight: 44)
                .background(PevDashboardCardBackground(cornerRadius: 8))
                .buttonStyle(.plain)
                .accessibilityIdentifier("dashboard.disconnect")
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
                    leadingAccessory
                    section
                }
            } else {
                HStack(alignment: .firstTextBaseline) {
                    leadingAccessory
                    Spacer()
                    section
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("dashboard.top.navigation")
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
            .foregroundStyle(PevColors.brand)
    }
}
