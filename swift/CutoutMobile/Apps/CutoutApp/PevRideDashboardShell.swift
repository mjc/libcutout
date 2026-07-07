import CutoutMobile
import SwiftUI

struct PevRideDashboardShell<LeadingAccessory: View, Content: View>: View {
    let sectionTitle: String
    let headerLeadingAccessory: ((CGFloat) -> AnyView)?
    let title: String
    let subtitle: String
    let statusFill: Color
    let captureStatusText: String?
    let speedValue: String
    let speedUnit: String
    let speedCaption: String
    let allowsVerticalScroll: Bool
    let topLeadingAccessory: (CGFloat) -> LeadingAccessory
    let content: (CGFloat, [GridItem]) -> Content

    init(
        sectionTitle: String,
        headerLeadingAccessory: ((CGFloat) -> AnyView)? = nil,
        title: String,
        subtitle: String,
        statusFill: Color,
        captureStatusText: String?,
        speedValue: String,
        speedUnit: String,
        speedCaption: String,
        allowsVerticalScroll: Bool = false,
        @ViewBuilder topLeadingAccessory: @escaping (CGFloat) -> LeadingAccessory,
        @ViewBuilder content: @escaping (CGFloat, [GridItem]) -> Content
    ) {
        self.sectionTitle = sectionTitle
        self.headerLeadingAccessory = headerLeadingAccessory
        self.title = title
        self.subtitle = subtitle
        self.statusFill = statusFill
        self.captureStatusText = captureStatusText
        self.speedValue = speedValue
        self.speedUnit = speedUnit
        self.speedCaption = speedCaption
        self.allowsVerticalScroll = allowsVerticalScroll
        self.topLeadingAccessory = topLeadingAccessory
        self.content = content
    }

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: sectionTitle,
            headerLeadingAccessory: headerLeadingAccessory,
            bottomPadding: 20,
            allowsVerticalScroll: allowsVerticalScroll,
            columnSpacing: 12
        ) { scale, columns in
            HStack(alignment: .firstTextBaseline) {
                topLeadingAccessory(scale)
                Spacer()
            }

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
