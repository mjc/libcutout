import CutoutMobileFFI
import Foundation

public enum LiveActivityRideLifecycleEndReason: String, Codable, Equatable, Hashable, Sendable {
    case disconnected
    case sessionEnded
    case unavailable
    case permissionDenied
    case platformUnsupported
}

public enum LiveActivityRideLifecycleError: Error, Equatable, Sendable {
    case authorizationDenied
    case requestFailed
    case activityUnavailable
}

public enum LiveActivityRideRecoveryResult: Equatable, Sendable {
    case noPersistedRide
    case adopted
    case reconnecting
    case ended(requiresUserAction: Bool)
}

public enum LiveActivityRideStartOutcome: Equatable, Sendable {
    case started(activityID: String)
    case adopted(activityID: String)

    var activityID: String {
        switch self {
        case .started(let activityID), .adopted(let activityID):
            activityID
        }
    }
}

public struct LiveActivityRideUpdateOutcome: Equatable, Sendable {
    public let activityID: String

    public init(activityID: String) {
        self.activityID = activityID
    }
}

public struct LiveActivityRideEndOutcome: Equatable, Sendable {
    public let activityIDs: [String]

    public init(activityIDs: [String]) {
        self.activityIDs = activityIDs
    }
}

public struct LiveActivityRideSessionIdentity: Codable, Equatable, Hashable, Sendable {
    public let platformIdentifier: String
    public let sessionID: String

    public init(platformIdentifier: String, sessionID: String) {
        self.platformIdentifier = platformIdentifier
        self.sessionID = sessionID
    }

    init(_ identity: MobileRideSessionIdentityDto) {
        self.init(
            platformIdentifier: identity.platformIdentifier,
            sessionID: identity.sessionId
        )
    }
}

