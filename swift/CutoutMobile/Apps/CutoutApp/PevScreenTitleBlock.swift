import CutoutMobile
import SwiftUI

struct PevScreenTitleBlock: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title)
                .font(.largeTitle.weight(.bold))
                .foregroundStyle(PevColors.primaryText)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityHeading(.h1)
            Text(subtitle)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(PevColors.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}
