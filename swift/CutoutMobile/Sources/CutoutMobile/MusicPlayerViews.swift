import CutoutMobileFFI
import Foundation
import ImageIO
import SwiftUI

/// A small, reusable control surface for Ride and Map. It renders metadata only;
/// neither artwork bytes nor an audio stream cross the app boundary.
public struct MusicCompactPlayer: View {
    public let nowPlaying: MusicNowPlaying
    public let timeline: [MobileMusicRideEventDto]
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onOpenSettings: () -> Void
    public let onDismiss: () -> Void
    @State private var isExpanded = false
    @State private var accessibilityAnnouncementTracker = MusicAccessibilityAnnouncementTracker()

    public init(
        nowPlaying: MusicNowPlaying,
        timeline: [MobileMusicRideEventDto] = [],
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onOpenSettings: @escaping () -> Void,
        onDismiss: @escaping () -> Void = {}
    ) {
        self.nowPlaying = nowPlaying
        self.timeline = timeline
        self.onCommand = onCommand
        self.onOpenSettings = onOpenSettings
        self.onDismiss = onDismiss
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 12) {
                artworkView
                VStack(alignment: .leading, spacing: 3) {
                    Text(nowPlaying.title)
                        .lineLimit(1)
                        .font(.subheadline.weight(.bold))
                        .accessibilityIdentifier("music.now-playing-title")
                        .accessibilityValue(String(describing: nowPlaying.state))
                    Text(nowPlaying.statusText ?? nowPlaying.artist)
                        .lineLimit(1)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
            }
            HStack(spacing: 8) {
                if nowPlaying.requiresSetup {
                    Button(action: onOpenSettings) {
                        Label(pevLocalizedText("music.settings.open"), systemImage: "gearshape")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .tint(PevDashboardColors.yellow)
                    .accessibilityIdentifier("music.open-settings")
                }
                Spacer(minLength: 0)
                MusicTransportControls(nowPlaying: nowPlaying, onCommand: onCommand)
                MusicPlayerIconButton(
                    systemImage: "ellipsis",
                    label: pevLocalizedText("music.expand"),
                    action: { isExpanded = true },
                    accessibilityIdentifier: "music.expand"
                )
                MusicPlayerIconButton(
                    systemImage: "xmark",
                    label: pevLocalizedText("music.hide"),
                    action: onDismiss
                )
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .tint(PevDashboardColors.yellow)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 20))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(nowPlaying.accessibilitySummary)
        .onChange(of: nowPlaying) { _, nowPlaying in
            guard let announcement = accessibilityAnnouncementTracker.next(for: nowPlaying) else {
                return
            }
            AccessibilityNotification.Announcement(announcement).post()
        }
        .sheet(isPresented: $isExpanded) {
            MusicExpandedPlayer(
                nowPlaying: nowPlaying,
                timeline: timeline,
                onCommand: onCommand
            )
        }
    }

    private var artworkView: some View {
        MusicArtworkView(
            artwork: nowPlaying.artwork,
            size: 44,
            cornerRadius: 6,
            accessibilityLabel: nowPlaying.artworkAccessibilityLabel
        )
    }
}

private struct MusicTransportControls: View {
    let nowPlaying: MusicNowPlaying
    let onCommand: (MobileMusicCommandDto) -> Void

