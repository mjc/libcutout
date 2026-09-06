import CutoutMobileFFI
import Foundation

private extension MobileMusicCapabilitiesDto {
    func supports(_ command: MobileMusicCommandDto) -> Bool {
        switch command {
        case .previous: previous
        case .play: play
        case .pause: pause
        case .next: next
        case .openProvider: openProvider
        }
    }
}

/// Provider-neutral music state used by the compact ride/map player.
public struct MusicNowPlaying: Equatable, Sendable {
    private static let transportCommands: [MobileMusicCommandDto] = [
        .previous,
        .play,
        .pause,
        .next,
    ]

    public let provider: MobileMusicProviderDto
    public let state: MobileMusicPlaybackStateDto
    public let item: MobileMusicItemDto?
    public let capabilities: MobileMusicCapabilitiesDto
    public let artwork: MusicArtwork?

    public init(
        provider: MobileMusicProviderDto,
        state: MobileMusicPlaybackStateDto,
        item: MobileMusicItemDto? = nil,
        artwork: MusicArtwork? = nil,
        capabilities: MobileMusicCapabilitiesDto = .init(
            previous: false,
            play: false,
            pause: false,
            next: false,
            openProvider: false
        )
    ) {
        self.provider = provider
        self.state = state
        self.item = item
        self.artwork = artwork
        self.capabilities = capabilities
    }

    public init(snapshot: MobileMusicSnapshotDto, artwork: MusicArtwork? = nil) {
        self.init(
            provider: snapshot.provider,
            state: snapshot.state,
            item: snapshot.item,
            artwork: artwork,
            capabilities: snapshot.capabilities
        )
    }

    public init(observation: MusicProviderObservation) {
        self.init(snapshot: observation.snapshot, artwork: observation.artwork)
    }

    public var providerName: String { provider.title }

    public var title: String { item?.title ?? pevLocalizedText("music.not_playing") }
    public var artist: String { item?.artist ?? providerName }

    public var statusText: String? {
        switch state {
        case .playing, .paused:
            nil
        case .buffering:
            pevLocalizedText("music.state.buffering")
        case .interrupted:
            pevLocalizedText("music.state.interrupted")
        case .stopped:
            pevLocalizedText("music.state.stopped")
        case .unauthorized:
            pevLocalizedText("music.state.authorization_required")
        case .unavailable:
            pevLocalizedText("music.state.unavailable")
        case .disconnected:
            pevLocalizedText("music.state.disconnected")
        case .stale:
            pevLocalizedText("music.state.stale")
        }
    }

    /// VoiceOver summary includes meaningful provider failure states without progress ticks.
    public var accessibilitySummary: String {
        var components = [providerName, title]
        if artist != providerName && artist != title {
            components.append(artist)
        }
        if let statusText {
            components.append(statusText)
        }
        return components.joined(separator: ", ")
    }

    public var artworkAccessibilityLabel: String {
        pevLocalizedText("music.artwork", title)
    }

    /// Projects a monitoring gap without recording a synthetic ride event.
    /// Provider handoff remains available, but transport commands are disabled
    /// until a fresh provider observation arrives.
    public var staleProjection: Self {
        Self(
            provider: provider,
            state: .stale,
            item: item,
            artwork: artwork,
            capabilities: .init(
                previous: false,
                play: false,
                pause: false,
                next: false,
                openProvider: capabilities.openProvider
            )
        )
    }

    public var playPauseCommand: MobileMusicCommandDto? {
        switch state {
        case .playing where capabilities.pause: .pause
        case .paused where capabilities.play: .play
        case .stopped where capabilities.play: .play
        default: nil
        }
    }

    public var availableTransportCommands: [MobileMusicCommandDto] {
        Self.transportCommands.filter { command in
            if command == .play || command == .pause {
                return command == playPauseCommand
            }
            return supports(command)
        }
    }

    /// Whether the command is both provider-supported and valid for this state.
    public func isCommandAvailable(_ command: MobileMusicCommandDto) -> Bool {
        switch command {
        case .openProvider:
            capabilities.openProvider
        case .play, .pause, .previous, .next:
            availableTransportCommands.contains(command)
        }
    }

    public func supports(_ command: MobileMusicCommandDto) -> Bool {
        capabilities.supports(command)
    }
}

