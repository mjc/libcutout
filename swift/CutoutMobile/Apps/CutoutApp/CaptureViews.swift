import CutoutMobile
import Foundation
import SwiftUI

func captureSessionDetailRows(progress: CaptureProgress) -> [PevDashboardKeyValueRow] {
    let elapsed = Duration.seconds(Double(progress.elapsedMilliseconds) / 1_000)
        .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
    let fileSize = ByteCountFormatter.string(
        fromByteCount: Int64(clamping: progress.fileSizeBytes),
        countStyle: .file
    )
    let writerHealth = progress.writerError == nil
        ? localizedAppText("capture.detail.writer.healthy")
        : localizedAppText("capture.detail.writer.failed")
    return [
        PevDashboardKeyValueRow(
            id: "capture-elapsed",
            label: localizedAppText("capture.detail.elapsed"),
            value: elapsed
        ),
        PevDashboardKeyValueRow(
            id: "capture-packets",
            label: localizedAppText("capture.detail.packets"),
            value: progress.notificationCount.formatted()
        ),
        PevDashboardKeyValueRow(
            id: "capture-file-size",
            label: localizedAppText("capture.detail.file_size"),
            value: fileSize
        ),
        PevDashboardKeyValueRow(
            id: "capture-queued-messages",
            label: localizedAppText("capture.detail.pending_writes"),
            value: progress.queuedMessageCount.formatted()
        ),
        PevDashboardKeyValueRow(
            id: "capture-writer-health",
            label: localizedAppText("capture.detail.writer"),
            value: writerHealth
        ),
    ]
}

struct CaptureRecordingScreen: View {
    let deviceKind: String?
    let captureStatusText: String?
    let captureProgress: CaptureProgress?
    let activeLabels: Set<CaptureQuickLabel>
    let isFinishing: Bool
    let finishCapture: () -> Void
    let startCaptureLabel: (CaptureQuickLabel) -> Void
    let stopCaptureLabel: (CaptureQuickLabel) -> Void

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: String(localized: "capture.section.record", table: "Localizable", bundle: appLocalizationBundle),
            bottomPadding: 24,
            allowsVerticalScroll: true,
            contentSpacing: 16,
            horizontalPadding: 18,
            showsHeader: false
        ) {
            PevScreenTitleBlock(
                title: deviceKind ?? String(
                    localized: "capture.session",
                    table: "Localizable",
                    bundle: appLocalizationBundle
                ),
                subtitle: String(
                    localized: "capture.session",
                    table: "Localizable",
                    bundle: appLocalizationBundle
                )
            )

            PevStatusStrip(
                text: captureStatusText ?? String(
                    localized: "capture.status.recording_locally_without_file",
                    table: "Localizable",
                    bundle: appLocalizationBundle
                )
            )

            if let captureProgress {
                PevDashboardKeyValueRows(rows: captureSessionDetailRows(progress: captureProgress))
                    .accessibilityIdentifier("capture.session-details")
            }

            CaptureLabelControls(
                activeLabels: activeLabels,
                startCaptureLabel: startCaptureLabel,
                stopCaptureLabel: stopCaptureLabel
            )
        }
        .safeAreaInset(edge: .bottom) {
            stopCaptureButton
                .frame(maxWidth: .infinity, alignment: .trailing)
                .padding(.horizontal, 18)
                .padding(.vertical, 12)
        }
        .foregroundStyle(PevColors.primaryText)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("capture.screen")
    }

    private var stopCaptureButton: some View {
        Button("capture.stop", role: .destructive, action: finishCapture)
            .buttonStyle(CaptureActionButtonStyle(tone: .finish))
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

    fileprivate var fill: Color {
        switch self {
        case .start: PevColors.yellow
        case .stop: PevColors.orange
        case .finish: PevColors.yellow
        }
    }
}

private struct CaptureActionButtonStyle: ButtonStyle {
    let tone: CaptureActionButtonTone

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body.weight(.semibold))
            .foregroundStyle(Color.black)
            .padding(.horizontal, 18)
            .frame(minHeight: 44)
            .background(Capsule().fill(tone.fill))
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
            Text(stateText)
                .font(.caption)
                .foregroundStyle(isActive ? PevColors.primaryText : PevColors.muted)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label.title)
        .accessibilityValue(stateText)
    }

    private var stateText: String {
        String(
            localized: isActive ? "capture.label.active" : "capture.label.idle",
            table: "Localizable",
            bundle: appLocalizationBundle
        )
    }

    private var actions: some View {
        Button(label.actionTitle(isActive: isActive), role: isActive ? .destructive : nil) {
            if isActive {
                stop()
            } else {
                start()
            }
        }
        .buttonStyle(CaptureActionButtonStyle(tone: .forState(isActive: isActive)))
        .accessibilityIdentifier("capture.label.\(label.id).action")
    }
}
