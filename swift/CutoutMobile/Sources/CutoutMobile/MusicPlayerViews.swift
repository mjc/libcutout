import CutoutMobileFFI
import Foundation
import SwiftUI

/// A small, reusable control surface for Ride and Map. It renders metadata only;
/// neither artwork nor an audio stream crosses the Rust ride boundary.
public struct MusicCompactPlayer: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    public let nowPlaying: MusicNowPlaying
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onOpenDetails: () -> Void
    public let onOpenSettings: () -> Void
    public let onDismiss: () -> Void
    @State private var accessibilityAnnouncementTracker = MusicAccessibilityAnnouncementTracker()

    public init(
        nowPlaying: MusicNowPlaying,
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onOpenDetails: @escaping () -> Void,
        onOpenSettings: @escaping () -> Void,
        onDismiss: @escaping () -> Void = {}
    ) {
        self.nowPlaying = nowPlaying
        self.onCommand = onCommand
        self.onOpenDetails = onOpenDetails
        self.onOpenSettings = onOpenSettings
        self.onDismiss = onDismiss
    }

    public var body: some View {
        HStack(spacing: 8) {
            MusicTrackButton(nowPlaying: nowPlaying, action: onOpenDetails)
                .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
            MusicTransportControls(nowPlaying: nowPlaying, includesPrevious: false, onCommand: onCommand)
                .fixedSize(horizontal: true, vertical: false)
            if nowPlaying.playPauseCommand == nil && !nowPlaying.isCommandAvailable(.next)
                && nowPlaying.isCommandAvailable(.openProvider)
            {
                MusicPlayerIconButton(
                    systemImage: "arrow.up.forward.app",
                    label: pevLocalizedText("music.open_named_provider", nowPlaying.providerName),
                    action: { onCommand(.openProvider) },
                    accessibilityIdentifier: "music.open-provider"
                )
            }
        }
        .padding(.horizontal, 8)
        .frame(height: 44)
        .tint(PevDashboardColors.brand)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("music.compact-player")
        .onChange(of: nowPlaying) { _, nowPlaying in
            guard let announcement = accessibilityAnnouncementTracker.next(for: nowPlaying) else {
                return
            }
            AccessibilityNotification.Announcement(announcement).post()
        }
    }
}

private struct MusicTrackButton: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    let nowPlaying: MusicNowPlaying
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                MusicArtworkView(
                    artwork: nowPlaying.artwork,
                    size: 32,
                    cornerRadius: 6,
                    accessibilityLabel: nowPlaying.artworkAccessibilityLabel
                )
                .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text(dynamicTypeSize > .large ? nowPlaying.statusText ?? nowPlaying.title : nowPlaying.title)
                        .font(.subheadline.weight(.semibold))
                        .accessibilityIdentifier("music.now-playing-title")
                    if dynamicTypeSize <= .large {
                        let subtitle =
                            nowPlaying.statusText == nowPlaying.title
                            ? nowPlaying.artist : nowPlaying.statusText ?? nowPlaying.artist
                        if !subtitle.isEmpty {
                            Text(subtitle)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .lineLimit(1)
                .dynamicTypeSize(...DynamicTypeSize.xxxLarge)
                .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
            }
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(PevDashboardColors.primaryText)
        // The compact title may truncate. Expose its complete metadata once as
        // the button value; the detail sheet honors the full Dynamic Type size.
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(pevLocalizedText("music.expand"))
        .accessibilityValue(nowPlaying.accessibilitySummary)
        .accessibilityIdentifier("music.expand")
    }
}

private struct MusicTransportControls: View {
    let nowPlaying: MusicNowPlaying
    var includesPrevious = true
    let onCommand: (MobileMusicCommandDto) -> Void

    var body: some View {
        HStack(spacing: 0) {
            if includesPrevious && nowPlaying.isCommandAvailable(.previous) {
                MusicPlayerIconButton(
                    systemImage: "backward.fill",
                    label: pevLocalizedText("music.previous"),
                    action: { onCommand(.previous) }
                )
            }
            if let command = nowPlaying.playPauseCommand {
                MusicPlayerIconButton(
                    systemImage: command == .pause ? "pause.fill" : "play.fill",
                    label: pevLocalizedText(command == .pause ? "music.pause" : "music.play"),
                    action: { onCommand(command) },
                    isProminent: true
                )
            }
            if nowPlaying.isCommandAvailable(.next) {
                MusicPlayerIconButton(
                    systemImage: "forward.fill",
                    label: pevLocalizedText("music.next"),
                    action: { onCommand(.next) }
                )
            }
        }
    }
}

private struct MusicPlayerIconButton: View {
    let systemImage: String
    let label: String
    let action: () -> Void
    var accessibilityIdentifier: String?
    var isProminent = false

