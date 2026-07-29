import CutoutMobile
import SwiftUI

struct PevRideDashboardShell<Content: View>: View {
    let sectionTitle: String
    let heroStyle: PevRideHeroStyle
    let title: String
    let subtitle: String
    let statusTone: PevDashboardStatusPillTone
    let captureStatusText: String?
    let speedReadout: RideHeroReadout
    let speedCaption: String
    let allowsVerticalScroll: Bool
    let content: Content

    init(
        sectionTitle: String,
        heroStyle: PevRideHeroStyle,
        title: String,
        subtitle: String,
        statusTone: PevDashboardStatusPillTone,
        captureStatusText: String?,
        speedReadout: RideHeroReadout,
        speedCaption: String,
        allowsVerticalScroll: Bool = true,
        @ViewBuilder content: () -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.heroStyle = heroStyle
        self.title = title
        self.subtitle = subtitle
        self.statusTone = statusTone
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
                statusTone: statusTone,
                captureStatusText: captureStatusText,
                speedReadout: speedReadout,
                speedCaption: speedCaption
            )

            content
        }
    }
}