/// Deduplicates VoiceOver announcements while the provider is polled.
private struct MusicAccessibilityAnnouncementKey: Equatable {
    let provider: MobileMusicProviderDto
    let state: MobileMusicPlaybackStateDto
    let itemIdentifier: String?
    let title: String?
    let artist: String?

    init(_ nowPlaying: MusicNowPlaying) {
        provider = nowPlaying.provider
        state = nowPlaying.state
        itemIdentifier = nowPlaying.item?.identifier
        title = nowPlaying.item?.title
        artist = nowPlaying.item?.artist
    }
}

struct MusicAccessibilityAnnouncementTracker {
    private var lastAnnounced: MusicAccessibilityAnnouncementKey?

    mutating func next(for nowPlaying: MusicNowPlaying) -> String? {
        let key = MusicAccessibilityAnnouncementKey(nowPlaying)
        guard lastAnnounced != key else { return nil }
        lastAnnounced = key
        return nowPlaying.accessibilitySummary
    }
}

public extension MobileMusicHistoryPolicyDto {
    static var allCases: [Self] { [.disabled, .opaqueItem, .humanReadable] }

    var title: String {
        switch self {
        case .disabled:
            pevLocalizedText("music.history.disabled")
        case .opaqueItem:
            pevLocalizedText("music.history.opaque_item")
        case .humanReadable:
            pevLocalizedText("music.history.human_readable")
        }
    }

    var explanation: String {
        switch self {
        case .disabled:
            pevLocalizedText("music.history.disabled.explanation")
        case .opaqueItem:
            pevLocalizedText("music.history.opaque_item.explanation")
        case .humanReadable:
            pevLocalizedText("music.history.human_readable.explanation")
        }
    }
}

public extension MobileMusicRideEventKindDto {
    var timelineTitle: String {
        switch self {
        case .play: pevLocalizedText("music.timeline.play")
        case .pause: pevLocalizedText("music.timeline.pause")
        case .skip: pevLocalizedText("music.timeline.skip")
        case .itemChanged: pevLocalizedText("music.timeline.item_changed")
        case .providerDisconnected: pevLocalizedText("music.timeline.provider_disconnected")
        }
    }
}

private extension MobileMusicProviderDto {
    var timelineIDComponent: String {
        switch self {
        case .appleMusic: "apple-music"
        case .spotify: "spotify"
        }
    }
}

private extension MobileMusicRideEventKindDto {
    var timelineIDComponent: String {
        switch self {
        case .play: "play"
        case .pause: "pause"
        case .skip: "skip"
        case .itemChanged: "item-changed"
        case .providerDisconnected: "provider-disconnected"
        }
    }
}

public extension MobileMusicRideEventDto {
    var timelineID: String {
        [
            String(sequence),
            provider.timelineIDComponent,
            String(monotonicAtMs),
            String(wallClockAtMs),
            kind.timelineIDComponent,
            itemIdentifier ?? "",
        ].joined(separator: "-")
    }

    var timelineItemTitle: String {
        title ?? itemIdentifier ?? pevLocalizedText("music.timeline.unknown_item")
    }
}

/// One provider observation entering the shared music pipeline.
///
/// Providers may attach bounded metadata, but never an audio buffer or artwork
/// payload.
public struct MusicProviderObservation: Equatable, Sendable {
    public let snapshot: MobileMusicSnapshotDto
    public let artwork: MusicArtwork?

    public init(snapshot: MobileMusicSnapshotDto, artworkData: Data? = nil) {
        self.snapshot = snapshot
        artwork = artworkData.flatMap(MusicArtwork.init(data:))
    }

    public static func unavailable(
        provider: MobileMusicProviderDto,
        sessionId: String,
        observedAtMs: UInt64,
        openProvider: Bool = false
    ) -> Self {
        Self(
            snapshot: MobileMusicSnapshotDto(
                provider: provider,
                sessionId: sessionId,
                state: .unavailable,
                item: nil,
                positionMilliseconds: nil,
                durationMilliseconds: nil,
                observedAtMs: observedAtMs,
                capabilities: MobileMusicCapabilitiesDto(
                    previous: false,
                    play: false,
                    pause: false,
                    next: false,
                    openProvider: openProvider
                )
            )
        )
    }
}
