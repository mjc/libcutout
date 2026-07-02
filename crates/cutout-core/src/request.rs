//! Request correlation, bounded queues, and polling plans.

use crate::{
    CommandKind, Duration, MonotonicTimestamp, ParserDiagnostics, ParserError, SafetyClass,
    VescControllerId,
};

/// Transport-independent request target used for correlation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RequestTarget {
    /// Direct request to the connected controller/device.
    #[default]
    Local,

    /// Request forwarded to a VESC CAN controller id.
    VescCanController {
        /// VESC CAN controller id.
        controller_id: VescControllerId,
    },
}

/// Transport-independent key used to correlate a scheduled request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestKey {
    /// Command kind represented by this request.
    pub command: CommandKind,

    /// Transport-independent target represented by this request.
    pub target: RequestTarget,
}

impl RequestKey {
    /// Creates a request key from a command kind.
    #[must_use]
    pub const fn new(command: CommandKind) -> Self {
        Self::for_target(command, RequestTarget::Local)
    }

    /// Creates a request key from a command kind and explicit target.
    #[must_use]
    pub const fn for_target(command: CommandKind, target: RequestTarget) -> Self {
        Self { command, target }
    }
}

/// Retry, timeout, and pacing policy for one scheduled request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestPolicy {
    /// Deadline duration for one attempt.
    pub timeout: Duration,

    /// Maximum retries after the first attempt.
    pub max_retries: u8,

    /// Minimum interval between starts for the same key.
    pub min_interval: Duration,
}

impl Default for RequestPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_milliseconds(1_000),
            max_retries: 0,
            min_interval: Duration::from_milliseconds(0),
        }
    }
}

/// Active scheduled request state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledRequest {
    /// Request correlation key.
    pub key: RequestKey,

    /// Request scheduling policy.
    pub policy: RequestPolicy,

    /// Monotonic start time for the current attempt.
    pub started_at_ms: MonotonicTimestamp,

    /// Zero-based retry count for the current attempt.
    pub retries: u8,
}

impl ScheduledRequest {
    const fn attempts(self) -> u8 {
        self.retries.saturating_add(1)
    }
}

/// Reason a request could not be started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestStartError {
    /// Another ambiguous request is already awaiting a reply.
    Busy {
        /// Key for the active request.
        key: RequestKey,
    },

    /// The request key is still inside its pacing interval.
    Pacing {
        /// Earliest monotonic time when the request can be started.
        ready_at_ms: MonotonicTimestamp,
    },

    /// No active request can be retried.
    NoActiveRequest,
}

/// Decision returned when advancing request time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestTick {
    /// No request is currently active.
    Idle,

    /// The active request has not reached its deadline.
    Waiting,

    /// The active request reached its deadline and may be retried.
    Retry {
        /// Request key eligible for retry.
        key: RequestKey,

        /// One-based retry attempt number.
        attempt: u8,
    },

    /// The active request reached its deadline and has no retries remaining.
    TimedOut {
        /// Request key that timed out.
        key: RequestKey,

        /// Total attempts including the initial attempt.
        attempts: u8,
    },
}

/// Result of correlating a reply with the active request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorrelationResult {
    /// Reply matched the active request and cleared the slot.
    Matched {
        /// Matched request key.
        key: RequestKey,

        /// Total attempts including the initial attempt.
        attempts: u8,
    },

    /// Reply did not match the active request, or no request was active.
    Unmatched {
        /// Reply key that could not be matched.
        key: RequestKey,
    },
}

/// One-slot request tracker for ambiguous protocol replies.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RequestTracker {
    in_flight: Option<ScheduledRequest>,
    last_started: Option<(RequestKey, MonotonicTimestamp)>,
}

impl RequestTracker {
    /// Returns the active in-flight request, if any.
    #[must_use]
    pub const fn in_flight(self) -> Option<ScheduledRequest> {
        self.in_flight
    }

