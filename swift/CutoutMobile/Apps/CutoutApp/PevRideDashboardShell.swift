import CutoutMobile
import SwiftUI

struct PevRideDashboardShell<Content: View>: View {
    let sectionTitle: String
    let title: String
    let subtitle: String
    let statusFill: Color
    let captureStatusText: String?
    let speedValue: String
    let speedUnit: String
    let speedCaption: String
    let allowsVerticalScroll: Bool
    let content: (CGFloat, [GridItem]) -> Content

    init(
        sectionTitle: String,
        title: String,
        subtitle: String,
        statusFill: Color,
        captureStatusText: String?,
        speedValue: String,
        speedUnit: String,
        speedCaption: String,
        allowsVerticalScroll: Bool = false,
        @ViewBuilder content: @escaping (CGFloat, [GridItem]) -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.title = title
        self.subtitle = subtitle
        self.statusFill = statusFill
        self.captureStatusText = captureStatusText
        self.speedValue = speedValue
        self.speedUnit = speedUnit
        self.speedCaption = speedCaption
        self.allowsVerticalScroll = allowsVerticalScroll
        self.content = content
    }

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: sectionTitle,
            bottomPadding: 20,
            allowsVerticalScroll: allowsVerticalScroll,
            columnSpacing: 12,
            showsHeader: false
        ) { scale, columns in
            PevRideHeroSection(
                title: title,
                subtitle: subtitle,
                statusFill: statusFill,
                captureStatusText: captureStatusText,
                speedValue: speedValue,
                speedUnit: speedUnit,
                speedCaption: speedCaption,
                scale: scale
            )

            content(scale, columns)
        }
    }
}
