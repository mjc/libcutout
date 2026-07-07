import CutoutMobile
import SwiftUI

struct PevScreenTitleBlock: View {
    let title: String
    let subtitle: String
    let scale: CGFloat
    let titleFontSize: CGFloat
    let subtitleFontSize: CGFloat
    let titleMinimumScaleFactor: CGFloat
    let subtitleLineLimit: Int

    init(
        title: String,
        subtitle: String,
        scale: CGFloat,
        titleFontSize: CGFloat,
        subtitleFontSize: CGFloat,
        titleMinimumScaleFactor: CGFloat,
        subtitleLineLimit: Int
    ) {
        self.title = title
        self.subtitle = subtitle
        self.scale = scale
        self.titleFontSize = titleFontSize
        self.subtitleFontSize = subtitleFontSize
        self.titleMinimumScaleFactor = titleMinimumScaleFactor
        self.subtitleLineLimit = subtitleLineLimit
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 7 * scale) {
            Text(title)
                .font(.system(size: titleFontSize * scale, weight: .bold))
                .foregroundStyle(PevColors.primaryText)
                .lineLimit(1)
                .minimumScaleFactor(titleMinimumScaleFactor)
            Text(subtitle)
                .font(.system(size: subtitleFontSize * scale, weight: .semibold))
                .foregroundStyle(PevColors.muted)
                .lineLimit(subtitleLineLimit)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}
