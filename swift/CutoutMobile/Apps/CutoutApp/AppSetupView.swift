import CutoutMobile
import SwiftUI

struct AppSetupView: View {
    let model: CutoutAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var path: [Destination]

    private enum Destination: Hashable {
        case music
    }

    init(model: CutoutAppModel, opensMusic: Bool = false) {
        self.model = model
        _path = State(initialValue: opensMusic ? [.music] : [])
    }

    var body: some View {
        NavigationStack(path: $path) {
            Form {
                Section {
                    NavigationLink(value: Destination.music) {
                        Label(pevLocalizedText("music.settings.title"), systemImage: "music.note")
                    }
                    .accessibilityIdentifier("setup.music")
                }
                Section(localizedAppText("setup.device")) {
                    Button(localizedAppText("picker.saved_device.forget"), role: .destructive) {
                        model.forgetSavedDevice()
                    }
                    .disabled(!model.hasSavedDevice)
                    .accessibilityIdentifier("setup.forget-saved-device")
                }
            }
            .formStyle(.grouped)
            .navigationTitle(localizedAppText("picker.section.setup"))
            .accessibilityIdentifier("setup.screen")
            .toolbar { doneToolbar }
            .navigationDestination(for: Destination.self) { _ in
                MusicSettingsView(
                    nowPlaying: model.musicSettingsNowPlaying,
                    selectedProvider: Binding(
                        get: { model.selectedMusicProvider },
                        set: model.selectMusicProvider
                    ),
                    historyPolicy: Binding(
                        get: { model.musicHistoryPolicy },
                        set: { _ = model.setMusicHistoryPolicy($0) }
                    ),
                    historyUnavailable: model.musicHistoryUnavailable,
                    onConnect: model.connectMusic,
                    onAuthorizeSpotify: model.authorizeSpotify,
                    onOpenProvider: {
                        Task { @MainActor in
                            _ = await model.handleMusicCommand(.openProvider)
                        }
                    }
                )
                .toolbar { doneToolbar }
            }
        }
        .tint(PevDashboardColors.yellow)
    }

    @ToolbarContentBuilder
    private var doneToolbar: some ToolbarContent {
        ToolbarItem(placement: .confirmationAction) {
            Button(pevLocalizedText("music.done")) { dismiss() }
                .accessibilityIdentifier("setup.done")
        }
    }
}
