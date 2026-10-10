import CoreGraphics
import CutoutMobileFFI
import Foundation
import SwiftUI

/// Presentation-only artwork retained in Swift at the provider-requested size.
/// Artwork never enters the Rust ride or UniFFI contracts.
public struct MusicArtwork: Equatable, Sendable {
    public static let maxPixelDimension = 1_024

    public let image: CGImage

    public init?(image: CGImage) {
        guard image.width <= Self.maxPixelDimension,
            image.height <= Self.maxPixelDimension
        else { return nil }
        self.image = image
    }
}

/// Keeps one positive artwork result so polling does not repeatedly decode the
/// same provider image. The value is already bounded by `MusicArtwork`.
struct MusicArtworkCache: Sendable {
    private var itemIdentifier: String?
    private var cachedArtwork: MusicArtwork?

    func cachedArtwork(for itemIdentifier: String?) -> MusicArtwork? {
        guard let itemIdentifier, self.itemIdentifier == itemIdentifier else { return nil }
        return cachedArtwork
    }

    mutating func insert(_ artwork: MusicArtwork, for itemIdentifier: String) {
        self.itemIdentifier = itemIdentifier
        cachedArtwork = artwork
    }

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

/// Visible command feedback derived from the terminal provider outcome.
public struct MusicCommandFeedback: Equatable, Sendable {
    public let requestID: MobileMusicCommandFeedbackId
    public let outcome: MusicCommandOutcome

    public init(requestID: MobileMusicCommandFeedbackId, outcome: MusicCommandOutcome) {
        self.requestID = requestID
        self.outcome = outcome
    }

    public var messageKey: String? {
        switch outcome {
        case .accepted: nil
        case .refused: "music.command.refused"
        case .failed: "music.command.failed"
        case .unavailable: "music.command.unavailable"
        }
    }
}

/// Resolves one provider transport callback exactly once under the Rust-owned lifecycle.
@MainActor
final class MusicTransportCoordinator {
    typealias Completion = @MainActor (MobileMusicTransportRequestId, MusicCommandOutcome) -> Void

    private let lifecycle: MobileMusicProviderLifecycle
    private var pending:
        (
            providerGeneration: MobileMusicProviderSessionId, requestID: MobileMusicTransportRequestId,
            completion: Completion
        )?

    init(lifecycle: MobileMusicProviderLifecycle) {
        self.lifecycle = lifecycle
    }

    func register(
        providerGeneration: MobileMusicProviderSessionId,
        effect: MobileMusicTransportEffect,
        completion: @escaping Completion
    ) -> Bool {
        guard pending == nil else { return false }
        pending = (providerGeneration, effect.id, completion)
        return true
    }

    func finish(
        providerGeneration: MobileMusicProviderSessionId,
        requestID: MobileMusicTransportRequestId,
        accepted: Bool
    ) {
        apply(
            lifecycle.finishTransport(
                providerGeneration: providerGeneration,
                requestId: requestID,
                outcome: accepted ? .accepted : .failed
            ),
            requestID: requestID
        )
    }

    func expire(
        providerGeneration: MobileMusicProviderSessionId, requestID: MobileMusicTransportRequestId, nowMs: UInt64
    ) {
        apply(
            lifecycle.expireTransport(providerGeneration: providerGeneration, nowMs: nowMs),
            requestID: requestID
        )
    }

    func cancel(providerGeneration: MobileMusicProviderSessionId, requestID: MobileMusicTransportRequestId) {
        apply(
            lifecycle.cancelTransport(
                providerGeneration: providerGeneration,
                requestId: requestID
            ),
            requestID: requestID
        )
    }

    func resolveCancelled(requestID: MobileMusicTransportRequestId?) {
        guard let requestID,
            pending?.requestID == requestID,
            let completion = pending?.completion
        else { return }
        pending = nil
        completion(requestID, .unavailable)
    }

    func apply(
        _ result: MobileMusicTransportCompletion, requestID fallbackRequestID: MobileMusicTransportRequestId? = nil
    ) {
        let requestID = result.requestId ?? fallbackRequestID
        guard let requestID, pending?.requestID == requestID, let completion = pending?.completion else { return }
        if result.state == .stale {
            pending = nil
            completion(requestID, .unavailable)
            return
        }
        guard result.state == .finished, let outcome = result.outcome else { return }
        pending = nil
        completion(requestID, outcome.commandOutcome)
    }
}

/// Executes one provider command under Rust admission, identity, and deadline policy.
@MainActor
final class MusicProviderTransportExecutor {
    private let lifecycle: MobileMusicProviderLifecycle
    private let effects: MusicProviderEffectExecutor
    private let nowMs: @MainActor () -> UInt64
    private lazy var coordinator = MusicTransportCoordinator(lifecycle: lifecycle)

    init(
        lifecycle: MobileMusicProviderLifecycle,
        effects: MusicProviderEffectExecutor,
        nowMs: @escaping @MainActor () -> UInt64
    ) {
        self.lifecycle = lifecycle
        self.effects = effects
        self.nowMs = nowMs
    }