public protocol LiveActivityRideLifecycleManaging: Sendable {
    func start(
        snapshot: LiveActivityRideSnapshot,
        rideSessionIdentity: LiveActivityRideSessionIdentity,
        staleAfterMilliseconds: UInt64,
        freshnessExpiresAt: Date
    ) async throws -> LiveActivityRideStartOutcome
    func update(
        snapshot: LiveActivityRideSnapshot,
        staleAfterMilliseconds: UInt64,
        freshnessExpiresAt: Date
    ) async throws -> LiveActivityRideUpdateOutcome
    func end(reason: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome
}

struct LiveActivityRideReconciliation: Equatable, Sendable {
    let adoptedIndex: Int?
    let staleIndices: [Int]
}

func liveActivityRideReconciliation<Identity: Equatable & Sendable>(
    existingIdentities: [Identity],
    desiredIdentity: Identity
) -> LiveActivityRideReconciliation {
    let adoptedIndex = existingIdentities.firstIndex(of: desiredIdentity)
    return LiveActivityRideReconciliation(
        adoptedIndex: adoptedIndex,
        staleIndices: existingIdentities.indices.filter { $0 != adoptedIndex }
    )
}

public actor LiveActivityRideLifecycleCoordinator {
    private let manager: any LiveActivityRideLifecycleManaging
    private let sessionState: CutoutSessionStateHandle
    private let markerStore: RideSessionMarkerStore
    private let loadMarker: @Sendable () async throws -> Data?
    private let workQueue = MobileRideActivityWorkQueue()
    private let presentationClock: MonotonicClock
    private let wallClock: @Sendable () -> Date
    /// Native clock calibration and immutable platform expiry; Rust classifies telemetry age.
    private struct PresentationTiming: Sendable {
        let telemetryAtMs: UInt64
        let presentationAtMs: UInt64
        let admittedAt: MonotonicMilliseconds
        let expiresAt: Date
    }
    private var latestPresentationTiming: PresentationTiming?
    private var hasReconciledInactiveState = false
    private var lastSnapshot: LiveActivityRideSnapshot?
    public private(set) var lastError: LiveActivityRideLifecycleError?
    private var pendingPresentation: (requestID: UInt64, operation: @Sendable () async -> Void)?
    private var terminalEndReason: (requestID: UInt64, reason: LiveActivityRideLifecycleEndReason)?
    private var operationQueueObservers: [(depth: Int, continuation: CheckedContinuation<Void, Never>)] = []

    public init(
        manager: some LiveActivityRideLifecycleManaging,
        sessionState: CutoutSessionStateHandle = CutoutSessionStateHandle(),
        markerStore: RideSessionMarkerStore = RideSessionMarkerStore(),
        presentationNow: @escaping @Sendable () -> UInt64 = {
            UInt64(ProcessInfo.processInfo.systemUptime * 1_000)
        },
        wallClock: @escaping @Sendable () -> Date = { Date() },
        loadMarker: (@Sendable () async throws -> Data?)? = nil
    ) {
        self.manager = manager
        self.sessionState = sessionState
        self.markerStore = markerStore
        self.loadMarker = loadMarker ?? { try await markerStore.load() }
        self.presentationClock = MonotonicClock(now: { MonotonicMilliseconds(presentationNow()) })
        self.wallClock = wallClock
    }

    /// Restores Rust lifecycle immediately; platform adoption uses bounded presentation work.
    public func recoverPersistedRide(
        requestID: UInt64,
        restoredPlatformIdentifier: String?,
        snapshot: LiveActivityRideSnapshot?,
        monotonicTimeMs: UInt64 = 0,
        presentationAtMs: UInt64? = nil,
        nativeEnqueuedAtMs: UInt64? = nil,
        persistedMarker: Data? = nil
    ) async -> LiveActivityRideRecoveryResult {
        let loadedMarker: Data?
        do {
            if let persistedMarker {
                loadedMarker = persistedMarker
            } else {
                loadedMarker = try await loadMarker()
            }
        } catch {
            guard workQueue.acceptRequest(requestId: requestID) else { return .noPersistedRide }
            lastError = Self.lifecycleError(from: error)
            return .ended(requiresUserAction: false)
        }
        // Loading may suspend behind storage. A newer terminal or ride intent still wins.
        guard workQueue.acceptRequest(requestId: requestID) else { return .noPersistedRide }
        guard let marker = loadedMarker else { return .noPersistedRide }
        let timing = makePresentationTiming(
            telemetryAtMs: monotonicTimeMs, presentationAtMs: presentationAtMs,
            nativeEnqueuedAtMs: nativeEnqueuedAtMs)
        do {
            let decision = try sessionState.recoverRideSessionMarker(
                marker: marker,
                restoredPlatformIdentifier: restoredPlatformIdentifier
            )
            let result: LiveActivityRideRecoveryResult =
                switch decision.effect {
                case .startActivity: .adopted
                case .endActivity: .ended(requiresUserAction: restoredPlatformIdentifier != nil)
                case .none: .reconnecting
                default: .ended(requiresUserAction: false)
                }
            persistSessionMarker()
            switch decision.effect {
            case .endActivity:
                await enqueueTerminal(requestID: requestID, decision: decision, endReason: .sessionEnded)
            case .none:
                break
            default:
                latestPresentationTiming = timing
                await enqueuePresentation(requestID: requestID) { [weak self] in
                    await self?.execute(
                        effect: decision.effect, snapshot: snapshot, endReason: .sessionEnded,
                        staleAfterMilliseconds: decision.snapshot.staleAfterMs,
                        telemetryAtMs: monotonicTimeMs, timing: timing, startRequestID: requestID
                    )
                }
            }
            return result
        } catch {
            try? markerStore.clear()
            lastError = Self.lifecycleError(from: error)
            return .ended(requiresUserAction: false)
        }
    }

    public func reconcile(
        requestID: UInt64,
        platformIdentifier: String? = nil,
        monotonicTimeMs: UInt64 = 0,
        presentationAtMs: UInt64? = nil,
        nativeEnqueuedAtMs: UInt64? = nil,
        snapshot: LiveActivityRideSnapshot?,
        shouldBeActive: Bool,
        endReason: LiveActivityRideLifecycleEndReason = .sessionEnded
    ) async {
        if snapshot == nil, sessionState.rideSessionSnapshot().phase == .reconnecting { return }
        guard shouldBeActive, let snapshot else {
            await end(requestID: requestID, reason: endReason)
            return
        }
        guard
            workQueue.acceptPresentationRequest(
                requestId: requestID, platformIdentifier: platformIdentifier ?? snapshot.identity.label,
                snapshot: sessionState.rideSessionSnapshot(), telemetryAtMs: monotonicTimeMs
            )
        else { return }
        let timing = makePresentationTiming(
            telemetryAtMs: monotonicTimeMs, presentationAtMs: presentationAtMs,
            nativeEnqueuedAtMs: nativeEnqueuedAtMs)
        latestPresentationTiming = timing
        await enqueuePresentation(requestID: requestID) { [weak self] in
            await self?.performReconciliation(
                requestID: requestID, platformIdentifier: platformIdentifier, monotonicTimeMs: monotonicTimeMs,
                snapshot: snapshot, endReason: endReason, timing: timing
            )
        }
    }

    private func performReconciliation(
        requestID: UInt64,
        platformIdentifier: String?,
        monotonicTimeMs: UInt64,
        snapshot: LiveActivityRideSnapshot,
        endReason: LiveActivityRideLifecycleEndReason,
        timing: PresentationTiming
    ) async {
        // Rust's terminal barrier also covers the hop into this not-yet-dispatched payload.
        guard workQueue.snapshot().pendingTerminalRequestId == nil else { return }
        let platformIdentifier = platformIdentifier ?? snapshot.identity.label
        let rustSnapshot = sessionState.rideSessionSnapshot()
        if rustSnapshot.identity?.platformIdentifier == platformIdentifier {
            let pendingStart = rideActivityPendingStartEffect(snapshot: rustSnapshot)
            if pendingStart != .none {
                await execute(
                    effect: pendingStart, snapshot: snapshot, endReason: endReason,
                    staleAfterMilliseconds: rustSnapshot.staleAfterMs, telemetryAtMs: monotonicTimeMs,
                    timing: timing, startRequestID: requestID
                )
                return
            }
        }
        if lastSnapshot == nil || rustSnapshot.identity?.platformIdentifier != platformIdentifier {
            await apply(
                input: .start(platformIdentifier: platformIdentifier), snapshot: snapshot,
                endReason: endReason,
                telemetryAtMs: monotonicTimeMs, timing: timing, startRequestID: requestID)
            return
        }
        if rustSnapshot.phase == .reconnecting {
            await apply(input: .bluetoothConnected, snapshot: snapshot, endReason: endReason)
        }
        do {
            let observed = try reduce(.telemetryObserved(atMs: monotonicTimeMs))
            let checked = try reduce(.freshnessChecked(nowMs: dispatchTime(for: timing)))
            let effect = checked.effect == .none ? observed.effect : checked.effect
            let value = snapshot.presented(isStale: checked.snapshot.phase == .stale)
            guard
                lastSnapshot != value
                    || workQueue.presentationRequiresRenewal(
                        telemetryAtMs: monotonicTimeMs, staleAfterMs: rustSnapshot.staleAfterMs
                    )
            else { return }
            await execute(
                effect: effect, snapshot: value, endReason: endReason,
                staleAfterMilliseconds: checked.snapshot.staleAfterMs,
                telemetryAtMs: monotonicTimeMs, timing: timing)
        } catch {
            lastError = Self.lifecycleError(from: error)
        }
    }

    private func makePresentationTiming(
        telemetryAtMs: UInt64, presentationAtMs: UInt64?, nativeEnqueuedAtMs: UInt64?
    )
        -> PresentationTiming
    {
        let presentedAt = presentationAtMs ?? telemetryAtMs
        let receiptAgeMs = presentedAt >= telemetryAtMs ? presentedAt - telemetryAtMs : 0
        let staleAfterMs = sessionState.rideSessionSnapshot().staleAfterMs
        let enqueuedAt = nativeEnqueuedAtMs.map(MonotonicMilliseconds.init) ?? presentationClock.now()
        let actorAdmissionDelayMs = presentationClock.now().elapsed(since: enqueuedAt).rawValue
        let receiptWallTime = wallClock().addingTimeInterval(
            -(TimeInterval(receiptAgeMs) + TimeInterval(actorAdmissionDelayMs)) / 1_000)
        return PresentationTiming(
            telemetryAtMs: telemetryAtMs, presentationAtMs: presentedAt,
            admittedAt: enqueuedAt,
            expiresAt: receiptWallTime.addingTimeInterval(TimeInterval(staleAfterMs) / 1_000))
    }

    private func dispatchTime(for timing: PresentationTiming) -> UInt64 {
        let elapsed = presentationClock.now().elapsed(since: timing.admittedAt).rawValue
        let sum = timing.presentationAtMs.addingReportingOverflow(elapsed)
        return sum.overflow ? UInt64.max : sum.partialValue
    }

    public func transportDisconnected(
        requestID: UInt64, atMs: UInt64, snapshot: LiveActivityRideSnapshot
    ) async {
        guard
            workQueue.acceptLifecycleRequest(
                requestId: requestID, kind: .transportDisconnected(atMs: atMs),
                snapshot: sessionState.rideSessionSnapshot()
            ) == .reduce
        else { return }
        do {
            _ = try reduce(.bluetoothDisconnected(atMs: atMs))
            await enqueuePresentation(requestID: requestID, lifecycleProjection: true) { [weak self] in
                await self?.projectCurrentLifecycle(snapshot: snapshot)
            }
        } catch {
            lastError = Self.lifecycleError(from: error)
        }
    }

    public func reconnectExhausted(requestID: UInt64, snapshot: LiveActivityRideSnapshot) async {
        await terminate(requestID: requestID, input: .reconnectExhausted)
    }

    public func unrecoverableSessionFailure(requestID: UInt64, snapshot: LiveActivityRideSnapshot)
        async
    {
        await terminate(requestID: requestID, input: .unrecoverableSessionFailure)
    }

    private func terminate(requestID: UInt64, input: MobileRideSessionInputDto) async {
        guard admitTerminal(requestID: requestID) else { return }
        do {
            let decision = try reduce(input)
            if case .endActivity = decision.effect {
                await enqueueTerminal(requestID: requestID, decision: decision, endReason: .unavailable)
            }
        } catch {
            lastError = Self.lifecycleError(from: error)
        }
    }

    public func appDidEnterBackground(
        requestID: UInt64,
        atMs: UInt64,
        snapshot: LiveActivityRideSnapshot,
        captureFlush: @escaping @Sendable () async -> Bool
    ) async {
        switch workQueue.acceptLifecycleRequest(
            requestId: requestID, kind: .background, snapshot: sessionState.rideSessionSnapshot()
        ) {
        case .rejected:
            return
        case .flushOnly:
            _ = await captureFlush()
            return
        case .reduceAndFlush:
            await apply(input: .appBackgrounded, snapshot: snapshot, endReason: .sessionEnded)
            _ = await captureFlush()
            await enqueueLifecycleProjectionIfNeeded(requestID: requestID, snapshot: snapshot)
            return
        case .reduce:
            break
        }
        // Required recording effects run before any optional Apple presentation await.
        await apply(
            input: .appBackgrounded, snapshot: snapshot, endReason: .sessionEnded,
            captureFlush: captureFlush)
        await enqueueLifecycleProjectionIfNeeded(requestID: requestID, snapshot: snapshot)
    }

    public func appDidBecomeActive(requestID: UInt64, snapshot: LiveActivityRideSnapshot) async {
        guard
            workQueue.acceptLifecycleRequest(
                requestId: requestID, kind: .foreground, snapshot: sessionState.rideSessionSnapshot()
            ) == .reduce
        else { return }
        await apply(input: .appForegrounded, snapshot: snapshot, endReason: .sessionEnded)
        await enqueueLifecycleProjectionIfNeeded(requestID: requestID, snapshot: snapshot)
    }

    private func enqueueLifecycleProjectionIfNeeded(
        requestID: UInt64, snapshot: LiveActivityRideSnapshot
    ) async {
        let effect = rideActivityBackgroundProjectionEffect(
            snapshot: sessionState.rideSessionSnapshot())
        switch effect {
        case .none:
            return
        case .markActivityStale:
            // A newer scene observation must preserve pending disconnect presentation.
            break
        default:
            guard lastSnapshot != snapshot else { return }
        }
        await enqueuePresentation(requestID: requestID, lifecycleProjection: true) { [weak self] in
            await self?.projectCurrentLifecycle(snapshot: snapshot)
        }
    }

    private func projectCurrentLifecycle(snapshot: LiveActivityRideSnapshot) async {
        if let timing = latestPresentationTiming {
            if let input = rideActivityAdmittedTelemetryInput(
                snapshot: sessionState.rideSessionSnapshot(), telemetryAtMs: timing.telemetryAtMs
            ) {
                _ = try? reduce(input)
            }
            _ = try? reduce(.freshnessChecked(nowMs: dispatchTime(for: timing)))
        }
        let rustSnapshot = sessionState.rideSessionSnapshot()
        await execute(
            effect: rideActivityBackgroundProjectionEffect(snapshot: rustSnapshot),
            snapshot: snapshot.presented(isStale: rustSnapshot.phase == .stale),
            endReason: .sessionEnded, staleAfterMilliseconds: rustSnapshot.staleAfterMs,
            timing: latestPresentationTiming
        )
    }

    public func end(requestID: UInt64, reason: LiveActivityRideLifecycleEndReason) async {
        guard admitTerminal(requestID: requestID) else { return }
        await requestEnd(requestID: requestID, reason: reason)
    }

    private func admitTerminal(requestID: UInt64) -> Bool {
        workQueue.acceptLifecycleRequest(
            requestId: requestID, kind: .terminal, snapshot: sessionState.rideSessionSnapshot()
        ) == .reduce
    }

    private func requestEnd(requestID: UInt64, reason: LiveActivityRideLifecycleEndReason) async {
        do {
            let input: MobileRideSessionInputDto =
                reason == .disconnected ? .userDisconnected : .userStopped
            let decision = try reduce(input)
            if case .endActivity = decision.effect {
                await enqueueTerminal(requestID: requestID, decision: decision, endReason: reason)
            } else if hasReconciledInactiveState == false {
                await enqueuePresentation(requestID: requestID) { [weak self] in
                    _ = await self?.endIfNeeded(reason: reason)
                }
            }
        } catch {
            lastError = Self.lifecycleError(from: error)
        }
    }

    private func enqueuePresentation(
        requestID: UInt64, lifecycleProjection: Bool = false,
        operation: @escaping @Sendable () async -> Void
    ) async {
        let admission =
            lifecycleProjection
            ? workQueue.enqueueLifecycleProjection(requestId: requestID)
            : workQueue.enqueuePresentation(requestId: requestID)
        switch admission {
        case .run(let work):
            await drainPlatformWork(work, initialOperation: operation)
        case .queued:
            pendingPresentation = (requestID: requestID, operation: operation)
            resumeReadyOperationQueueObservers()
        case .rejected:
            return
        }
    }

    private func enqueueTerminal(
        requestID: UInt64, decision: MobileRideSessionDecisionDto,
        endReason: LiveActivityRideLifecycleEndReason
    ) async {
        let admission = workQueue.enqueueTerminal(
            requestId: requestID, effect: decision.effect, staleAfterMs: decision.snapshot.staleAfterMs
        )
        switch admission {
        case .run(let work):
            terminalEndReason = (requestID: requestID, reason: endReason)
            await drainPlatformWork(work, initialOperation: nil)
        case .queued(let replacedRequestID):
            if pendingPresentation?.requestID == replacedRequestID { pendingPresentation = nil }
            if workQueue.snapshot().pendingTerminalRequestId == requestID {
                terminalEndReason = (requestID: requestID, reason: endReason)
            }
            resumeReadyOperationQueueObservers()
        case .rejected:
            return
        }
    }

    /// The sole worker owns one platform await; Rust transfers ownership to terminal work first.
    private func drainPlatformWork(
        _ initialWork: MobileRideActivityWorkDto, initialOperation: (@Sendable () async -> Void)?
    ) async {
        var work: MobileRideActivityWorkDto? = initialWork
        var operation = initialOperation
        while let current = work {
            let requestID: UInt64
            switch current {
            case .presentation(let id):
                requestID = id
                if operation == nil, pendingPresentation?.requestID == id {
                    operation = pendingPresentation?.operation
                    pendingPresentation = nil
                }
                await operation?()
            case .terminal(let id, let effect, let staleAfterMs):
                requestID = id
                let endReason = terminalEndReason?.requestID == id ? terminalEndReason?.reason : nil
                if terminalEndReason?.requestID == id { terminalEndReason = nil }
                await execute(
                    effect: effect, snapshot: lastSnapshot, endReason: endReason ?? .sessionEnded,
                    staleAfterMilliseconds: staleAfterMs
                )
            }
            operation = nil
            work = workQueue.finishWork(requestId: requestID)
        }
        pendingPresentation = nil
    }

    internal func waitForOperationQueueDepthForTesting(_ depth: Int) async {
        guard pendingWorkCount < depth else { return }
        await withCheckedContinuation { continuation in
            operationQueueObservers.append((depth: depth, continuation: continuation))
            resumeReadyOperationQueueObservers()
        }
    }

    private var pendingWorkCount: Int {
        let snapshot = workQueue.snapshot()
        return [snapshot.pendingPresentationRequestId, snapshot.pendingTerminalRequestId].compactMap {
            $0
        }.count
    }

    private func resumeReadyOperationQueueObservers() {
        let depth = pendingWorkCount
        let ready = operationQueueObservers.filter { depth >= $0.depth }
        operationQueueObservers.removeAll { depth >= $0.depth }
        for observer in ready { observer.continuation.resume() }
    }

    private func apply(
        input: MobileRideSessionInputDto,
        snapshot: LiveActivityRideSnapshot?,
        endReason: LiveActivityRideLifecycleEndReason,
        captureFlush: (@Sendable () async -> Bool)? = nil,
        telemetryAtMs: UInt64? = nil,
        timing: PresentationTiming? = nil,
        startRequestID: UInt64? = nil
    ) async {
        do {
            let decision = try reduce(input)
            await execute(
                effect: decision.effect,
                snapshot: snapshot,
                endReason: endReason,
                staleAfterMilliseconds: decision.snapshot.staleAfterMs,
                captureFlush: captureFlush,
                telemetryAtMs: telemetryAtMs,
                timing: timing,
                startRequestID: startRequestID
            )
        } catch {
            lastError = Self.lifecycleError(from: error)
        }
    }

    private func execute(
        effect: MobileRideSessionEffectDto,
        snapshot: LiveActivityRideSnapshot?,
        endReason: LiveActivityRideLifecycleEndReason,
        staleAfterMilliseconds: UInt64,
        captureFlush: (@Sendable () async -> Bool)? = nil,
        telemetryAtMs: UInt64? = nil,
        timing: PresentationTiming? = nil,
        startRequestID: UInt64? = nil
    ) async {
        switch effect {
        case .none:
            return
        case .startActivity(let identity):
            if let startRequestID {
                workQueue.noteSessionStart(requestId: startRequestID, identity: identity)
            }
            guard let snapshot else {
                lastError = .requestFailed
                return
            }
            do {
                if let timing {
                    _ = try reduce(.telemetryObserved(atMs: timing.telemetryAtMs))
                    _ = try reduce(.freshnessChecked(nowMs: dispatchTime(for: timing)))
                }
                let value = snapshot.presented(isStale: sessionState.rideSessionSnapshot().phase == .stale)
                let outcome = try await manager.start(
                    snapshot: value,
                    rideSessionIdentity: LiveActivityRideSessionIdentity(identity),
                    staleAfterMilliseconds: staleAfterMilliseconds,
                    freshnessExpiresAt: timing?.expiresAt
                        ?? wallClock().addingTimeInterval(TimeInterval(staleAfterMilliseconds) / 1_000)
                )
                if let telemetryAtMs { workQueue.presentationUpdated(telemetryAtMs: telemetryAtMs) }
                hasReconciledInactiveState = false
                lastSnapshot = value
                lastError = nil
                await apply(
                    input: .activityStarted(identity: identity, activityId: outcome.activityID),
                    snapshot: snapshot,
                    endReason: endReason,
                    telemetryAtMs: telemetryAtMs,
                    timing: timing,
                    startRequestID: startRequestID
                )
            } catch {
                lastSnapshot = nil
                lastError = Self.lifecycleError(from: error)
                _ = try? reduce(.activityUnavailable(identity: identity))
            }
        case .updateActivity, .markActivityStale:
            guard let snapshot else {
                lastError = .requestFailed
                return
            }
            do {
                let updateStaleAfterMilliseconds: UInt64 =
                    switch effect {
                    case .markActivityStale:
                        0
                    default:
                        staleAfterMilliseconds
                    }
                let expiry =
                    timing?.expiresAt
                    ?? wallClock().addingTimeInterval(
                        TimeInterval(updateStaleAfterMilliseconds) / 1_000)
                let freshnessExpiresAt: Date
                switch effect {
                case .markActivityStale:
                    freshnessExpiresAt = min(expiry, wallClock())
                default:
                    freshnessExpiresAt = expiry
                }
                _ = try await manager.update(
                    snapshot: snapshot,
                    staleAfterMilliseconds: updateStaleAfterMilliseconds,
                    freshnessExpiresAt: freshnessExpiresAt
                )
                if let telemetryAtMs { workQueue.presentationUpdated(telemetryAtMs: telemetryAtMs) }
                lastSnapshot = snapshot
                lastError = nil
            } catch {
                lastError = Self.lifecycleError(from: error)
            }
        case .endActivity(let identity, let reason):
            do {
                _ = try await manager.end(reason: Self.endReason(from: reason, fallback: endReason))
                lastSnapshot = nil
                lastError = nil
                hasReconciledInactiveState = true
                await apply(
                    input: .activityEnded(identity: identity),
                    snapshot: snapshot,
                    endReason: endReason,
                    telemetryAtMs: telemetryAtMs,
                    timing: timing,
                    startRequestID: startRequestID
                )
            } catch {
                lastError = Self.lifecycleError(from: error)
                _ = try? reduce(.activityUnavailable(identity: identity))
            }
        case .requestCaptureFlush:
            _ = await captureFlush?()
        }
    }

    private static func lifecycleError(from error: Error) -> LiveActivityRideLifecycleError {
        (error as? LiveActivityRideLifecycleError) ?? .requestFailed
    }

    private static func endReason(
        from reason: MobileRideSessionEndReasonDto,
        fallback: LiveActivityRideLifecycleEndReason
    ) -> LiveActivityRideLifecycleEndReason {
        switch reason {
        case .userDisconnect:
            .disconnected
        case .replacedByNewSession:
            .sessionEnded
        case .userStop, .reconnectExhausted, .appReset, .unrecoverableSessionFailure:
            fallback
        }
    }

    private func endIfNeeded(reason: LiveActivityRideLifecycleEndReason) async -> Bool {
        if lastSnapshot != nil || sessionState.rideSessionSnapshot().identity != nil {
            let input: MobileRideSessionInputDto =
                reason == .disconnected ? .userDisconnected : .userStopped
            do {
                let decision = try reduce(input)
                if decision.effect != .none {
                    await execute(
                        effect: decision.effect,
                        snapshot: lastSnapshot,
                        endReason: reason,
                        staleAfterMilliseconds: decision.snapshot.staleAfterMs
                    )
                    return lastError == nil
                }
            } catch {
                lastError = Self.lifecycleError(from: error)
                return false
            }
        }

        guard hasReconciledInactiveState == false else {
            lastSnapshot = nil
            return true
        }

        do {
            _ = try await manager.end(reason: reason)
            lastError = nil
            hasReconciledInactiveState = true
            lastSnapshot = nil
            return true
        } catch {
            lastError = Self.lifecycleError(from: error)
            return false
        }
    }

    private func reduce(_ input: MobileRideSessionInputDto) throws -> MobileRideSessionDecisionDto {
        let decision = try sessionState.reduceRideSession(input: input)
        persistSessionMarker()
        return decision
    }

    private func persistSessionMarker() {
        guard let marker = try? sessionState.exportRideSessionMarker() else {
            if rideActivityMayClearMarker(snapshot: sessionState.rideSessionSnapshot()) {
                try? markerStore.clear()
            }
            return
        }
        markerStore.save(marker)
    }
}

#if canImport(ActivityKit) && !os(macOS)
    @preconcurrency import ActivityKit

