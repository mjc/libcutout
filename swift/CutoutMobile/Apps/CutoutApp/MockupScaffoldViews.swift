import CutoutMobile
import SwiftUI

struct MockupScreenScaffold<Content: View>: View {
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

struct MockupScreenHeader: View {
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

struct GenericMockupView: View {
    let screen: MockupScreen
    let speedText: String

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
                    Text("\(speedText) mph").monospacedDigit()
                }
            }
            .padding(24)
        }
        .background(Color.black)
        .foregroundStyle(.white)
    }
}
