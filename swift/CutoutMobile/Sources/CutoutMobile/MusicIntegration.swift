import CutoutMobileFFI
import Foundation
import SwiftUI
#if canImport(UIKit) && os(iOS)
import UIKit
#endif
#if canImport(AppKit)
import AppKit
#endif
#if canImport(MusicKit) && os(iOS)
@preconcurrency import MusicKit
#endif

/// Presentation-only artwork retained in Swift and bounded before decoding.
/// Artwork never enters the Rust ride or UniFFI contracts.
public struct MusicArtwork: Equatable, Sendable {
    public static let maxBytes = 512 * 1024

    public let data: Data

    public init?(data: Data) {
        guard data.isEmpty == false, data.count <= Self.maxBytes else { return nil }
        self.data = data
    }
}

public extension MobileMusicProviderDto {
    static var allCases: [Self] { [.appleMusic, .spotify] }

    var title: String {
        switch self {
        case .appleMusic: pevLocalizedText("music.provider.apple_music")
        case .spotify: pevLocalizedText("music.provider.spotify")
        }
    }
}

/// Persists only the compact-player visibility preference.
public struct MusicPlayerVisibilityStore {
    private static let key = "io.cutout.music.compact-player.hidden"
    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var isHidden: Bool {
        defaults.bool(forKey: Self.key)
    }

    public func setHidden(_ hidden: Bool) {
        defaults.set(hidden, forKey: Self.key)
    }
}

/// Provider-neutral music state used by the compact ride/map player.
public struct MusicNowPlaying: Equatable, Sendable {
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

    public var providerName: String {
        switch provider {
        case .appleMusic: pevLocalizedText("music.provider.apple_music")
        case .spotify: pevLocalizedText("music.provider.spotify")
        }
    }

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

    public var playPauseCommand: MobileMusicCommandDto? {
        switch state {
        case .playing where capabilities.pause: .pause
        case .paused where capabilities.play: .play
        case .stopped where capabilities.play: .play
        default: nil
        }
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
}

/// The Rust-owned ride association is the only path for music metadata to enter a ride.
@MainActor
public final class MusicIntegrationCoordinator {
    public private(set) var nowPlaying: MusicNowPlaying?
    private let rideMapState: MobileRideMapState

    public init(rideMapState: MobileRideMapState) {
        self.rideMapState = rideMapState
    }

    public func update(snapshot: MobileMusicSnapshotDto, artwork: MusicArtwork? = nil) {
        nowPlaying = MusicNowPlaying(snapshot: snapshot, artwork: artwork)
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
        try ingest(
            snapshot: snapshot,
            artwork: nil,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs
        )
    }

