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
/// Spotify uses App Remote when the iOS SDK and app credentials are present;
/// otherwise it remains a truthful handoff/unavailable state. It must never
/// fall through to the Apple Music system-player monitor.
public enum MusicProviderMonitoringMode: Equatable, Sendable {
    case appleMusicSystemPlayer
    case spotifyAppRemote
    case unavailable
}

/// Holds a transport hint until the provider reports the resulting state.
///
/// System-player notifications can arrive after the immediate post-command
/// poll, so the hint survives several unchanged snapshots but expires when the
/// provider never reports a resulting item change.
public struct MusicTransitionHintTracker: Sendable {
    private static let maxAgeMilliseconds: UInt64 = 5_000
    private static let maximumUnchangedObservations = 5
    private struct PendingHint: Sendable {
        let id: UInt64
        let hint: MusicTransitionHint
        let issuedAtMs: UInt64?
    }

    private var pendingHints = [PendingHint]()
    private var nextID: UInt64 = 0
    private var remainingUnchangedObservations: Int?

    public var pendingHint: MusicTransitionHint? { pendingHints.first?.hint }

    public init() {
        pendingHints = []
    }

    public var hint: MusicTransitionHint? { pendingHint }

    @discardableResult
    public mutating func issue(_ hint: MusicTransitionHint, issuedAtMs: UInt64? = nil) -> UInt64 {
        let id = nextID
        nextID &+= 1
        pendingHints.append(PendingHint(id: id, hint: hint, issuedAtMs: issuedAtMs))
        remainingUnchangedObservations = Self.maximumUnchangedObservations
        return id
    }

    public mutating func hint(atMonotonicMs monotonicMs: UInt64) -> MusicTransitionHint? {
        pendingHints.removeAll { hint in
            guard let issuedAtMs = hint.issuedAtMs,
                  monotonicMs >= issuedAtMs
            else { return false }
            return monotonicMs - issuedAtMs > Self.maxAgeMilliseconds
        }
        if pendingHints.isEmpty {
            remainingUnchangedObservations = nil
        }
        return pendingHints.first?.hint
    }

    public mutating func clear() {
        pendingHints.removeAll(keepingCapacity: true)
        remainingUnchangedObservations = nil
    }

    public mutating func clear(id: UInt64) {
        let clearsFront = pendingHints.first?.id == id
        pendingHints.removeAll { $0.id == id }
        guard clearsFront else { return }
        remainingUnchangedObservations = pendingHints.isEmpty ? nil : Self.maximumUnchangedObservations
    }

    public mutating func resolve(
        previous: MusicNowPlaying?,
        current: MusicNowPlaying?,
        appliedHint: MusicTransitionHint?,
        currentObservedAtMs: UInt64? = nil
    ) {
        guard pendingHint == .skip, appliedHint == .skip,
              let pending = pendingHints.first
        else { return }
        if let issuedAtMs = pending.issuedAtMs,
           let currentObservedAtMs,
           currentObservedAtMs >= issuedAtMs,
           currentObservedAtMs - issuedAtMs > Self.maxAgeMilliseconds {
            removeFirstPending()
            return
        }
        guard let current else {
            removeFirstPending()
            return
        }
        if MusicTransitionHintTracker.isTerminalState(current.state) {
            removeFirstPending()
            return
        }
        guard let previous, current.item != nil else {
            consumeUnchangedObservation()
            return
        }
        if previous.provider != current.provider
            || previous.item?.identifier != current.item?.identifier
        {
            removeFirstPending()
        } else {
            consumeUnchangedObservation()
        }
    }

    private mutating func removeFirstPending() {
        pendingHints.removeFirst()
        remainingUnchangedObservations = pendingHints.isEmpty
            ? nil
            : Self.maximumUnchangedObservations
    }

    private mutating func consumeUnchangedObservation() {
        let remaining = remainingUnchangedObservations ?? Self.maximumUnchangedObservations
        guard remaining > 1 else {
            removeFirstPending()
            return
        }
        remainingUnchangedObservations = remaining - 1
    }

    private static func isTerminalState(_ state: MobileMusicPlaybackStateDto) -> Bool {
        switch state {
        case .stopped, .unauthorized, .unavailable, .disconnected, .stale:
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
#if canImport(SpotifyiOS) && os(iOS)
        case .spotify: .spotifyAppRemote
#else
        case .spotify: .unavailable
#endif
        }
    }

    var title: String {
        switch self {
        case .appleMusic: pevLocalizedText("music.provider.apple_music")
        case .spotify: pevLocalizedText("music.provider.spotify")
        }
    }
}

extension MobileMusicHistoryPolicyDto {
    var musicAccessibilityIdentifier: String {
        switch self {
        case .disabled: "disabled"
        case .opaqueItem: "opaque-item"
        case .humanReadable: "human-readable"
        }
    }
}

/// Persists the provider selected for the compact music player.
public struct MusicProviderSelectionStore {
    private static let key = "io.cutout.music.provider.selected"
    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var provider: MobileMusicProviderDto {
        defaults.string(forKey: Self.key) == "spotify" ? .spotify : .appleMusic
    }

