import CutoutMobile
import SwiftUI

struct CaptureStatusPill: View {
    let text: String
    let scale: CGFloat

    var body: some View {
        HStack(spacing: 10 * scale) {
            Circle()
                .fill(MockupColors.green)
                .frame(width: 10 * scale, height: 10 * scale)
            Text(text)
                .font(.system(size: 13 * scale, weight: .semibold))
                .foregroundStyle(MockupColors.primaryText)
                .lineLimit(2)
                .minimumScaleFactor(0.78)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16 * scale)
        .frame(minHeight: 42 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 18 * scale))
    }
}

struct CaptureRecordingScreen: View {
    let deviceKind: String?
    let captureStatusText: String?
    let activeLabels: Set<CaptureQuickLabel>
    let disconnect: () -> Void
    let startCaptureLabel: (CaptureQuickLabel) -> Void
    let stopCaptureLabel: (CaptureQuickLabel) -> Void

    var body: some View {
        MockupScreenScaffold(
            sectionTitle: "record",
            bottomPadding: 24,
            allowsVerticalScroll: false,
            contentSpacing: 16,
            horizontalPadding: 18
        ) { scale, _ in
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 7 * scale) {
                    Text(deviceKind ?? "Capture session")
                        .font(.system(size: 34 * scale, weight: .bold))
                        .lineLimit(1)
                        .minimumScaleFactor(0.72)
                    Text("Capture session")
                        .font(.system(size: 15 * scale, weight: .semibold))
                        .foregroundStyle(MockupColors.muted)
                }
                Spacer(minLength: 12 * scale)
                Button {
                    disconnect()
                } label: {
                    Text("Stop")
                        .font(.system(size: 18 * scale, weight: .bold))
                        .foregroundStyle(MockupColors.yellow)
                }
                .buttonStyle(.plain)
            }

            CaptureStatusPill(text: captureStatusText ?? "Recording locally", scale: scale)

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
        LazyVGrid(columns: columns, spacing: 10 * scale) {
            ForEach(CaptureQuickLabel.allCases) { label in
                HStack(spacing: 10 * scale) {
                    VStack(alignment: .leading, spacing: 3 * scale) {
                        Text(label.title)
                            .font(.system(size: 16 * scale, weight: .bold))
                            .foregroundStyle(MockupColors.primaryText)
                            .lineLimit(1)
                        Text(activeLabels.contains(label) ? "active" : "idle")
                            .font(.system(size: 11 * scale, weight: .semibold))
                            .foregroundStyle(activeLabels.contains(label) ? MockupColors.green : MockupColors.muted)
                    }
                    Spacer(minLength: 8 * scale)
                    captureLabelButton("Start", scale: scale) {
                        startCaptureLabel(label)
                    }
                    captureLabelButton("Stop", scale: scale) {
                        stopCaptureLabel(label)
                    }
                }
                .padding(.horizontal, 14 * scale)
                .frame(minHeight: 58 * scale)
                .background(CardBackground(cornerRadius: 8 * scale))
            }
        }
    }

    private func captureLabelButton(_ title: String, scale: CGFloat, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12 * scale, weight: .bold))
                .lineLimit(1)
                .foregroundStyle(MockupColors.yellow)
                .frame(width: 58 * scale, height: 32 * scale)
                .background(
                    RoundedRectangle(cornerRadius: 8 * scale, style: .continuous)
                        .fill(MockupColors.warningFill)
                        .overlay(
                            RoundedRectangle(cornerRadius: 8 * scale, style: .continuous)
                                .stroke(MockupColors.warningStroke, lineWidth: 1 * scale)
                        )
                )
        }
        .buttonStyle(.plain)
    }
}