    func perform(
        owner: MobileMusicTransportOwner,
        command: MobileMusicCommandDto,
        onTerminal: @escaping @MainActor (MobileMusicTransportRequestId) -> Void = { _ in },
        dispatch: @escaping @MainActor @Sendable (
            MobileMusicTransportRequestId,
            @escaping @MainActor @Sendable (Bool) -> Void
        ) -> Void
    ) async -> MusicCommandOutcome {
        guard !Task.isCancelled else { return .unavailable }
        let providerGeneration =
            switch owner {
            case let .provider(providerGeneration): providerGeneration
            case let .connection(providerGeneration, _): providerGeneration
            }
        let effect = lifecycle.beginTransportEffect(
            owner: owner,
            command: command,
            nowMs: nowMs()
        )
        guard let effect else { return .refused }

        return await perform(
            providerGeneration: providerGeneration, effect: effect, onTerminal: onTerminal, dispatch: dispatch)
    }

    /// Executes an effect already admitted by Rust without admitting a second request.
    func perform(
        providerGeneration: MobileMusicProviderSessionId,
        effect: MobileMusicTransportEffect,
        onTerminal: @escaping @MainActor (MobileMusicTransportRequestId) -> Void = { _ in },
        dispatch: @escaping @MainActor @Sendable (
            MobileMusicTransportRequestId,
            @escaping @MainActor @Sendable (Bool) -> Void
        ) -> Void
    ) async -> MusicCommandOutcome {

        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                guard
                    coordinator.register(
                        providerGeneration: providerGeneration,
                        effect: effect,
                        completion: { [effects] requestID, outcome in
                            effects.cancel(.transport(requestID))
                            onTerminal(requestID)
                            continuation.resume(returning: outcome)
                        }
                    )
                else {
                    coordinator.cancel(
                        providerGeneration: providerGeneration,
                        requestID: effect.id
                    )
                    onTerminal(effect.id)
                    continuation.resume(returning: .refused)
                    return
                }
                guard !Task.isCancelled else {
                    coordinator.cancel(
                        providerGeneration: providerGeneration,
                        requestID: effect.id
                    )
                    onTerminal(effect.id)
                    return
                }
                effects.run(
                    .transport(effect.id),
                    until: effect.deadlineMs,
                    nowMs: nowMs
                ) { [weak self] in
                    self?.coordinator.expire(
                        providerGeneration: providerGeneration,
                        requestID: effect.id,
                        nowMs: effect.deadlineMs
                    )
                }
                dispatch(effect.id) { [weak self] accepted in
                    guard let self else { return }
                    self.coordinator.finish(
                        providerGeneration: providerGeneration,
                        requestID: effect.id,
                        accepted: accepted
                    )
                }
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                guard let self else { return }
                self.effects.cancel(.transport(effect.id))
                self.coordinator.cancel(
                    providerGeneration: providerGeneration,
                    requestID: effect.id
                )
                onTerminal(effect.id)
            }
        }
    }

    func apply(_ completion: MobileMusicTransportCompletion) {
        if let requestID = completion.requestId {
            effects.cancel(.transport(requestID))
        }
        coordinator.apply(completion)
    }

    func apply(_ suspension: MobileMusicProviderSuspension) {
        guard let requestID = suspension.cancelledTransportRequestId else { return }
        effects.cancel(.transport(requestID))
        coordinator.resolveCancelled(requestID: requestID)
    }
}

struct MusicCommandTaskID: Equatable, Sendable {
    fileprivate let rawValue: UInt64
}

@MainActor
final class MusicCommandTaskSlot {
    private var nextID: UInt64 = 0
    private var task: Task<Void, Never>?
    private(set) var currentID: MusicCommandTaskID?

    @discardableResult
    func reserve() -> MusicCommandTaskID {
        task?.cancel()
        task = nil
        nextID &+= 1
        let id = MusicCommandTaskID(rawValue: nextID)
        currentID = id
        return id
    }

    func install(_ task: Task<Void, Never>, for id: MusicCommandTaskID) {
        guard currentID == id else {
            task.cancel()
            return
        }
        self.task = task
    }

    func finish(_ id: MusicCommandTaskID) {
        guard currentID == id else { return }
        task = nil
        currentID = nil
    }

    func cancel() {
        task?.cancel()
        task = nil
        currentID = nil
    }

    func cancel(_ id: MusicCommandTaskID) {
        guard currentID == id else { return }
        task?.cancel()
        task = nil
        currentID = nil
    }
}

extension MobileMusicTransportOutcome {
    fileprivate var commandOutcome: MusicCommandOutcome {
        switch self {
        case .accepted: .accepted
        case .failed, .timedOut: .failed
        case .cancelled: .unavailable
        }
    }
}

/// Describes the provider lifecycle that the app can currently monitor.
///
/// Spotify uses App Remote when the iOS SDK and app credentials are present;
/// otherwise it remains a truthful handoff/unavailable state. It must never
/// fall through to the Apple Music system-player monitor.
public enum MusicProviderMonitoringMode: Equatable, Sendable {
    case appleMusicSystemPlayer
    case spotifyAppRemote
    case appHandoffOnly
    case unavailable
}