    init(
        systemImage: String,
        label: String,
        action: @escaping () -> Void,
        accessibilityIdentifier: String? = nil,
        isProminent: Bool = false
    ) {
        self.systemImage = systemImage
        self.label = label
        self.action = action
        self.accessibilityIdentifier = accessibilityIdentifier
        self.isProminent = isProminent
    }

    var body: some View {
        if let accessibilityIdentifier, !accessibilityIdentifier.isEmpty {
            button.accessibilityIdentifier(accessibilityIdentifier)
        } else {
            button
        }
    }

    private var button: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.body.weight(.semibold))
                .dynamicTypeSize(...DynamicTypeSize.xxxLarge)
                .foregroundStyle(isProminent ? PevDashboardColors.brand : PevDashboardColors.primaryText)
                .frame(minWidth: 44, minHeight: 44)
                .background(
                    isProminent ? PevDashboardColors.brand.opacity(0.12) : .clear,
                    in: Circle()
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

enum MusicSettingsPresentation {
    static func connectionActionTitle(
        provider: MobileMusicProviderDto,
        state: MobileMusicPlaybackStateDto?
    ) -> String {
        let key =
            state == .stale || state == .disconnected
            ? "music.reconnect_provider" : "music.connect_provider"
        return pevLocalizedText(key, provider.title)
    }

    static func showsReauthorize(
        provider: MobileMusicProviderDto,
        state: MobileMusicPlaybackStateDto?
    ) -> Bool {
        provider == .spotify && state == .unauthorized
    }
}

private struct MusicTimelineRow: View {
    let event: MobileMusicRideEventDto

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(event.timelineItemTitle)
                    .lineLimit(1)
                Text(event.timelineSubtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            Text(
                Date(timeIntervalSince1970: Double(event.wallClockAtMs) / 1_000),
                style: .time
            )
            .font(.caption2.monospacedDigit())
            .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}

public struct MusicTimelineRows: View {
    public let events: [MobileMusicRideEventDto]

    public init(events: [MobileMusicRideEventDto]) {
        self.events = events.listeningHistoryEvents
    }

    public var body: some View {
        ForEach(events, id: \.timelineID) { event in
            MusicTimelineRow(event: event)
        }
    }
}

public struct MusicExpandedPlayer: View {
    @Environment(\.dismiss) private var dismiss
    public let nowPlaying: MusicNowPlaying
    public let timeline: [MobileMusicRideEventDto]
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onOpenSettings: (() -> Void)?
    public let onDismissPlayer: (() -> Void)?

    public init(
        nowPlaying: MusicNowPlaying,
        timeline: [MobileMusicRideEventDto] = [],
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onOpenSettings: (() -> Void)? = nil,
        onDismissPlayer: (() -> Void)? = nil
    ) {
        self.nowPlaying = nowPlaying
        self.timeline = timeline.listeningHistoryEvents
        self.onCommand = onCommand
        self.onOpenSettings = onOpenSettings
        self.onDismissPlayer = onDismissPlayer
    }

    public var body: some View {
        NavigationStack {
            Form {
                Section {
                    MusicExpandedHero(nowPlaying: nowPlaying)
                    MusicTransportControls(nowPlaying: nowPlaying, onCommand: onCommand)
                        .frame(maxWidth: .infinity)
                    if nowPlaying.isCommandAvailable(.openProvider) {
                        Button(action: { onCommand(.openProvider) }) {
                            Label(
                                pevLocalizedText("music.open_named_provider", nowPlaying.providerName),
                                systemImage: "arrow.up.forward.app"
                            )
                        }
                        .accessibilityIdentifier("music.open-provider")
                    } else {
                        Text(nowPlaying.providerName)
                            .foregroundStyle(.secondary)
                    }
                }
                if onOpenSettings != nil || onDismissPlayer != nil {
                    Section {
                        if let onOpenSettings {
                            Button(action: onOpenSettings) {
                                Label(pevLocalizedText("music.settings.open"), systemImage: "gearshape")
                            }
                            .accessibilityIdentifier("music.open-settings")
                        }
                        if onDismissPlayer != nil {
                            Button(action: hidePlayer) {
                                Label(pevLocalizedText("music.hide"), systemImage: "eye.slash")
                            }
                            .accessibilityIdentifier("music.hide")
                        }
                    }
                }
                if !timeline.isEmpty {
                    Section(pevLocalizedText("music.timeline.title")) {
                        MusicTimelineRows(events: timeline)
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(pevLocalizedText("music.expand"))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        dismiss()
                    } label: {
                        Text(pevLocalizedText("music.done"))
                            .frame(minWidth: 44, minHeight: 44)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("music.done")
                }
            }
        }
        .tint(PevDashboardColors.brand)
    }

    private func hidePlayer() {
        onDismissPlayer?()
        dismiss()
    }
}

/// Configuration is separate from live playback. Bindings publish changes
/// directly to the app's persisted, Rust-owned settings instead of shadow state.
public struct MusicSettingsView: View {
    let nowPlaying: MusicNowPlaying?
    @Binding var selectedProvider: MobileMusicProviderDto
    @Binding var historyPolicy: MobileMusicHistoryPolicyDto
    let historyUnavailable: Bool
    let historySaveError: MobileRideMapError?
    let onConnect: () -> Void
    let onAuthorizeSpotify: () -> Void
    let onOpenProvider: () -> Void

    public init(
        nowPlaying: MusicNowPlaying?,
        selectedProvider: Binding<MobileMusicProviderDto>,
        historyPolicy: Binding<MobileMusicHistoryPolicyDto>,
        historyUnavailable: Bool,
        historySaveError: MobileRideMapError?,
        onConnect: @escaping () -> Void,
        onAuthorizeSpotify: @escaping () -> Void,
        onOpenProvider: @escaping () -> Void
    ) {
        self.nowPlaying = nowPlaying
        _selectedProvider = selectedProvider
        _historyPolicy = historyPolicy
        self.historyUnavailable = historyUnavailable
        self.historySaveError = historySaveError
        self.onConnect = onConnect
        self.onAuthorizeSpotify = onAuthorizeSpotify
        self.onOpenProvider = onOpenProvider
    }

    public var body: some View {
        Form {
            Section(pevLocalizedText("music.provider.select")) {
                Picker(pevLocalizedText("music.provider.select"), selection: $selectedProvider) {
                    ForEach(MobileMusicProviderDto.allCases, id: \.self) { provider in
                        Text(provider.title).tag(provider)
                    }
                }
                .accessibilityIdentifier("music.provider-picker")
                if let nowPlaying {
                    Text(nowPlaying.statusText ?? nowPlaying.title)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("music.connection-status")
                } else {
                    Text(pevLocalizedText("music.state.not_connected"))
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("music.connection-status")
                }
                Button(
                    MusicSettingsPresentation.connectionActionTitle(
                        provider: selectedProvider, state: nowPlaying?.state
                    ), action: onConnect
                )
                .accessibilityIdentifier("music.connect-provider")
                if MusicSettingsPresentation.showsReauthorize(
                    provider: selectedProvider,
                    state: nowPlaying?.state
                ) {
                    Button(pevLocalizedText("music.authorize_spotify"), action: onAuthorizeSpotify)
                        .accessibilityIdentifier("music.authorize-spotify")
                }
                Button(pevLocalizedText("music.open_named_provider", selectedProvider.title), action: onOpenProvider)
                    .accessibilityIdentifier("music.open-provider")
            }
            Section {
                Picker(pevLocalizedText("music.history.title"), selection: $historyPolicy) {
                    ForEach(MobileMusicHistoryPolicyDto.allCases, id: \.self) { policy in
                        MusicHistoryPolicyLabel(policy: policy).tag(policy)
                    }
                }
                .accessibilityIdentifier("music.history-picker")
                .accessibilityValue(historyPolicy.title)
                if historyUnavailable {
                    Label(pevLocalizedText("music.state.unavailable"), systemImage: "exclamationmark.triangle")
                        .accessibilityIdentifier("music.history-unavailable")
                }
                if historySaveError != nil {
                    Label(pevLocalizedText("music.history.save_error"), systemImage: "exclamationmark.triangle")
                        .accessibilityIdentifier("music.history-error")
                }
            } header: {
                Text(pevLocalizedText("music.history.title"))
            } footer: {
                Text(historyPolicy.explanation + " " + pevLocalizedText("music.history.privacy"))
            }
        }
        .formStyle(.grouped)
        .navigationTitle(pevLocalizedText("music.settings.title"))
        .accessibilityIdentifier("music.settings.screen")
    }
}

private struct MusicExpandedHero: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    let nowPlaying: MusicNowPlaying

    var body: some View {
        let layout =
            dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 16))
            : AnyLayout(HStackLayout(spacing: 16))
        layout {
            MusicArtworkView(
                artwork: nowPlaying.artwork,
                size: 84,
                cornerRadius: 12,
                accessibilityLabel: nowPlaying.artworkAccessibilityLabel
            )
            .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 6) {
                Text(nowPlaying.title)
                    .font(.title2.weight(.bold))
                    .lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 2)
                if !nowPlaying.artist.isEmpty {
                    Text(nowPlaying.artist)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(dynamicTypeSize.isAccessibilitySize ? nil : 2)
                }
                if let status = nowPlaying.statusText, status != nowPlaying.title {
                    Text(status)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, 8)
    }
}

