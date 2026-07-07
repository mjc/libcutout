import CutoutMobile
import SwiftUI

struct CaptureRecordingScreen: View {
    let deviceKind: String?
    let captureStatusText: String?
    let activeLabels: Set<CaptureQuickLabel>
    let disconnect: () -> Void
    let startCaptureLabel: (CaptureQuickLabel) -> Void
    let stopCaptureLabel: (CaptureQuickLabel) -> Void

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: "record",
            bottomPadding: 24,
            allowsVerticalScroll: false,
            contentSpacing: 16,
            horizontalPadding: 18
        ) { scale, _ in
            HStack(alignment: .top) {
                PevScreenTitleBlock(
                    title: deviceKind ?? "Capture session",
                    subtitle: "Capture session",
                    scale: scale,
                    titleFontSize: 34,
                    subtitleFontSize: 15,
                    titleMinimumScaleFactor: 0.72,
                    subtitleLineLimit: 1
                )
                Spacer(minLength: 12 * scale)
                PevActionButton(
                    title: "Stop",
                    systemImageName: nil,
                    scale: scale,
                    isEnabled: true,
                    fillsAvailableWidth: false,
                    width: nil,
                    height: 30 * scale,
                    cornerRadius: 8 * scale,
                    horizontalPadding: 12 * scale,
                    iconSpacing: 0,
                    foregroundEnabled: PevColors.yellow,
                    foregroundDisabled: PevColors.yellow,
                    fillEnabled: PevDashboardColors.cardFill,
                    fillDisabled: PevDashboardColors.cardFill,
                    strokeEnabled: PevDashboardColors.cardStroke,
                    strokeDisabled: PevDashboardColors.cardStroke,
                    action: disconnect
                )
            }

            PevStatusStrip(
                text: captureStatusText ?? "Recording locally",
                scale: scale,
                indicatorColor: PevColors.green,
                background: PevColors.cardFill,
                foreground: PevColors.primaryText,
                cornerRadius: 18
            )

            ScrollView(.vertical, showsIndicators: false) {
                CaptureLabelControls(
                    scale: scale,
                    activeLabels: activeLabels,
                    startCaptureLabel: startCaptureLabel,
                    stopCaptureLabel: stopCaptureLabel
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .foregroundStyle(.white)
    }
}

struct CaptureLabelControls: View {
    let scale: CGFloat
    let activeLabels: Set<CaptureQuickLabel>
    let startCaptureLabel: (CaptureQuickLabel) -> Void
    let stopCaptureLabel: (CaptureQuickLabel) -> Void

    private let columns = [
        GridItem(.flexible(), spacing: 10),
    ]

    var body: some View {
        PevDashboardGrid(columns: columns, spacing: 10 * scale) {
            ForEach(CaptureQuickLabel.allCases) { label in
                HStack(spacing: 10 * scale) {
                    VStack(alignment: .leading, spacing: 3 * scale) {
                        Text(label.title)
                            .font(.system(size: 16 * scale, weight: .bold))
                            .foregroundStyle(PevColors.primaryText)
                            .lineLimit(1)
                        Text(activeLabels.contains(label) ? "active" : "idle")
                            .font(.system(size: 11 * scale, weight: .semibold))
                            .foregroundStyle(activeLabels.contains(label) ? PevColors.green : PevColors.muted)
                    }
                    Spacer(minLength: 8 * scale)
                    PevActionButton(
                        title: "Start",
                        systemImageName: nil,
                        scale: scale,
                        isEnabled: true,
                        fillsAvailableWidth: false,
                        width: 58 * scale,
                        height: 32 * scale,
                        cornerRadius: 8 * scale,
                        horizontalPadding: 0,
                        iconSpacing: 0,
                        foregroundEnabled: PevColors.yellow,
                        foregroundDisabled: PevColors.muted,
                        fillEnabled: PevColors.warningFill,
                        fillDisabled: PevColors.disabledFill,
                        strokeEnabled: PevColors.warningStroke,
                        strokeDisabled: PevColors.cardStroke
                    ) {
                        startCaptureLabel(label)
                    }

                    PevActionButton(
                        title: "Stop",
                        systemImageName: nil,
                        scale: scale,
                        isEnabled: true,
                        fillsAvailableWidth: false,
                        width: 58 * scale,
                        height: 32 * scale,
                        cornerRadius: 8 * scale,
                        horizontalPadding: 0,
                        iconSpacing: 0,
                        foregroundEnabled: PevColors.yellow,
                        foregroundDisabled: PevColors.muted,
                        fillEnabled: PevColors.warningFill,
                        fillDisabled: PevColors.disabledFill,
                        strokeEnabled: PevColors.warningStroke,
                        strokeDisabled: PevColors.cardStroke
                    ) {
                        stopCaptureLabel(label)
                    }
                }
                .padding(.horizontal, 14 * scale)
                .frame(minHeight: 58 * scale)
                .background(PevDashboardCardBackground(cornerRadius: 8 * scale))
            }
        }
    }
}
