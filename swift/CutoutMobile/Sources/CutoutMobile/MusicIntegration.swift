import CutoutMobileFFI
import Foundation
import SwiftUI

/// Provider-neutral music state used by the compact ride/map player.
public struct MusicNowPlaying: Equatable, Sendable {
    public let provider: MobileMusicProviderDto
    public let state: MobileMusicPlaybackStateDto
    public let item: MobileMusicItemDto?
    public let capabilities: MobileMusicCapabilitiesDto

    public init(
        provider: MobileMusicProviderDto,
        state: MobileMusicPlaybackStateDto,
        item: MobileMusicItemDto? = nil,
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
        self.capabilities = capabilities
    }

    public init(snapshot: MobileMusicSnapshotDto) {
        self.init(
            provider: snapshot.provider,
            state: snapshot.state,
            item: snapshot.item,
            capabilities: snapshot.capabilities
        )
    }

    public var providerName: String {
        switch provider {
        case .appleMusic: pevLocalizedText("music.provider.apple_music")
        case .spotify: pevLocalizedText("music.provider.spotify")
        }
    }

    public var title: String { item?.title ?? pevLocalizedText("music.not_playing") }
    public var artist: String { item?.artist ?? providerName }

    public var playPauseCommand: MobileMusicCommandDto? {
        switch state {
        case .playing where capabilities.pause: .pause
        case .paused where capabilities.play: .play
        case .stopped where capabilities.play: .play
        default: nil
        }
    }
}

/// The Rust-owned ride association is the only path for music metadata to enter a ride.
@MainActor
public final class MusicIntegrationCoordinator {
    public private(set) var nowPlaying: MusicNowPlaying?
    private let rideMapState: MobileRideMapState

    public init(rideMapState: MobileRideMapState) {
        self.rideMapState = rideMapState
    }

    public func update(snapshot: MobileMusicSnapshotDto) {
        nowPlaying = MusicNowPlaying(snapshot: snapshot)
    }

    /// Applies one provider observation and records only a meaningful transition.
    /// A disabled history policy still updates the compact player but never writes
    /// to the ride database.
    @discardableResult
    public func ingest(
        snapshot: MobileMusicSnapshotDto,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) throws -> MobileMusicTimelineOutcomeDto? {
        let previous = nowPlaying
        update(snapshot: snapshot)
        guard let kind = Self.transitionKind(from: previous, to: nowPlaying) else {
            return nil
        }
        return try rideMapState.recordMusicEvent(
            snapshot: snapshot,
            kind: kind,
            monotonicAtMs: snapshot.observedAtMs,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs
        )
    }

    public func setHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) throws {
        try rideMapState.setMusicHistoryPolicy(policy: policy)
    }

    public func record(
        snapshot: MobileMusicSnapshotDto,
        kind: MobileMusicRideEventKindDto,
        monotonicAtMs: UInt64,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) throws -> MobileMusicTimelineOutcomeDto {
        update(snapshot: snapshot)
        return try rideMapState.recordMusicEvent(
            snapshot: snapshot,
            kind: kind,
            monotonicAtMs: monotonicAtMs,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs
        )
    }

    public var recordedEvents: [MobileMusicRideEventDto] {
        rideMapState.currentMusicEvents() ?? []
    }

    private static func transitionKind(
        from previous: MusicNowPlaying?,
        to current: MusicNowPlaying?
    ) -> MobileMusicRideEventKindDto? {
        guard let current else { return .providerDisconnected }
        guard let previous else { return current.item == nil ? nil : .itemChanged }
        if previous.provider != current.provider || previous.item?.identifier != current.item?.identifier {
            return .itemChanged
        }
        switch (previous.state, current.state) {
        case (_, .playing) where previous.state != .playing:
            return MobileMusicRideEventKindDto.play
        case (_, .paused) where previous.state != .paused:
            return MobileMusicRideEventKindDto.pause
        case (_, .disconnected) where previous.state != .disconnected:
            return MobileMusicRideEventKindDto.providerDisconnected
        default:
            return nil
        }
    }
}

/// A small, reusable control surface for Ride and Map. It renders metadata only;
/// neither artwork bytes nor an audio stream cross the app boundary.
public struct MusicCompactPlayer: View {
    public let nowPlaying: MusicNowPlaying
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onDismiss: () -> Void

    public init(
        nowPlaying: MusicNowPlaying,
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onDismiss: @escaping () -> Void = {}
    ) {
        self.nowPlaying = nowPlaying
        self.onCommand = onCommand
        self.onDismiss = onDismiss
    }