    public func set(_ provider: MobileMusicProviderDto) {
        defaults.set(provider == .spotify ? "spotify" : "apple_music", forKey: Self.key)
    }
}

/// Persists whether the user asked the app to monitor the selected music provider.
/// The provider session itself remains owned by the platform adapter; this is
/// only the durable setup intent used to restore monitoring on the next launch.
public struct MusicMonitoringPreferenceStore {
    private static let key = "io.cutout.music.monitoring.enabled"
    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var isEnabled: Bool {
        defaults.bool(forKey: Self.key)
    }

    public func setEnabled(_ enabled: Bool) {
        defaults.set(enabled, forKey: Self.key)
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

    public var title: String {
        item?.title.flatMap { $0.isEmpty ? nil : $0 }
            ?? pevLocalizedText(musicPlaybackTitleKey(state: state))
    }
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

    /// Setup remains available after a provider handoff or failed connection.
    /// A non-nil snapshot is still useful for showing the truthful lifecycle
    /// state, but it must not hide the main-screen setup action.
    public var requiresSetup: Bool {
        switch state {
        case .unauthorized, .unavailable, .disconnected, .stale:
            true
        default:
            false
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
        let artworkName = item?.title.flatMap { $0.isEmpty ? nil : $0 } ?? providerName
        return pevLocalizedText("music.artwork", artworkName)
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
            if command == .previous || command == .next {
                guard state == .playing || state == .paused else { return false }
            }
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
        case .stopped: pevLocalizedText("music.timeline.stopped")
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
        case .stopped: "stopped"
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

/// Converts provider seconds into bounded milliseconds without trapping.
enum MusicTimeConversion {
    static func milliseconds(_ seconds: TimeInterval) -> UInt64? {
        guard seconds.isFinite, seconds >= 0 else { return nil }
        let milliseconds = seconds * 1_000
        guard milliseconds < Double(UInt64.max) else { return nil }
        return UInt64(milliseconds)
    }
}

/// The Rust-owned ride association is the only path for music metadata to enter a ride.
@MainActor
public final class MusicIntegrationCoordinator {
    public private(set) var nowPlaying: MusicNowPlaying?
    private let rideMapState: MobileRideMapState?

    private var lastObservedAtByProvider = [MobileMusicProviderDto: UInt64]()
    private var lastCorrelationRideID: String?
    private var lastPersistedSnapshotByProvider = [MobileMusicProviderDto: MobileMusicSnapshotDto]()
    private var historyPolicy = MobileMusicHistoryPolicyDto.disabled
    public private(set) var lastRecordedSequence: UInt64?
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
        guard let snapshot = try? normalizeMusicSnapshot(snapshot: snapshot) else { return nil }
        guard accept(snapshot) else { return nil }
        let previous = lastPersistedSnapshotByProvider[snapshot.provider]
        update(snapshot: snapshot, artwork: artwork)
        guard let kind = try musicTransitionKind(
            previous: previous,
            current: snapshot,
            skipHint: transitionHint == .skip
        ) else {
            return nil
        }
        do {
            guard let rideMapState else {
                rememberPersistedState(.disabled, snapshot: snapshot)
                return .disabled
            }
            let result = try rideMapState.recordMusicEventWithSequence(
                snapshot: snapshot,
                kind: kind,
                monotonicAtMs: snapshot.observedAtMs,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
            lastRecordedSequence = result.sequence
            let outcome = result.outcome
            rememberPersistedState(outcome, snapshot: snapshot)
            return outcome
        } catch MobileRideMapError.noActiveRide {
            if historyPolicy == .disabled {
                rememberPersistedState(.disabled, snapshot: snapshot)
                return .disabled
            }
            lastPersistedSnapshotByProvider[snapshot.provider] = snapshot
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
            // A future-ride preference is still valid without a current ride store.
            adoptHistoryPolicy(policy)
            return
        }
        try rideMapState.setMusicHistoryPolicy(policy)
        adoptHistoryPolicy(policy)
    }

    /// Drops provider-local observations before a deliberate provider switch.
    /// Selection itself must not synthesize a stop, disconnect, or item change.
    public func resetProviderCorrelation() {
        lastObservedAtByProvider.removeAll(keepingCapacity: true)
        lastCorrelationRideID = rideMapState?.currentSnapshot()?.rideID
        nowPlaying = nil
        lastRecordedSequence = nil
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
        let snapshot = try validatedMusicSnapshot(snapshot)
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
        rememberPersistedState(outcome, snapshot: snapshot)
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
        lastPersistedSnapshotByProvider.removeAll()
        lastRecordedSequence = nil
    }

    private func rememberPersistedState(
        _ outcome: MobileMusicTimelineOutcomeDto,
        snapshot: MobileMusicSnapshotDto
    ) {
        switch outcome {
        case .recorded, .duplicate, .disabled:
            lastPersistedSnapshotByProvider[snapshot.provider] = snapshot
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
            lastPersistedSnapshotByProvider.removeAll(keepingCapacity: true)
        case (_, .disabled):
            // Keep the current player as the baseline while history is off so
            // a later re-enable can deliberately start a new association.
            lastPersistedSnapshotByProvider.removeAll(keepingCapacity: true)
        default:
            // Redaction and display-policy changes are not music transitions.
            // Preserve the baseline so the next poll cannot duplicate one.
            break
        }
    }

    private func accept(_ snapshot: MobileMusicSnapshotDto) -> Bool {
        let lastObservedAtMs = lastObservedAtByProvider[snapshot.provider]
        guard acceptMusicSnapshot(
            previousObservedAtMs: lastObservedAtMs,
            currentObservedAtMs: snapshot.observedAtMs
        ) else { return false }
        lastObservedAtByProvider[snapshot.provider] = snapshot.observedAtMs
        return true
    }

    private func validatedMusicSnapshot(
        _ snapshot: MobileMusicSnapshotDto
    ) throws -> MobileMusicSnapshotDto {
        do {
            return try normalizeMusicSnapshot(snapshot: snapshot)
        } catch let error as MobileRideMapCoreErrorDto {
            switch error {
            case let .InvalidMusicInput(message):
                throw MobileRideMapError.invalidMusicInput(message)
            default:
                throw MobileRideMapError.storageError(String(describing: error))
            }
        }
    }

}

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

    @ViewBuilder
    private var artworkView: some View {
#if canImport(UIKit) && os(iOS)
        if let data = nowPlaying.artwork?.data, let image = UIImage(data: data) {
            Image(uiImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: 44, height: 44)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .accessibilityLabel(nowPlaying.artworkAccessibilityLabel)
        } else {
            Image(systemName: "music.note")
                .font(.title3)
                .frame(width: 44, height: 44)
                .background(PevDashboardColors.yellow.opacity(0.16), in: RoundedRectangle(cornerRadius: 12))
                .foregroundStyle(PevDashboardColors.yellow)
                .accessibilityHidden(true)
        }
#elseif canImport(AppKit)
        if let data = nowPlaying.artwork?.data, let image = NSImage(data: data) {
            Image(nsImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: 44, height: 44)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .accessibilityLabel(nowPlaying.artworkAccessibilityLabel)
        } else {
            Image(systemName: "music.note")
                .font(.title3)
                .frame(width: 44, height: 44)
                .background(PevDashboardColors.yellow.opacity(0.16), in: RoundedRectangle(cornerRadius: 12))
                .foregroundStyle(PevDashboardColors.yellow)
                .accessibilityHidden(true)
        }
#else
        Image(systemName: "music.note")
            .accessibilityHidden(true)
#endif
    }
}

private struct MusicTransportControls: View {
    let nowPlaying: MusicNowPlaying
    let onCommand: (MobileMusicCommandDto) -> Void

    var body: some View {
        HStack(spacing: 4) {
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
                .frame(minWidth: isProminent ? 36 : 30, minHeight: 36)
                .background(
                    isProminent ? PevDashboardColors.yellow.opacity(0.18) : .clear,
                    in: Circle()
                )
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

    @ViewBuilder
    private var artworkView: some View {
#if canImport(UIKit) && os(iOS)
        if let data = nowPlaying.artwork?.data, let image = UIImage(data: data) {
            Image(uiImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: 84, height: 84)
                .clipShape(RoundedRectangle(cornerRadius: 16))
                .accessibilityLabel(nowPlaying.artworkAccessibilityLabel)
        } else {
            MusicArtworkPlaceholder()
        }
#elseif canImport(AppKit)
        if let data = nowPlaying.artwork?.data, let image = NSImage(data: data) {
            Image(nsImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: 84, height: 84)
                .clipShape(RoundedRectangle(cornerRadius: 16))
                .accessibilityLabel(nowPlaying.artworkAccessibilityLabel)
        } else {
            MusicArtworkPlaceholder()
        }
#else
        MusicArtworkPlaceholder()
#endif
    }
}

private struct MusicArtworkPlaceholder: View {
    var body: some View {
        Image(systemName: "music.note")
            .font(.largeTitle)
            .foregroundStyle(PevDashboardColors.yellow)
            .frame(width: 84, height: 84)
            .background(PevDashboardColors.yellow.opacity(0.16), in: RoundedRectangle(cornerRadius: 16))
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

    isolated deinit {
        let center = NotificationCenter.default
        notificationTokens.forEach(center.removeObserver)
        player.endGeneratingPlaybackNotifications()
    }

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
        artworkCache.artwork(for: player.nowPlayingItem.map(appleMusicIdentifier)) {
            loadArtwork()
        }?.data
    }

    private func loadArtwork() -> MusicArtwork? {
#if canImport(UIKit) && os(iOS)
        guard let artwork = player.nowPlayingItem?.artwork,
              let image = artwork.image(at: Self.artworkSize),
              let data = image.jpegData(compressionQuality: 0.8)
        else {
            return nil
        }
        return MusicArtwork(data: data)
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
#endif
