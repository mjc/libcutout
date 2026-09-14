import CutoutMobileFFI
import Foundation
#if canImport(UIKit) && os(iOS)
import UIKit
#endif
#if canImport(MusicKit) && os(iOS)
@preconcurrency import MusicKit
#endif

#if canImport(MediaPlayer) && os(iOS)
@preconcurrency import MediaPlayer

@MainActor
private final class SystemAppleMusicObservationService: AppleMusicObservationService {
    private static let artworkSize = CGSize(width: 256, height: 256)

    private let makePlayer: @Sendable () -> MPMusicPlayerController
    private var player: MPMusicPlayerController?
    private var activeGeneration: UInt64?
    private var notificationTokens = [NSObjectProtocol]()
    private var notificationGeneration = AppleMusicNotificationGeneration()
    private var artworkCache = MusicArtworkCache()

    init(
        makePlayer: @escaping @Sendable () -> MPMusicPlayerController = {
            MPMusicPlayerController.systemMusicPlayer
        }
    ) {
        self.makePlayer = makePlayer
    }

    func subscribe(
        generation: UInt64,
        onChange: @escaping @Sendable (UInt64) -> Void
    ) {
        removeObservers()
        activeGeneration = generation
        let player = musicPlayer()
        notificationGeneration.begin {
            player.beginGeneratingPlaybackNotifications()
        }
        let center = NotificationCenter.default
        let names: [Notification.Name] = [
            .MPMusicPlayerControllerPlaybackStateDidChange,
            .MPMusicPlayerControllerNowPlayingItemDidChange,
        ]
        notificationTokens = names.map { name in
            center.addObserver(forName: name, object: player, queue: nil) { _ in
                onChange(generation)
            }
        }
    }

    func unsubscribe(generation: UInt64) {
        guard activeGeneration == generation else { return }
        let player = musicPlayer()
        removeObservers()
        notificationGeneration.end {
            player.endGeneratingPlaybackNotifications()
        }
        activeGeneration = nil
    }

