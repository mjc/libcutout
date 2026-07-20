import CutoutMobile
import SwiftUI

struct PevScreenTitleBlock: View {
    let title: String
    let subtitle: String
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 7 * scale) {
            Text(title)
                .font(.largeTitle.weight(.bold))
                .foregroundStyle(PevColors.primaryText)
                .accessibilityHeading(.h1)
            Text(subtitle)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(PevColors.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .contain)
    }
}
