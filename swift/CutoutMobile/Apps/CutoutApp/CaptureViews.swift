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
            horizontalPadding: 18,
            showsHeader: false
        ) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .top) {
                    titleBlock
                    Spacer(minLength: 12)
                    stopCaptureButton
                }
                VStack(alignment: .leading, spacing: 12) {
                    titleBlock
                    stopCaptureButton
                }
            }

            PevStatusStrip(
                text: captureStatusText ?? "Recording locally",
                indicatorColor: PevColors.green,
                background: PevColors.cardFill,
                foreground: PevColors.primaryText,
                cornerRadius: 18
            )

            ScrollView(.vertical, showsIndicators: false) {
                CaptureLabelControls(
                    activeLabels: activeLabels,
                    startCaptureLabel: startCaptureLabel,
                    stopCaptureLabel: stopCaptureLabel
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .foregroundStyle(PevColors.primaryText)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("capture.screen")
    }

    private var titleBlock: some View {
        PevScreenTitleBlock(
            title: deviceKind ?? "Capture session",
            subtitle: "Capture session"
        )
    }

    private var stopCaptureButton: some View {
        Button("Stop capture", action: disconnect)
            .buttonStyle(CaptureActionButtonStyle(fill: PevColors.yellow))
            .accessibilityIdentifier("capture.stop")
    }
}

private struct CaptureActionButtonStyle: ButtonStyle {
    let fill: Color

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body.weight(.semibold))
            .foregroundStyle(Color.black)
            .padding(.horizontal, 18)
            .frame(minHeight: 44)
            .background(Capsule().fill(fill))
            .opacity(configuration.isPressed ? 0.82 : 1)
    }
}

struct CaptureLabelControls: View {
    let activeLabels: Set<CaptureQuickLabel>
    let startCaptureLabel: (CaptureQuickLabel) -> Void
    let stopCaptureLabel: (CaptureQuickLabel) -> Void

    private let columns = [
        GridItem(.flexible(), spacing: 10),
    ]

    var body: some View {
        PevDashboardGrid(columns: columns, spacing: 10) {
            ForEach(CaptureQuickLabel.allCases) { label in
                CaptureLabelControlRow(
                    label: label,
                    isActive: activeLabels.contains(label),
                    start: { startCaptureLabel(label) },
                    stop: { stopCaptureLabel(label) }
                )
            }
        }
    }
}

private struct CaptureLabelControlRow: View {
    let label: CaptureQuickLabel
    let isActive: Bool
    let start: () -> Void
    let stop: () -> Void

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 10) {
                status
                Spacer(minLength: 8)
                actions
            }
            VStack(alignment: .leading, spacing: 10) {
                status
                actions
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, minHeight: 58, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 8))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("capture.label.\(label.id)")
    }

    private var status: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label.title)
                .font(.headline)
                .foregroundStyle(PevColors.primaryText)
            Text(isActive ? "Active" : "Idle")
                .font(.caption)
                .foregroundStyle(isActive ? PevColors.primaryText : PevColors.muted)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label.title)
        .accessibilityValue(isActive ? "Active" : "Idle")
    }

    private var actions: some View {
        Button(label.actionTitle(isActive: isActive), role: isActive ? .destructive : nil) {
            if isActive {
                stop()
            } else {
                start()
            }
        }
        .buttonStyle(
            CaptureActionButtonStyle(fill: isActive ? PevColors.orange : PevColors.yellow)
        )
        .accessibilityIdentifier("capture.label.\(label.id).action")
    }
}
