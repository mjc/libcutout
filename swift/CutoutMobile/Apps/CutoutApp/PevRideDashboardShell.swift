import CutoutMobile
import SwiftUI

struct PevRideDashboardShell<Content: View>: View {
    let sectionTitle: String
    let heroStyle: PevRideHeroStyle
    let title: String
    let subtitle: String
    let statusFill: Color
    let captureStatusText: String?
    let speedReadout: PevRideHeroReadout
    let speedCaption: String
    let allowsVerticalScroll: Bool
    let content: Content

    init(
        sectionTitle: String,
        heroStyle: PevRideHeroStyle,
        title: String,
        subtitle: String,
        statusFill: Color,
        captureStatusText: String?,
        speedReadout: PevRideHeroReadout,
        speedCaption: String,
        allowsVerticalScroll: Bool = true,
        @ViewBuilder content: () -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.heroStyle = heroStyle
        self.title = title
        self.subtitle = subtitle
        self.statusFill = statusFill
        self.captureStatusText = captureStatusText
        self.speedReadout = speedReadout
        self.speedCaption = speedCaption
        self.allowsVerticalScroll = allowsVerticalScroll
        self.content = content()
    }

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: sectionTitle,
            bottomPadding: 20,
            allowsVerticalScroll: allowsVerticalScroll,
            showsHeader: false
        ) {
            PevRideHeroSection(
                style: heroStyle,
                title: title,
                subtitle: subtitle,
                statusFill: statusFill,
                captureStatusText: captureStatusText,
                speedReadout: speedReadout,
                speedCaption: speedCaption
            )

            content
        }
    }
}