    /// Starts a request if no ambiguous request is active and pacing allows it.
    ///
    /// # Errors
    ///
    /// Returns [`RequestStartError::Busy`] when an earlier request is still
    /// active, or [`RequestStartError::Pacing`] when the key is inside its
    /// minimum start interval.
    pub fn start(
        &mut self,
        key: RequestKey,
        policy: RequestPolicy,
        now_ms: MonotonicTimestamp,
    ) -> Result<(), RequestStartError> {
        if let Some(active) = self.in_flight {
            return Err(RequestStartError::Busy { key: active.key });
        }

        if let Some((last_key, started_at_ms)) = self.last_started {
            let ready_at_ms = started_at_ms.saturating_add_duration(policy.min_interval);
            if last_key == key && now_ms < ready_at_ms {
                return Err(RequestStartError::Pacing { ready_at_ms });
            }
        }

        self.in_flight = Some(ScheduledRequest {
            key,
            policy,
            started_at_ms: now_ms,
            retries: 0,
        });
        self.last_started = Some((key, now_ms));
        Ok(())
    }

    /// Advances scheduler time and reports timeout or retry eligibility.
    #[must_use]
    pub const fn on_tick(self, now_ms: MonotonicTimestamp) -> RequestTick {
        let Some(active) = self.in_flight else {
            return RequestTick::Idle;
        };
        let deadline_ms = active
            .started_at_ms
            .saturating_add_duration(active.policy.timeout);
        if now_ms.get() < deadline_ms.get() {
            RequestTick::Waiting
        } else if active.retries < active.policy.max_retries {
            RequestTick::Retry {
                key: active.key,
                attempt: active.retries.saturating_add(1),
            }
        } else {
            RequestTick::TimedOut {
                key: active.key,
                attempts: active.attempts(),
            }
        }
    }

    /// Marks the active request retry as started at `now_ms`.
    ///
    /// # Errors
    ///
    /// Returns [`RequestStartError::NoActiveRequest`] when no request is
    /// active, or [`RequestStartError::Busy`] when no retries remain.
    pub const fn retry_started(
        &mut self,
        now_ms: MonotonicTimestamp,
    ) -> Result<(), RequestStartError> {
        let Some(mut active) = self.in_flight else {
            return Err(RequestStartError::NoActiveRequest);
        };
        if active.retries >= active.policy.max_retries {
            return Err(RequestStartError::Busy { key: active.key });
        }
        active.retries = active.retries.saturating_add(1);
        active.started_at_ms = now_ms;
        self.in_flight = Some(active);
        Ok(())
    }

    /// Correlates a reply key with the active request and updates diagnostics.
    pub fn correlate_reply(
        &mut self,
        key: RequestKey,
        diagnostics: &mut ParserDiagnostics,
    ) -> CorrelationResult {
        let Some(active) = self.in_flight else {
            diagnostics.record_error(ParserError::UnmatchedReply);
            return CorrelationResult::Unmatched { key };
        };

        if active.key == key {
            self.in_flight = None;
            CorrelationResult::Matched {
                key,
                attempts: active.attempts(),
            }
        } else {
            diagnostics.record_error(ParserError::UnmatchedReply);
            CorrelationResult::Unmatched { key }
        }
    }
}

/// Relative scheduling urgency for queued read-only requests.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RequestUrgency {
    /// Routine polling work such as regular telemetry refreshes.
    Routine,

    /// Higher-value probes such as identity or capability refreshes.
    High,

    /// Critical read-only probes that should be sent before other queued work.
    Critical,
}

/// Number of higher-priority pops allowed before older queued work can age ahead.
pub const REQUEST_STARVATION_SKIP_THRESHOLD: u8 = 2;

/// Request staged in a bounded scheduler queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuedRequest {
    /// Request correlation key.
    pub key: RequestKey,

    /// Request scheduling policy.
    pub policy: RequestPolicy,

    /// Relative scheduling urgency.
    pub urgency: RequestUrgency,
}

impl QueuedRequest {
    /// Creates a queued request with routine urgency.
    #[must_use]
    pub const fn new(key: RequestKey, policy: RequestPolicy) -> Self {
        Self::with_urgency(key, policy, RequestUrgency::Routine)
    }

    /// Creates a queued request with explicit urgency.
    #[must_use]
    pub const fn with_urgency(
        key: RequestKey,
        policy: RequestPolicy,
        urgency: RequestUrgency,
    ) -> Self {
        Self {
            key,
            policy,
            urgency,
        }
    }
}

