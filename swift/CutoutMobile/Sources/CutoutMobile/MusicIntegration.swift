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

/// Keeps one positive artwork result so polling does not repeatedly decode the
/// same provider image. The value is already bounded by `MusicArtwork`.
struct MusicArtworkCache: Sendable {
    private var itemIdentifier: String?
    private var cachedArtwork: MusicArtwork?

    mutating func artwork(
        for itemIdentifier: String?,
        load: () -> MusicArtwork?
    ) -> MusicArtwork? {
        guard let itemIdentifier else {
            clear()
            return nil
        }
        if self.itemIdentifier == itemIdentifier, let cachedArtwork {
            return cachedArtwork
        }
        let artwork = load()
        if let artwork {
            self.itemIdentifier = itemIdentifier
            cachedArtwork = artwork
        } else {
            clear()
        }
        return artwork
    }

    private mutating func clear() {
        itemIdentifier = nil
        cachedArtwork = nil
    }
}

/// Result of dispatching one provider transport command.
public enum MusicCommandOutcome: Equatable, Sendable {
    /// The provider adapter accepted the command for dispatch.
    case accepted
    /// The current provider capabilities refused the command.
    case refused
    /// The provider was available but rejected the transport operation.
    case failed
    /// No usable provider adapter or current player is available.
    case unavailable
}

/// A provider command can explain a subsequent item change without replacing
/// the Rust-owned event kind contract.
public enum MusicTransitionHint: Equatable, Sendable {
    /// The provider accepted a previous/next transport command.
    case skip
}

/// Describes the provider lifecycle that the app can currently monitor.
///
/// Spotify remains an explicit handoff/unavailable path until its App Remote
/// credentials and on-device lifecycle are proven; it must not fall through
/// to the Apple Music system-player monitor.
public enum MusicProviderMonitoringMode: Equatable, Sendable {
    case appleMusicSystemPlayer
    case unavailable
}

/// Holds a transport hint until the provider reports the resulting state.
///
/// System-player notifications can arrive after the immediate post-command
/// poll, so the hint survives several unchanged snapshots but expires when the
/// provider never reports a resulting item change.
public struct MusicTransitionHintTracker: Sendable {
    private static let maximumUnchangedObservations = 5
    public private(set) var pendingHint: MusicTransitionHint?
    private var remainingUnchangedObservations: Int?

    public init() {}

    public var hint: MusicTransitionHint? { pendingHint }

    public mutating func issue(_ hint: MusicTransitionHint) {
        pendingHint = hint
        remainingUnchangedObservations = Self.maximumUnchangedObservations
    }

    public mutating func clear() {
        pendingHint = nil
        remainingUnchangedObservations = nil
    }

    public mutating func resolve(
        previous: MusicNowPlaying?,
        current: MusicNowPlaying?,
        appliedHint: MusicTransitionHint?
    ) {
        guard pendingHint == .skip, appliedHint == .skip else { return }
        guard let current else {
            clear()
            return
        }
        if MusicTransitionHintTracker.isProviderFailure(current.state) {
            clear()
            return
        }
        guard let previous, current.item != nil else {
            consumeUnchangedObservation()
            return
        }
        if previous.provider != current.provider
            || previous.item?.identifier != current.item?.identifier
        {
            clear()
        } else {
            consumeUnchangedObservation()
        }
    }

    private mutating func consumeUnchangedObservation() {
        guard let remainingUnchangedObservations else { return }
        guard remainingUnchangedObservations > 1 else {
            clear()
            return
        }
        self.remainingUnchangedObservations = remainingUnchangedObservations - 1
    }

    private static func isProviderFailure(_ state: MobileMusicPlaybackStateDto) -> Bool {
        switch state {
        case .unauthorized, .unavailable, .disconnected, .stale:
            true
        default:
            false
        }
    }
}


public extension MobileMusicProviderDto {
    static var allCases: [Self] { [.appleMusic, .spotify] }

    var monitoringMode: MusicProviderMonitoringMode {
        switch self {
        case .appleMusic: .appleMusicSystemPlayer
        case .spotify: .unavailable
        }
    }

    var title: String {
        switch self {
        case .appleMusic: pevLocalizedText("music.provider.apple_music")
        case .spotify: pevLocalizedText("music.provider.spotify")
        }
    }
}

/// Identifies the currently owning music-monitor task.
///
/// A cancelled task may finish after a replacement task starts. The generation
/// keeps that stale task from tearing down the replacement provider observer.
public struct MusicMonitorGeneration: Sendable, Equatable {
    public private(set) var current: UInt64 = 0

    public init() {}

