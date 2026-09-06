import CutoutMobileFFI
import Foundation
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
        guard MusicObservationValidator.accepts(snapshot) else { return nil }
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
        guard MusicObservationValidator.accepts(snapshot) else {
            throw MobileRideMapError.storageError("invalid music observation")
        }
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