/// Reason a request could not be queued.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestQueueError {
    /// The queue has no free slots.
    Full {
        /// Queue capacity in requests.
        capacity: usize,
    },

    /// A request with the same key is already queued.
    DuplicateKey {
        /// Duplicate request key.
        key: RequestKey,
    },
}

/// Per-urgency scheduler counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RequestUrgencyCounters {
    /// Routine request count.
    pub routine: u64,

    /// High-priority request count.
    pub high: u64,

    /// Critical request count.
    pub critical: u64,
}

impl RequestUrgencyCounters {
    fn increment(&mut self, urgency: RequestUrgency) {
        match urgency {
            RequestUrgency::Routine => self.routine = self.routine.saturating_add(1),
            RequestUrgency::High => self.high = self.high.saturating_add(1),
            RequestUrgency::Critical => self.critical = self.critical.saturating_add(1),
        }
    }
}

/// Structured scheduler diagnostics for bounded request queues.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RequestSchedulerDiagnostics {
    /// Requests refused because a matching key was already queued.
    pub duplicate_refusals: u64,

    /// Requests refused because the queue was full.
    pub overflow_refusals: u64,

    /// Requests accepted by urgency.
    pub enqueued: RequestUrgencyCounters,

    /// Requests popped by urgency.
    pub dequeued: RequestUrgencyCounters,

    /// Starvation-aging promotions or interventions.
    pub starvation_aging_events: u64,
}

/// Fixed-capacity FIFO request queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestQueue<const N: usize> {
    entries: [Option<QueuedRequest>; N],
    len: usize,
}

impl<const N: usize> Default for RequestQueue<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> RequestQueue<N> {
    /// Creates an empty request queue.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: [None; N],
            len: 0,
        }
    }

    /// Returns the queue capacity.
    #[must_use]
    pub const fn capacity(self) -> usize {
        N
    }

    /// Returns the number of queued requests.
    #[must_use]
    pub const fn len(self) -> usize {
        self.len
    }

    /// Returns whether the queue is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    /// Returns whether a request key is already queued.
    #[must_use]
    pub fn contains_key(self, key: RequestKey) -> bool {
        let mut index = 0;
        while index < self.len {
            if let Some(request) = self.entries[index]
                && request.key == key
            {
                return true;
            }
            index += 1;
        }
        false
    }

    /// Enqueues a request at the back of the queue.
    ///
    /// # Errors
    ///
    /// Returns [`RequestQueueError::DuplicateKey`] when the same key is already
    /// queued, or [`RequestQueueError::Full`] when the fixed capacity is
    /// exhausted.
    pub fn enqueue(&mut self, request: QueuedRequest) -> Result<(), RequestQueueError> {
        if self.contains_key(request.key) {
            return Err(RequestQueueError::DuplicateKey { key: request.key });
        }

        if self.len == N {
            return Err(RequestQueueError::Full { capacity: N });
        }

        self.entries[self.len] = Some(request);
        self.len += 1;
        Ok(())
    }

    /// Enqueues a request ahead of lower-urgency work.
    ///
    /// Requests with the same urgency retain FIFO order.
    ///
    /// # Errors
    ///
    /// Returns [`RequestQueueError::DuplicateKey`] when the same key is already
    /// queued, or [`RequestQueueError::Full`] when the fixed capacity is
    /// exhausted.
    pub fn enqueue_by_urgency(&mut self, request: QueuedRequest) -> Result<(), RequestQueueError> {
        self.enqueue_by_urgency_with_index(request).map(|_| ())
    }

    fn enqueue_by_urgency_with_index(
        &mut self,
        request: QueuedRequest,
    ) -> Result<usize, RequestQueueError> {
        if self.contains_key(request.key) {
            return Err(RequestQueueError::DuplicateKey { key: request.key });
        }

        if self.len == N {
            return Err(RequestQueueError::Full { capacity: N });
        }

        let insert_at = self
            .entries
            .iter()
            .take(self.len)
            .position(|entry| entry.is_some_and(|queued| request.urgency > queued.urgency))
            .unwrap_or(self.len);

        let mut move_from = self.len;
        while move_from > insert_at {
            self.entries[move_from] = self.entries[move_from - 1];
            move_from -= 1;
        }
        self.entries[insert_at] = Some(request);
        self.len += 1;
        Ok(insert_at)
    }

    /// Removes and returns the front request.
    pub const fn pop_next(&mut self) -> Option<QueuedRequest> {
        if self.len == 0 {
            return None;
        }

        let next = self.entries[0];
        let mut index = 1;
        while index < self.len {
            self.entries[index - 1] = self.entries[index];
            index += 1;
        }
        self.len -= 1;
        self.entries[self.len] = None;
        next
    }
}