    func observation(
        generation: UInt64,
        observedAtMs: UInt64
    ) -> MusicProviderObservation? {
        guard activeGeneration == generation else { return nil }
        let player = musicPlayer()
        let nowPlayingItem = player.nowPlayingItem
        let item = nowPlayingItem.map {
            MobileMusicItemDto(
                identifier: appleMusicIdentifier(for: $0),
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
        let artwork = artworkCache.artwork(for: nowPlayingItem.map(appleMusicIdentifier)) {
            loadArtwork(from: nowPlayingItem)
        }
        return MusicProviderObservation(
            snapshot: MobileMusicSnapshotDto(
                provider: .appleMusic,
                sessionId: "system-music-player",
                state: state,
                item: item,
                positionMilliseconds: MusicTimeConversion.milliseconds(player.currentPlaybackTime),
                durationMilliseconds: nowPlayingItem.flatMap {
                    MusicTimeConversion.milliseconds($0.playbackDuration)
                },
                observedAtMs: observedAtMs,
                capabilities: MobileMusicCapabilitiesDto(
                    previous: item != nil,
                    play: state == .paused || state == .stopped,
                    pause: state == .playing,
                    next: item != nil,
                    openProvider: true
                )
            ),
            artwork: artwork
        )
    }

    private func musicPlayer() -> MPMusicPlayerController {
        if let player { return player }
        let player = makePlayer()
        self.player = player
        return player
    }

    private func removeObservers() {
        let center = NotificationCenter.default
        notificationTokens.forEach(center.removeObserver)
        notificationTokens.removeAll(keepingCapacity: true)
    }

    private func loadArtwork(from item: MPMediaItem?) -> MusicArtwork? {
#if canImport(UIKit) && os(iOS)
        guard let artwork = item?.artwork,
              let image = artwork.image(at: Self.artworkSize)?.cgImage
        else { return nil }
        return MusicArtwork(image: image)
#else
        nil
#endif
    }

    private func appleMusicIdentifier(for item: MPMediaItem) -> String {
        let storeID = item.playbackStoreID
        if !storeID.isEmpty, storeID != "0" {
            return "apple:catalog:\(storeID)"
        }
        return "apple:local:\(item.persistentID)"
    }
}

/// Apple Music's system-player bridge. MusicKit owns transport; MediaPlayer is
/// retained only for the system now-playing metadata/artwork surface. iOS does
/// not provide a system PCM tap for another app's playback.
@MainActor
public final class AppleMusicProviderAdapter {
    public static let providerURL = URL(string: "https://music.apple.com/")!
    private let observationBridge: AppleMusicObservationBridge
    private let transport: MusicProviderTransportExecutor
    private let pendingCommandTask = MusicCommandTaskSlot()
#if canImport(MusicKit) && os(iOS)
    private let makeSystemPlayer: () -> SystemMusicPlayer
    private lazy var systemPlayer = makeSystemPlayer()
#else
    private lazy var legacyTransportPlayer = MPMusicPlayerController.systemMusicPlayer
#endif

    public convenience init() {
        let lifecycle = MobileMusicProviderLifecycle()
        self.init(
            lifecycle: lifecycle,
            effects: MusicProviderEffectExecutor(),
            service: SystemAppleMusicObservationService()
        )
    }

    public convenience init(
        lifecycle: MobileMusicProviderLifecycle,
        effects: MusicProviderEffectExecutor
    ) {
        self.init(
            lifecycle: lifecycle,
            effects: effects,
            service: SystemAppleMusicObservationService()
        )
    }

    init(
        lifecycle: MobileMusicProviderLifecycle,
        effects: MusicProviderEffectExecutor,
        service: any AppleMusicObservationService
    ) {
        observationBridge = AppleMusicObservationBridge(
            service: service,
            lifecycle: lifecycle,
            effects: effects
        )
        transport = MusicProviderTransportExecutor(
            lifecycle: lifecycle,
            effects: effects,
            nowMs: { UInt64(ProcessInfo.processInfo.systemUptime * 1_000) }
        )
#if canImport(MusicKit) && os(iOS)
        makeSystemPlayer = { SystemMusicPlayer.shared }
#endif
    }

    /// Starts the system-player callbacks used to refresh bounded metadata.
    /// Polling remains the fallback for position and lifecycle reconciliation.
    public func startMonitoring(
        observedAtMs: @escaping @MainActor () -> UInt64,
        onObservation: @escaping @MainActor (MusicProviderObservation) -> Void
    ) async {
        stopMonitoring()
        await observationBridge.startMonitoring(
            observedAtMs: observedAtMs,
            onObservation: onObservation
        )
    }

    public func stopMonitoring() {
        pendingCommandTask.cancel()
        if let completion = observationBridge.stopMonitoring() {
            transport.apply(completion)
        }
    }

    public func applySuspension(_ suspension: MobileMusicProviderSuspension) {
        transport.apply(suspension)
    }

    public func refreshObservation(observedAtMs: UInt64) {
        observationBridge.refresh(observedAtMs: observedAtMs)
    }

    public func requestAuthorization(allowPrompt: Bool = true) async -> Bool {
#if canImport(MusicKit) && os(iOS)
        guard allowPrompt else { return MusicAuthorization.currentStatus == .authorized }
        return await MusicAuthorization.request() == .authorized
#else
        guard allowPrompt else { return MPMediaLibrary.authorizationStatus() == .authorized }
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
        if command == .openProvider {
#if canImport(UIKit) && os(iOS)
            guard UIApplication.shared.canOpenURL(Self.providerURL) else { return .unavailable }
            guard await UIApplication.shared.open(Self.providerURL) else { return .failed }
            return .accepted
#else
            return .unavailable
#endif
        }
        guard let providerGeneration = observationBridge.providerGeneration else {
            return .unavailable
        }
        return await transport.perform(providerGeneration: providerGeneration) { [weak self] completion in
            guard let self else {
                completion(false)
                return
            }
            let taskID = self.pendingCommandTask.reserve()
            let task = Task { @MainActor [weak self] in
                defer { self?.pendingCommandTask.finish(taskID) }
                guard let self,
                      self.observationBridge.providerGeneration == providerGeneration,
                      !Task.isCancelled else {
                    completion(false)
                    return
                }
                completion(await self.execute(command))
            }
            self.pendingCommandTask.install(task, for: taskID)
        }
    }

    private func execute(_ command: MobileMusicCommandDto) async -> Bool {
        switch command {
            case .previous:
#if canImport(MusicKit) && os(iOS)
                do {
                    try await systemPlayer.skipToPreviousEntry()
                    return true
                } catch {
                    return false
                }
#else
                legacyTransportPlayer.skipToPreviousItem()
                return true
#endif
            case .play:
#if canImport(MusicKit) && os(iOS)
                do {
                    try await systemPlayer.play()
                    return true
                } catch {
                    return false
                }
#else
                legacyTransportPlayer.play()
                return true
#endif
            case .pause:
#if canImport(MusicKit) && os(iOS)
                systemPlayer.pause()
#else
                legacyTransportPlayer.pause()
#endif
                return true
            case .next:
#if canImport(MusicKit) && os(iOS)
                do {
                    try await systemPlayer.skipToNextEntry()
                    return true
                } catch {
                    return false
                }
#else
                legacyTransportPlayer.skipToNextItem()
                return true
#endif
            case .openProvider:
                return false
        }
    }

    /// Returns the most recently completed cached observation with a current timestamp.
    /// Call `refreshObservation` to request a new provider read.
    public func snapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        observation(observedAtMs: observedAtMs).snapshot
    }

    /// Returns the most recently completed cached provider observation with a
    /// current timestamp. The artwork never enters the Rust ride contract.
    public func observation(observedAtMs: UInt64) -> MusicProviderObservation {
        observationBridge.cachedObservation?.observedAt(observedAtMs)
            ?? .unavailable(
                provider: .appleMusic,
                sessionId: "system-music-player",
                observedAtMs: observedAtMs,
                openProvider: true
            )
    }
}
#endif