    private func ingest(
        snapshot: MobileMusicSnapshotDto,
        artwork: MusicArtwork?,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) throws -> MobileMusicTimelineOutcomeDto? {
        let previous = nowPlaying
        update(snapshot: snapshot, artwork: artwork)
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

    /// Applies one provider observation through the same path used by ride
    /// recording and compact-player state.
    @discardableResult
    public func ingest(
        observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) throws -> MobileMusicTimelineOutcomeDto? {
        try ingest(
            snapshot: observation.snapshot,
            artwork: observation.artwork,
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
    public let timeline: [MobileMusicRideEventDto]
    public let selectedProvider: MobileMusicProviderDto
    public let historyPolicy: MobileMusicHistoryPolicyDto
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onDismiss: () -> Void
    public let onSelectProvider: (MobileMusicProviderDto) -> Void
    public let onSetHistoryPolicy: (MobileMusicHistoryPolicyDto) -> Bool
    @State private var isExpanded = false

    public init(
        nowPlaying: MusicNowPlaying,
        timeline: [MobileMusicRideEventDto] = [],
        selectedProvider: MobileMusicProviderDto = .appleMusic,
        historyPolicy: MobileMusicHistoryPolicyDto = .disabled,
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onDismiss: @escaping () -> Void = {},
        onSelectProvider: @escaping (MobileMusicProviderDto) -> Void = { _ in },
        onSetHistoryPolicy: @escaping (MobileMusicHistoryPolicyDto) -> Bool = { _ in false }
    ) {
        self.nowPlaying = nowPlaying
        self.timeline = timeline
        self.selectedProvider = selectedProvider
        self.historyPolicy = historyPolicy
        self.onCommand = onCommand
        self.onDismiss = onDismiss
        self.onSelectProvider = onSelectProvider
        self.onSetHistoryPolicy = onSetHistoryPolicy
    }

    public var body: some View {
        HStack(spacing: 12) {
            artworkView
            VStack(alignment: .leading, spacing: 2) {
                Text(nowPlaying.title)
                    .lineLimit(1)
                    .font(.subheadline.weight(.semibold))
                Text(nowPlaying.statusText ?? nowPlaying.artist)
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
            if nowPlaying.capabilities.openProvider {
                Button { onCommand(.openProvider) } label: {
                    Image(systemName: "arrow.up.forward.app")
                }
                .accessibilityLabel(pevLocalizedText("music.open_provider"))
            }
            Button { isExpanded = true } label: {
                Image(systemName: "ellipsis.circle")
            }
            .accessibilityLabel(pevLocalizedText("music.expand"))
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
        .sheet(isPresented: $isExpanded) {
            MusicExpandedPlayer(
                nowPlaying: nowPlaying,
                timeline: timeline,
                selectedProvider: selectedProvider,
                historyPolicy: historyPolicy,
                onSelectProvider: onSelectProvider,
                onSetHistoryPolicy: onSetHistoryPolicy
            )
        }
    }

    @ViewBuilder
    private var artworkView: some View {
#if canImport(UIKit) && os(iOS)
        if let data = nowPlaying.artwork?.data, let image = UIImage(data: data) {
            Image(uiImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: 34, height: 34)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .accessibilityLabel("Artwork for \(nowPlaying.title)")
        } else {
            Image(systemName: "music.note")
                .accessibilityHidden(true)
        }
#elseif canImport(AppKit)
        if let data = nowPlaying.artwork?.data, let image = NSImage(data: data) {
            Image(nsImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: 34, height: 34)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .accessibilityLabel("Artwork for \(nowPlaying.title)")
        } else {
            Image(systemName: "music.note")
                .accessibilityHidden(true)
        }
#else
        Image(systemName: "music.note")
            .accessibilityHidden(true)
#endif
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

public struct MusicExpandedPlayer: View {
    public let nowPlaying: MusicNowPlaying
    public let timeline: [MobileMusicRideEventDto]
    public let selectedProvider: MobileMusicProviderDto
    public let historyPolicy: MobileMusicHistoryPolicyDto
    public let onSelectProvider: (MobileMusicProviderDto) -> Void
    public let onSetHistoryPolicy: (MobileMusicHistoryPolicyDto) -> Bool
    @Environment(\.dismiss) private var dismiss
    @State private var selectedPolicy: MobileMusicHistoryPolicyDto

    public init(
        nowPlaying: MusicNowPlaying,
        timeline: [MobileMusicRideEventDto] = [],
        selectedProvider: MobileMusicProviderDto,
        historyPolicy: MobileMusicHistoryPolicyDto,
        onSelectProvider: @escaping (MobileMusicProviderDto) -> Void,
        onSetHistoryPolicy: @escaping (MobileMusicHistoryPolicyDto) -> Bool
    ) {
        self.nowPlaying = nowPlaying
        self.timeline = timeline
        self.selectedProvider = selectedProvider
        self.historyPolicy = historyPolicy
        self.onSelectProvider = onSelectProvider
        self.onSetHistoryPolicy = onSetHistoryPolicy
        _selectedPolicy = State(initialValue: historyPolicy)
    }

    public var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(nowPlaying.title)
                        .font(.headline)
                    Text(nowPlaying.artist)
                        .foregroundStyle(.secondary)
                    if let status = nowPlaying.statusText {
                        Text(status)
                            .foregroundStyle(.secondary)
                    }
                } header: {
                    Text(nowPlaying.providerName)
                }

                if timeline.isEmpty == false {
                    Section {
                        ForEach(timeline, id: \.timelineID) { event in
                            MusicTimelineRow(event: event)
                        }
                    } header: {
                        Text(pevLocalizedText("music.timeline.title"))
                    }
                }

                Section {
                    Picker(
                        pevLocalizedText("music.provider.select"),
                        selection: Binding(
                            get: { selectedProvider },
                            set: { provider in onSelectProvider(provider) }
                        )
                    ) {
                        ForEach(MobileMusicProviderDto.allCases, id: \.self) { provider in
                            Text(provider.title).tag(provider)
                        }
                    }
                } header: {
                    Text(pevLocalizedText("music.provider.select"))
                }

                Section {
                    Picker(pevLocalizedText("music.history.title"), selection: $selectedPolicy) {
                        ForEach(MobileMusicHistoryPolicyDto.allCases, id: \.self) { policy in
                            Text(policy.title)
                                .tag(policy)
                        }
                    }
                    Text(selectedPolicy.explanation)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                } header: {
                    Text(pevLocalizedText("music.history.title"))
                }
            }
            .navigationTitle(pevLocalizedText("music.expand"))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(pevLocalizedText("music.done")) { dismiss() }
                }
            }
            .onChange(of: selectedPolicy) { _, policy in
                if !onSetHistoryPolicy(policy) {
                    selectedPolicy = historyPolicy
                }
            }
        }
    }
}

/// Shared Ride/Map composition for the compact player.
public struct MusicCompactPlayerInset: ViewModifier {
    public let nowPlaying: MusicNowPlaying?
    public let timeline: [MobileMusicRideEventDto]
    public let selectedProvider: MobileMusicProviderDto
    public let isHidden: Bool
    public let historyPolicy: MobileMusicHistoryPolicyDto
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onDismiss: () -> Void
    public let onRestore: () -> Void
    public let onSelectProvider: (MobileMusicProviderDto) -> Void
    public let onSetHistoryPolicy: (MobileMusicHistoryPolicyDto) -> Bool

