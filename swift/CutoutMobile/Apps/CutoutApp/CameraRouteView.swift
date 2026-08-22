import CutoutMobile
import SwiftUI

struct CameraRouteContainerView: View {
    @State private var adapter = CameraLocalNetworkAdapter()

    var body: some View {
        CameraRouteView(presentation: adapter.presentation)
            .task {
                adapter.start()
            }
            .onDisappear {
                adapter.stop()
            }
    }
}

struct CameraRouteView: View {
    let presentation: CameraPresentation

    init(presentation: CameraPresentation = .initial) {
        self.presentation = presentation
    }

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: localizedAppText("navigation.section.camera"),
            bottomPadding: 32,
            horizontalPadding: 20
        ) {
            if #available(iOS 26, macOS 26, *) {
                GlassEffectContainer(spacing: 16) {
                    cameraCards
                }
            } else {
                cameraCards
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("camera.screen")
    }

    @ViewBuilder
    private var cameraCards: some View {
        CameraStatusCard(presentation: presentation)
        CameraTruthCard(presentation: presentation)
    }
}

private struct CameraStatusCard: View {
    let presentation: CameraPresentation

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(localizedAppText("camera.setup.title"), systemImage: "video.fill")
                .font(.title3.weight(.bold))

            Text(connectionTitle)
                .font(.headline)

            Text(connectionDetail)
                .font(.body)
                .foregroundStyle(PevColors.muted)

            if let profileName = presentation.profileName {
                Label(profileName, systemImage: "checkmark.seal")
                    .font(.subheadline.weight(.semibold))
            } else {
                Label(localizedAppText("camera.profile.pending"), systemImage: "questionmark.diamond")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(20)
        .cameraSurface(tint: PevColors.cyan)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("camera.setup.card")
    }

    private var connectionTitle: String {
        switch presentation.connection {
        case .notConfigured: localizedAppText("camera.connection.not_configured")
        case .permissionRequired: localizedAppText("camera.connection.permission_required")
        case .wifiRequired: localizedAppText("camera.connection.wifi_required")
        case .discovering: localizedAppText("camera.connection.discovering")
        case .connected: localizedAppText("camera.connection.connected")
        case .unsupported: localizedAppText("camera.connection.unsupported")
        }
    }

    private var connectionDetail: String {
        switch presentation.connection {
        case .notConfigured:
            localizedAppText("camera.connection.detail.not_configured")
        case .permissionRequired:
            localizedAppText("camera.connection.detail.permission_required")
        case .wifiRequired:
            localizedAppText("camera.connection.detail.wifi_required")
        case .discovering:
            localizedAppText("camera.connection.detail.discovering")
        case .connected:
            localizedAppText("camera.connection.detail.connected")
        case .unsupported:
            localizedAppText("camera.connection.detail.unsupported")
        }
    }
}

private struct CameraTruthCard: View {
    let presentation: CameraPresentation

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(localizedAppText("camera.truth.title"))
                .font(.headline)

            CameraTruthRow(
                title: localizedAppText("camera.truth.preview"),
                value: previewText,
                systemImage: "play.rectangle"
            )
            CameraTruthRow(
                title: localizedAppText("camera.truth.recording"),
                value: recordingText,
                systemImage: "record.circle"
            )
            CameraTruthRow(
                title: localizedAppText("camera.truth.storage"),
                value: storageText,
                systemImage: "sdcard"
            )

            Text(localizedAppText("camera.privacy.local_only"))
                .font(.footnote)
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(20)
        .cameraSurface(tint: PevColors.purple)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("camera.truth.card")
    }

    private var previewText: String {
        switch presentation.preview {
        case .stopped: localizedAppText("camera.preview.stopped")
        case .buffering: localizedAppText("camera.preview.buffering")
        case .live: localizedAppText("camera.preview.live")
        case .stale: localizedAppText("camera.preview.stale")
        case .interrupted: localizedAppText("camera.preview.interrupted")
        case .unavailable: localizedAppText("camera.preview.unavailable")
        }
    }

    private var recordingText: String {
        switch presentation.recording {
        case .unknown: localizedAppText("camera.recording.unknown")
        case .stopped: localizedAppText("camera.recording.stopped")
        case .recording: localizedAppText("camera.recording.active")
        }
    }

    private var storageText: String {
        switch presentation.storage {
        case .unknown: localizedAppText("camera.storage.unknown")
        case .present: localizedAppText("camera.storage.present")
        case .missing: localizedAppText("camera.storage.missing")
        case .error: localizedAppText("camera.storage.error")
        }
    }
}

private struct CameraTruthRow: View {
    let title: String
    let value: String
    let systemImage: String

    var body: some View {
        Label {
            HStack {
                Text(title)
                Spacer(minLength: 12)
                Text(value)
                    .foregroundStyle(PevColors.muted)
            }
        } icon: {
            Image(systemName: systemImage)
                .accessibilityHidden(true)
        }
        .font(.subheadline)
        .accessibilityElement(children: .combine)
    }
}

private extension View {
    @ViewBuilder
    func cameraSurface(tint: Color) -> some View {
        if #available(iOS 26, macOS 26, *) {
            self.glassEffect(.regular.tint(tint.opacity(0.18)), in: .rect(cornerRadius: 24))
        } else {
            self.background(PevDashboardCardBackground(cornerRadius: 24))
        }
    }
}

#Preview("Camera setup") {
    CameraRouteView()
}
