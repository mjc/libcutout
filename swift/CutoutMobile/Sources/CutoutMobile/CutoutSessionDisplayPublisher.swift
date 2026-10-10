import Foundation
import Synchronization

protocol CutoutSessionDisplayPublishing: AnyObject {
    func submit(_ state: RideDisplayState, queuedAt: MonotonicMilliseconds)
    func cancel()
    func setActive(_ active: Bool)
}

extension CutoutSessionDisplayPublishing {
    func setActive(_ active: Bool) {}
}

/// Holds one replaceable rendering payload before crossing the main queue.
/// Recording, alarms and protocol effects have independent delivery paths.
final class CutoutSessionDisplayPublisher: CutoutSessionDisplayPublishing, @unchecked Sendable {
    private struct Pending {
        let value: RideDisplayState
        let queuedAt: MonotonicMilliseconds
        let severity: EucRideWarningSeverity?
    }
    private struct State {
        var active = true
        var pending: Pending?
        var lastPublication: MonotonicMilliseconds?
        var lastSeverity: EucRideWarningSeverity?
        var due: ContinuousClock.Instant?
    }
    private let state = Mutex(State())
    private let clock: MonotonicClock
    private let intervalMilliseconds: UInt64
    private let backgroundIntervalMilliseconds: UInt64?
    private let onDisplayStateChange: (RideDisplayState) -> Void
    private let onRecord: (String) -> Void
    private let timer: DispatchSourceTimer

    init(
        clock: MonotonicClock,
        intervalMilliseconds: UInt64 = 333,
        backgroundIntervalMilliseconds: UInt64? = nil,
        onDisplayStateChange: @escaping (RideDisplayState) -> Void,
        onRecord: @escaping (String) -> Void
    ) {
        self.clock = clock
        self.intervalMilliseconds = intervalMilliseconds
        self.backgroundIntervalMilliseconds = backgroundIntervalMilliseconds
        self.onDisplayStateChange = onDisplayStateChange
        self.onRecord = onRecord
        timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .distantFuture)
        timer.setEventHandler { [weak self] in self?.deliver() }
        timer.resume()
    }

    deinit { timer.cancel() }

    func submit(_ value: RideDisplayState, queuedAt: MonotonicMilliseconds) {
        let active = state.withLock { $0.active }
        let pending = Pending(
            value: value, queuedAt: queuedAt,
            severity: active ? EucRideScreenState(phase: .live, displayState: value).warningState.severity : nil
        )
        let now = clock.now()
        let immediate = state.withLock { state in
            state.pending = pending
            return schedule(&state, now: now)
        }
        if immediate && Thread.isMainThread { deliver() }
    }

    func setActive(_ active: Bool) {
        let now = clock.now()
        let immediate = state.withLock { state in
            guard state.active != active else { return false }
            state.active = active
            state.due = nil
            // Catch up once on foreground entry. Background cadence begins at entry.
            state.lastPublication = active ? nil : now
            return schedule(&state, now: now)
        }
        if immediate && Thread.isMainThread { deliver() }
    }

    func cancel() {
        state.withLock { state in
            state.pending = nil
            state.due = nil
            timer.schedule(deadline: .distantFuture)
        }
    }

    /// Must run under the native ownership lock; no input allocates queued work.
    private func schedule(_ state: inout State, now: MonotonicMilliseconds) -> Bool {
        guard let pending = state.pending,
            let interval = state.active ? intervalMilliseconds : backgroundIntervalMilliseconds
        else {
            state.due = nil
            timer.schedule(deadline: .distantFuture)
            return false
        }
        let elapsed = state.lastPublication.map { now.elapsed(since: $0).rawValue } ?? interval
        let warningChanged = state.active && state.lastSeverity.map { $0 != pending.severity } == true
        let delay = warningChanged || elapsed >= interval ? 0 : interval - elapsed
        if let due = state.due {
            if delay == 0, due <= .now { return true }
            if delay > 0, due > .now { return false }
        }
        state.due = .now.advanced(by: .milliseconds(delay))
        timer.schedule(deadline: .now() + .milliseconds(Int(delay)), leeway: .milliseconds(1))
        return delay == 0
    }

    private func deliver() {
        precondition(Thread.isMainThread)
        let now = clock.now()
        let pending: Pending? = state.withLock { state in
            guard state.active || backgroundIntervalMilliseconds != nil else { return nil }
            guard let pending = state.pending else { return nil }
            guard let due = state.due else { return nil }
            if due > .now {
                // A timer event already queued before rescheduling cannot consume a newer payload early.
                let parts = ContinuousClock.now.duration(to: due).components
                let delay = max(0, Double(parts.seconds) + Double(parts.attoseconds) / 1e18)
                timer.schedule(deadline: .now() + delay, leeway: .milliseconds(1))
                return nil
            }
            state.pending = nil
            state.due = nil
            state.lastPublication = now
            state.lastSeverity =
                state.active
                ? pending.severity
                    ?? EucRideScreenState(phase: .live, displayState: pending.value).warningState.severity
                : nil
            timer.schedule(deadline: .distantFuture)
            return pending
        }
        guard let pending else { return }
        onDisplayStateChange(pending.value)
        onRecord("snapshot_publication_ms=\(now.elapsed(since: pending.queuedAt).rawValue)")
    }
}