    public func body(content: Content) -> some View {
        content.safeAreaInset(edge: .bottom, spacing: 8) {
            if let nowPlaying {
                MusicCompactPlayer(
                    nowPlaying: nowPlaying,
                    timeline: timeline,
                    selectedProvider: selectedProvider,
                    historyPolicy: historyPolicy,
                    onCommand: onCommand,
                    onDismiss: onDismiss,
                    onSelectProvider: onSelectProvider,
                    onSetHistoryPolicy: onSetHistoryPolicy
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
            }
        }
    }
}

public extension View {
    func musicCompactPlayer(
        nowPlaying: MusicNowPlaying?,
        timeline: [MobileMusicRideEventDto] = [],
        selectedProvider: MobileMusicProviderDto,
        isHidden: Bool,
        historyPolicy: MobileMusicHistoryPolicyDto,
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onDismiss: @escaping () -> Void,
        onRestore: @escaping () -> Void,
        onSelectProvider: @escaping (MobileMusicProviderDto) -> Void,
        onSetHistoryPolicy: @escaping (MobileMusicHistoryPolicyDto) -> Bool
    ) -> some View {
        modifier(MusicCompactPlayerInset(
            nowPlaying: nowPlaying,
            timeline: timeline,
            selectedProvider: selectedProvider,
            isHidden: isHidden,
            historyPolicy: historyPolicy,
            onCommand: onCommand,
            onDismiss: onDismiss,
            onRestore: onRestore,
            onSelectProvider: onSelectProvider,
            onSetHistoryPolicy: onSetHistoryPolicy
        ))
    }
}

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
                openProvider: true
            )
        )
    }

    @MainActor
    public func perform(_ command: MobileMusicCommandDto) {
        switch command {
        case .previous:
#if canImport(MusicKit) && os(iOS)
            Task { try? await systemPlayer.skipToPreviousEntry() }
#else
            player.skipToPreviousItem()
#endif
        case .play:
#if canImport(MusicKit) && os(iOS)
            Task { try? await systemPlayer.play() }
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
            Task { try? await systemPlayer.skipToNextEntry() }
#else
            player.skipToNextItem()
#endif
        case .openProvider:
#if canImport(UIKit) && os(iOS)
            UIApplication.shared.open(Self.providerURL)
#endif
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
#if canImport(UIKit) && os(iOS)
        guard
            let artwork = player.nowPlayingItem?.artwork,
            let image = artwork.image(at: Self.artworkSize)
        else {
            return nil
        }
        return image.jpegData(compressionQuality: 0.8)
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
    public func perform(_ command: MobileMusicCommandDto) {
        guard case .openProvider = command else { return }
#if canImport(UIKit) && os(iOS)
        UIApplication.shared.open(Self.providerURL)
#endif
    }

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