    @available(iOS 16.2, *)
    public struct LiveActivityRideAttributes: ActivityAttributes, Codable, Hashable, Sendable {
        public struct ContentState: Codable, Hashable, Sendable {
            public let snapshot: LiveActivityRideSnapshot
            public let staleAt: Date?

            public init(snapshot: LiveActivityRideSnapshot, staleAt: Date? = nil) {
                self.snapshot = snapshot
                self.staleAt = staleAt
            }

            public func presentationSnapshot(isStale: Bool, now: Date) -> LiveActivityRideSnapshot {
                snapshot.presented(isStale: isStale || staleAt.map { now >= $0 } == true)
            }
        }

        public let identity: LiveActivityRideIdentity
        public let rideSessionIdentity: LiveActivityRideSessionIdentity

        public init(
            identity: LiveActivityRideIdentity,
            rideSessionIdentity: LiveActivityRideSessionIdentity
        ) {
            self.identity = identity
            self.rideSessionIdentity = rideSessionIdentity
        }
    }

    @available(iOS 16.2, *)
    public actor LiveActivityRideActivityKitManager: LiveActivityRideLifecycleManaging {
        private let state = LiveActivityRideActivityKitState()

        public init() {}

        public func start(
            snapshot: LiveActivityRideSnapshot,
            rideSessionIdentity: LiveActivityRideSessionIdentity,
            staleAfterMilliseconds: UInt64,
            freshnessExpiresAt: Date
        ) async throws -> LiveActivityRideStartOutcome {
            try await state.start(
                snapshot: snapshot,
                rideSessionIdentity: rideSessionIdentity,
                staleAfterMilliseconds: staleAfterMilliseconds,
                freshnessExpiresAt: freshnessExpiresAt
            )
        }

        public func update(
            snapshot: LiveActivityRideSnapshot,
            staleAfterMilliseconds: UInt64,
            freshnessExpiresAt: Date
        ) async throws -> LiveActivityRideUpdateOutcome {
            try await state.update(
                snapshot: snapshot,
                staleAfterMilliseconds: staleAfterMilliseconds,
                freshnessExpiresAt: freshnessExpiresAt
            )
        }

        public func end(reason: LiveActivityRideLifecycleEndReason) async throws
            -> LiveActivityRideEndOutcome
        {
            try await state.end(reason: reason)
        }
    }