extension MobileMusicProviderDto {
    public static var allCases: [Self] { [.appleMusic, .spotify, .soundCloud] }

    public var profile: MobileMusicProviderProfile { musicProviderProfile(provider: self) }

    public var monitoringMode: MusicProviderMonitoringMode {
        switch profile.interface {
        case .appleMusicSystemPlayer: .appleMusicSystemPlayer
        case .appHandoffOnly: .appHandoffOnly
        #if canImport(SpotifyiOS) && os(iOS)
            case .spotifyAppRemote: .spotifyAppRemote
        #else
            case .spotifyAppRemote: .unavailable
        #endif
        }
    }

    public var title: String {
        pevLocalizedText(profile.titleKey)
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
        musicProviderFromStorage(value: defaults.string(forKey: Self.key))
    }

    public func set(_ provider: MobileMusicProviderDto) {
        defaults.set(provider.profile.storageKey, forKey: Self.key)
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

/// Executes platform effects without inventing lifecycle identities in Swift.
@MainActor
public final class MusicProviderEffectExecutor {
    public enum Namespace: Equatable, Sendable {
        case monitor
        case authorization
        case provider
        case playerState
        case playerStateTimeout
        case transport
        case artwork
        case artworkRetry
        case handoff
    }

    public enum Key: Hashable, Sendable {
        case monitor(MobileMusicMonitorId)
        case authorization(MobileMusicAuthorizationId)
        case provider(MobileMusicProviderSessionId)
        case playerState(MobileMusicPlayerStateRequestId)
        case playerStateTimeout(MobileMusicPlayerStateRequestId)
        case transport(MobileMusicTransportRequestId)
        case artwork(MobileMusicArtworkRequestId)
        case artworkRetry(MobileMusicArtworkRetryId)
        case handoff(MobileMusicPlayHandoffId)

        var namespace: Namespace {
            switch self {
            case .monitor: .monitor
            case .authorization: .authorization
            case .provider: .provider
            case .playerState: .playerState
            case .playerStateTimeout: .playerStateTimeout
            case .transport: .transport
            case .artwork: .artwork
            case .artworkRetry: .artworkRetry
            case .handoff: .handoff
            }
        }
    }

    private var tasks = [Key: Task<Void, Never>]()

    public init() {}

    public func run(
        _ key: Key,
        operation: @escaping @MainActor @Sendable () async -> Void
    ) {
        tasks.removeValue(forKey: key)?.cancel()
        tasks[key] = Task { [weak self] in
            await operation()
            guard !Task.isCancelled else { return }
            self?.tasks[key] = nil
        }
    }

    func run(
        _ key: Key,
        until deadlineMs: UInt64,
        nowMs: @escaping @MainActor @Sendable () -> UInt64,
        operation: @escaping @MainActor @Sendable () async -> Void
    ) {
        run(key) {
            guard await Self.wait(until: deadlineMs, nowMs: nowMs) else { return }
            await operation()
        }
    }

    public func cancel(_ key: Key) {
        tasks.removeValue(forKey: key)?.cancel()
    }

    public func cancelAll(in namespace: Namespace) {
        let keys = tasks.keys.filter { $0.namespace == namespace }
        for key in keys {
            cancel(key)
        }
    }

    public func cancelAll() {
        for task in tasks.values {
            task.cancel()
        }
        tasks.removeAll()
    }

    public func isRunning(_ key: Key) -> Bool {
        tasks[key].map { !$0.isCancelled } ?? false
    }

    public func isRunning(in namespace: Namespace) -> Bool {
        tasks.contains { $0.key.namespace == namespace && !$0.value.isCancelled }
    }

    public static func wait(
        until deadlineMs: UInt64,
        nowMs: @escaping @MainActor @Sendable () -> UInt64
    ) async -> Bool {
        let now = nowMs()
        let remaining = deadlineMs > now ? deadlineMs - now : 0
        do {
            try await Task.sleep(for: .milliseconds(remaining))
            return true
        } catch {
            return false
        }
    }
}

@MainActor
struct AppleMusicNotificationGeneration {
    private(set) var isActive = false

    mutating func begin(_ operation: () -> Void) {
        guard !isActive else { return }
        operation()
        isActive = true
    }

    mutating func end(_ operation: () -> Void) {
        guard isActive else { return }
        operation()
        isActive = false
    }
}

protocol AppleMusicObservationService: Sendable {
    func subscribe(
        generation: UInt64,
        onChange: @escaping @Sendable (UInt64) -> Void
    ) async
    func unsubscribe(generation: UInt64) async
    func observation(
        generation: UInt64,
        observedAtMs: UInt64
    ) async -> MusicProviderObservation?
}

/// Bridges Apple Music SDK effects to Rust-owned lifecycle identities.
@MainActor
final class AppleMusicObservationBridge {
    private let service: any AppleMusicObservationService
    private let lifecycle: MobileMusicProviderLifecycle
    private let effects: MusicProviderEffectExecutor
    private let playerStateTimeout: Duration
    private var activeGeneration: MobileMusicProviderSessionId?
    private var observedAtMs: (@MainActor () -> UInt64)?
    private var onObservation: (@MainActor (MusicProviderObservation) -> Void)?
    private var readInFlight: MobileMusicPlayerStateRequestId?
    private var pendingRefreshObservedAtMs: UInt64?

    private(set) var cachedObservation: MusicProviderObservation?
    var providerGeneration: MobileMusicProviderSessionId? { activeGeneration }

    init(
        service: any AppleMusicObservationService,
        lifecycle: MobileMusicProviderLifecycle,
        effects: MusicProviderEffectExecutor,
        playerStateTimeout: Duration = .seconds(10)
    ) {
        self.service = service
        self.lifecycle = lifecycle
        self.effects = effects
        self.playerStateTimeout = playerStateTimeout
    }

    @discardableResult
    func startMonitoring(
        observedAtMs: @escaping @MainActor () -> UInt64,
        onObservation: @escaping @MainActor (MusicProviderObservation) -> Void
    ) async -> MobileMusicTransportCompletion? {
        let previousObservation = cachedObservation
        let retiredTransport = stopMonitoring()
        guard let generation = lifecycle.beginProviderSession() else { return retiredTransport }
        activeGeneration = generation
        self.observedAtMs = observedAtMs
        self.onObservation = onObservation
        let replacementObservedAtMs = observedAtMs()
        cachedObservation =
            previousObservation?.staleProjection(
                observedAtMs: replacementObservedAtMs
            )
            ?? MusicProviderObservation.unavailable(
                provider: .appleMusic,
                sessionId: "system-music-player",
                observedAtMs: replacementObservedAtMs,
                openProvider: true
            )
        if let cachedObservation {
            onObservation(cachedObservation)
        }

        await service.subscribe(generation: generation.value) { [weak self] reportedGeneration in
            Task { @MainActor [weak self] in
                self?.providerDidChange(generation: reportedGeneration)
            }
        }
        guard activeGeneration == generation,
            lifecycle.classifyProviderSession(id: generation) == .current
        else {
            unsubscribe(generation: generation)
            return retiredTransport
        }
        return retiredTransport
    }

    func refresh(observedAtMs: UInt64) {
        guard let generation = activeGeneration,
            lifecycle.classifyProviderSession(id: generation) == .current
        else { return }
        guard readInFlight == nil else {
            pendingRefreshObservedAtMs = max(pendingRefreshObservedAtMs ?? 0, observedAtMs)
            return
        }
        guard let requestID = lifecycle.beginPlayerStateRequest(nowMs: observedAtMs) else { return }
        readInFlight = requestID
        effects.run(.playerStateTimeout(requestID)) { [weak self] in
            do { try await Task.sleep(for: self?.playerStateTimeout ?? .seconds(10)) } catch { return }
            guard let self,
                self.activeGeneration == generation,
                self.lifecycle.classifyProviderSession(id: generation) == .current,
                self.lifecycle.expirePlayerStateRequest(
                    id: requestID,
                    nowMs: self.observedAtMs?() ?? observedAtMs
                ) == .expired,
                self.readInFlight == requestID
            else { return }
            self.readInFlight = nil
            self.publishStaleCachedObservation(fallbackObservedAtMs: observedAtMs)
            self.effects.cancel(.playerState(requestID))
            self.runPendingRefresh(generation: generation)
        }
        effects.run(.playerState(requestID)) { [weak self, service] in
            let observation = await service.observation(
                generation: generation.value,
                observedAtMs: observedAtMs
            )
            guard let self else { return }
            guard self.activeGeneration == generation,
                self.lifecycle.classifyProviderSession(id: generation) == .current
            else { return }
            let completion = self.lifecycle.completePlayerStateRequest(
                id: requestID,
                nowMs: self.observedAtMs?() ?? observedAtMs
            )
            guard self.readInFlight == requestID else {
                return
            }
            self.readInFlight = nil
            self.effects.cancel(.playerStateTimeout(requestID))
            guard completion == .accepted else {
                self.publishStaleCachedObservation(fallbackObservedAtMs: observedAtMs)
                self.runPendingRefresh(generation: generation)
                return
            }
            if let observation {
                self.cachedObservation = observation
                self.onObservation?(observation)
            }
            self.runPendingRefresh(generation: generation)
        }
    }

    private func publishStaleCachedObservation(fallbackObservedAtMs: UInt64) {
        cachedObservation = cachedObservation?.staleProjection(
            observedAtMs: observedAtMs?() ?? fallbackObservedAtMs
        )
        if let cachedObservation {
            onObservation?(cachedObservation)
        }
    }

    private func runPendingRefresh(generation: MobileMusicProviderSessionId) {
        guard activeGeneration == generation,
            lifecycle.classifyProviderSession(id: generation) == .current,
            let pendingRefreshObservedAtMs
        else { return }
        self.pendingRefreshObservedAtMs = nil
        refresh(observedAtMs: pendingRefreshObservedAtMs)
    }

    func stopMonitoring() -> MobileMusicTransportCompletion? {
        guard let generation = activeGeneration else { return nil }
        activeGeneration = nil
        effects.cancelAll(in: .playerState)
        effects.cancelAll(in: .playerStateTimeout)
        readInFlight = nil
        pendingRefreshObservedAtMs = nil
        let completion = lifecycle.retireProviderSession(id: generation)
        cachedObservation = nil
        observedAtMs = nil
        onObservation = nil
        unsubscribe(generation: generation)
        return completion
    }

    private func unsubscribe(generation: MobileMusicProviderSessionId) {
        effects.run(.provider(generation)) { [service] in
            await service.unsubscribe(generation: generation.value)
        }
    }

    private func providerDidChange(generation: UInt64) {
        guard activeGeneration?.value == generation,
            let activeGeneration,
            lifecycle.classifyProviderSession(id: activeGeneration) == .current,
            let observedAtMs
        else { return }
        refresh(observedAtMs: observedAtMs())
    }
}

extension MobileMusicCapabilitiesDto {
    fileprivate func supports(_ command: MobileMusicCommandDto) -> Bool {
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
    public let positionMilliseconds: UInt64?
    public let capabilities: MobileMusicCapabilitiesDto
    public let artwork: MusicArtwork?

    public init(
        provider: MobileMusicProviderDto,
        state: MobileMusicPlaybackStateDto,
        item: MobileMusicItemDto? = nil,
        positionMilliseconds: UInt64? = nil,
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
        self.positionMilliseconds = positionMilliseconds
        self.artwork = artwork
        self.capabilities = capabilities
    }

    public init(snapshot: MobileMusicSnapshotDto, artwork: MusicArtwork? = nil) {
        self.init(
            provider: snapshot.provider,
            state: snapshot.state,
            item: snapshot.item,
            positionMilliseconds: snapshot.positionMilliseconds,
            artwork: artwork,
            capabilities: snapshot.capabilities
        )
    }

    public init(observation: MusicProviderObservation) {
        self.init(snapshot: observation.snapshot, artwork: observation.artwork)
    }

    public var providerName: String { provider.title }

    public var title: String {
        if provider.profile.interface == .appHandoffOnly && item == nil { return providerName }
        return item?.title.flatMap { $0.isEmpty ? nil : $0 }
            ?? pevLocalizedText(musicPlaybackTitleKey(state: state))
    }
    public var artist: String { item?.artist ?? "" }

    public var showsCompactPlayer: Bool {
        switch state {
        case .playing, .paused, .buffering, .interrupted:
            true
        case .stopped, .disconnected, .stale:
            item != nil
        case .unavailable where capabilities.play:
            item != nil
        case .unauthorized, .unavailable:
            false
        }
    }

    public var statusText: String? {
        switch state {
        case .playing, .paused:
            nil
        case .buffering:
            pevLocalizedText("music.state.buffering")
        case .interrupted:
            pevLocalizedText("music.state.interrupted")
        case .stopped:
            provider == .spotify && item != nil
                ? pevLocalizedText("music.open_to_resume", providerName)
                : pevLocalizedText("music.state.stopped")
        case .unauthorized:
            pevLocalizedText("music.state.authorization_required")
        case .unavailable:
            pevLocalizedText(provider.profile.unavailableKey ?? "music.state.unavailable")
        case .disconnected:
            pevLocalizedText("music.state.disconnected")
        case .stale:
            pevLocalizedText("music.state.stale")
        }
    }

    /// Playback stopping or a temporary connection gap does not require setup.
    public var requiresSetup: Bool {
        if provider.profile.interface == .appHandoffOnly { return false }
        return switch state {
        case .unauthorized:
            true
        case .unavailable:
            !(provider == .spotify && capabilities.play)
        default:
            false
        }
    }

    /// VoiceOver summary includes meaningful provider failure states without progress ticks.
    public var accessibilitySummary: String {
        var components = [providerName, title]
        if !artist.isEmpty && artist != providerName && artist != title {
            components.append(artist)
        }
        let playbackState = statusText ?? pevLocalizedText(musicPlaybackTitleKey(state: state))
        if playbackState != title {
            components.append(playbackState)
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
            positionMilliseconds: positionMilliseconds,
            artwork: artwork,
            capabilities: .init(
                previous: false,
                play: provider == .spotify && capabilities.openProvider,
                pause: false,
                next: false,
                openProvider: capabilities.openProvider
            )
        )
    }

    public var playPauseCommand: MobileMusicCommandDto? {
        switch state {
        case .playing where capabilities.pause: .pause
        case .buffering where capabilities.pause: .pause
        case .paused where capabilities.play: .play
        case .stopped where capabilities.play: .play
        case .disconnected where capabilities.play: .play
        case .stale where capabilities.play: .play
        case .unavailable where provider == .spotify && capabilities.play: .play
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

extension MobileMusicHistoryPolicyDto {
    public static var allCases: [Self] { [.disabled, .opaqueItem, .humanReadable] }

    public var title: String {
        switch self {
        case .disabled:
            pevLocalizedText("music.history.disabled")
        case .opaqueItem:
            pevLocalizedText("music.history.opaque_item")
        case .humanReadable:
            pevLocalizedText("music.history.human_readable")
        }
    }

    public var explanation: String {
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

extension MobileMusicRideEventKindDto {
    public var timelineTitle: String {
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

extension MobileMusicProviderDto {
    fileprivate var timelineIDComponent: String {
        switch self {
        case .appleMusic: "apple-music"
        case .spotify: "spotify"
        case .soundCloud: "soundcloud"
        }
    }
}

extension MobileMusicRideEventKindDto {
    fileprivate var timelineIDComponent: String {
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

extension Array where Element == MobileMusicRideEventDto {
    /// Connection diagnostics and itemless stop events remain stored, not displayed.
    public var listeningHistoryEvents: Self {
        filter {
            $0.kind != .providerDisconnected
                && !($0.kind == .stopped && $0.itemIdentifier == nil && $0.title == nil)
        }
    }
}

extension MobileMusicRideEventDto {
    public var timelineID: String {
        [
            String(sequence),
            provider.timelineIDComponent,
            String(monotonicAtMs),
            String(wallClockAtMs),
            kind.timelineIDComponent,
            itemIdentifier ?? "",
        ].joined(separator: "-")
    }

    public var timelineItemTitle: String {
        if kind == .providerDisconnected {
            return kind.timelineTitle
        }
        return title ?? itemIdentifier ?? pevLocalizedText("music.timeline.unknown_item")
    }

    public var timelineSubtitle: String {
        if kind == .providerDisconnected {
            return provider.title
        }
        return "\(provider.title) · \(kind.timelineTitle)"
    }
}

/// One provider observation entering the shared music pipeline.
///
/// Providers may attach bounded presentation artwork, but never an audio buffer.
public struct MusicProviderObservation: Equatable, Sendable {
    public let snapshot: MobileMusicSnapshotDto
    public let artwork: MusicArtwork?

    public init(snapshot: MobileMusicSnapshotDto, artwork: MusicArtwork? = nil) {
        self.snapshot = snapshot
        self.artwork = artwork
    }

    var staleProjection: Self {
        staleProjection(observedAtMs: snapshot.observedAtMs)
    }

    func staleProjection(observedAtMs: UInt64) -> Self {
        let nextObservedAtMs = max(
            observedAtMs,
            snapshot.observedAtMs == .max ? .max : snapshot.observedAtMs + 1
        )
        return Self(
            snapshot: MobileMusicSnapshotDto(
                provider: snapshot.provider,
                sessionId: snapshot.sessionId,
                state: .stale,
                item: snapshot.item,
                positionMilliseconds: snapshot.positionMilliseconds,
                durationMilliseconds: snapshot.durationMilliseconds,
                observedAtMs: nextObservedAtMs,
                capabilities: .init(
                    previous: false,
                    play: snapshot.provider == .spotify && snapshot.capabilities.openProvider,
                    pause: false,
                    next: false,
                    openProvider: snapshot.capabilities.openProvider
                )
            ),
            artwork: artwork
        )
    }

    func observedAt(_ observedAtMs: UInt64) -> Self {
        Self(
            snapshot: MobileMusicSnapshotDto(
                provider: snapshot.provider,
                sessionId: snapshot.sessionId,
                state: snapshot.state,
                item: snapshot.item,
                positionMilliseconds: snapshot.positionMilliseconds,
                durationMilliseconds: snapshot.durationMilliseconds,
                observedAtMs: max(observedAtMs, snapshot.observedAtMs),
                capabilities: snapshot.capabilities
            ),
            artwork: artwork
        )
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
public enum MusicIntegrationIngestError: Error {
    case observation(Error)
    case history(Error)
}

public struct MusicObservationCompletion {
    public let settledCaptureTarget: MobileMusicCaptureTarget
    public let effectivePolicy: MobileMusicHistoryPolicyDto?
    public let previousRideHistoryFailure: MobileMusicHistoryTerminalFailure?
    public let outcome: MobileMusicTimelineOutcomeDto?
    public let settledTransition: MobileMusicHistoryTransition?
    public let recordedSequence: UInt64?
    public let nowPlaying: MusicNowPlaying?
}

@MainActor
public final class MusicIntegrationCoordinator {
    public private(set) var nowPlaying: MusicNowPlaying?
    private let rideMapState: MobileRideMapState?
    private let lifecycle: MobileMusicProviderLifecycle
    private let observationBindingError: Error?

    private let correlationRideIDReader: (@MainActor () async -> String?)?
    private var lastCorrelationRideID: String?
    private var historyPolicy = MobileMusicHistoryPolicyDto.disabled
    public private(set) var lastRecordedSequence: UInt64?
    public private(set) var previousRideHistoryFailure: MobileMusicHistoryTerminalFailure?
    public init(
        rideMapState: MobileRideMapState?,
        lifecycle: MobileMusicProviderLifecycle,
        correlationRideIDReader: (@MainActor () async -> String?)? = nil
    ) {
        self.rideMapState = rideMapState
        self.lifecycle = lifecycle
        self.correlationRideIDReader = correlationRideIDReader
        do {
            try rideMapState?.bindMusicObservationLifecycle(lifecycle: lifecycle)
            observationBindingError = nil
        } catch {
            observationBindingError = error
        }
    }

    /// Removes retained diagnostic metadata after explicit successful history deletion.
    public func clearPresentationHistoryFailures() {
        previousRideHistoryFailure = nil
    }

    public func update(snapshot: MobileMusicSnapshotDto, artwork: MusicArtwork? = nil) {
        nowPlaying = MusicNowPlaying(snapshot: snapshot, artwork: artwork)
    }

    /// Applies one provider observation and records only a meaningful transition.
    /// A disabled history policy still updates the compact player but never writes
    /// to the ride database.
    @discardableResult
    public func ingestAsync(
        snapshot: MobileMusicSnapshotDto,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) async throws -> MobileMusicTimelineOutcomeDto? {
        try await ingestAsync(
            snapshot: snapshot,
            artwork: nil,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs
        )
    }

    /// Admits callback ownership before any native asynchronous identity lookup.
    public func beginObservation() -> MobileMusicObservationRequest {
        lifecycle.beginMusicObservation()
    }

    /// The caller retains this request through capture publication and history readback.
    public func completeObservation(
        submission: MobileMusicObservationRequest,
        observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64,
        captureTarget: MobileMusicCaptureTarget = .unavailable
    ) async throws -> MusicObservationCompletion {
        if let observationBindingError {
            throw MusicIntegrationIngestError.history(observationBindingError)
        }
        func completion(
            _ outcome: MobileMusicTimelineOutcomeDto? = nil,
            _ transition: MobileMusicHistoryTransition? = nil, _ sequence: UInt64? = nil,
            _ effectivePolicy: MobileMusicHistoryPolicyDto? = nil
        )
            -> MusicObservationCompletion
        {
            MusicObservationCompletion(
                settledCaptureTarget: transition?.captureTarget ?? .unavailable,
                effectivePolicy: effectivePolicy,
                previousRideHistoryFailure: previousRideHistoryFailure,
                outcome: outcome, settledTransition: transition,
                recordedSequence: sequence, nowPlaying: nowPlaying)
        }
        let lease: MobileMusicObservationLease
        switch submission.admission() {
        case let .admitted(accepted): lease = accepted
        case .full, .exhausted: return completion(.full)
        }
        guard try await lease.wait() == .ready, !Task.isCancelled, submission.isCurrent() else {
            return completion()
        }
        do {
            if let canonical = try lifecycle.observeAdmittedPlayer(lease: lease, snapshot: observation.snapshot) {
                update(snapshot: canonical, artwork: observation.artwork)
            }
        } catch {
            throw MusicIntegrationIngestError.observation(error)
        }
        if let rideMapState {
            let failure = try await rideMapState.retireClosedMusicHistoryAsync(lifecycle: lifecycle, lease: lease)
            guard !Task.isCancelled, submission.isCurrent() else { return completion() }
            if let failure { previousRideHistoryFailure = failure }
        }
        let rideID: String?
        if let correlationRideIDReader {
            rideID = await correlationRideIDReader()
        } else {
            rideID = await rideMapState?.currentSnapshotAsync()?.rideID
        }
        guard !Task.isCancelled, submission.isCurrent() else { return completion() }
        var recording: MobileRideMapRecordingTokenDto?
        if let rideID, let rideMapState {
            switch try await rideMapState.musicObservationContextAsync(
                expectedRideID: rideID, observedAtMs: observation.snapshot.observedAtMs, lease: lease)
            {
            case let .ready(token): recording = token
            case .rideNotOpen: break
            case .outOfOrder: return completion(.outOfOrder)
            }
        }
        guard !Task.isCancelled, submission.isCurrent() else { return completion() }
        let decision: MobileMusicObservationDecision
        do {
            guard
                let accepted = try lifecycle.observeAdmittedMusic(
                    lease: lease, recording: recording, captureTarget: captureTarget,
                    snapshot: observation.snapshot, wallClockAtMs: wallClockAtMs,
                    clockUncertaintyMs: clockUncertaintyMs)
            else { return completion() }
            decision = accepted
        } catch MobileRideMapCoreErrorDto.MusicHistoryFull {
            return completion(.full)
        } catch {
            throw MusicIntegrationIngestError.observation(error)
        }
        guard let transition = decision.historyTransition else { return completion() }
        guard let rideMapState, recording != nil else {
            _ = lifecycle.acknowledgeHistoryTransition(id: transition.id)
            return completion(.disabled, transition)
        }
        do {
            let result = try await rideMapState.recordMusicObservationAsync(lease: lease)
            guard !Task.isCancelled, submission.isCurrent() else { return completion() }
            switch result.outcome {
            case .recorded, .duplicate, .disabled:
                _ = lifecycle.acknowledgeHistoryTransition(id: transition.id)
            case .full, .outOfOrder, .rideNotOpen:
                break
            }
            if let sequence = result.sequence { lastRecordedSequence = sequence }
            return completion(result.outcome, transition, result.sequence, result.effectivePolicy)
        } catch {
            throw MusicIntegrationIngestError.history(error)
        }
    }

    private func ingestAsync(
        snapshot: MobileMusicSnapshotDto,
        artwork: MusicArtwork?,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) async throws -> MobileMusicTimelineOutcomeDto? {
        let submission = beginObservation()
        defer { submission.release() }
        let completion = try await completeObservation(
            submission: submission,
            observation: MusicProviderObservation(snapshot: snapshot, artwork: artwork),
            wallClockAtMs: wallClockAtMs, clockUncertaintyMs: clockUncertaintyMs
        )
        if completion.outcome != .full {
            try submission.settle(capture: nil)
        }
        return completion.outcome
    }

    /// Applies one provider observation through the same path used by ride
    /// recording and compact-player state.
    @discardableResult
    public func ingestAsync(
        observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) async throws -> MobileMusicTimelineOutcomeDto? {
        try await ingestAsync(
            snapshot: observation.snapshot,
            artwork: observation.artwork,
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs
        )
    }

    /// Synchronous compatibility entry point for non-production tests and previews.
    @discardableResult
    public func ingest(
        snapshot: MobileMusicSnapshotDto,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) throws -> MobileMusicTimelineOutcomeDto? {
        try ingest(
            observation: MusicProviderObservation(snapshot: snapshot),
            wallClockAtMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs
        )
    }

    /// Synchronous compatibility entry point for non-production tests and previews.
    @discardableResult
    public func ingest(
        observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) throws -> MobileMusicTimelineOutcomeDto? {
        resetCorrelationIfRideChanged()
        let decision: MobileMusicObservationDecision
        do {
            guard
                let accepted = try lifecycle.observeMusic(
                    snapshot: observation.snapshot,
                    wallClockAtMs: wallClockAtMs,
                    clockUncertaintyMs: clockUncertaintyMs
                )
            else { return nil }
            decision = accepted
        } catch {
            if let nowPlaying { self.nowPlaying = nowPlaying.staleProjection }
            throw MusicIntegrationIngestError.observation(error)
        }
        update(snapshot: decision.snapshot, artwork: observation.artwork)
        guard let transition = decision.historyTransition else { return nil }
        do {
            guard let rideMapState else {
                _ = lifecycle.acknowledgeHistoryTransition(id: transition.id)
                return .disabled
            }
            let result = try rideMapState.recordMusicEventWithSequence(
                snapshot: transition.snapshot,
                kind: transition.kind,
                monotonicAtMs: transition.snapshot.observedAtMs,
                wallClockAtMs: transition.wallClockAtMs,
                clockUncertaintyMs: transition.clockUncertaintyMs
            )
            if let sequence = result.sequence { lastRecordedSequence = sequence }
            _ = lifecycle.acknowledgeHistoryTransition(id: transition.id)
            return result.outcome
        } catch MobileRideMapError.noActiveRide {
            if historyPolicy == .disabled {
                _ = lifecycle.acknowledgeHistoryTransition(id: transition.id)
                return .disabled
            }
            throw MusicIntegrationIngestError.history(MobileRideMapError.noActiveRide)
        } catch {
            throw MusicIntegrationIngestError.history(error)
        }
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

    public func setHistoryPolicyAsync(_ policy: MobileMusicHistoryPolicyDto) async throws {
        guard let rideMapState else {
            // A future-ride preference is still valid without a current ride store.
            adoptHistoryPolicy(policy)
            return
        }
        try await rideMapState.setMusicHistoryPolicyAsync(policy)
        adoptHistoryPolicy(policy)
    }

    /// Drops provider-local observations before a deliberate provider switch.
    /// Selection itself must not synthesize a stop, disconnect, or item change.
    public func resetProviderCorrelation() {
        lifecycle.resetObservationCorrelation()
        lastCorrelationRideID = nil
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
        guard
            let decision = try lifecycle.observeMusic(
                snapshot: snapshot,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
        else {
            return .outOfOrder
        }
        let snapshot = decision.snapshot
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
        if let transition = decision.historyTransition {
            _ = lifecycle.acknowledgeHistoryTransition(id: transition.id)
        }
        return outcome
    }

    public var recordedEvents: [MobileMusicRideEventDto] {
        rideMapState?.currentMusicEvents() ?? []
    }

    public func recordedEventsAsync() async throws -> [MobileMusicRideEventDto] {
        try await rideMapState?.currentMusicHistoryAsync()?.events ?? []
    }

    private func resetCorrelationIfRideChanged() {
        let rideID = rideMapState?.currentSnapshot()?.rideID
        guard rideID != lastCorrelationRideID else { return }
        lastCorrelationRideID = rideID
        lifecycle.resetObservationCorrelation()
        lastRecordedSequence = nil
    }

    private func rebasePersistedState(
        from previousPolicy: MobileMusicHistoryPolicyDto,
        to policy: MobileMusicHistoryPolicyDto
    ) {
        switch (previousPolicy, policy) {
        case (.disabled, .opaqueItem), (.disabled, .humanReadable):
            // Enabling history should capture the current item on the next
            // accepted observation, even if it was already playing.
            lifecycle.resetObservationBaselines()
        case (_, .disabled):
            // Keep the current player as the baseline while history is off so
            // a later re-enable can deliberately start a new association.
            lifecycle.resetObservationBaselines()
        default:
            // Redaction and display-policy changes are not music transitions.
            // Preserve the baseline so the next poll cannot duplicate one.
            break
        }
    }

}
