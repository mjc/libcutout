import CutoutMobile
import SwiftUI

func captureSessionDetailRows(progress: CaptureProgress) -> [PevDashboardKeyValueRow] {
    let writerHealth = switch progress.writerHealth {
    case .healthy:
        localizedAppText("capture.detail.writer.healthy")
    case .failed:
        localizedAppText("capture.detail.writer.failed")
    }
    let rows = [
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
    return rows
}

struct CaptureRecordingScreen: View {
    let deviceKind: String?
    let advertisedName: String?
    let captureStatusText: String?
    let captureStatusTone: PevStatusStripTone
    let captureProgress: CaptureProgress?
    let activeLabels: Set<CaptureQuickLabel>
    let annotationErrorText: String?
    let dismissAnnotationError: () -> Void
    let isFinishing: Bool
    let canFinish: Bool
    let canAnnotate: Bool
    let finishCapture: () -> Void
    let startCaptureLabel: (CaptureQuickLabel) -> Void
    let stopCaptureLabel: (CaptureQuickLabel) -> Void

    var body: some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 8) {
                    Text(deviceKind ?? localizedAppText("capture.session"))
                        .font(.title2.weight(.semibold))
                    if let advertisedName {
                        Text(advertisedName).foregroundStyle(.secondary)
                    }
                    Text(captureProgress?.elapsedMetricValue.displayText ?? "—")
                        .font(.largeTitle.monospacedDigit())
                        .accessibilityLabel(localizedAppText("capture.detail.elapsed"))
                        .accessibilityValue(captureProgress?.elapsedMetricValue.accessibilityText ?? "—")
                    PevStatusStrip(
                        text: captureStatusText ?? localizedAppText("captures.starting"),
                        tone: captureStatusTone
                    )
                    .accessibilityIdentifier("capture.status")
                }
                .padding(.vertical, 8)
            }

            Section {
                DisclosureGroup {
                    CaptureLabelControls(
                        activeLabels: activeLabels,
                        startCaptureLabel: startCaptureLabel,
                        stopCaptureLabel: stopCaptureLabel
                    )
                    .disabled(!canAnnotate)
                } label: {
                    Text(localizedAppText("captures.labels"))
                        .accessibilityIdentifier("captures.labels")
                }
            } footer: {
                Text(localizedAppText("captures.labels_hint"))
            }

            if let captureProgress {
                Section {
                    DisclosureGroup {
                        PevDashboardKeyValueRows(rows: captureSessionDetailRows(progress: captureProgress))
                    } label: {
                        Text(localizedAppText("captures.technical"))
                            .accessibilityIdentifier("captures.technical")
                    }
                }
            }

        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 8) {
                if canFinish {
                    Button(action: finishCapture) {
                        HStack {
                            if isFinishing { ProgressView().tint(.black) }
                            Text(localizedAppText(isFinishing ? "captures.saving" : "capture.stop"))
                        }
                        .frame(maxWidth: .infinity, minHeight: 44)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(PevColors.yellow)
                    .foregroundStyle(.black)
                    .disabled(isFinishing)
                    .accessibilityIdentifier("capture.stop")
                } else {
                    Text(localizedAppText("captures.automatic_hint")).foregroundStyle(.secondary)
                }
                Text(localizedAppText("captures.leave_hint"))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            .padding(18)
            .background(PevColors.pageBackground)
        }
        .scrollContentBackground(.hidden)
        .background(PevColors.pageBackground)
        .navigationTitle(localizedAppText("captures.active"))
        .tint(PevColors.yellow)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("capture.screen")
        .alert(localizedAppText("captures.label_not_recorded"), isPresented: Binding(
            get: { annotationErrorText != nil },
            set: { if !$0 { dismissAnnotationError() } }
        )) {
            Button(localizedAppText("captures.label_error_dismiss"), role: .cancel, action: dismissAnnotationError)
        } message: {
            Text(annotationErrorText ?? "")
        }
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