struct MusicArtworkView: View {
    let artwork: MusicArtwork?
    let size: CGFloat
    let cornerRadius: CGFloat
    let accessibilityLabel: String

    var body: some View {
        Group {
            if let image = artwork?.image {
                Image(decorative: image, scale: 1, orientation: .up)
                    .resizable()
                    .scaledToFill()
                    .frame(width: size, height: size)
                    .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
                    .accessibilityLabel(accessibilityLabel)
            }
        }
    }
}

private struct MusicHistoryPolicyLabel: View {
    let policy: MobileMusicHistoryPolicyDto

    var body: some View {
        Text(policy.title)
            .accessibilityIdentifier("music.history-policy.\(policy.musicAccessibilityIdentifier)")
    }
}

private struct MusicCompactPlayerFrameKey: EnvironmentKey {
    static let defaultValue: CGRect? = nil
}

extension EnvironmentValues {
    /// Floating native accessories can cover fixed layouts without changing their safe-area inset.
    public var musicCompactPlayerFrame: CGRect? {
        get { self[MusicCompactPlayerFrameKey.self] }
        set { self[MusicCompactPlayerFrameKey.self] = newValue }
    }
}

/// Shared primary-navigation composition for the compact player.
public struct MusicCompactPlayerInset: ViewModifier {
    @State private var accessoryFrame: CGRect?
    public let nowPlaying: MusicNowPlaying?
    public let isHidden: Bool
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onOpenDetails: () -> Void
    public let onOpenSettings: () -> Void
    public let onDismiss: () -> Void

