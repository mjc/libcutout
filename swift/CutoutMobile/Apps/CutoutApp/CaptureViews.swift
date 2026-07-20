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
        ) { scale, _ in
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .top) {
                    titleBlock(scale: scale)
                    Spacer(minLength: 12 * scale)
                    stopCaptureButton
                }
                VStack(alignment: .leading, spacing: 12 * scale) {
                    titleBlock(scale: scale)
                    stopCaptureButton
                }
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
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("capture.screen")
    }

    private func titleBlock(scale: CGFloat) -> some View {
        PevScreenTitleBlock(
            title: deviceKind ?? "Capture session",
            subtitle: "Capture session",
            scale: scale,
            titleFontSize: 34,
            subtitleFontSize: 15,
            titleMinimumScaleFactor: 0.72,
            subtitleLineLimit: 1
        )
    }

    private var stopCaptureButton: some View {
        Button("Stop capture", role: .destructive, action: disconnect)
            .buttonStyle(.bordered)
            .tint(PevColors.yellow)
            .frame(minHeight: 44)
            .accessibilityIdentifier("capture.stop")
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
                CaptureLabelControlRow(
                    label: label,
                    isActive: activeLabels.contains(label),
                    scale: scale,
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
    let scale: CGFloat
    let start: () -> Void
    let stop: () -> Void

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 10 * scale) {
                status
                Spacer(minLength: 8 * scale)
                actions
            }
            VStack(alignment: .leading, spacing: 10 * scale) {
                status
                actions
            }
        }
        .padding(14 * scale)
        .frame(maxWidth: .infinity, minHeight: 58 * scale, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 8 * scale))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("capture.label.\(label.id)")
    }

    private var status: some View {
        VStack(alignment: .leading, spacing: 3 * scale) {
            Text(label.title)
                .font(.headline)
                .foregroundStyle(PevColors.primaryText)
            Text(isActive ? "Active" : "Idle")
                .font(.caption)
                .foregroundStyle(isActive ? PevColors.green : PevColors.muted)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label.title)
        .accessibilityValue(isActive ? "Active" : "Idle")
    }

    private var actions: some View {
        HStack(spacing: 10 * scale) {
            Button("Start", action: start)
                .buttonStyle(.borderedProminent)
                .disabled(isActive)
                .accessibilityLabel("Start \(label.title)")
                .accessibilityHint(isActive ? "This label is already active" : "")
                .accessibilityIdentifier("capture.label.\(label.id).start")

            Button("Stop", role: .destructive, action: stop)
                .buttonStyle(.bordered)
                .disabled(!isActive)
                .accessibilityLabel("Stop \(label.title)")
                .accessibilityHint(isActive ? "" : "This label is not active")
                .accessibilityIdentifier("capture.label.\(label.id).stop")
        }
        .tint(PevColors.yellow)
        .controlSize(.large)
    }
}