/// Native ownership mailbox for replaceable presentation or acquisition notifications.
/// The payload is computed by Rust; this bridge only bounds main-queue ownership.
final class CutoutMainQueueLatest<Value: Sendable>: @unchecked Sendable {
    private struct State {
        var active = true
        var pending: Value?
        var requiredWorkCount = 0
        var lastPublication: ContinuousClock.Instant?
        var due: ContinuousClock.Instant?
    }
    private let state = Mutex(State())
    private let interval: Duration
    private let timer: DispatchSourceTimer
    private let publish: @MainActor (Value) -> Void

    init(intervalMilliseconds: Int = 0, publish: @escaping @MainActor (Value) -> Void) {
        interval = .milliseconds(intervalMilliseconds)
        self.publish = publish
        timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .distantFuture)
        timer.setEventHandler { [weak self] in self?.deliver() }
        timer.resume()
    }

    deinit { timer.cancel() }

    func submit(_ value: Value, deliverImmediately: Bool = false) {
        state.withLock { state in
            state.pending = value
            schedule(&state)
        }
        if deliverImmediately, interval == .zero, Thread.isMainThread { deliver() }
    }

    /// Keep one payload while earlier required main-queue effects own their handoffs.
    func beginRequiredWork(_ value: Value) {
        state.withLock { state in
            state.requiredWorkCount += 1
            state.pending = value
            schedule(&state)
        }
    }

    func finishRequiredWork(deliverImmediately: Bool = false) {
        state.withLock { state in
            precondition(state.requiredWorkCount > 0)
            state.requiredWorkCount -= 1
            schedule(&state)
        }
        if deliverImmediately, interval == .zero, Thread.isMainThread { deliver() }
    }

    func setActive(_ active: Bool) {
        state.withLock { state in
            guard state.active != active else { return }
            state.active = active
            state.due = nil
            if active { state.lastPublication = nil }
            schedule(&state)
        }
    }

    private func schedule(_ state: inout State) {
        guard state.active, state.requiredWorkCount == 0, state.pending != nil else {
            state.due = nil
            timer.schedule(deadline: .distantFuture)
            return
        }
        if state.due != nil { return }
        let remaining = state.lastPublication.map { max(.zero, interval - $0.duration(to: .now)) } ?? .zero
        let parts = remaining.components
        let delay = Double(parts.seconds) + Double(parts.attoseconds) / 1e18
        state.due = .now.advanced(by: remaining)
        timer.schedule(deadline: .now() + delay, leeway: .milliseconds(1))
    }

    private func deliver() {
        let value = state.withLock { state in
            guard state.active, state.requiredWorkCount == 0, let value = state.pending else { return nil as Value? }
            guard let due = state.due else { return nil }
            if due > .now {
                state.due = nil
                schedule(&state)
                return nil
            }
            state.pending = nil
            state.due = nil
            state.lastPublication = .now
            timer.schedule(deadline: .distantFuture)
            return value
        }
        if let value { MainActor.assumeIsolated { publish(value) } }
    }
}
