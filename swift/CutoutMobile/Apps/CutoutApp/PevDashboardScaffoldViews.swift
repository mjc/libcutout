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
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.verticalSizeClass) private var verticalSizeClass

    let sectionTitle: String
    let isRideScreen: Bool
    let disconnect: (() -> Void)?
    let back: (() -> Void)?
    let content: Content

    init(
        sectionTitle: String,
        isRideScreen: Bool = false,
        disconnect: (() -> Void)? = nil,
        back: (() -> Void)? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.isRideScreen = isRideScreen
        self.disconnect = disconnect
        self.back = back
        self.content = content()
    }

    var body: some View {
        VStack(spacing: 0) {
            PevDashboardHeader(
                sectionTitle: sectionTitle,
                stacksAtAccessibilitySizes: !isRideScreen
            ) {
                if let back {
                    Button(action: back) {
                        Label(localizedAppText("ride_map.detail_back"), systemImage: "chevron.backward")
                            .frame(minWidth: 44, minHeight: 44)
                            .contentShape(Rectangle())
                    }
                    .font(.callout.weight(.bold))
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("dashboard.back")
                } else if let disconnect {
                    Button(action: disconnect) {
                        if dynamicTypeSize.isAccessibilitySize && (isRideScreen || verticalSizeClass == .compact) {
                            Image(systemName: "xmark")
                                .accessibilityHidden(true)
                        } else {
                            Text(localizedAppText("ride.action.disconnect"))
                        }
                    }
                    .font(.callout.weight(.bold))
                    .foregroundStyle(PevDashboardColors.primaryText)
                    .padding(.horizontal, 12)
                    .frame(minWidth: 44, minHeight: 44)
                    .background(PevDashboardCardBackground(cornerRadius: 8))
                    .buttonStyle(.plain)
                    .accessibilityLabel(localizedAppText("ride.action.disconnect"))
                    .accessibilityIdentifier("dashboard.disconnect")
                } else {
                    PevDashboardBrand()
                }
            }
            .frame(minHeight: 44)
            .padding(.horizontal, 24)

            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .padding(.top, verticalSizeClass == .compact ? 8 : 0)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(PevColors.pageBackground)
    }
}

struct PevDashboardHeader<LeadingAccessory: View>: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.verticalSizeClass) private var verticalSizeClass

    let sectionTitle: String
    let stacksAtAccessibilitySizes: Bool
    let leadingAccessory: LeadingAccessory

    init(
        sectionTitle: String,
        stacksAtAccessibilitySizes: Bool = true,
        @ViewBuilder leadingAccessory: () -> LeadingAccessory
    ) {
        self.sectionTitle = sectionTitle
        self.stacksAtAccessibilitySizes = stacksAtAccessibilitySizes
        self.leadingAccessory = leadingAccessory()
    }

    var body: some View {
        Group {
            if stacksAtAccessibilitySizes && dynamicTypeSize.isAccessibilitySize && verticalSizeClass != .compact {
                VStack(alignment: .leading, spacing: 8) {
                    leadingAccessory
                        .frame(minHeight: 44)
                    section
                }
            } else {
                HStack(alignment: .firstTextBaseline) {
                    leadingAccessory
                        .frame(minHeight: 44)
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
            .foregroundStyle(PevColors.primaryText)
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
            .foregroundStyle(PevColors.primaryText)
    }
}
