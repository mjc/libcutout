import Foundation

protocol CutoutSessionDisplayPublishing: AnyObject {
    func submit(_ state: RideDisplayState, queuedAt: MonotonicMilliseconds)
    func cancel()
}

/// Publishes display snapshots on the main queue while keeping UI update pressure bounded.
///
/// Rust remains the source of truth for the snapshot. This collaborator owns only the
/// presentation-side coalescing, throttling, and publication diagnostics.
final class CutoutSessionDisplayPublisher: CutoutSessionDisplayPublishing {
    private let clock: MonotonicClock
    private let intervalMilliseconds: UInt64
    private let onDisplayStateChange: (RideDisplayState) -> Void
    private let onRecord: (String) -> Void
    private var pendingState: RideDisplayState?
    private var pendingStateQueuedAt: MonotonicMilliseconds?
    private var workItem: DispatchWorkItem?
    private var lastPublication: MonotonicMilliseconds?
    private var lastWarningSeverity: EucRideWarningSeverity?

    init(
        clock: MonotonicClock,
        intervalMilliseconds: UInt64 = 333,
        onDisplayStateChange: @escaping (RideDisplayState) -> Void,
        onRecord: @escaping (String) -> Void
    ) {
        self.clock = clock
        self.intervalMilliseconds = intervalMilliseconds
        self.onDisplayStateChange = onDisplayStateChange
        self.onRecord = onRecord
    }

    func submit(_ state: RideDisplayState, queuedAt: MonotonicMilliseconds) {
        pendingState = state
        pendingStateQueuedAt = queuedAt

        let now = clock.now()
        let elapsed = lastPublication.map {
            now.elapsed(since: $0).rawValue
        } ?? intervalMilliseconds
        let warningSeverity = EucRideScreenState(
            phase: .live,
            displayState: state
        ).warningState.severity
        let warningChanged = lastWarningSeverity.map { $0 != warningSeverity } ?? false

        guard elapsed >= intervalMilliseconds || warningChanged else {
            guard workItem == nil else { return }
            let work = DispatchWorkItem { [weak self] in
                guard let self else { return }
                self.workItem = nil
                if let pendingState = self.pendingState {
                    self.submitOnMain(pendingState)
                }
            }
            workItem = work
            DispatchQueue.main.asyncAfter(
                deadline: .now() + .milliseconds(Int(intervalMilliseconds - elapsed)),
                execute: work
            )
            return
        }

        submitOnMain(state)
    }

    func cancel() {
        workItem?.cancel()
        workItem = nil
        pendingState = nil
        pendingStateQueuedAt = nil
    }

    private func submitOnMain(_ state: RideDisplayState) {
        workItem?.cancel()
        workItem = nil
        pendingState = nil
        let publicationDelayMilliseconds = pendingStateQueuedAt.map {
            clock.now().elapsed(since: $0).rawValue
        } ?? 0
        pendingStateQueuedAt = nil
        let now = clock.now()
        lastPublication = now
        let warningSeverity = EucRideScreenState(
            phase: .live,
            displayState: state
        ).warningState.severity
        lastWarningSeverity = warningSeverity
        onDisplayStateChange(state)
        onRecord("snapshot_publication_ms=\(publicationDelayMilliseconds)")
    }
}
