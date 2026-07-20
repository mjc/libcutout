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
        ) { _, _ in
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
                scale: 1,
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
        .foregroundStyle(.white)
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
        Button("Stop capture", role: .destructive, action: disconnect)
            .buttonStyle(.bordered)
            .tint(PevColors.yellow)
            .frame(minHeight: 44)
            .accessibilityIdentifier("capture.stop")
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
                .foregroundStyle(isActive ? PevColors.green : PevColors.muted)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label.title)
        .accessibilityValue(isActive ? "Active" : "Idle")
    }

    private var actions: some View {
        HStack(spacing: 10) {
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