    public var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "music.note")
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(nowPlaying.title)
                    .lineLimit(1)
                    .font(.subheadline.weight(.semibold))
                Text(nowPlaying.artist)
                    .lineLimit(1)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 4)
            Button { onCommand(.previous) } label: {
                Image(systemName: "backward.fill")
            }
            .disabled(!nowPlaying.capabilities.previous)
            if let command = nowPlaying.playPauseCommand {
                Button { onCommand(command) } label: {
                    Image(systemName: command == .pause ? "pause.fill" : "play.fill")
                }
            }
            Button { onCommand(.next) } label: {
                Image(systemName: "forward.fill")
            }
            .disabled(!nowPlaying.capabilities.next)
            Button(action: onDismiss) {
                Image(systemName: "xmark")
            }
            .accessibilityLabel(pevLocalizedText("music.hide"))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 16))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(nowPlaying.providerName), \(nowPlaying.title), \(nowPlaying.artist)")
    }
}

/// Shared Ride/Map composition for the compact player.
public struct MusicCompactPlayerInset: ViewModifier {
    public let nowPlaying: MusicNowPlaying?
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onDismiss: () -> Void

    public func body(content: Content) -> some View {
        content.safeAreaInset(edge: .bottom, spacing: 8) {
            if let nowPlaying {
                MusicCompactPlayer(
                    nowPlaying: nowPlaying,
                    onCommand: onCommand,
                    onDismiss: onDismiss
                )
                .padding(.horizontal, 12)
            }
        }
    }
}

public extension View {
    func musicCompactPlayer(
        nowPlaying: MusicNowPlaying?,
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onDismiss: @escaping () -> Void
    ) -> some View {
        modifier(MusicCompactPlayerInset(
            nowPlaying: nowPlaying,
            onCommand: onCommand,
            onDismiss: onDismiss
        ))
    }
}

#if canImport(MediaPlayer) && os(iOS)
import MediaPlayer

/// Apple Music's system-player bridge. It exposes transport and bounded metadata
/// only; iOS does not provide a system PCM tap for another app's playback.
@MainActor
public final class AppleMusicProviderAdapter {
    private let player = MPMusicPlayerController.systemMusicPlayer

    public init() {}

    public func requestAuthorization() async -> Bool {
        await withCheckedContinuation { continuation in
            MPMediaLibrary.requestAuthorization { status in
                continuation.resume(returning: status == .authorized)
            }
        }
    }

    public func unauthorizedSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "system-music-player",
            state: .unauthorized,
            item: nil,
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: observedAtMs,
            capabilities: MobileMusicCapabilitiesDto(
                previous: false,
                play: false,
                pause: false,
                next: false,
                openProvider: false
            )
        )
    }

    public func perform(_ command: MobileMusicCommandDto) {
        switch command {
        case .previous: player.skipToPreviousItem()
        case .play: player.play()
        case .pause: player.pause()
        case .next: player.skipToNextItem()
        case .openProvider: break
        }
    }

    public func snapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        let item = player.nowPlayingItem.map {
            MobileMusicItemDto(
                identifier: String($0.persistentID),
                title: $0.title,
                artist: $0.artist
            )
        }
        let state: MobileMusicPlaybackStateDto = switch player.playbackState {
        case .playing: .playing
        case .paused: .paused
        case .interrupted: .interrupted
        case .stopped: .stopped
        default: .unavailable
        }
        let position = player.currentPlaybackTime >= 0
            ? UInt64(player.currentPlaybackTime * 1_000)
            : nil
        let duration = player.nowPlayingItem.map { UInt64(max(0, $0.playbackDuration) * 1_000) }
        return MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "system-music-player",
            state: state,
            item: item,
            positionMilliseconds: position,
            durationMilliseconds: duration,
            observedAtMs: observedAtMs,
            capabilities: MobileMusicCapabilitiesDto(
                previous: item != nil,
                play: state == .paused || state == .stopped,
                pause: state == .playing,
                next: item != nil,
                openProvider: false
            )
        )
    }
}
#endif

/// Spotify is intentionally represented without an SDK dependency. A future
/// App Remote adapter can feed the same snapshot/command contract once its
/// redirect, entitlement, and account lifecycle are proven on-device.
public struct SpotifyProviderAdapter: Sendable {
    public init() {}

    public func unavailableSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        MobileMusicSnapshotDto(
            provider: .spotify,
            sessionId: "spotify-unavailable",
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
                openProvider: true
            )
        )
    }
}
