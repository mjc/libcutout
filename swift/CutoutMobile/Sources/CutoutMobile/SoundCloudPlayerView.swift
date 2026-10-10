import CutoutMobileFFI
import SwiftUI

/// A small track picker and native transport for the PEV app.
public struct SoundCloudPlayerView: View {
    let player: SoundCloudNativePlayer
    @State private var query = ""

    public init(player: SoundCloudNativePlayer) { self.player = player }

    public var body: some View {
        List {
            Section {
                // Official 7-bar Cloudmark from developers.soundcloud.com/docs/api/buttons-logos.
                if let source = URL(string: "https://soundcloud.com") {
                    Link(destination: source) {
                        Image("SoundCloudLogo", bundle: .module)
                            .resizable().scaledToFit().frame(width: 40, height: 24)
                            .padding(10).background(Color(red: 18 / 255, green: 18 / 255, blue: 18 / 255))
                    }
                    .accessibilityLabel(pevLocalizedText("music.provider.soundcloud"))
                }
                TextField(pevLocalizedText("music.soundcloud.search"), text: $query)
                    .submitLabel(.search)
                    .onSubmit { player.search(query) }
                    .accessibilityIdentifier("music.soundcloud.search-field")
                Button(pevLocalizedText("music.soundcloud.search")) { player.search(query) }
                    .disabled(query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityIdentifier("music.soundcloud.search-button")
            }
            if let track = player.snapshot.track {
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(track.title).font(.headline)
                            .accessibilityIdentifier("music.soundcloud.current-title")
                        Text(track.uploader).foregroundStyle(.secondary)
                        Text("\(player.elapsedSeconds / 60):\(String(format: "%02d", player.elapsedSeconds % 60))")
                            .monospacedDigit()
                            .accessibilityIdentifier("music.soundcloud.elapsed")
                            .accessibilityValue(String(player.elapsedSeconds))
                        if let url = URL(string: track.permalink) {
                            Link(pevLocalizedText("music.soundcloud.track_link"), destination: url)
                        }
                    }
                    HStack(spacing: 24) {
                        control("music.previous", icon: "backward.end.fill", command: .previous)
                            .disabled(!player.snapshot.hasPrevious)
                        control(
                            player.snapshot.state == .playing || player.snapshot.state == .loading
                                ? "music.pause" : "music.play",
                            icon: player.snapshot.state == .playing || player.snapshot.state == .loading
                                ? "pause.fill" : "play.fill",
                            command: player.snapshot.state == .playing || player.snapshot.state == .loading
                                ? .pause : .play
                        )
                        control("music.next", icon: "forward.end.fill", command: .next)
                            .disabled(!player.snapshot.hasNext)
                        if player.snapshot.state == .loading { ProgressView() }
                    }
                    .buttonStyle(.borderless)
                }
            }
            if let error = player.errorText {
                Section { Text(error).foregroundStyle(.red).accessibilityIdentifier("music.soundcloud.error") }
            }
            Section {
                if player.isSearching { ProgressView() }
                ForEach(player.tracks, id: \.urn) { track in
                    Button {
                        player.select(track)
                    } label: {
                        HStack {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(track.title).foregroundStyle(.primary)
                                Text(track.uploader).font(.subheadline).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Image(systemName: "play.fill")
                        }
                        .frame(minHeight: 44)
                    }
                    .accessibilityLabel("\(track.title), \(track.uploader), \(pevLocalizedText("music.play"))")
                    .accessibilityIdentifier("music.soundcloud.track.\(track.urn)")
                }
            }
        }
        .navigationTitle(pevLocalizedText("music.provider.soundcloud"))
        .accessibilityIdentifier("music.soundcloud.player")
    }

    private func control(_ label: String, icon: String, command: MobileSoundCloudCommand) -> some View {
        Button {
            player.command(command)
        } label: {
            Image(systemName: icon).frame(minWidth: 44, minHeight: 44)
        }
        .accessibilityLabel(pevLocalizedText(label))
        .accessibilityIdentifier("music.soundcloud.\(label)")
    }
}