    @available(iOS 16.2, *)
    private actor LiveActivityRideActivityKitState {
        private var activity: Activity<LiveActivityRideAttributes>?
        private var lastSnapshot: LiveActivityRideSnapshot?

        func start(
            snapshot: LiveActivityRideSnapshot,
            rideSessionIdentity: LiveActivityRideSessionIdentity,
            staleAfterMilliseconds: UInt64,
            freshnessExpiresAt: Date
        ) async throws -> LiveActivityRideStartOutcome {
            guard ActivityAuthorizationInfo().areActivitiesEnabled else {
                throw LiveActivityRideLifecycleError.authorizationDenied
            }

            let existingActivities = Activity<LiveActivityRideAttributes>.activities
            let reconciliation = liveActivityRideReconciliation(
                existingIdentities: existingActivities.map(\.attributes.rideSessionIdentity),
                desiredIdentity: rideSessionIdentity
            )
            for staleIndex in reconciliation.staleIndices {
                let staleActivity = existingActivities[staleIndex]
                await staleActivity.end(staleActivity.content, dismissalPolicy: .immediate)
            }

            if let adoptedIndex = reconciliation.adoptedIndex {
                activity = existingActivities[adoptedIndex]
                _ = try await update(
                    snapshot: snapshot,
                    staleAfterMilliseconds: staleAfterMilliseconds,
                    freshnessExpiresAt: freshnessExpiresAt
                )
                return .adopted(activityID: existingActivities[adoptedIndex].id)
            }

            activity = nil
            do {
                let startedActivity = try Activity.request(
                    attributes: LiveActivityRideAttributes(
                        identity: snapshot.identity,
                        rideSessionIdentity: rideSessionIdentity
                    ),
                    content: content(
                        snapshot: snapshot,
                        staleAfterMilliseconds: staleAfterMilliseconds,
                        freshnessExpiresAt: freshnessExpiresAt
                    ),
                    pushType: nil
                )
                activity = startedActivity
                lastSnapshot = snapshot
                return .started(activityID: startedActivity.id)
            } catch {
                activity = nil
                lastSnapshot = nil
                throw LiveActivityRideLifecycleError.requestFailed
            }
        }

        func update(
            snapshot: LiveActivityRideSnapshot,
            staleAfterMilliseconds: UInt64,
            freshnessExpiresAt: Date
        ) async throws -> LiveActivityRideUpdateOutcome {
            guard let activity else {
                throw LiveActivityRideLifecycleError.activityUnavailable
            }

            await activity.update(
                content(
                    snapshot: snapshot,
                    staleAfterMilliseconds: staleAfterMilliseconds,
                    freshnessExpiresAt: freshnessExpiresAt
                )
            )
            lastSnapshot = snapshot
            return LiveActivityRideUpdateOutcome(activityID: activity.id)
        }

        func end(reason _: LiveActivityRideLifecycleEndReason) async throws
            -> LiveActivityRideEndOutcome
        {
            let currentActivityID = activity?.id
            let activities = Activity<LiveActivityRideAttributes>.activities
            for currentActivity in activities {
                let finalContent =
                    currentActivity.id == currentActivityID
                    ? lastSnapshot.map {
                        content(snapshot: $0, staleAfterMilliseconds: 0)
                    } ?? currentActivity.content
                    : currentActivity.content
                await currentActivity.end(finalContent, dismissalPolicy: .immediate)
            }

            activity = nil
            lastSnapshot = nil
            return LiveActivityRideEndOutcome(activityIDs: activities.map(\.id))
        }

        private func content(
            snapshot: LiveActivityRideSnapshot,
            staleAfterMilliseconds: UInt64,
            freshnessExpiresAt: Date? = nil
        ) -> ActivityContent<LiveActivityRideAttributes.ContentState> {
            let staleAt =
                freshnessExpiresAt
                ?? Date().addingTimeInterval(TimeInterval(staleAfterMilliseconds) / 1_000)
            return ActivityContent(
                state: LiveActivityRideAttributes.ContentState(
                    snapshot: snapshot,
                    staleAt: staleAt
                ),
                staleDate: staleAt
            )
        }
    }
#else
    public actor LiveActivityRideActivityKitManager: LiveActivityRideLifecycleManaging {
        public init() {}

        public func start(
            snapshot _: LiveActivityRideSnapshot,
            rideSessionIdentity _: LiveActivityRideSessionIdentity,
            staleAfterMilliseconds _: UInt64,
            freshnessExpiresAt _: Date
        ) async throws -> LiveActivityRideStartOutcome {
            throw LiveActivityRideLifecycleError.activityUnavailable
        }

        public func update(
            snapshot _: LiveActivityRideSnapshot,
            staleAfterMilliseconds _: UInt64,
            freshnessExpiresAt _: Date
        ) async throws -> LiveActivityRideUpdateOutcome {
            throw LiveActivityRideLifecycleError.activityUnavailable
        }

        public func end(reason _: LiveActivityRideLifecycleEndReason) async throws
            -> LiveActivityRideEndOutcome
        {
            LiveActivityRideEndOutcome(activityIDs: [])
        }
    }
#endif
