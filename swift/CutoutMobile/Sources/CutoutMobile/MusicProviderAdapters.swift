import CutoutMobileFFI
import Foundation
#if canImport(UIKit) && os(iOS)
import UIKit
#endif
#if canImport(MusicKit) && os(iOS)
@preconcurrency import MusicKit
#endif

#if canImport(MediaPlayer) && os(iOS)
import MediaPlayer

/// Apple Music's system-player bridge. MusicKit owns transport; MediaPlayer is
/// retained only for the system now-playing metadata/artwork surface. iOS does
/// not provide a system PCM tap for another app's playback.
@MainActor
public final class AppleMusicProviderAdapter {
    public static let providerURL = URL(string: "https://music.apple.com/")!
    private static let artworkSize = CGSize(width: 256, height: 256)
    private let player = MPMusicPlayerController.systemMusicPlayer
#if canImport(MusicKit) && os(iOS)
    private let systemPlayer = SystemMusicPlayer.shared
#endif
    private var notificationTokens = [NSObjectProtocol]()
    private var artworkCache = MusicArtworkCache()

    public init() {}

    /// Starts the system-player callbacks used to refresh bounded metadata.
    /// Polling remains the fallback for position and lifecycle reconciliation.
    public func startMonitoring(onChange: @escaping @MainActor () -> Void) {
        stopMonitoring()
        player.beginGeneratingPlaybackNotifications()
        let center = NotificationCenter.default
        let names: [Notification.Name] = [
            .MPMusicPlayerControllerPlaybackStateDidChange,
            .MPMusicPlayerControllerNowPlayingItemDidChange,
        ]
        notificationTokens = names.map { name in
            center.addObserver(forName: name, object: player, queue: .main) { _ in
                Task { @MainActor in onChange() }
            }
        }
    }

    public func stopMonitoring() {
        let center = NotificationCenter.default
        notificationTokens.forEach(center.removeObserver)
        notificationTokens.removeAll(keepingCapacity: true)
        player.endGeneratingPlaybackNotifications()
    }

    public func requestAuthorization() async -> Bool {
#if canImport(MusicKit) && os(iOS)
        return await MusicAuthorization.request() == .authorized
#else
        return await withCheckedContinuation { continuation in
            MPMediaLibrary.requestAuthorization { status in
                continuation.resume(returning: status == .authorized)
            }
        }
#endif
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
                openProvider: true
            )
        )
    }

    @MainActor
    public func perform(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        switch command {
        case .previous:
#if canImport(MusicKit) && os(iOS)
            do {
                try await systemPlayer.skipToPreviousEntry()
            } catch {
                return .failed
            }
#else
            player.skipToPreviousItem()
#endif
        case .play:
#if canImport(MusicKit) && os(iOS)
            do {
                try await systemPlayer.play()
            } catch {
                return .failed
            }
#else
            player.play()
#endif
        case .pause:
#if canImport(MusicKit) && os(iOS)
            systemPlayer.pause()
#else
            player.pause()
#endif
        case .next:
#if canImport(MusicKit) && os(iOS)
            do {
                try await systemPlayer.skipToNextEntry()
            } catch {
                return .failed
            }
#else
            player.skipToNextItem()
#endif
        case .openProvider:
#if canImport(UIKit) && os(iOS)
            guard UIApplication.shared.canOpenURL(Self.providerURL) else { return .unavailable }
            guard await UIApplication.shared.open(Self.providerURL) else { return .failed }
#else
            return .unavailable
#endif
        }
        return .accepted
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
        let position = MusicTimeConversion.milliseconds(player.currentPlaybackTime)
        let duration = player.nowPlayingItem.flatMap {
            MusicTimeConversion.milliseconds($0.playbackDuration)
        }
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
                openProvider: true
            )
        )
    }

    /// Returns the same bounded provider snapshot plus permitted artwork for
    /// SwiftUI. The artwork bytes never enter the Rust ride contract.
    public func observation(observedAtMs: UInt64) -> MusicProviderObservation {
        MusicProviderObservation(
            snapshot: snapshot(observedAtMs: observedAtMs),
            artworkData: artworkData()
        )
    }

    private func artworkData() -> Data? {
        artworkCache.artwork(for: currentItemIdentifier) {
            loadArtwork()
        }?.data
    }

    private var currentItemIdentifier: String? {
        player.nowPlayingItem.map { String($0.persistentID) }
    }

    private func loadArtwork() -> MusicArtwork? {
#if canImport(UIKit) && os(iOS)
        guard
            let artwork = player.nowPlayingItem?.artwork,
            let image = artwork.image(at: Self.artworkSize)
        else {
            return nil
        }
        return image.jpegData(compressionQuality: 0.8).flatMap(MusicArtwork.init(data:))
#else
        nil
#endif
    }
}
#endif

/// Spotify is intentionally represented without an SDK dependency. A future
/// App Remote adapter can feed the same snapshot/command contract once its
/// redirect, entitlement, and account lifecycle are proven on-device.
public struct SpotifyProviderAdapter: Sendable {
    public static let providerURL = URL(string: "spotify://")!

    public init() {}

    /// Opens Spotify when the provider can be handed off to its own app.
    /// Playback control and metadata remain unavailable until App Remote is
    /// integrated and proven on a physical device.
    @MainActor
    public func perform(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        guard case .openProvider = command else { return .unavailable }
#if canImport(UIKit) && os(iOS)
        guard UIApplication.shared.canOpenURL(Self.providerURL) else { return .unavailable }
        guard await UIApplication.shared.open(Self.providerURL) else { return .failed }
        return .accepted
#else
        return .unavailable
#endif
    }

    public func unavailableSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        MusicProviderObservation.unavailable(
            provider: .spotify,
            sessionId: "spotify-unavailable",
            observedAtMs: observedAtMs,
            openProvider: true
        ).snapshot
    }
}