    public mutating func begin() -> UInt64 {
        current &+= 1
        return current
    }

    public mutating func invalidate() {
        current &+= 1
    }

    public func owns(_ generation: UInt64) -> Bool {
        generation == current
    }
}

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

/// Persists the user's default ride-music history choice for future rides.
///
/// The active ride's policy remains Rust-owned; this store is only the app
/// preference used when no ride is open yet.
public struct MusicHistoryPolicyStore {
    private static let key = "io.cutout.music.history-policy.default"
    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var policy: MobileMusicHistoryPolicyDto {
        switch defaults.string(forKey: Self.key) {
        case "opaque_item": .opaqueItem
        case "human_readable": .humanReadable
        default: .disabled
        }
    }

    public func set(_ policy: MobileMusicHistoryPolicyDto) {
        defaults.set(Self.storageValue(for: policy), forKey: Self.key)
    }

    private static func storageValue(for policy: MobileMusicHistoryPolicyDto) -> String {
        switch policy {
        case .disabled: "disabled"
        case .opaqueItem: "opaque_item"
        case .humanReadable: "human_readable"
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

/// The Rust-owned ride association is the only path for music metadata to enter a ride.
@MainActor
public final class MusicIntegrationCoordinator {
    public private(set) var nowPlaying: MusicNowPlaying?
    private let rideMapState: MobileRideMapState?

    private var lastObservedAtByProvider = [MobileMusicProviderDto: UInt64]()
    private var lastCorrelationRideID: String?
    private var lastPersistedNowPlaying: MusicNowPlaying?
    private var historyPolicy = MobileMusicHistoryPolicyDto.disabled
    public init(rideMapState: MobileRideMapState?) {
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
        clockUncertaintyMs: UInt64,
        transitionHint: MusicTransitionHint? = nil
    ) throws -> MobileMusicTimelineOutcomeDto? {
        try ingest(
            snapshot: snapshot,
            artwork: nil,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs,
            transitionHint: transitionHint
        )
    }

    private func ingest(
        snapshot: MobileMusicSnapshotDto,
        artwork: MusicArtwork?,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64,
        transitionHint: MusicTransitionHint?
    ) throws -> MobileMusicTimelineOutcomeDto? {
        resetCorrelationIfRideChanged()
        guard accept(snapshot) else { return nil }
        let previous = lastPersistedNowPlaying
        update(snapshot: snapshot, artwork: artwork)
        guard let kind = Self.transitionKind(
            from: previous,
            to: nowPlaying,
            hint: transitionHint
        ) else {
            return nil
        }
        do {
            guard let rideMapState else {
                rememberPersistedState(.disabled)
                return .disabled
            }
            let outcome = try rideMapState.recordMusicEvent(
                snapshot: snapshot,
                kind: kind,
                monotonicAtMs: snapshot.observedAtMs,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
            rememberPersistedState(outcome)
            return outcome
        } catch MobileRideMapError.noActiveRide {
            if historyPolicy == .disabled {
                rememberPersistedState(.disabled)
                return .disabled
            }
            lastPersistedNowPlaying = nowPlaying
            throw MobileRideMapError.noActiveRide
        }
    }

    /// Applies one provider observation through the same path used by ride
    /// recording and compact-player state.
    @discardableResult
    public func ingest(
        observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64,
        transitionHint: MusicTransitionHint? = nil
    ) throws -> MobileMusicTimelineOutcomeDto? {
        try ingest(
            snapshot: observation.snapshot,
            artwork: observation.artwork,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs,
            transitionHint: transitionHint
        )
    }

    public func setHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) throws {
        guard let rideMapState else {
            throw MobileRideMapError.storageError("Rust ride database is unavailable")
        }
        try rideMapState.setMusicHistoryPolicy(policy)
        adoptHistoryPolicy(policy)
    }

    /// Adopts a policy restored by Rust without issuing a second persistence write.
    public func restoreHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) {
        adoptHistoryPolicy(policy)
    }

    private func adoptHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) {
        let previousPolicy = historyPolicy
        historyPolicy = policy
        rebasePersistedState(from: previousPolicy, to: policy)
    }

    public func record(
        snapshot: MobileMusicSnapshotDto,
        kind: MobileMusicRideEventKindDto,
        monotonicAtMs: UInt64,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) throws -> MobileMusicTimelineOutcomeDto {
        resetCorrelationIfRideChanged()
        guard accept(snapshot) else { return .outOfOrder }
        update(snapshot: snapshot)
        guard let rideMapState else {
            return .rideNotOpen
        }
        let outcome = try rideMapState.recordMusicEvent(
            snapshot: snapshot,
            kind: kind,
            monotonicAtMs: monotonicAtMs,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs
        )
        rememberPersistedState(outcome)
        return outcome
    }

    public var recordedEvents: [MobileMusicRideEventDto] {
        rideMapState?.currentMusicEvents() ?? []
    }

    private func resetCorrelationIfRideChanged() {
        let rideID = rideMapState?.currentSnapshot()?.rideID
        guard rideID != lastCorrelationRideID else { return }
        lastCorrelationRideID = rideID
        lastObservedAtByProvider.removeAll()
        lastPersistedNowPlaying = nil
    }

    private func rememberPersistedState(_ outcome: MobileMusicTimelineOutcomeDto) {
        switch outcome {
        case .recorded, .duplicate, .disabled:
            lastPersistedNowPlaying = nowPlaying
        case .outOfOrder, .rideNotOpen, .full:
            break
        }
    }

    private func rebasePersistedState(
        from previousPolicy: MobileMusicHistoryPolicyDto,
        to policy: MobileMusicHistoryPolicyDto
    ) {
        switch (previousPolicy, policy) {
        case (.disabled, .opaqueItem), (.disabled, .humanReadable):
            // Enabling history should capture the current item on the next
            // accepted observation, even if it was already playing.
            lastPersistedNowPlaying = nil
        case (_, .disabled):
            // Keep the current player as the baseline while history is off so
            // a later re-enable can deliberately start a new association.
            lastPersistedNowPlaying = nowPlaying
        default:
            // Redaction and display-policy changes are not music transitions.
            // Preserve the baseline so the next poll cannot duplicate one.
            break
        }
    }

    private func accept(_ snapshot: MobileMusicSnapshotDto) -> Bool {
        guard let lastObservedAtMs = lastObservedAtByProvider[snapshot.provider] else {
            lastObservedAtByProvider[snapshot.provider] = snapshot.observedAtMs
            return true
        }
        guard snapshot.observedAtMs > lastObservedAtMs else { return false }
        lastObservedAtByProvider[snapshot.provider] = snapshot.observedAtMs
        return true
    }

    private static func transitionKind(
        from previous: MusicNowPlaying?,
        to current: MusicNowPlaying?,
        hint: MusicTransitionHint?
    ) -> MobileMusicRideEventKindDto? {
        guard let current else { return .providerDisconnected }
        guard let previous else { return current.item == nil ? nil : .itemChanged }
        if Self.isProviderFailure(current.state) {
            return Self.isProviderFailure(previous.state)
                ? nil
                : .providerDisconnected
        }
        if previous.provider != current.provider {
            return .itemChanged
        }
        if previous.item?.identifier != current.item?.identifier {
            if hint == .skip, previous.item != nil, current.item != nil {
                return .skip
            }
            return .itemChanged
        }
        switch (previous.state, current.state) {
        case (_, .playing) where previous.state != .playing:
            return MobileMusicRideEventKindDto.play
        case (_, .paused) where previous.state != .paused:
            return MobileMusicRideEventKindDto.pause
        default:
            return nil
        }
    }

    private static func isProviderFailure(_ state: MobileMusicPlaybackStateDto) -> Bool {
        switch state {
        case .unauthorized, .unavailable, .disconnected, .stale:
            true
        default:
            false
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
    @State private var accessibilityAnnouncementTracker = MusicAccessibilityAnnouncementTracker()

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
            if nowPlaying.supports(.previous) {
                Button { onCommand(.previous) } label: {
                    Image(systemName: "backward.fill")
                }
                .accessibilityLabel(pevLocalizedText("music.previous"))
            }
            if let command = nowPlaying.playPauseCommand {
                Button { onCommand(command) } label: {
                    Image(systemName: command == .pause ? "pause.fill" : "play.fill")
                }
                .accessibilityLabel(
                    pevLocalizedText(command == .pause ? "music.pause" : "music.play")
                )
            }
            if nowPlaying.supports(.next) {
                Button { onCommand(.next) } label: {
                    Image(systemName: "forward.fill")
                }
                .accessibilityLabel(pevLocalizedText("music.next"))
            }
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
                .accessibilityLabel(nowPlaying.artworkAccessibilityLabel)
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
                .accessibilityLabel(nowPlaying.artworkAccessibilityLabel)
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
                        MusicTimelineRows(events: timeline)
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
    public let onConnect: () -> Void
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
            } else {
                Button(action: onConnect) {
                    Label(
                        pevLocalizedText("music.connect"),
                        systemImage: "music.note"
                    )
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("music.connect")
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
        onConnect: @escaping () -> Void,
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
            onConnect: onConnect,
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