    var body: some View {
        HStack(spacing: 0) {
            if nowPlaying.isCommandAvailable(.previous) {
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
            if nowPlaying.isCommandAvailable(.openProvider) {
                MusicPlayerIconButton(
                    systemImage: "arrow.up.forward.app",
                    label: pevLocalizedText("music.open_provider"),
                    action: { onCommand(.openProvider) }
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
                .frame(minWidth: 44, minHeight: 44)
                .background(
                    isProminent ? PevDashboardColors.yellow.opacity(0.18) : .clear,
                    in: Circle()
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

private struct MusicTimelineRow: View {
    let event: MobileMusicRideEventDto

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(event.timelineItemTitle)
                    .lineLimit(1)
                Text("\(event.provider.title) · \(event.kind.timelineTitle)")
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
        self.events = events
    }

    public var body: some View {
        ForEach(events, id: \.timelineID) { event in
            MusicTimelineRow(event: event)
        }
    }
}

public struct MusicExpandedPlayer: View {
    public let nowPlaying: MusicNowPlaying
    public let timeline: [MobileMusicRideEventDto]
    public let onCommand: (MobileMusicCommandDto) -> Void
    @Environment(\.dismiss) private var dismiss

    public init(
        nowPlaying: MusicNowPlaying,
        timeline: [MobileMusicRideEventDto] = [],
        onCommand: @escaping (MobileMusicCommandDto) -> Void
    ) {
        self.nowPlaying = nowPlaying
        self.timeline = timeline
        self.onCommand = onCommand
    }

    public var body: some View {
        NavigationStack {
            Form {
                Section {
                    MusicExpandedHero(nowPlaying: nowPlaying)
                    MusicTransportControls(nowPlaying: nowPlaying, onCommand: onCommand)
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
                    Button(pevLocalizedText("music.done")) { dismiss() }
                        .accessibilityIdentifier("music.done")
                }
            }
        }
        .tint(PevDashboardColors.yellow)
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
                Button(pevLocalizedText("music.connect_provider", selectedProvider.title), action: onConnect)
                    .accessibilityIdentifier("music.connect-provider")
                if selectedProvider == .spotify {
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
                .accessibilityValue(historyPolicy.musicAccessibilityIdentifier)
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
    let nowPlaying: MusicNowPlaying

    var body: some View {
        HStack(spacing: 16) {
            artworkView
            VStack(alignment: .leading, spacing: 5) {
                Text(nowPlaying.providerName.uppercased())
                    .font(.caption.weight(.bold))
                    .foregroundStyle(PevDashboardColors.yellow)
                    .tracking(0.8)
                Text(nowPlaying.title)
                    .font(.title3.weight(.bold))
                    .lineLimit(2)
                Text(nowPlaying.artist)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if let status = nowPlaying.statusText {
                    Text(status)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(16)
        .background(PevDashboardColors.yellow.opacity(0.10), in: RoundedRectangle(cornerRadius: 20))
    }

    private var artworkView: some View {
        MusicArtworkView(
            artwork: nowPlaying.artwork,
            size: 84,
            cornerRadius: 16,
            accessibilityLabel: nowPlaying.artworkAccessibilityLabel
        )
    }
}

private struct MusicArtworkView: View {
    let artwork: MusicArtwork?
    let size: CGFloat
    let cornerRadius: CGFloat
    let accessibilityLabel: String

    @State private var image: CGImage?

    var body: some View {
        Group {
            if let image {
                Image(decorative: image, scale: 1, orientation: .up)
                    .resizable()
                    .scaledToFill()
                    .frame(width: size, height: size)
                    .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
                    .accessibilityLabel(accessibilityLabel)
            } else {
                MusicArtworkPlaceholder(size: size)
            }
        }
        .task(id: artwork?.data) {
            let data = artwork?.data
            let maxPixelSize = max(1, Int(size * 3))
            let decoded: CGImage? = await Task.detached(priority: .userInitiated) {
                guard let data else { return nil }
                return Self.decode(data: data, maxPixelSize: maxPixelSize)
            }.value
            guard !Task.isCancelled else { return }
            image = decoded
        }
    }

    nonisolated private static func decode(data: Data, maxPixelSize: Int) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        return CGImageSourceCreateThumbnailAtIndex(
            source,
            0,
            [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maxPixelSize,
            ] as CFDictionary
        )
    }
}

private struct MusicArtworkPlaceholder: View {
    let size: CGFloat

    var body: some View {
        Image(systemName: "music.note")
            .font(size >= 64 ? .largeTitle : .title3)
            .foregroundStyle(PevDashboardColors.yellow)
            .frame(width: size, height: size)
            .background(PevDashboardColors.yellow.opacity(0.16), in: RoundedRectangle(cornerRadius: size >= 64 ? 16 : 12))
            .accessibilityHidden(true)
    }
}

private struct MusicHistoryPolicyLabel: View {
    let policy: MobileMusicHistoryPolicyDto

    var body: some View {
        Text(policy.title)
            .accessibilityIdentifier("music.history-policy.\(policy.musicAccessibilityIdentifier)")
    }
}

/// Shared Ride/Map composition for the compact player.
public struct MusicCompactPlayerInset: ViewModifier {
    public let nowPlaying: MusicNowPlaying?
    public let timeline: [MobileMusicRideEventDto]
    public let isHidden: Bool
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onOpenSettings: () -> Void
    public let onDismiss: () -> Void
    public let onRestore: () -> Void

    public func body(content: Content) -> some View {
        content.safeAreaInset(edge: .bottom, spacing: 8) {
            if let nowPlaying {
                MusicCompactPlayer(
                    nowPlaying: nowPlaying,
                    timeline: timeline,
                    onCommand: onCommand,
                    onOpenSettings: onOpenSettings,
                    onDismiss: onDismiss
                )
                .padding(.horizontal, 12)
            } else if isHidden {
                Button(action: onRestore) {
                    Label(
                        pevLocalizedText("music.restore"),
                        systemImage: "music.note"
                    )
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("music.restore")
            } else {
                Button(action: onOpenSettings) {
                    Label(pevLocalizedText("music.settings.open"), systemImage: "gearshape")
                }
                .buttonStyle(.bordered)
                .tint(PevDashboardColors.yellow)
                .accessibilityIdentifier("music.open-settings")
            }
        }
    }
}

public extension View {
    func musicCompactPlayer(
        nowPlaying: MusicNowPlaying?,
        timeline: [MobileMusicRideEventDto] = [],
        isHidden: Bool,
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onOpenSettings: @escaping () -> Void,
        onDismiss: @escaping () -> Void,
        onRestore: @escaping () -> Void
    ) -> some View {
        modifier(MusicCompactPlayerInset(
            nowPlaying: nowPlaying,
            timeline: timeline,
            isHidden: isHidden,
            onCommand: onCommand,
            onOpenSettings: onOpenSettings,
            onDismiss: onDismiss,
            onRestore: onRestore
        ))
    }
}
