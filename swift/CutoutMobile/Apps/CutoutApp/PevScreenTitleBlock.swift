import CutoutMobile
import SwiftUI

struct PevScreenTitleBlock: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.verticalSizeClass) private var verticalSizeClass

    let title: String
    let subtitle: String

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize && verticalSizeClass == .compact {
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    titleText
                    Spacer(minLength: 0)
                    subtitleText
                }
            } else {
                VStack(alignment: .leading, spacing: 7) {
                    titleText
                    subtitleText
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityHeading(.h1)
    }

    private var titleText: some View {
        Text(title)
            .font(.largeTitle.weight(.bold))
            .foregroundStyle(PevColors.primaryText)
            .fixedSize(horizontal: false, vertical: true)
    }

    private var subtitleText: some View {
        Text(subtitle)
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(PevColors.muted)
            .fixedSize(horizontal: false, vertical: true)
    }
}