/// Fixed-capacity request scheduler with observable diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestScheduler<const N: usize> {
    queue: RequestQueue<N>,
    skip_counts: [u8; N],
    diagnostics: RequestSchedulerDiagnostics,
}

impl<const N: usize> Default for RequestScheduler<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> RequestScheduler<N> {
    /// Creates an empty request scheduler.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            queue: RequestQueue::new(),
            skip_counts: [0; N],
            diagnostics: RequestSchedulerDiagnostics {
                duplicate_refusals: 0,
                overflow_refusals: 0,
                enqueued: RequestUrgencyCounters {
                    routine: 0,
                    high: 0,
                    critical: 0,
                },
                dequeued: RequestUrgencyCounters {
                    routine: 0,
                    high: 0,
                    critical: 0,
                },
                starvation_aging_events: 0,
            },
        }
    }

    /// Returns scheduler diagnostics accumulated so far.
    #[must_use]
    pub const fn diagnostics(self) -> RequestSchedulerDiagnostics {
        self.diagnostics
    }

    /// Returns the number of queued requests.
    #[must_use]
    pub const fn len(self) -> usize {
        self.queue.len()
    }

    /// Returns whether the scheduler queue is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.queue.is_empty()
    }

    /// Enqueues a request at FIFO priority while updating diagnostics.
    ///
    /// # Errors
    ///
    /// Returns the same refusal reason as [`RequestQueue::enqueue`].
    pub fn enqueue(&mut self, request: QueuedRequest) -> Result<(), RequestQueueError> {
        let previous_len = self.queue.len();
        let result = self.queue.enqueue(request);
        if result.is_ok() {
            self.skip_counts[previous_len] = 0;
        }
        self.record_enqueue_result(request, result)
    }

    /// Enqueues by urgency while updating diagnostics.
    ///
    /// # Errors
    ///
    /// Returns the same refusal reason as [`RequestQueue::enqueue_by_urgency`].
    pub fn enqueue_by_urgency(&mut self, request: QueuedRequest) -> Result<(), RequestQueueError> {
        let result = self.queue.enqueue_by_urgency_with_index(request);
        if let Ok(insert_at) = result {
            self.insert_skip_count(insert_at);
        }
        self.record_enqueue_result(request, result.map(|_| ()))
    }

    /// Removes and returns the next request while updating diagnostics.
    pub fn pop_next(&mut self) -> Option<QueuedRequest> {
        let selected = self.aged_pop_index()?;
        let request = self.remove_at(selected)?;
        if selected > 0 {
            self.diagnostics.starvation_aging_events =
                self.diagnostics.starvation_aging_events.saturating_add(1);
        }
        self.age_skipped_after_pop(selected);
        self.diagnostics.dequeued.increment(request.urgency);
        Some(request)
    }

    fn insert_skip_count(&mut self, insert_at: usize) {
        let mut move_from = self.queue.len().saturating_sub(1);
        while move_from > insert_at {
            self.skip_counts[move_from] = self.skip_counts[move_from - 1];
            move_from -= 1;
        }
        self.skip_counts[insert_at] = 0;
    }

    fn aged_pop_index(&self) -> Option<usize> {
        if self.queue.is_empty() {
            return None;
        }
        let mut index = 1;
        while index < self.queue.len() {
            if self.skip_counts[index] >= REQUEST_STARVATION_SKIP_THRESHOLD {
                return Some(index);
            }
            index += 1;
        }
        Some(0)
    }

    fn remove_at(&mut self, selected: usize) -> Option<QueuedRequest> {
        let request = self.queue.entries[selected]?;
        let mut index = selected + 1;
        while index < self.queue.len() {
            self.queue.entries[index - 1] = self.queue.entries[index];
            self.skip_counts[index - 1] = self.skip_counts[index];
            index += 1;
        }
        self.queue.len -= 1;
        self.queue.entries[self.queue.len] = None;
        self.skip_counts[self.queue.len] = 0;
        Some(request)
    }

    fn age_skipped_after_pop(&mut self, selected: usize) {
        for (index, skip_count) in self
            .skip_counts
            .iter_mut()
            .take(self.queue.len())
            .enumerate()
        {
            if index >= selected {
                *skip_count = skip_count.saturating_add(1);
            }
        }
    }

    fn record_enqueue_result(
        &mut self,
        request: QueuedRequest,
        result: Result<(), RequestQueueError>,
    ) -> Result<(), RequestQueueError> {
        match result {
            Ok(()) => {
                self.diagnostics.enqueued.increment(request.urgency);
                Ok(())
            }
            Err(RequestQueueError::DuplicateKey { key }) => {
                self.diagnostics.duplicate_refusals =
                    self.diagnostics.duplicate_refusals.saturating_add(1);
                Err(RequestQueueError::DuplicateKey { key })
            }
            Err(RequestQueueError::Full { capacity }) => {
                self.diagnostics.overflow_refusals =
                    self.diagnostics.overflow_refusals.saturating_add(1);
                Err(RequestQueueError::Full { capacity })
            }
        }
    }
}