    public func body(content: Content) -> some View {
        #if os(iOS)
            content
                .environment(\.musicCompactPlayerFrame, showsPlayer ? accessoryFrame : nil)
                .tabViewBottomAccessory(isEnabled: showsPlayer) {
                    player
                        .frame(maxHeight: .infinity)
                        .onGeometryChange(for: CGRect.self) { proxy in
                            proxy.frame(in: .global)
                        } action: { frame in
                            accessoryFrame = frame.width > 0 && frame.height > 0 ? frame : nil
                        }
                }
                .onChange(of: showsPlayer) { _, isShown in
                    if !isShown { accessoryFrame = nil }
                }
        #else
            content.safeAreaInset(edge: .bottom, spacing: 0) {
                if showsPlayer { player }
            }
        #endif
    }

    var showsPlayer: Bool {
        !isHidden && nowPlaying?.showsCompactPlayer == true
    }

    private var player: some View {
        Group {
            if let nowPlaying {
                MusicCompactPlayer(
                    nowPlaying: nowPlaying,
                    onCommand: onCommand,
                    onOpenDetails: onOpenDetails,
                    onOpenSettings: onOpenSettings,
                    onDismiss: onDismiss
                )
            }
        }
        .padding(.horizontal, 4)
    }
}

extension View {
    /// Attach to the primary TabView on iOS so playback stays above the system tab bar.
    public func musicCompactPlayer(
        nowPlaying: MusicNowPlaying?,
        isHidden: Bool,
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onOpenDetails: @escaping () -> Void,
        onOpenSettings: @escaping () -> Void,
        onDismiss: @escaping () -> Void
    ) -> some View {
        modifier(
            MusicCompactPlayerInset(
                nowPlaying: nowPlaying,
                isHidden: isHidden,
                onCommand: onCommand,
                onOpenDetails: onOpenDetails,
                onOpenSettings: onOpenSettings,
                onDismiss: onDismiss
            )
        )
    }
}

#if DEBUG
    private struct MusicCompactPlayerPreview: View {
        let state: MobileMusicPlaybackStateDto

        var body: some View {
            MusicCompactPlayer(
                nowPlaying: MusicNowPlaying(
                    provider: .appleMusic,
                    state: state,
                    item: MobileMusicItemDto(
                        identifier: "preview-track",
                        title: "Everything In Its Right Place",
                        artist: "Radiohead"
                    ),
                    capabilities: .init(
                        previous: true,
                        play: true,
                        pause: true,
                        next: true,
                        openProvider: true
                    )
                ),
                onCommand: { _ in },
                onOpenDetails: {},
                onOpenSettings: {}
            )
            .padding(12)
            .frame(width: 360)
            .background(PevDashboardColors.pageBackground)
        }
    }

    #Preview("Playing") {
        MusicCompactPlayerPreview(state: .playing)
    }

    #Preview("Paused") {
        MusicCompactPlayerPreview(state: .paused)
    }

    #Preview("Recovery") {
        MusicCompactPlayerPreview(state: .stale)
    }

    #Preview("Accessibility") {
        MusicCompactPlayerPreview(state: .playing)
            .environment(\.dynamicTypeSize, .accessibility3)
    }
#endif
