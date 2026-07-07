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
    private let content: (CGFloat, [GridItem]) -> Content

    init(
        sectionTitle: String,
        headerLeadingAccessory: ((CGFloat) -> AnyView)? = nil,
        bottomPadding: CGFloat,
        allowsVerticalScroll: Bool = true,
        columnSpacing: CGFloat = 26,
        contentSpacing: CGFloat = 16,
        horizontalPadding: CGFloat = 24,
        @ViewBuilder content: @escaping (CGFloat, [GridItem]) -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.headerLeadingAccessory = headerLeadingAccessory
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
            .background(PevColors.pageBackground)
            .foregroundStyle(PevColors.primaryText)
        }
    }

    private func scaffoldContent(scale: CGFloat, columns: [GridItem], width: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: contentSpacing * scale) {
            PevDashboardHeader(sectionTitle: sectionTitle, scale: scale, leadingAccessory: headerLeadingAccessory.map { $0(scale) })

            content(scale, columns)
        }
        .padding(.horizontal, horizontalPadding * scale)
        .padding(.bottom, bottomPadding * scale)
        .frame(width: width, alignment: .topLeading)
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