/// One read-only request entry in a protocol polling plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PollRequest {
    /// Command kind to request.
    pub kind: CommandKind,

    /// Request scheduling policy.
    pub policy: RequestPolicy,

    /// Relative scheduling urgency.
    pub urgency: RequestUrgency,
}

impl PollRequest {
    /// Creates a poll request entry.
    #[must_use]
    pub const fn new(kind: CommandKind, policy: RequestPolicy, urgency: RequestUrgency) -> Self {
        Self {
            kind,
            policy,
            urgency,
        }
    }

    /// Converts this poll entry to a queued request.
    ///
    /// # Errors
    ///
    /// Returns [`PollingPlanError::UnsupportedCommand`] when the command is not
    /// read-only.
    pub const fn to_queued_request(self) -> Result<QueuedRequest, PollingPlanError> {
        let safety_class = self.kind.safety_class();
        if matches!(safety_class, SafetyClass::ReadOnly) {
            Ok(QueuedRequest::with_urgency(
                RequestKey::new(self.kind),
                self.policy,
                self.urgency,
            ))
        } else {
            Err(PollingPlanError::UnsupportedCommand {
                kind: self.kind,
                safety_class,
            })
        }
    }
}

/// Reason a polling plan could not be enqueued.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PollingPlanError {
    /// Polling plans may only contain read-only commands.
    UnsupportedCommand {
        /// Rejected command kind.
        kind: CommandKind,

        /// Safety class that made the command unsupported for polling.
        safety_class: SafetyClass,
    },

    /// The destination queue refused a request.
    Queue(RequestQueueError),
}

/// Fixed protocol polling plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PollingPlan<const N: usize> {
    items: [PollRequest; N],
}

impl<const N: usize> PollingPlan<N> {
    /// Creates a polling plan from fixed poll entries.
    #[must_use]
    pub const fn new(items: [PollRequest; N]) -> Self {
        Self { items }
    }

    /// Returns the plan entries.
    #[must_use]
    pub const fn items(self) -> [PollRequest; N] {
        self.items
    }

    /// Enqueues the plan into a bounded request queue.
    ///
    /// # Errors
    ///
    /// Returns [`PollingPlanError::UnsupportedCommand`] for non-read-only plan
    /// entries, or [`PollingPlanError::Queue`] when the destination queue
    /// refuses a converted request.
    pub fn enqueue_into<const Q: usize>(
        self,
        queue: &mut RequestQueue<Q>,
    ) -> Result<(), PollingPlanError> {
        for item in self.items {
            queue
                .enqueue_by_urgency(item.to_queued_request()?)
                .map_err(PollingPlanError::Queue)?;
        }
        Ok(())
    }
}
