import CutoutMobile
import SwiftUI

func captureSessionDetailRows(progress: CaptureProgress) -> [PevDashboardKeyValueRow] {
    let writerHealth = switch progress.writerHealth {
    case .healthy:
        localizedAppText("capture.detail.writer.healthy")
    case .failed:
        localizedAppText("capture.detail.writer.failed")
    }
    return [
        PevDashboardKeyValueRow(
            id: "capture-elapsed",
            label: localizedAppText("capture.detail.elapsed"),
            metricValue: progress.elapsedMetricValue
        ),
        PevDashboardKeyValueRow(
            id: "capture-packets",
            label: localizedAppText("capture.detail.packets"),
            metricValue: progress.notificationCountMetricValue
        ),
        PevDashboardKeyValueRow(
            id: "capture-file-size",
            label: localizedAppText("capture.detail.file_size"),
            metricValue: progress.fileSizeMetricValue
        ),
        PevDashboardKeyValueRow(
            id: "capture-queued-messages",
            label: localizedAppText("capture.detail.pending_writes"),
            metricValue: progress.queuedMessageCountMetricValue
        ),
        PevDashboardKeyValueRow(
            id: "capture-writer-health",
            label: localizedAppText("capture.detail.writer"),
            metricValue: progress.writerHealth.metricValue(display: writerHealth)
        ),
    ]
}

struct CaptureRecordingScreen: View {
    let deviceKind: String?
    let captureStatusText: String?
    let captureStatusTone: PevStatusStripTone
    let captureProgress: CaptureProgress?
    let activeLabels: Set<CaptureQuickLabel>
    let isFinishing: Bool
    let finishCapture: () -> Void
    let startCaptureLabel: (CaptureQuickLabel) -> Void
    let stopCaptureLabel: (CaptureQuickLabel) -> Void

    var body: some View {
        VStack(spacing: 0) {
            PevDashboardScaffold(
                sectionTitle: localizedAppText("capture.section.record"),
                bottomPadding: 24,
                allowsVerticalScroll: true,
                contentSpacing: 16,
                horizontalPadding: 18,
                showsHeader: false
            ) {
                PevScreenTitleBlock(
                    title: deviceKind ?? localizedAppText("capture.session"),
                    subtitle: localizedAppText("capture.session")
                )

                PevStatusStrip(
                    text: captureStatusText ?? localizedAppText("capture.status.recording_locally_without_file"),
                    tone: captureStatusTone
                )
                .accessibilityIdentifier("capture.status")

                if let captureProgress {
                    PevDashboardKeyValueRows(rows: captureSessionDetailRows(progress: captureProgress))
                }

                CaptureLabelControls(
                    activeLabels: activeLabels,
                    startCaptureLabel: startCaptureLabel,
                    stopCaptureLabel: stopCaptureLabel
                )
            }

            stopCaptureButton
                .padding(.horizontal, 18)
                .padding(.vertical, 12)
                .background(PevColors.pageBackground)
        }
        .background(PevColors.pageBackground)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("capture.screen")
    }

    private var stopCaptureButton: some View {
        Button(role: .destructive, action: finishCapture) {
            Text(localizedAppText("capture.stop"))
                .font(.callout.weight(.bold))
                .foregroundStyle(.black)
                .padding(.horizontal, 16)
                .frame(minHeight: 44)
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity, minHeight: 44)
        .background(Capsule().fill(CaptureActionButtonTone.finish.tint))
        .disabled(isFinishing)
        .accessibilityIdentifier("capture.stop")
    }
}

enum CaptureActionButtonTone: Equatable, Sendable {
    case start
    case stop
    case finish

    static func forState(isActive: Bool) -> Self {
        isActive ? .stop : .start
    }

    fileprivate var tint: Color {
        switch self {
        case .start: PevColors.yellow
        case .stop: PevColors.orange
        case .finish: PevColors.yellow
        }
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
        .background(PevDashboardCardBackground(cornerRadius: 8, fill: .black))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("capture.label.\(label.id)")
    }

    private var status: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label.title)
                .font(.headline)
                .foregroundStyle(.white)
            Text(stateText)
                .font(.caption)
                .foregroundStyle(.white)
                .accessibilityHidden(true)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label.title)
        .accessibilityValue(stateText)
    }

    private var stateText: String {
        localizedAppText(isActive ? "capture.label.active" : "capture.label.idle")
    }

    private var actions: some View {
        Button(role: isActive ? .destructive : nil) {
            if isActive {
                stop()
            } else {
                start()
            }
        } label: {
            Text(label.actionTitle(isActive: isActive))
                .font(.callout.weight(.bold))
                .foregroundStyle(CaptureActionButtonTone.forState(isActive: isActive).tint)
        }
        .buttonStyle(.plain)
        .frame(minHeight: 44)
        .contentShape(.rect)
        .accessibilityIdentifier("capture.label.\(label.id).action")
    }
}
