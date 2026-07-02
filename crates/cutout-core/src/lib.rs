#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Core types and setup scaffolding for Cutout.

use std::fmt;

use arrayvec::ArrayVec;
use thiserror::Error;
use uuid::Uuid;

mod pevcap;
pub use pevcap::*;
mod quantity;
pub use quantity::*;
mod telemetry;
pub use telemetry::*;
mod command;
pub use command::*;
mod registry;
pub use registry::*;
mod parser;
pub use parser::*;
mod battery_page;
pub use battery_page::*;
mod ffi;
pub use ffi::*;

#[cfg(test)]
mod gatt_channel_tests;

/// Monotonic timestamp supplied by the host.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicTimestamp(u64);

impl MonotonicTimestamp {
    /// Creates a monotonic timestamp from milliseconds.
    #[must_use]
    pub const fn from_milliseconds(value: u64) -> Self {
        Self(value)
    }

    /// Creates a monotonic timestamp from milliseconds.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self::from_milliseconds(value)
    }

    /// Returns the timestamp as milliseconds.
    #[must_use]
    pub const fn as_milliseconds(self) -> u64 {
        self.0
    }

    /// Returns the timestamp as milliseconds.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.as_milliseconds()
    }

    /// Adds a duration to this timestamp, saturating at `u64::MAX`.
    #[must_use]
    pub const fn saturating_add_duration(self, duration: Duration) -> Self {
        Self::from_milliseconds(self.0.saturating_add(duration.as_milliseconds()))
    }

    /// Returns the elapsed duration between this timestamp and an earlier one.
    #[must_use]
    pub const fn saturating_duration_since(self, earlier: Self) -> Duration {
        Duration::from_milliseconds(self.0.saturating_sub(earlier.0))
    }
}

impl fmt::Display for MonotonicTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Wall-clock timestamp represented as Unix epoch milliseconds.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WallClockUnixTimestamp(u64);

impl WallClockUnixTimestamp {
    /// Creates a wall-clock timestamp from Unix epoch milliseconds.
    #[must_use]
    pub const fn from_milliseconds(value: u64) -> Self {
        Self(value)
    }

    /// Creates a wall-clock timestamp from Unix epoch milliseconds.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self::from_milliseconds(value)
    }

    /// Returns the timestamp as Unix epoch milliseconds.
    #[must_use]
    pub const fn as_milliseconds(self) -> u64 {
        self.0
    }

    /// Returns the timestamp as Unix epoch milliseconds.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.as_milliseconds()
    }
}

impl fmt::Display for WallClockUnixTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Maximum payload bytes accepted for a single GATT write value.
pub const MAX_TRANSPORT_WRITE_LEN: usize = 512;

/// Payload bytes stored inline before falling back to an explicit large write.
pub const MAX_INLINE_TRANSPORT_WRITE_LEN: usize = 32;

/// Maximum payload accepted by a transport write.
pub type TransportWriteLimit = Quantity<Information, Byte, u16>;

/// Transport-independent identifier for a GATT characteristic or endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GattChannel(Uuid);

impl GattChannel {
    /// Creates a channel identifier from its 16-byte representation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    /// Creates a channel identifier from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the channel identifier as raw bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 16] {
        *self.0.as_bytes()
    }

    /// Returns the channel identifier as a UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Host-observed link details supplied when a transport connects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkInfo {
    /// Host monotonic connection timestamp.
    pub monotonic_ms: MonotonicTimestamp,

    /// Maximum write payload length reported by the host, when known.
    pub max_write_len: Option<TransportWriteLimit>,
}

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

/// Transport write behavior requested by a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    /// Write with transport-level acknowledgement.
    WithResponse,

    /// Write without transport-level acknowledgement.
    WithoutResponse,
}

/// Input supplied to a protocol session by the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionInput<'a> {
    /// The underlying transport link is available.
    LinkUp(LinkInfo),

    /// The underlying transport link is no longer available.
    LinkDown,

    /// Notification bytes received from a transport endpoint.
    Notification {
        /// Transport endpoint that produced the bytes.
        channel: GattChannel,

        /// Borrowed notification payload for this reactor step.
        bytes: &'a [u8],

        /// Host monotonic receive timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Timer tick supplied by the host.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Command requested by the host application.
    Command(DeviceCommand),
}

/// Bounded transport write payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WritePayload(WritePayloadStorage);

#[derive(Clone, Debug, Eq, PartialEq)]
enum WritePayloadStorage {
    Inline(ArrayVec<u8, MAX_INLINE_TRANSPORT_WRITE_LEN>),
    Large(Box<ArrayVec<u8, MAX_TRANSPORT_WRITE_LEN>>),
}

impl WritePayload {
    /// Creates a bounded write payload by copying bytes from a slice.
    ///
    /// # Errors
    ///
    /// Returns [`WritePayloadTooLong`] when `bytes` exceeds
    /// [`MAX_TRANSPORT_WRITE_LEN`].
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, WritePayloadTooLong> {
        if bytes.len() > MAX_TRANSPORT_WRITE_LEN {
            return Err(WritePayloadTooLong {
                len: bytes.len(),
                max: MAX_TRANSPORT_WRITE_LEN,
            });
        }

        if bytes.len() <= MAX_INLINE_TRANSPORT_WRITE_LEN {
            return Ok(Self(WritePayloadStorage::Inline(
                ArrayVec::<u8, MAX_INLINE_TRANSPORT_WRITE_LEN>::try_from(bytes).map_err(|_| {
                    WritePayloadTooLong {
                        len: bytes.len(),
                        max: MAX_TRANSPORT_WRITE_LEN,
                    }
                })?,
            )));
        }

        Ok(Self(WritePayloadStorage::Large(Box::new(
            ArrayVec::<u8, MAX_TRANSPORT_WRITE_LEN>::try_from(bytes).map_err(|_| {
                WritePayloadTooLong {
                    len: bytes.len(),
                    max: MAX_TRANSPORT_WRITE_LEN,
                }
            })?,
        ))))
    }

    /// Returns the write payload as bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        match &self.0 {
            WritePayloadStorage::Inline(bytes) => bytes.as_slice(),
            WritePayloadStorage::Large(bytes) => bytes.as_slice(),
        }
    }

    /// Returns the payload length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Returns whether the payload is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Returns whether this payload uses the common inline representation.
    #[must_use]
    pub const fn is_inline(&self) -> bool {
        matches!(self.0, WritePayloadStorage::Inline(_))
    }
}

/// Error returned when constructing an oversized write payload.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("write payload length {len} exceeds maximum {max}")]
pub struct WritePayloadTooLong {
    /// Attempted payload length.
    pub len: usize,

    /// Maximum accepted payload length.
    pub max: usize,
}

/// Action a host transport must perform for a protocol session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportAction {
    /// Subscribe to notifications from a transport endpoint.
    Subscribe {
        /// Transport endpoint to subscribe to.
        channel: GattChannel,
    },

    /// Write bytes to a transport endpoint.
    Write {
        /// Transport endpoint to write to.
        channel: GattChannel,

        /// Bounded bytes to write after this reactor step.
        bytes: WritePayload,

        /// Transport write behavior.
        mode: WriteMode,
    },

    /// Disconnect the underlying transport.
    Disconnect,
}

/// Semantic event emitted by a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceEvent {
    /// Link-up event accepted by the session.
    LinkUp(LinkInfo),

    /// Link-down event accepted by the session.
    LinkDown,

    /// Tick event accepted by the session.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Telemetry update emitted by a protocol session.
    Telemetry(TelemetryDelta),

    /// Read-only response emitted by a protocol session.
    ReadOnlyResponse(ReadOnlyResponse),

    /// Control command refused before transport writes.
    ControlRefusal(ControlRefusal),

    /// Parser diagnostics emitted by a protocol session.
    Diagnostics(ParserDiagnostics),

    /// Detailed parser diagnostic error emitted by a protocol session.
    DiagnosticError(DiagnosticError),
}

/// Output emitted by a protocol session for the host to drain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionOutput {
    /// Transport action to execute outside the protocol engine.
    Transport(TransportAction),

    /// Semantic event to report to the application.
    Event(DeviceEvent),

    /// Parser-level notification ingest outcome.
    NotificationIngest(NotificationIngestOutcome),
}

/// Default number of session outputs retained by the host facade before drain.
pub const DEFAULT_SESSION_OUTPUT_CAPACITY: usize = 16;

/// Session output sink capacity.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct SessionOutputCapacity(usize);

impl SessionOutputCapacity {
    /// Creates a session output capacity from an already parsed value.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the underlying output count.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Error returned when a session cannot emit an output.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SessionOutputError {
    /// The output sink is full.
    #[error("session output sink is full at capacity {capacity:?}")]
    Full {
        /// Configured output capacity.
        capacity: SessionOutputCapacity,
    },
}

/// Session output sink used by protocol engines.
pub trait SessionOutputSink {
    /// Pushes one output into the sink.
    ///
    /// # Errors
    ///
    /// Returns [`SessionOutputError::Full`] when the sink has no free slot.
    fn push(&mut self, output: SessionOutput) -> Result<(), SessionOutputError>;
}

impl SessionOutputSink for Vec<SessionOutput> {
    fn push(&mut self, output: SessionOutput) -> Result<(), SessionOutputError> {
        Vec::push(self, output);
        Ok(())
    }
}

/// Bounded session output storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedSessionOutput<const CAPACITY: usize> {
    output: ArrayVec<SessionOutput, CAPACITY>,
}

impl<const CAPACITY: usize> BoundedSessionOutput<CAPACITY> {
    /// Creates an empty bounded output buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            output: ArrayVec::new_const(),
        }
    }

    /// Returns buffered outputs.
    #[must_use]
    pub fn as_slice(&self) -> &[SessionOutput] {
        self.output.as_slice()
    }

    /// Returns buffered output count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.output.len()
    }

    /// Returns true when no outputs are buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.output.is_empty()
    }

    /// Drains buffered outputs into an owned vector.
    #[must_use]
    pub fn drain(&mut self) -> Vec<SessionOutput> {
        self.output.drain(..).collect()
    }

    /// Drains buffered outputs into an existing vector.
    pub fn drain_into(&mut self, output: &mut Vec<SessionOutput>) {
        output.extend(self.output.drain(..));
    }

    /// Returns the configured output capacity.
    #[must_use]
    pub const fn capacity(&self) -> SessionOutputCapacity {
        SessionOutputCapacity::new(CAPACITY)
    }
}

impl<const CAPACITY: usize> Default for BoundedSessionOutput<CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAPACITY: usize> SessionOutputSink for BoundedSessionOutput<CAPACITY> {
    fn push(&mut self, output: SessionOutput) -> Result<(), SessionOutputError> {
        self.output
            .try_push(output)
            .map_err(|_| SessionOutputError::Full {
                capacity: self.capacity(),
            })
    }
}

/// Synchronous protocol reactor.
pub trait ProtocolSession {
    /// Handles one input and appends any resulting outputs.
    ///
    /// # Errors
    ///
    /// Returns [`SessionOutputError`] when the output sink cannot accept every
    /// output produced for the input.
    fn handle(
        &mut self,
        input: SessionInput<'_>,
        output: &mut dyn SessionOutputSink,
    ) -> Result<(), SessionOutputError>;
}

/// Host-facing synchronous session facade.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostSession<S, const OUTPUT_CAPACITY: usize = DEFAULT_SESSION_OUTPUT_CAPACITY> {
    session: S,
    output: Box<BoundedSessionOutput<OUTPUT_CAPACITY>>,
    snapshot: TelemetrySnapshot,
    diagnostics: ParserDiagnostics,
}

impl<S> HostSession<S>
where
    S: ProtocolSession,
{
    /// Creates a host session around a protocol session.
    #[must_use]
    pub fn new(session: S) -> Self {
        Self {
            session,
            output: Box::new(BoundedSessionOutput::new()),
            snapshot: TelemetrySnapshot {
                at_ms: None,
                speed: None,
                voltage: None,
                battery_current: None,
                motor_current: None,
                power: None,
                controller_temperature: None,
                motor_temperature: None,
                battery_temperature: None,
                pwm: None,
                distance: None,
                pitch: None,
                roll: None,
                battery_level_reported: None,
                battery_level_estimated: None,
            },
            diagnostics: ParserDiagnostics::default(),
        }
    }
}

impl<S, const OUTPUT_CAPACITY: usize> HostSession<S, OUTPUT_CAPACITY>
where
    S: ProtocolSession,
{
    /// Creates a host session with an explicit bounded output capacity.
    #[must_use]
    pub fn with_output_capacity(session: S) -> Self {
        Self {
            session,
            output: Box::new(BoundedSessionOutput::new()),
            snapshot: TelemetrySnapshot {
                at_ms: None,
                speed: None,
                voltage: None,
                battery_current: None,
                motor_current: None,
                power: None,
                controller_temperature: None,
                motor_temperature: None,
                battery_temperature: None,
                pwm: None,
                distance: None,
                pitch: None,
                roll: None,
                battery_level_reported: None,
                battery_level_estimated: None,
            },
            diagnostics: ParserDiagnostics::default(),
        }
    }

    /// Supplies a link-up event to the protocol session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionOutputError::Full`] when the host output buffer fills.
    pub fn ingest_link_up(&mut self, link: LinkInfo) -> Result<(), SessionOutputError> {
        self.handle(SessionInput::LinkUp(link))
    }

    /// Supplies a link-down event to the protocol session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionOutputError::Full`] when the host output buffer fills.
    pub fn ingest_link_down(&mut self) -> Result<(), SessionOutputError> {
        self.handle(SessionInput::LinkDown)
    }

    /// Supplies owned notification bytes to the protocol session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionOutputError::Full`] when the host output buffer fills.
    pub fn ingest_notification_owned(
        &mut self,
        channel: GattChannel,
        bytes: Vec<u8>,
        monotonic_ms: MonotonicTimestamp,
    ) -> Result<(), SessionOutputError> {
        let bytes = bytes.into_boxed_slice();
        self.handle(SessionInput::Notification {
            channel,
            bytes: &bytes,
            monotonic_ms,
        })
    }

    /// Supplies a host timer tick to the protocol session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionOutputError::Full`] when the host output buffer fills.
    pub fn tick(&mut self, monotonic_ms: MonotonicTimestamp) -> Result<(), SessionOutputError> {
        self.handle(SessionInput::Tick { monotonic_ms })
    }

    /// Supplies a host command to the protocol session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionOutputError::Full`] when the host output buffer fills.
    pub fn issue_command(&mut self, command: DeviceCommand) -> Result<(), SessionOutputError> {
        self.handle(SessionInput::Command(command))
    }

    /// Supplies one borrowed host input to the protocol session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionOutputError::Full`] when the host output buffer fills.
    pub fn ingest(&mut self, input: SessionInput<'_>) -> Result<(), SessionOutputError> {
        self.handle(input)
    }

    /// Drains owned session outputs accumulated so far.
    #[must_use]
    pub fn drain_outputs(&mut self) -> Vec<SessionOutput> {
        self.output.drain()
    }

    /// Moves accumulated session outputs into an existing buffer.
    pub fn drain_outputs_into(&mut self, output: &mut Vec<SessionOutput>) {
        self.output.drain_into(output);
    }

    /// Returns the latest telemetry snapshot.
    #[must_use]
    pub const fn current_snapshot(&self) -> TelemetrySnapshot {
        self.snapshot
    }

    /// Returns accumulated parser diagnostics.
    #[must_use]
    pub const fn diagnostics(&self) -> ParserDiagnostics {
        self.diagnostics
    }

    fn handle(&mut self, input: SessionInput<'_>) -> Result<(), SessionOutputError> {
        let start = self.output.len();
        self.session.handle(input, &mut *self.output)?;
        self.apply_state_from_outputs(start);
        Ok(())
    }

    fn apply_state_from_outputs(&mut self, start: usize) {
        for output in &self.output.as_slice()[start..] {
            if let SessionOutput::Event(event) = output {
                match event {
                    DeviceEvent::Telemetry(delta) => {
                        self.snapshot.apply_delta(*delta);
                    }
                    DeviceEvent::Diagnostics(diagnostics) => {
                        self.diagnostics.merge(*diagnostics);
                    }
                    DeviceEvent::ReadOnlyResponse(_)
                    | DeviceEvent::ControlRefusal(_)
                    | DeviceEvent::DiagnosticError(_)
                    | DeviceEvent::LinkUp(_)
                    | DeviceEvent::LinkDown
                    | DeviceEvent::Tick { .. } => {}
                }
            }
        }
    }
}

/// Owned host input captured for deterministic replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureRecord {
    /// Captured link-up input.
    LinkUp(LinkInfo),

    /// Captured link-down input.
    LinkDown,

    /// Captured notification input with owned bytes.
    Notification {
        /// Transport endpoint that produced the bytes.
        channel: GattChannel,

        /// Owned notification payload.
        bytes: Vec<u8>,

        /// Host monotonic receive timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Captured timer tick.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Captured host command.
    Command(DeviceCommand),

    /// Captured host command with target metadata for correlation.
    TargetedCommand {
        /// Captured command.
        command: DeviceCommand,

        /// Captured request target.
        target: RequestTarget,
    },
}

impl CaptureRecord {
    /// Creates a notification capture record with owned bytes.
    #[must_use]
    pub const fn notification(
        channel: GattChannel,
        bytes: Vec<u8>,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Notification {
            channel,
            bytes,
            monotonic_ms,
        }
    }

    /// Creates a captured host command with explicit target metadata.
    #[must_use]
    pub const fn targeted_command(command: DeviceCommand, target: RequestTarget) -> Self {
        Self::TargetedCommand { command, target }
    }

    /// Splits a notification record into chunks no larger than `chunk_len`.
    ///
    /// Non-notification records are returned unchanged. A zero `chunk_len`
    /// leaves the record unchanged.
    #[must_use]
    pub fn split_notification_bytes(self, chunk_len: NotificationChunkLen) -> Vec<Self> {
        let Self::Notification {
            channel,
            bytes,
            monotonic_ms,
        } = self
        else {
            return vec![self];
        };

        if chunk_len.is_whole() {
            return vec![Self::notification(channel, bytes, monotonic_ms)];
        }

        bytes
            .chunks(chunk_len.as_bytes())
            .map(|chunk| Self::notification(channel, chunk.to_vec(), monotonic_ms))
            .collect()
    }

    /// Splits a notification record by requested chunk lengths.
    ///
    /// Extra bytes are appended as a final chunk. Non-notification records are
    /// returned unchanged.
    #[must_use]
    pub fn split_notification_by_lengths(self, lengths: &[NotificationChunkLen]) -> Vec<Self> {
        let Self::Notification {
            channel,
            bytes,
            monotonic_ms,
        } = self
        else {
            return vec![self];
        };

        let mut records = Vec::new();
        let mut offset = 0;
        for length in lengths.iter().copied().filter(|length| !length.is_whole()) {
            if offset >= bytes.len() {
                break;
            }
            let end = offset.saturating_add(length.as_bytes()).min(bytes.len());
            records.push(Self::notification(
                channel,
                bytes[offset..end].to_vec(),
                monotonic_ms,
            ));
            offset = end;
        }
        if offset < bytes.len() {
            records.push(Self::notification(
                channel,
                bytes[offset..].to_vec(),
                monotonic_ms,
            ));
        }
        records
    }
}

/// Replays captured host inputs through a host session and returns outputs.
#[must_use]
pub fn replay_capture<S>(host: &mut HostSession<S>, records: &[CaptureRecord]) -> Vec<SessionOutput>
where
    S: ProtocolSession,
{
    let mut outputs = Vec::new();
    if replay_capture_into(host, records, &mut outputs).is_err() {
        return outputs;
    }
    outputs
}

/// Replays captured host inputs through a host session and returns outputs.
///
/// # Errors
///
/// Returns [`SessionOutputError::Full`] when the host output buffer fills.
pub fn try_replay_capture<S, const OUTPUT_CAPACITY: usize>(
    host: &mut HostSession<S, OUTPUT_CAPACITY>,
    records: &[CaptureRecord],
) -> Result<Vec<SessionOutput>, SessionOutputError>
where
    S: ProtocolSession,
{
    let mut outputs = Vec::new();
    replay_capture_into(host, records, &mut outputs)?;
    Ok(outputs)
}

/// Replays captured host inputs through a host session into an existing buffer.
///
/// # Errors
///
/// Returns [`SessionOutputError::Full`] when the host output buffer fills.
pub fn replay_capture_into<S, const OUTPUT_CAPACITY: usize>(
    host: &mut HostSession<S, OUTPUT_CAPACITY>,
    records: &[CaptureRecord],
    outputs: &mut Vec<SessionOutput>,
) -> Result<(), SessionOutputError>
where
    S: ProtocolSession,
{
    for record in records {
        match record {
            CaptureRecord::LinkUp(link) => host.ingest_link_up(*link),
            CaptureRecord::LinkDown => host.ingest_link_down(),
            CaptureRecord::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => host.ingest(SessionInput::Notification {
                channel: *channel,
                bytes,
                monotonic_ms: *monotonic_ms,
            }),
            CaptureRecord::Tick { monotonic_ms } => host.tick(*monotonic_ms),
            CaptureRecord::Command(command) | CaptureRecord::TargetedCommand { command, .. } => {
                host.issue_command(*command)
            }
        }?;
        host.drain_outputs_into(outputs);
    }
    Ok(())
}

/// Summary of deterministic replay equivalence across notification chunking
/// modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayChunkComparison {
    /// Semantic event count from whole-notification replay.
    pub whole_semantic_events: SemanticEventCount,

    /// Semantic event count from one-byte notification replay.
    pub one_byte_semantic_events: SemanticEventCount,

    /// Semantic event count from arbitrary notification chunk replay.
    pub arbitrary_semantic_events: SemanticEventCount,

    /// Whether one-byte replay produced the same semantic events as whole
    /// replay.
    pub one_byte_matches: bool,

    /// Whether arbitrary chunk replay produced the same semantic events as
    /// whole replay.
    pub arbitrary_matches: bool,
}

/// Named replay case for testing parser behavior across notification
/// boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationBoundaryReplayCase {
    /// Stable case name for assertion diagnostics.
    pub name: &'static str,

    /// Replay records for this notification boundary layout.
    pub records: Vec<CaptureRecord>,
}

/// Named replay case for malformed or lossy notification streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationImpairmentReplayCase {
    /// Stable case name for assertion diagnostics.
    pub name: &'static str,

    /// Replay records for this impaired notification stream.
    pub records: Vec<CaptureRecord>,
}

/// Replays a capture and returns semantic events only.
///
/// Typed ingest outcomes are intentionally excluded because notification
/// boundaries differ between chunking modes even when decoded protocol behavior
/// is equivalent.
#[must_use]
pub fn replay_capture_semantic_events<S>(
    host: &mut HostSession<S>,
    records: &[CaptureRecord],
) -> Vec<DeviceEvent>
where
    S: ProtocolSession,
{
    replay_capture(host, records)
        .into_iter()
        .filter_map(|output| match output {
            SessionOutput::Transport(_) | SessionOutput::NotificationIngest(_) => None,
            SessionOutput::Event(event) => Some(event),
        })
        .collect()
}

/// Compares whole-notification replay against one-byte and arbitrary
/// notification chunk replay.
#[must_use]
pub fn compare_replay_capture_chunks<S, F>(
    mut make_session: F,
    records: &[CaptureRecord],
    arbitrary_lengths: &[NotificationChunkLen],
) -> ReplayChunkComparison
where
    S: ProtocolSession,
    F: FnMut() -> S,
{
    let whole = replay_capture_semantic_events(&mut HostSession::new(make_session()), records);
    let one_byte_records =
        split_capture_notifications_by_len(records, NotificationChunkLen::from_bytes(1));
    let one_byte =
        replay_capture_semantic_events(&mut HostSession::new(make_session()), &one_byte_records);
    let arbitrary_records = split_capture_notifications_by_lengths(records, arbitrary_lengths);
    let arbitrary =
        replay_capture_semantic_events(&mut HostSession::new(make_session()), &arbitrary_records);

    ReplayChunkComparison {
        whole_semantic_events: SemanticEventCount::from_events(whole.len()),
        one_byte_semantic_events: SemanticEventCount::from_events(one_byte.len()),
        arbitrary_semantic_events: SemanticEventCount::from_events(arbitrary.len()),
        one_byte_matches: one_byte == whole,
        arbitrary_matches: arbitrary == whole,
    }
}

/// Builds a deterministic arbitrary notification chunk plan from replay
/// records.
///
/// The plan is sized to split the longest notification in the capture using a
/// repeating 2/3/5 byte pattern. Shorter notifications ignore extra chunk
/// lengths during replay.
#[must_use]
pub fn replay_arbitrary_chunk_lengths(records: &[CaptureRecord]) -> Vec<NotificationChunkLen> {
    let max_notification_len = records
        .iter()
        .filter_map(|record| match record {
            CaptureRecord::Notification { bytes, .. } => Some(bytes.len()),
            CaptureRecord::LinkUp(_)
            | CaptureRecord::LinkDown
            | CaptureRecord::Tick { .. }
            | CaptureRecord::Command(_)
            | CaptureRecord::TargetedCommand { .. } => None,
        })
        .max()
        .unwrap_or_default();

    let mut lengths = Vec::new();
    let mut covered = 0usize;
    for chunk_len in [2usize, 3, 5].into_iter().cycle() {
        if covered >= max_notification_len {
            break;
        }
        let remaining = max_notification_len - covered;
        let next = chunk_len.min(remaining);
        lengths.push(NotificationChunkLen::from_bytes(next));
        covered += next;
    }
    lengths
}

/// Builds reusable replay cases for parser tests from protocol frames.
///
/// The returned cases cover one frame per notification, one byte per
/// notification, caller-supplied arbitrary chunk lengths, and all frames
/// coalesced into one notification. Parser tests can state canonical protocol
/// frames once, then compare expected semantic events across these boundary
/// layouts.
#[must_use]
pub fn notification_boundary_replay_cases(
    channel: GattChannel,
    frames: &[&[u8]],
    monotonic_ms: MonotonicTimestamp,
    arbitrary_lengths: &[NotificationChunkLen],
) -> Vec<NotificationBoundaryReplayCase> {
    let whole_records = notification_records(channel, frames, monotonic_ms);
    let one_byte_records =
        split_capture_notifications_by_len(&whole_records, NotificationChunkLen::from_bytes(1));
    let arbitrary_records =
        split_capture_notifications_by_lengths(&whole_records, arbitrary_lengths);
    let coalesced_records = coalesced_notification_record(channel, frames, monotonic_ms);

    vec![
        NotificationBoundaryReplayCase {
            name: "whole",
            records: whole_records,
        },
        NotificationBoundaryReplayCase {
            name: "one-byte",
            records: one_byte_records,
        },
        NotificationBoundaryReplayCase {
            name: "arbitrary",
            records: arbitrary_records,
        },
        NotificationBoundaryReplayCase {
            name: "coalesced",
            records: coalesced_records,
        },
    ]
}

/// Builds reusable replay cases for parser tests that exercise malformed
/// streams.
///
/// The returned cases include garbage before a valid frame, duplicate first
/// chunks, missing final bytes, and a timeout tick after a partial frame.
/// Parser tests should state the expected behavior for each named case because
/// some protocols recover while others intentionally reject or wait.
#[must_use]
pub fn notification_impairment_replay_cases(
    channel: GattChannel,
    frame: &[u8],
    monotonic_ms: MonotonicTimestamp,
    garbage_prefix: &[u8],
    timeout_ms: MonotonicTimestamp,
) -> Vec<NotificationImpairmentReplayCase> {
    vec![
        NotificationImpairmentReplayCase {
            name: "garbage-prefix",
            records: vec![CaptureRecord::notification(
                channel,
                prefixed_bytes(garbage_prefix, frame),
                monotonic_ms,
            )],
        },
        NotificationImpairmentReplayCase {
            name: "duplicate-first-chunk",
            records: duplicate_first_chunk_records(channel, frame, monotonic_ms),
        },
        NotificationImpairmentReplayCase {
            name: "missing-final-byte",
            records: missing_final_byte_record(channel, frame, monotonic_ms),
        },
        NotificationImpairmentReplayCase {
            name: "timeout-after-partial",
            records: timeout_after_partial_records(channel, frame, monotonic_ms, timeout_ms),
        },
    ]
}

fn notification_records(
    channel: GattChannel,
    frames: &[&[u8]],
    monotonic_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    frames
        .iter()
        .map(|frame| CaptureRecord::notification(channel, (*frame).to_vec(), monotonic_ms))
        .collect()
}

fn coalesced_notification_record(
    channel: GattChannel,
    frames: &[&[u8]],
    monotonic_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    let len = frames.iter().map(|frame| frame.len()).sum();
    let mut bytes = Vec::with_capacity(len);
    for frame in frames {
        bytes.extend_from_slice(frame);
    }

    if bytes.is_empty() {
        Vec::new()
    } else {
        vec![CaptureRecord::notification(channel, bytes, monotonic_ms)]
    }
}

fn prefixed_bytes(prefix: &[u8], bytes: &[u8]) -> Vec<u8> {
    let mut prefixed = Vec::with_capacity(prefix.len().saturating_add(bytes.len()));
    prefixed.extend_from_slice(prefix);
    prefixed.extend_from_slice(bytes);
    prefixed
}

fn duplicate_first_chunk_records(
    channel: GattChannel,
    frame: &[u8],
    monotonic_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    if frame.is_empty() {
        return Vec::new();
    }

    let split = frame.len().clamp(1, 4);
    let first = frame[..split].to_vec();
    let mut records = vec![
        CaptureRecord::notification(channel, first.clone(), monotonic_ms),
        CaptureRecord::notification(channel, first, monotonic_ms),
    ];

    if split < frame.len() {
        records.push(CaptureRecord::notification(
            channel,
            frame[split..].to_vec(),
            monotonic_ms,
        ));
    }

    records
}

fn missing_final_byte_record(
    channel: GattChannel,
    frame: &[u8],
    monotonic_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    let Some(truncated_len) = frame.len().checked_sub(1) else {
        return Vec::new();
    };

    vec![CaptureRecord::notification(
        channel,
        frame[..truncated_len].to_vec(),
        monotonic_ms,
    )]
}

fn timeout_after_partial_records(
    channel: GattChannel,
    frame: &[u8],
    monotonic_ms: MonotonicTimestamp,
    timeout_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    let split = frame.len().saturating_sub(1);
    vec![
        CaptureRecord::notification(channel, frame[..split].to_vec(), monotonic_ms),
        CaptureRecord::Tick {
            monotonic_ms: timeout_ms,
        },
    ]
}

fn split_capture_notifications_by_len(
    records: &[CaptureRecord],
    chunk_len: NotificationChunkLen,
) -> Vec<CaptureRecord> {
    records
        .iter()
        .cloned()
        .flat_map(|record| record.split_notification_bytes(chunk_len))
        .collect()
}

fn split_capture_notifications_by_lengths(
    records: &[CaptureRecord],
    lengths: &[NotificationChunkLen],
) -> Vec<CaptureRecord> {
    records
        .iter()
        .cloned()
        .flat_map(|record| record.split_notification_by_lengths(lengths))
        .collect()
}

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-core"
}

#[cfg(test)]
mod tests {
    use super::crate_name;
    use crate::round_div_i32;
    use crate::{
        Angle, BatteryCurrent, BatteryLevel, Capacity, CellVoltage, Current, DeviceCommand,
        DeviceEvent, Distance, Duration, DutyCycle, Energy, GattChannel, LinkInfo, Measured,
        MonotonicTimestamp, ParallelCount, PeakCurrent, PhaseCurrent, Power, ProtocolSession,
        SeriesCount, SessionInput, SessionOutput, SessionOutputSink, Speed, TelemetryDelta,
        TelemetrySnapshot, Temperature, TransportAction, UnsupportedReason, ValueQuality,
        ValueSource, VerificationStatus, Voltage, WriteMode, WritePayload,
    };
    use core::mem::size_of;
    use proptest::prelude::*;

    const TEST_CHANNEL: GattChannel = GattChannel::from_bytes([0xA1; 16]);

    const fn ms(value: u64) -> MonotonicTimestamp {
        MonotonicTimestamp::new(value)
    }

    const fn dropped_bytes(value: u64) -> crate::ParserDroppedBytes {
        crate::ParserDroppedBytes::from_bytes(value)
    }

    const fn diag_count(value: u64) -> crate::ParserDiagnosticCount {
        crate::ParserDiagnosticCount::from_events(value)
    }

    const fn write_len(value: u16) -> crate::TransportWriteLimit {
        crate::TransportWriteLimit::from_bytes(value)
    }

    const fn frame_len(value: usize) -> crate::ParserFrameLen {
        crate::ParserFrameLen::from_bytes(value)
    }

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-core");
    }

    #[test]
    fn write_payload_preserves_bytes_without_vec_storage() {
        let payload = WritePayload::try_from_slice(b"telemetry").expect("payload fits");

        assert_eq!(payload.as_slice(), b"telemetry");
        assert_eq!(payload.len(), 9);
        assert!(payload.is_inline());
    }

    #[test]
    fn write_payload_uses_explicit_large_variant_for_rare_max_size_writes() {
        let bytes = [0xa5; crate::MAX_TRANSPORT_WRITE_LEN];
        let payload = WritePayload::try_from_slice(&bytes).expect("max payload fits");

        assert_eq!(payload.as_slice(), bytes);
        assert_eq!(payload.len(), crate::MAX_TRANSPORT_WRITE_LEN);
        assert!(!payload.is_inline());
    }

    #[test]
    fn write_payload_rejects_oversized_writes() {
        let bytes = vec![0; crate::MAX_TRANSPORT_WRITE_LEN + 1];

        assert_eq!(
            WritePayload::try_from_slice(&bytes),
            Err(crate::WritePayloadTooLong {
                len: crate::MAX_TRANSPORT_WRITE_LEN + 1,
                max: crate::MAX_TRANSPORT_WRITE_LEN,
            })
        );
    }

    #[test]
    fn wire_voltage_keeps_protocol_voltage_units_explicit() {
        let voltage = crate::WireVoltage::from_centivolts(6_005);

        assert_eq!(voltage.as_centivolts(), 6_005);
        assert_eq!(voltage.as_millivolts(), 60_050);
        assert_eq!(
            voltage.as_scaled_voltage(1_000),
            Voltage::from_millivolts(60_050)
        );
        assert_eq!(
            crate::WireVoltage::from_scaled_voltage(Voltage::from_millivolts(60_050), 1_000),
            voltage
        );
    }

    #[test]
    fn battery_page_types_remain_small() {
        assert_eq!(size_of::<crate::BatteryPageKind>(), 1);
        assert_eq!(size_of::<crate::BatteryPageMetadata>(), 3);
        assert!(size_of::<crate::BatteryInfo>() <= 64);
        assert!(size_of::<crate::BatteryPagePayload>() <= 128);
        assert!(size_of::<crate::RawTelemetryReadback>() <= 96);
        assert!(size_of::<crate::ReadOnlyResponse>() <= 104);
        assert_eq!(size_of::<SessionOutput>(), 128);
        assert_eq!(size_of::<TransportAction>(), 64);
    }

    #[test]
    fn inline_write_capacity_size_snapshot_quantifies_transport_cost() {
        assert_eq!(crate::MAX_TRANSPORT_WRITE_LEN, 512);
        assert_eq!(crate::MAX_INLINE_TRANSPORT_WRITE_LEN, 32);
        assert_eq!(size_of::<WritePayload>(), 40);
        assert_eq!(size_of::<TransportAction>(), 64);
        assert_eq!(size_of::<SessionOutput>(), 128);
    }

    #[test]
    fn raw_telemetry_response_preserves_protocol_native_fields() {
        let response = crate::ReadOnlyResponse::RawTelemetry(crate::RawTelemetryReadback {
            fields: [
                Some(crate::RawFieldValue::new(0x8001, 989)),
                Some(crate::RawFieldValue::new(0x8002, -21_973)),
                Some(crate::RawFieldValue::new(0x8003, 20)),
                Some(crate::RawFieldValue::new(0x8004, 0)),
            ],
        });

        assert_eq!(
            response.command_kind(),
            crate::CommandKind::RequestTelemetry
        );
        let crate::ReadOnlyResponse::RawTelemetry(raw) = response else {
            panic!("expected raw telemetry");
        };
        assert_eq!(raw.fields[0], Some(crate::RawFieldValue::new(0x8001, 989)));
        assert_eq!(
            raw.fields[1],
            Some(crate::RawFieldValue::new(0x8002, -21_973))
        );
    }

    #[test]
    fn raw_telemetry_readback_reports_over_capacity_fields() {
        let fields = [
            crate::RawFieldValue::new(0x8001, 1),
            crate::RawFieldValue::new(0x8002, 2),
            crate::RawFieldValue::new(0x8003, 3),
            crate::RawFieldValue::new(0x8004, 4),
        ];
        let readback = crate::RawTelemetryReadback::try_from_fields(&fields)
            .expect("exact raw telemetry capacity is accepted");

        assert_eq!(readback.fields, fields.map(Some));
        assert_eq!(
            crate::RawTelemetryReadback::try_from_fields(&[
                fields[0], fields[1], fields[2], fields[3], fields[0],
            ]),
            Err(crate::ReadbackCapacityError::TooManyItems {
                capacity: crate::RAW_TELEMETRY_READBACK_CAPACITY,
                requested: crate::RAW_TELEMETRY_READBACK_CAPACITY + 1,
            })
        );
    }

    #[test]
    fn request_scheduler_size_snapshot_separates_queue_and_diagnostics_cost() {
        assert_eq!(size_of::<crate::QueuedRequest>(), 32);
        assert_eq!(size_of::<crate::RequestSchedulerDiagnostics>(), 72);
        assert_eq!(size_of::<crate::RequestQueue<3>>(), 104);
        assert_eq!(size_of::<crate::RequestScheduler<3>>(), 184);
        assert_eq!(size_of::<crate::RequestQueue<8>>(), 264);
        assert_eq!(size_of::<crate::RequestScheduler<8>>(), 344);
    }

    #[test]
    fn request_hot_path_types_remain_small() {
        assert!(size_of::<crate::RequestKey>() <= 16);
        assert!(size_of::<crate::RequestPolicy>() <= 24);
        assert!(size_of::<crate::QueuedRequest>() <= 32);
        assert!(size_of::<crate::RequestTracker>() <= 56);
        assert!(size_of::<crate::PollRequest>() <= 32);
        assert!(size_of::<crate::RequestQueue<3>>() <= 104);
        assert!(size_of::<crate::RequestScheduler<3>>() <= 184);
        assert!(size_of::<crate::PollingPlan<4>>() <= 128);
    }

    #[test]
    fn parser_hot_path_types_remain_small() {
        assert!(size_of::<Measured<u16>>() <= 8);
        assert!(size_of::<Measured<i32>>() <= 16);
        assert!(size_of::<Measured<u64>>() <= 24);
        assert_eq!(size_of::<crate::NotificationByteLen>(), size_of::<usize>());
        assert_eq!(size_of::<crate::NotificationChunkLen>(), size_of::<usize>());
        assert_eq!(size_of::<crate::PayloadBodyLen>(), size_of::<usize>());
        assert_eq!(size_of::<crate::SemanticEventCount>(), size_of::<usize>());
        assert_eq!(size_of::<crate::ProtocolSelector>(), size_of::<u8>());
        assert_eq!(size_of::<crate::ProtocolTag>(), size_of::<u16>());
        assert_eq!(size_of::<SeriesCount>(), size_of::<u8>());
        assert_eq!(size_of::<ParallelCount>(), size_of::<u8>());
        assert_eq!(size_of::<crate::BmsCellValuesPerPage>(), size_of::<u8>());
        assert_eq!(
            size_of::<crate::BmsTemperatureValuesPerPage>(),
            size_of::<u8>()
        );
        assert_eq!(size_of::<crate::BmsPackIndex>(), size_of::<u8>());
        assert_eq!(size_of::<crate::BmsHalfIndex>(), size_of::<u8>());
        assert_eq!(size_of::<crate::BmsCellIndex>(), size_of::<u16>());
        assert_eq!(size_of::<crate::ParserDiagnostics>(), 56);
        assert_eq!(size_of::<crate::DiagnosticSnapshot>(), 56);
        assert!(size_of::<crate::DiagnosticError>() <= 80);
        assert!(size_of::<crate::NotificationIngestOutcome>() <= 128);
        assert!(size_of::<crate::NotificationEvidence>() <= 64);
        assert!(size_of::<crate::PayloadClassifier>() <= 4);
        assert!(size_of::<crate::ReservedPayloadEvidence>() <= 64);
        assert!(size_of::<TelemetrySnapshot>() <= 256);
        assert!(size_of::<crate::CaptureRecord>() <= 48);
        assert!(size_of::<crate::HostSession<EchoSession>>() <= 352);
    }

    #[test]
    fn notification_ingest_evidence_uses_distinct_typed_protocol_values() {
        let notification_len = crate::NotificationByteLen::from_bytes(77);
        let body_len = crate::PayloadBodyLen::from_bytes(24);
        let event_count = crate::SemanticEventCount::from_events(3);
        let selector = crate::ProtocolSelector::new(8);
        let tag = crate::ProtocolTag::new(0x5c);

        assert_eq!(notification_len.as_bytes(), 77);
        assert_eq!(body_len.as_bytes(), 24);
        assert_eq!(event_count.as_events(), 3);
        assert_eq!(selector.get(), 8);
        assert_eq!(tag.get(), 0x5c);
    }

    #[test]
    fn notification_ingest_outcome_distinguishes_buffered_fragments_from_ignored_traffic() {
        let buffered = crate::NotificationIngestOutcome::buffered_fragment(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(20),
            ms(7),
        );
        let ignored = crate::NotificationIngestOutcome::ignored_wrong_channel(
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(20),
            ms(7),
        );

        assert!(matches!(
            buffered,
            crate::NotificationIngestOutcome::BufferedFragment(evidence)
                if evidence.family == Some(crate::ProtocolFamily::VeteranLeaperkimNosfet)
                    && evidence.channel == TEST_CHANNEL
                    && evidence.len == crate::NotificationByteLen::from_bytes(20)
                    && evidence.monotonic_ms == ms(7)
        ));
        assert!(matches!(
            ignored,
            crate::NotificationIngestOutcome::Ignored(evidence)
                if evidence.family.is_none()
                    && evidence.channel == TEST_CHANNEL
                    && evidence.len == crate::NotificationByteLen::from_bytes(20)
                    && evidence.monotonic_ms == ms(7)
        ));
    }

    #[test]
    fn notification_ingest_outcome_carries_known_reserved_payload_evidence() {
        let outcome = crate::NotificationIngestOutcome::known_reserved(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(75),
            ms(12),
            crate::ReservedPayloadEvidence {
                classifier: crate::PayloadClassifier::selector(crate::ProtocolSelector::new(8)),
                body_len: crate::PayloadBodyLen::from_bytes(68),
                verification: VerificationStatus::HardwareVerified,
            },
        );

        assert!(matches!(
            outcome,
            crate::NotificationIngestOutcome::KnownReserved {
                notification,
                payload,
            } if notification.family == Some(crate::ProtocolFamily::VeteranLeaperkimNosfet)
                && notification.channel == TEST_CHANNEL
                && notification.len == crate::NotificationByteLen::from_bytes(75)
                && notification.monotonic_ms == ms(12)
                && payload.classifier.selector_value() == Some(crate::ProtocolSelector::new(8))
                && payload.classifier.tag_value().is_none()
                && payload.body_len == crate::PayloadBodyLen::from_bytes(68)
                && payload.verification == VerificationStatus::HardwareVerified
        ));
    }

    #[test]
    fn notification_ingest_outcome_counts_semantic_events_without_storing_them() {
        let outcome = crate::NotificationIngestOutcome::semantic_events(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(77),
            ms(21),
            crate::SemanticEventCount::from_events(3),
        );

        assert!(matches!(
            outcome,
            crate::NotificationIngestOutcome::SemanticEvents {
                notification,
                event_count,
            } if notification.family == Some(crate::ProtocolFamily::VeteranLeaperkimNosfet)
                && notification.channel == TEST_CHANNEL
                && notification.len == crate::NotificationByteLen::from_bytes(77)
                && notification.monotonic_ms == ms(21)
                && event_count == crate::SemanticEventCount::from_events(3)
        ));
    }

    #[test]
    fn notification_ingest_outcome_carries_parser_diagnostics_without_raw_bytes() {
        let outcome = crate::NotificationIngestOutcome::parser_diagnostic(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(77),
            ms(22),
            crate::ParserError::BadChecksum,
        );

        assert!(matches!(
            outcome,
            crate::NotificationIngestOutcome::ParserDiagnostic {
                notification,
                error: crate::ParserError::BadChecksum,
            } if notification.family == Some(crate::ProtocolFamily::VeteranLeaperkimNosfet)
                && notification.channel == TEST_CHANNEL
        ));
    }

    #[test]
    fn notification_ingest_debug_redacts_raw_bytes() {
        let outcome = crate::NotificationIngestOutcome::parser_gap(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(77),
            ms(15),
            crate::ParserGapEvidence {
                classifier: crate::PayloadClassifier::tag(crate::ProtocolTag::new(0x5c)),
                body_len: crate::PayloadBodyLen::from_bytes(70),
            },
        );
        let debug = format!("{outcome:?}");

        assert!(debug.contains("ParserGap"));
        assert!(debug.contains("body_len"));
        assert!(debug.contains("value: 70"));
        assert!(!debug.contains("dc5a5c"));
        assert!(!debug.contains("bytes"));
    }

    #[derive(Default)]
    struct EchoSession {
        last_notification_len: usize,
        link_is_up: bool,
    }

    impl ProtocolSession for EchoSession {
        fn handle(
            &mut self,
            input: SessionInput<'_>,
            output: &mut dyn SessionOutputSink,
        ) -> Result<(), crate::SessionOutputError> {
            match input {
                SessionInput::LinkUp(info) => {
                    self.link_is_up = true;
                    output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)))?;
                }
                SessionInput::LinkDown => {
                    self.link_is_up = false;
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown))?;
                }
                SessionInput::Notification {
                    bytes,
                    channel,
                    monotonic_ms,
                } => {
                    self.last_notification_len = bytes.len();
                    output.push(SessionOutput::NotificationIngest(
                        crate::NotificationIngestOutcome::ignored_wrong_channel(
                            channel,
                            crate::NotificationByteLen::from_bytes(bytes.len()),
                            monotonic_ms,
                        ),
                    ))?;
                }
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }))?;
                }
                SessionInput::Command(DeviceCommand::RequestTelemetry) => {
                    output.push(SessionOutput::Transport(TransportAction::Write {
                        channel: GattChannel::from_bytes([1; 16]),
                        bytes: WritePayload::try_from_slice(b"telemetry")
                            .expect("test write payload fits"),
                        mode: WriteMode::WithResponse,
                    }))?;
                }
                SessionInput::Command(DeviceCommand::RequestIdentity) => {
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes([2; 16]),
                    }))?;
                }
                SessionInput::Command(
                    DeviceCommand::RequestFirmwareInfo
                    | DeviceCommand::RequestBatteryInfo
                    | DeviceCommand::RequestDiagnostics
                    | DeviceCommand::RequestSettings
                    | DeviceCommand::SetLights(_)
                    | DeviceCommand::SoundHorn
                    | DeviceCommand::SetRawMotorCurrent { .. },
                ) => {}
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct BurstSession;

    impl ProtocolSession for BurstSession {
        fn handle(
            &mut self,
            input: SessionInput<'_>,
            output: &mut dyn SessionOutputSink,
        ) -> Result<(), crate::SessionOutputError> {
            if let SessionInput::LinkUp(info) = input {
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)))?;
                output.push(SessionOutput::Event(DeviceEvent::LinkDown))?;
            }
            Ok(())
        }
    }

    #[test]
    fn drives_a_session_without_runtime_or_ble_stack() {
        let mut session = EchoSession::default();
        let mut output = Vec::new();
        let link = LinkInfo {
            monotonic_ms: ms(10),
            max_write_len: Some(write_len(185)),
        };

        session
            .handle(SessionInput::LinkUp(link), &mut output)
            .expect("Vec-backed test sink accepts link output");

        assert!(session.link_is_up);
        assert_eq!(
            output.as_slice(),
            &[SessionOutput::Event(DeviceEvent::LinkUp(link))]
        );
    }

    #[test]
    fn passes_notification_bytes_through_borrowed_input() {
        let mut session = EchoSession::default();
        let mut output = Vec::new();
        let channel = GattChannel::from_bytes([0xfe; 16]);

        session
            .handle(
                SessionInput::Notification {
                    channel,
                    bytes: &[0xdc, 0x5a, 0x5c],
                    monotonic_ms: ms(20),
                },
                &mut output,
            )
            .expect("Vec-backed test sink accepts notification output");

        assert_eq!(session.last_notification_len, 3);
        assert_eq!(
            output.as_slice(),
            &[SessionOutput::NotificationIngest(
                crate::NotificationIngestOutcome::ignored_wrong_channel(
                    channel,
                    crate::NotificationByteLen::from_bytes(3),
                    ms(20)
                )
            )]
        );
    }

    #[test]
    fn hosts_can_drain_owned_actions_after_each_input() {
        let mut session = EchoSession::default();
        let mut output = Vec::new();

        session
            .handle(
                SessionInput::Command(DeviceCommand::RequestTelemetry),
                &mut output,
            )
            .expect("Vec-backed test sink accepts command output");
        let drained = core::mem::take(&mut output);

        assert!(output.is_empty());
        assert_eq!(
            drained.as_slice(),
            &[SessionOutput::Transport(TransportAction::Write {
                channel: GattChannel::from_bytes([1; 16]),
                bytes: WritePayload::try_from_slice(b"telemetry").expect("test write payload fits"),
                mode: WriteMode::WithResponse,
            })]
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn quantity_conversions_keep_unit_math_in_core() {
        assert_eq!(Speed::from_mph(10).as_millimetres_per_second(), 4_474);
        assert_eq!(Speed::from_millimetres_per_second(4_470).as_mph(), 10);
        assert_eq!(Speed::from_kmh(50).as_kmh_rounded(), 50);
        assert_eq!(
            Speed::from_centimetres_per_second(1_336).as_millimetres_per_second(),
            13_360
        );
        assert_eq!(
            Speed::from_millimetres_per_second(22_222).as_kmh_rounded(),
            80
        );
        assert_eq!(
            Speed::from_millimetres_per_second(15_277).as_deci_kmh_rounded(),
            550
        );
        assert_eq!(
            Speed::from_milli_kmh_scaled(10_000, 1_609_344).as_millimetres_per_second(),
            4_470
        );
        assert_eq!(
            Speed::from_milli_kmh_scaled(-10_000, 1_609_344).as_millimetres_per_second(),
            -4_470
        );
        assert_eq!(
            Speed::from_milli_kmh_scaled(35, 500_000).as_millimetres_per_second(),
            5
        );
        assert_eq!(
            Speed::from_milli_kmh_scaled(-35, 500_000).as_millimetres_per_second(),
            -5
        );
        assert_eq!(
            Speed::from_milli_kmh_scaled(0, 1_609_344).as_millimetres_per_second(),
            0
        );
        assert_eq!(
            Speed::from_milli_kmh_scaled(-10_000, 0).as_millimetres_per_second(),
            0
        );
        assert_eq!(round_div_i32(5, 2), 3);
        assert_eq!(round_div_i32(-5, 2), -3);

        assert_eq!(Voltage::from_volts(126).as_millivolts(), 126_000);
        assert_eq!(Voltage::from_millivolts(84_400).as_whole_volts(), 84);
        assert_eq!(
            <Voltage as crate::QuantityDisplayValue>::display_value(Voltage::from_millivolts(
                84_400,
            )),
            84
        );
        assert_eq!(Voltage::from_deci_volts(915).as_millivolts(), 91_500);
        assert_eq!(
            Current::from_milliamps(-1_700).abs(),
            Current::from_milliamps(1_700)
        );
        assert_eq!(
            Temperature::from_mpu6050_counts(0).as_millicelsius(),
            36_530
        );
        assert_eq!(
            Voltage::from_millivolts(91_000).as_cell_voltage(SeriesCount::new(30)),
            CellVoltage::from_microvolts(3_033_333)
        );
        let voltage_range = Voltage::from_volts(91)..=Voltage::from_volts(126);
        assert_eq!(
            Voltage::from_millivolts(108_500)
                .percent_of_range(&voltage_range)
                .as_percent(),
            50
        );
        assert_eq!(Voltage::from_centivolts(9_150).as_millivolts(), 91_500);
        assert_eq!(
            Voltage::from_cell_voltage(CellVoltage::from_microvolts(3_050_000), 30).as_millivolts(),
            91_500
        );
        assert_eq!(
            Capacity::from_parallel_packs(5_000, ParallelCount::new(2)).as_milliamp_hours(),
            10_000
        );
        assert_eq!(
            Energy::from_cell_geometry(18, SeriesCount::new(20), ParallelCount::new(2))
                .as_watt_hours(),
            720
        );

        assert_eq!(Current::from_amps(-12).as_milliamps(), -12_000);
        assert_eq!(Current::from_centiamps(-1_240).as_milliamps(), -12_400);
        assert_eq!(Current::from_deciamps(-124).as_milliamps(), -12_400);
        assert_eq!(Current::from_milliamps(-12_400).as_whole_amps(), -12);
        assert_eq!(Current::from_milliamps(-12_400).as_abs_whole_amps(), 12);
        assert_eq!(BatteryLevel::from_percent_i32(-1).as_percent(), 0);
        assert_eq!(BatteryLevel::from_percent_i32(120).as_percent(), 100);
        assert_eq!(
            <BatteryLevel as crate::PercentQuantity>::as_percent(BatteryLevel::from_percent(75)),
            75
        );
        assert_eq!(
            <BatteryLevel as crate::QuantityDisplayValue>::display_value(
                BatteryLevel::from_percent(42)
            ),
            42
        );
        assert!((BatteryLevel::from_percent(75).as_ratio() - 0.75).abs() < f64::EPSILON);
        assert_eq!(
            BatteryLevel::interpolate(
                BatteryLevel::from_percent(20),
                BatteryLevel::from_percent(80),
                50,
                0,
                100,
            )
            .as_percent(),
            50
        );
        assert_eq!(
            BatteryLevel::from_piecewise_linear(
                5_440,
                &[
                    (5_120, BatteryLevel::from_percent(0)),
                    (5_440, BatteryLevel::from_percent(9)),
                    (6_680, BatteryLevel::from_percent(100)),
                ],
            )
            .as_percent(),
            9
        );
    }

    #[test]
    fn quantity_conversions_cover_angles_ratios_and_power() {
        assert_eq!(Temperature::from_celsius(36).as_millicelsius(), 36_000);
        assert_eq!(
            Temperature::from_centi_celsius(-3_660).as_millicelsius(),
            -36_600
        );
        assert_eq!(
            Temperature::from_millicelsius(-36_600).as_abs_whole_celsius(),
            37
        );
        assert_eq!(Duration::from_deciseconds(12).as_milliseconds(), 1_200);

        assert_eq!(Angle::from_degrees(69).as_millidegrees(), 69_000);
        assert_eq!(Angle::from_deci_degrees(690).as_millidegrees(), 6_900);
        assert_eq!(Angle::from_millidegrees(69_060).as_whole_degrees(), 69);

        assert_eq!(DutyCycle::from_decipermille(524).as_permille(), 52);
        assert_eq!(DutyCycle::from_centered_pwm(0).as_permille(), -1_000);
        assert_eq!(DutyCycle::from_centered_pwm(0x8000).as_permille(), 0);
        assert_eq!(DutyCycle::from_centered_pwm(u16::MAX).as_permille(), 999);

        assert_eq!(
            Power::from_voltage_current(Voltage::from_volts(53), Current::from_amps(-6)),
            Power::from_watts(-318)
        );
        assert_eq!(
            Power::from_voltage_current(
                Voltage::from_millivolts(i32::MAX),
                Current::from_milliamps(i32::MAX),
            ),
            Power::from_milliwatts(4_611_686_014_132_420)
        );
        assert_eq!(DutyCycle::from_centipercent(755).as_permille(), 75);
    }

    #[test]
    fn whole_unit_constructors_saturate_at_storage_bounds() {
        assert_eq!(Voltage::from_volts(u64::MAX).as_millivolts(), i32::MAX);
        assert_eq!(Current::from_amps(i64::MAX).as_milliamps(), i32::MAX);
        assert_eq!(Current::from_amps(i64::MIN).as_milliamps(), i32::MIN);
        assert_eq!(
            Temperature::from_celsius(i64::MAX).as_millicelsius(),
            i32::MAX
        );
        assert_eq!(Angle::from_degrees(i64::MIN).as_millidegrees(), i32::MIN);
    }

    #[test]
    fn telemetry_delta_updates_only_present_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        let first = TelemetryDelta {
            at_ms: ms(100),
            speed: Some(Measured::reported(Speed::from_millimetres_per_second(
                1_500,
            ))),
            voltage: Some(Measured::reported(Voltage::from_millivolts(81_000))),
            battery_current: Some(Measured::reported(BatteryCurrent::from_milliamps(-2_000))),
            ..TelemetryDelta::empty(ms(100))
        };
        let second = TelemetryDelta {
            at_ms: ms(150),
            motor_temperature: Some(Measured::reported(Temperature::from_millicelsius(42_500))),
            ..TelemetryDelta::empty(ms(150))
        };

        snapshot.apply_delta(first);
        snapshot.apply_delta(second);

        assert_eq!(snapshot.at_ms, Some(ms(150)));
        assert_eq!(
            snapshot.speed,
            Some(Measured::reported(Speed::from_millimetres_per_second(
                1_500
            )))
        );
        assert_eq!(
            snapshot.voltage,
            Some(Measured::reported(Voltage::from_millivolts(81_000)))
        );
        assert_eq!(
            snapshot.motor_temperature,
            Some(Measured::reported(Temperature::from_millicelsius(42_500)))
        );
    }

    #[test]
    fn zero_measurement_is_not_unknown() {
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(TelemetryDelta {
            at_ms: ms(200),
            speed: Some(Measured::reported(Speed::from_millimetres_per_second(0))),
            battery_current: Some(Measured::reported(BatteryCurrent::from_milliamps(0))),
            ..TelemetryDelta::empty(ms(200))
        });

        assert_eq!(
            snapshot.speed,
            Some(Measured::reported(Speed::from_millimetres_per_second(0)))
        );
        assert_eq!(
            snapshot.battery_current,
            Some(Measured::reported(BatteryCurrent::from_milliamps(0)))
        );
        assert_eq!(snapshot.motor_current, None);
    }

    #[test]
    fn duration_quantity_converts_protocol_time_units_to_milliseconds() {
        assert_eq!(Duration::from_milliseconds(750).as_milliseconds(), 750);
        assert_eq!(Duration::from_seconds(11).as_milliseconds(), 11_000);
        assert_eq!(Duration::from_minutes(15).as_milliseconds(), 900_000);
        assert_eq!(Duration::from_minutes(15).as_seconds(), 900);
        assert_eq!(Duration::from_minutes(15).as_minutes(), 15);
    }

    #[test]
    fn battery_quantity_types_preserve_capacity_and_energy_units() {
        assert_eq!(
            Capacity::from_milliamp_hours(10_000).as_milliamp_hours(),
            10_000
        );
        assert_eq!(Energy::from_watt_hours(900).as_watt_hours(), 900);
        assert_eq!(BatteryCurrent::from_milliamps(1_250).as_milliamps(), 1_250);
        assert_eq!(PhaseCurrent::from_amps_f32(-1.25).as_milliamps(), -1_250);
        assert_eq!(PeakCurrent::from_milliamps(1_250).as_milliamps(), 1_250);
        assert_eq!(PeakCurrent::from_amps_f32(-1.25).as_milliamps(), -1_250);
    }

    #[test]
    fn signal_strength_quantity_preserves_dbm_unit() {
        let signal = crate::SignalStrength::from_dbm(-61);
        assert_eq!(signal.as_dbm(), -61);
        assert_eq!(signal.as_quality_percent(), 78);
        assert_eq!(
            <crate::SignalStrength as crate::QuantityDisplayValue>::display_value(signal),
            78
        );
        assert_eq!(
            crate::SignalStrength::from_dbm(-120).as_quality_percent(),
            0
        );
    }

    #[test]
    fn rotational_speed_quantity_preserves_erpm_unit() {
        assert_eq!(crate::RotationalSpeed::from_erpm(4_500).as_erpm(), 4_500);
    }

    #[test]
    fn rotational_speed_quantity_converts_to_linear_speed_with_drive_geometry() {
        let wheel = Distance::from_millimetres(2_100);

        assert_eq!(
            crate::RotationalSpeed::from_erpm(4_500).as_speed(15, 1, wheel),
            Some(Speed::from_millimetres_per_second(10_500))
        );
        assert_eq!(
            crate::RotationalSpeed::from_erpm(4_500).as_speed(15, 2, wheel),
            Some(Speed::from_millimetres_per_second(5_250))
        );
        assert_eq!(
            crate::RotationalSpeed::from_erpm(4_500).as_speed(0, 1, wheel),
            None
        );
    }

    #[test]
    fn tachometer_reading_quantity_preserves_signed_counts() {
        assert_eq!(
            crate::TachometerReading::from_counts(-21_973).as_counts(),
            -21_973
        );
    }

    #[test]
    fn distance_offset_quantity_preserves_signed_length_unit() {
        assert_eq!(
            crate::DistanceOffset::from_metres(805).as_millimetres(),
            805_000
        );
        assert_eq!(
            crate::DistanceOffset::from_metres(-2).as_millimetres(),
            -2_000
        );
    }

    #[test]
    fn measured_constructors_preserve_provenance_and_verification() {
        let reported = Measured::reported(7);
        let calculated = Measured::calculated(11);
        let estimated = Measured::estimated(13);

        assert_eq!(reported.source, ValueSource::Reported);
        assert_eq!(reported.quality, ValueQuality::Known);
        assert_eq!(reported.verification, VerificationStatus::HardwareVerified);

        assert_eq!(calculated.source, ValueSource::Calculated);
        assert_eq!(calculated.quality, ValueQuality::Known);
        assert_eq!(calculated.verification, VerificationStatus::Inferred);

        assert_eq!(estimated.source, ValueSource::Estimated);
        assert_eq!(estimated.quality, ValueQuality::Inferred);
        assert_eq!(estimated.verification, VerificationStatus::Inferred);
    }

    #[test]
    fn telemetry_keeps_distinct_current_temperature_and_estimate_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        let estimated_level = Measured::estimated(BatteryLevel::from_percent(76));

        snapshot.apply_delta(TelemetryDelta {
            at_ms: ms(300),
            battery_current: Some(Measured::reported(BatteryCurrent::from_milliamps(-1_200))),
            motor_current: Some(Measured::reported(PhaseCurrent::from_milliamps(3_400))),
            controller_temperature: Some(Measured::reported(Temperature::from_millicelsius(
                35_000,
            ))),
            motor_temperature: Some(Measured::reported(Temperature::from_millicelsius(45_000))),
            battery_temperature: Some(Measured::reported(Temperature::from_millicelsius(31_000))),
            battery_level_reported: Some(Measured::reported(BatteryLevel::from_percent(80))),
            battery_level_estimated: Some(estimated_level),
            ..TelemetryDelta::empty(ms(300))
        });

        assert_eq!(
            snapshot.battery_current,
            Some(Measured::reported(BatteryCurrent::from_milliamps(-1_200)))
        );
        assert_eq!(
            snapshot.motor_current,
            Some(Measured::reported(PhaseCurrent::from_milliamps(3_400)))
        );
        assert_eq!(
            snapshot.controller_temperature,
            Some(Measured::reported(Temperature::from_millicelsius(35_000)))
        );
        assert_eq!(
            snapshot.motor_temperature,
            Some(Measured::reported(Temperature::from_millicelsius(45_000)))
        );
        assert_eq!(
            snapshot.battery_temperature,
            Some(Measured::reported(Temperature::from_millicelsius(31_000)))
        );
        assert_eq!(
            snapshot.battery_level_reported,
            Some(Measured::reported(BatteryLevel::from_percent(80)))
        );
        assert_eq!(snapshot.battery_level_estimated, Some(estimated_level));
        assert_eq!(
            snapshot
                .battery_level_estimated
                .map(|value| value.verification),
            Some(VerificationStatus::Inferred)
        );
    }

    #[test]
    fn telemetry_delta_can_be_emitted_as_device_event() {
        let delta = TelemetryDelta {
            at_ms: ms(400),
            distance: Some(Measured::reported(Distance::from_millimetres(12_345))),
            ..TelemetryDelta::empty(ms(400))
        };

        assert_eq!(
            DeviceEvent::Telemetry(delta),
            DeviceEvent::Telemetry(TelemetryDelta {
                at_ms: ms(400),
                distance: Some(Measured::reported(Distance::from_millimetres(12_345))),
                ..TelemetryDelta::empty(ms(400))
            })
        );
    }

    #[test]
    fn firmware_response_preserves_version_fields_and_evidence() {
        let response = crate::FirmwareInfo {
            protocol_version: Some(Measured::reported(3)),
            firmware_major: Some(Measured::reported(1)),
            firmware_minor: Some(Measured::reported(14)),
            firmware_patch: None,
            build_id: Some(crate::RawFieldValue::new(0x20, 0x0000_1234)),
        };

        assert_eq!(response.protocol_version, Some(Measured::reported(3)));
        assert_eq!(response.firmware_patch, None);
        assert_eq!(
            response.build_id,
            Some(crate::RawFieldValue::new(0x20, 0x0000_1234))
        );
    }

    #[test]
    fn battery_response_distinguishes_reported_estimated_and_unknown_percent() {
        let battery = crate::BatteryInfo {
            voltage: Some(Measured::reported(Voltage::from_millivolts(80_400))),
            current: Some(Measured::reported(BatteryCurrent::from_milliamps(0))),
            level_reported: Some(Measured::reported(BatteryLevel::from_percent(0))),
            level_estimated: Some(Measured::estimated(BatteryLevel::from_percent(42))),
            temperature: None,
            raw_state: None,
        };
        let response = crate::BatteryPagePayload::Raw(crate::BatteryRawPage::new(
            crate::BatteryPageMetadata::raw(
                crate::ProtocolSelector::new(8),
                VerificationStatus::SourceVerified,
            ),
            battery,
        ));

        assert_eq!(
            response.page(),
            crate::BatteryPageMetadata::raw(
                crate::ProtocolSelector::new(8),
                VerificationStatus::SourceVerified,
            )
        );
        assert_eq!(
            response.battery().current,
            Some(Measured::reported(BatteryCurrent::from_milliamps(0)))
        );
        assert_eq!(
            response.battery().level_reported,
            Some(Measured::reported(BatteryLevel::from_percent(0)))
        );
        assert_eq!(
            response.battery().voltage.map(|value| value.verification),
            Some(VerificationStatus::HardwareVerified)
        );
        assert_eq!(
            response
                .battery()
                .level_estimated
                .map(|value| value.verification),
            Some(VerificationStatus::Inferred)
        );
        assert_eq!(response.battery().temperature, None);
    }

    #[test]
    fn diagnostic_detail_preserves_raw_field_identifier_and_severity() {
        let detail = crate::DiagnosticDetail {
            field: crate::RawFieldValue::new(0x55, -7),
            severity: crate::DiagnosticSeverity::Warning,
            quality: ValueQuality::Inferred,
            verification: VerificationStatus::Inferred,
        };

        assert_eq!(detail.field.id, 0x55);
        assert_eq!(detail.field.value, -7);
        assert_eq!(detail.severity, crate::DiagnosticSeverity::Warning);
        assert_eq!(detail.quality, ValueQuality::Inferred);
        assert_eq!(detail.verification, VerificationStatus::Inferred);
    }

    #[test]
    fn diagnostic_readback_reports_over_capacity_details() {
        let detail = crate::DiagnosticDetail {
            field: crate::RawFieldValue::new(0x55, -7),
            severity: crate::DiagnosticSeverity::Warning,
            quality: ValueQuality::Inferred,
            verification: VerificationStatus::Inferred,
        };
        let details = [detail; crate::DIAGNOSTIC_READBACK_CAPACITY];
        let readback = crate::DiagnosticReadback::try_from_details(&details)
            .expect("exact diagnostic capacity is accepted");

        assert_eq!(readback.details, details.map(Some));
        assert_eq!(
            crate::DiagnosticReadback::try_from_details(&[
                details[0], details[1], details[2], details[3], details[0],
            ]),
            Err(crate::ReadbackCapacityError::TooManyItems {
                capacity: crate::DIAGNOSTIC_READBACK_CAPACITY,
                requested: crate::DIAGNOSTIC_READBACK_CAPACITY + 1,
            })
        );
    }

    #[test]
    fn settings_readback_entry_carries_numeric_values_without_writes() {
        let entry = crate::SettingsEntry {
            field: crate::RawFieldValue::new(0x10, 2),
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::HardwareVerified,
        };
        let response = crate::SettingsReadback {
            entries: [Some(entry), None, None, None],
        };

        assert_eq!(response.entries[0], Some(entry));
        assert_eq!(response.entries[1], None);
        assert_eq!(
            response.entries[0].map(|entry| entry.verification),
            Some(VerificationStatus::HardwareVerified)
        );
    }

    #[test]
    fn settings_readback_reports_over_capacity_entries() {
        let entry = crate::SettingsEntry {
            field: crate::RawFieldValue::new(0x10, 2),
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::HardwareVerified,
        };
        let entries = [entry; crate::SETTINGS_READBACK_CAPACITY];
        let readback = crate::SettingsReadback::try_from_entries(&entries)
            .expect("exact settings capacity is accepted");

        assert_eq!(readback.entries, entries.map(Some));
        assert_eq!(
            crate::SettingsReadback::try_from_entries(&[
                entries[0], entries[1], entries[2], entries[3], entries[0],
            ]),
            Err(crate::ReadbackCapacityError::TooManyItems {
                capacity: crate::SETTINGS_READBACK_CAPACITY,
                requested: crate::SETTINGS_READBACK_CAPACITY + 1,
            })
        );
    }

    #[test]
    fn registry_entry_represents_capture_backed_aero_metadata() {
        const AERO_GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
            service: GattChannel::from_bytes([
                0x00, 0x00, 0xff, 0xe0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]),
            characteristic: GattChannel::from_bytes([
                0x00, 0x00, 0xff, 0xe1, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]),
            roles: crate::GattRoles::empty()
                .with_read()
                .with_write()
                .with_write_without_response()
                .with_notify(),
            verification: VerificationStatus::HardwareVerified,
        }];
        const AERO_BMS_SELECTORS: [crate::BmsPageSelectorSpec; 2] = [
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(0),
                kind: crate::BatteryPageKind::Metadata,
                verification: VerificationStatus::HardwareVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(1),
                kind: crate::BatteryPageKind::CellVoltage,
                verification: VerificationStatus::HardwareVerified,
            },
        ];
        let entry = crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new("NOSFET"),
            model: crate::ModelKey::new("Aero"),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &["NF2557"],
            wire_model_id: Some(crate::VerifiedValue {
                value: 43_u16,
                verification: VerificationStatus::HardwareVerified,
            }),
            battery: Some(crate::BatterySpec {
                series_cells: SeriesCount::new(30),
                nominal_capacity: Some(Capacity::from_milliamp_hours(10_000)),
                voltage_range: Voltage::from_millivolts(99_180)..=Voltage::from_millivolts(123_370),
                verification: VerificationStatus::SourceAndHardwareVerified,
            }),
            bms: Some(crate::BmsLayoutSpec {
                series_cells: SeriesCount::new(30),
                parallel_packs: ParallelCount::new(2),
                cell_values_per_page: crate::BmsCellValuesPerPage::new(15),
                temperature_values_per_page: crate::BmsTemperatureValuesPerPage::new(6),
                selectors: &AERO_BMS_SELECTORS,
                verification: VerificationStatus::HardwareVerified,
            }),
            gatt: &AERO_GATT,
            capabilities: crate::Capabilities::from_supported_commands([
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::RequestTelemetry,
                crate::CommandKind::RequestFirmwareInfo,
                crate::CommandKind::RequestBatteryInfo,
                crate::CommandKind::RequestDiagnostics,
            ]),
            verification: VerificationStatus::HardwareVerified,
        };

        assert_eq!(entry.manufacturer, "NOSFET");
        assert_eq!(
            entry.protocol_family,
            crate::ProtocolFamily::VeteranLeaperkimNosfet
        );
        assert!(
            entry
                .capabilities
                .supports_command_kind(crate::CommandKind::RequestTelemetry)
        );
        assert!(
            !entry
                .capabilities
                .supports_command_kind(crate::CommandKind::SetRawMotorCurrent)
        );
        assert_eq!(entry.wire_model_id.map(|model_id| model_id.value), Some(43));
        assert!(entry.gatt[0].roles.supports_read());
        assert!(entry.gatt[0].roles.supports_write());
        assert!(entry.gatt[0].roles.supports_write_without_response());
        assert!(entry.gatt[0].roles.supports_notify());
        assert!(!entry.gatt[0].roles.supports_indicate());
        let bms = entry
            .bms
            .expect("Aero registry entry should carry BMS layout");
        assert_eq!(bms.series_cells, SeriesCount::new(30));
        assert_eq!(bms.parallel_packs, ParallelCount::new(2));
        assert_eq!(bms.selectors[1].kind, crate::BatteryPageKind::CellVoltage);
    }

    #[test]
    fn registry_hash_is_stable_for_same_entries() {
        let entry = sample_registry_entry("NOSFET", "Aero");

        assert_eq!(
            crate::registry_entries_hash(&[&entry]),
            crate::registry_entries_hash(&[&entry])
        );
    }

    #[test]
    fn registry_hash_changes_when_entry_metadata_changes() {
        let aero = sample_registry_entry("NOSFET", "Aero");
        let aeon = sample_registry_entry("NOSFET", "Aeon");

        assert_ne!(
            crate::registry_entries_hash(&[&aero]),
            crate::registry_entries_hash(&[&aeon])
        );
    }

    #[test]
    fn registry_hash_changes_when_bms_layout_changes() {
        let without_bms = sample_registry_entry("NOSFET", "Aero");
        let with_bms = sample_registry_entry_with_bms("NOSFET", "Aero", 30, 2);

        assert_ne!(
            crate::registry_entries_hash(&[&without_bms]),
            crate::registry_entries_hash(&[&with_bms])
        );
    }

    #[test]
    fn registry_validation_accepts_well_formed_entries() {
        let aero = sample_registry_entry("NOSFET", "Aero");
        let falcon = sample_registry_entry("Begode", "Falcon");

        assert_eq!(crate::validate_registry_entries(&[&aero, &falcon]), Ok(()));
    }

    #[test]
    fn registry_validation_rejects_empty_manufacturer_or_model() {
        let empty_manufacturer = sample_registry_entry("", "Aero");
        let empty_model = sample_registry_entry("NOSFET", "");

        assert_eq!(
            crate::validate_registry_entries(&[&empty_manufacturer]),
            Err(crate::RegistryValidationError::EmptyManufacturer { index: 0 })
        );
        assert_eq!(
            crate::validate_registry_entries(&[&empty_model]),
            Err(crate::RegistryValidationError::EmptyModel { index: 0 })
        );
    }

    #[test]
    fn registry_validation_rejects_duplicate_model_keys() {
        let first = sample_registry_entry("NOSFET", "Aero");
        let duplicate = sample_registry_entry("NOSFET", "Aero");

        assert_eq!(
            crate::validate_registry_entries(&[&first, &duplicate]),
            Err(crate::RegistryValidationError::DuplicateModel {
                index: 1,
                first_index: 0,
            })
        );
    }

    #[test]
    fn registry_validation_rejects_conflicting_wire_model_claims() {
        let mut first = sample_registry_entry("NOSFET", "Aero");
        first.wire_model_id = Some(crate::VerifiedValue {
            value: 43,
            verification: VerificationStatus::HardwareVerified,
        });
        let mut duplicate_wire_id = sample_registry_entry("NOSFET", "Aero Pro");
        duplicate_wire_id.wire_model_id = Some(crate::VerifiedValue {
            value: 43,
            verification: VerificationStatus::Inferred,
        });

        assert_eq!(
            crate::validate_registry_entries(&[&first, &duplicate_wire_id]),
            Err(crate::RegistryValidationError::ConflictingWireModelId {
                index: 1,
                first_index: 0,
            })
        );
    }

    #[test]
    fn registry_validation_rejects_missing_gatt_fingerprint() {
        let mut entry = sample_registry_entry("NOSFET", "Aero");
        entry.gatt = &[];

        assert_eq!(
            crate::validate_registry_entries(&[&entry]),
            Err(crate::RegistryValidationError::MissingGattFingerprint { index: 0 })
        );
    }

    #[test]
    fn registry_validation_rejects_empty_capabilities() {
        let mut entry = sample_registry_entry("NOSFET", "Aero");
        entry.capabilities = crate::Capabilities::default();

        assert_eq!(
            crate::validate_registry_entries(&[&entry]),
            Err(crate::RegistryValidationError::EmptyCapabilities { index: 0 })
        );
    }

    #[test]
    fn model_authoring_emits_static_registry_and_catalog_entries() {
        const AUTHORING: crate::CompleteModelAuthoring = crate::ModelAuthoring::new()
            .manufacturer(crate::ManufacturerKey::new("TypedCo"))
            .model(crate::ModelKey::new("Typed Model"))
            .family(crate::FamilyKey::new(
                crate::ProtocolFamily::VeteranLeaperkimNosfet,
            ))
            .advertised_name_hints(&["TypedHint"])
            .gatt(&STATIC_SAMPLE_GATT)
            .capabilities(crate::Capabilities::from_supported_commands([
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::RequestTelemetry,
            ]))
            .verification(VerificationStatus::Inferred)
            .active_runtime(
                crate::ParserKey::new("typed-parser"),
                crate::SessionKey::new("typed-session"),
            );
        static AUTHORED_MODEL: crate::ModelRegistryEntry = AUTHORING.registry_entry();
        const CATALOG: [crate::ModelCatalogEntry; 1] = [AUTHORING.catalog_entry(&AUTHORED_MODEL)];

        assert_eq!(crate::validate_model_catalog(&CATALOG), Ok(()));
        assert_eq!(
            crate::ModelCatalog::new(&CATALOG)
                .find_model(
                    crate::ManufacturerKey::new("TypedCo"),
                    crate::ModelKey::new("Typed Model")
                )
                .map(|entry| entry.registration.parser),
            Some(Some(crate::ParserKey::new("typed-parser")))
        );
    }

    #[test]
    fn catalog_entry_exposes_typed_keys_for_common_model_path() {
        let catalog = crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        };

        assert_eq!(catalog.manufacturer_key().as_str(), "NOSFET");
        assert_eq!(catalog.model_key().as_str(), "Aero");
        assert_eq!(
            catalog.family_key().protocol_family(),
            crate::ProtocolFamily::VeteranLeaperkimNosfet
        );
        assert_eq!(crate::validate_model_catalog(&[catalog]), Ok(()));
    }

    #[test]
    fn catalog_validation_rejects_missing_active_registrations() {
        let missing_parser = crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: None,
                session: Some(crate::SessionKey::new("test-session")),
            },
        };
        let missing_session = crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: None,
            },
        };

        assert_eq!(
            crate::validate_model_catalog(&[missing_parser]),
            Err(crate::RegistryValidationError::MissingParserRegistration { index: 0 })
        );
        assert_eq!(
            crate::validate_model_catalog(&[missing_session]),
            Err(crate::RegistryValidationError::MissingSessionRegistration { index: 0 })
        );
    }

    #[test]
    fn catalog_lookup_uses_typed_keys_over_static_entries() {
        const CATALOG: [crate::ModelCatalogEntry; 1] = [crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        }];
        let catalog = crate::ModelCatalog::new(&CATALOG);

        assert_eq!(
            catalog
                .find_model(
                    crate::ManufacturerKey::new("NOSFET"),
                    crate::ModelKey::new("Aero")
                )
                .map(|entry| entry.registry.model),
            Some(crate::ModelKey::new("Aero"))
        );
        assert_eq!(
            catalog
                .family_entries(crate::FamilyKey::new(
                    crate::ProtocolFamily::VeteranLeaperkimNosfet
                ))
                .count(),
            1
        );
    }

    #[test]
    fn catalog_lookup_finds_registered_parser_and_session_keys() {
        const CATALOG: [crate::ModelCatalogEntry; 1] = [crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        }];
        let catalog = crate::ModelCatalog::new(&CATALOG);

        assert_eq!(
            catalog
                .find_parser(crate::ParserKey::new("test-parser"))
                .map(|entry| entry.model_key()),
            Some(crate::ModelKey::new("Aero"))
        );
        assert_eq!(
            catalog
                .find_session(crate::SessionKey::new("test-session"))
                .map(|entry| entry.model_key()),
            Some(crate::ModelKey::new("Aero"))
        );
        assert!(
            catalog
                .find_parser(crate::ParserKey::new("missing"))
                .is_none()
        );
        assert!(
            catalog
                .find_session(crate::SessionKey::new("missing"))
                .is_none()
        );
    }

    #[test]
    fn catalog_resolves_borrowed_display_model_without_allocating_keys() {
        let entries = [crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        }];
        let catalog = crate::ModelCatalog::new(&entries);

        assert_eq!(
            catalog
                .find_model_names("NOSFET", "Aero")
                .map(|entry| entry.model_key()),
            Some(crate::ModelKey::new("Aero"))
        );
        let crate::CatalogModelResolution::Matched(entry) = catalog
            .resolve_display_model(crate::ProtocolFamily::VeteranLeaperkimNosfet, "NOSFET Aero")
        else {
            panic!("display model should resolve");
        };
        assert_eq!(entry.model_key(), crate::ModelKey::new("Aero"));

        assert!(matches!(
            catalog.resolve_display_model(crate::ProtocolFamily::BegodeGotway, "NOSFET Aero"),
            crate::CatalogModelResolution::NoMatch
        ));
    }

    #[test]
    fn catalog_resolves_advertised_name_hints_without_allocating_keys() {
        let entries = [crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        }];
        let catalog = crate::ModelCatalog::new(&entries);

        let crate::CatalogModelResolution::Matched(entry) =
            catalog.resolve_advertised_name("Aero NF2557")
        else {
            panic!("advertised name should resolve through registry hints");
        };

        assert_eq!(
            entry.manufacturer_key(),
            crate::ManufacturerKey::new("NOSFET")
        );
        assert_eq!(entry.model_key(), crate::ModelKey::new("Aero"));
        assert!(matches!(
            catalog.resolve_advertised_name("mystery device"),
            crate::CatalogModelResolution::NoMatch
        ));
    }

    #[test]
    fn catalog_display_model_resolution_reports_ambiguity() {
        static OTHER_AERO_REGISTRY_ENTRY: crate::ModelRegistryEntry = crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new("Other"),
            model: crate::ModelKey::new("Aero"),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &[],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: STATIC_AERO_REGISTRY_ENTRY.gatt,
            capabilities: STATIC_AERO_REGISTRY_ENTRY.capabilities,
            verification: VerificationStatus::Inferred,
        };
        let entries = [
            crate::ModelCatalogEntry {
                registry: &STATIC_AERO_REGISTRY_ENTRY,
                registration: crate::ModelRuntimeRegistration {
                    parser: Some(crate::ParserKey::new("test-parser")),
                    session: Some(crate::SessionKey::new("test-session")),
                },
            },
            crate::ModelCatalogEntry {
                registry: &OTHER_AERO_REGISTRY_ENTRY,
                registration: crate::ModelRuntimeRegistration {
                    parser: Some(crate::ParserKey::new("other-parser")),
                    session: Some(crate::SessionKey::new("other-session")),
                },
            },
        ];
        let catalog = crate::ModelCatalog::new(&entries);

        assert!(matches!(
            catalog.resolve_display_model(crate::ProtocolFamily::VeteranLeaperkimNosfet, "Aero"),
            crate::CatalogModelResolution::Ambiguous
        ));
    }

    #[test]
    fn catalog_advertised_name_resolution_reports_ambiguity() {
        static OTHER_AERO_REGISTRY_ENTRY: crate::ModelRegistryEntry = crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new("Other"),
            model: crate::ModelKey::new("Shared"),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &["NF2557"],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: STATIC_AERO_REGISTRY_ENTRY.gatt,
            capabilities: STATIC_AERO_REGISTRY_ENTRY.capabilities,
            verification: VerificationStatus::Inferred,
        };
        let entries = [
            crate::ModelCatalogEntry {
                registry: &STATIC_AERO_REGISTRY_ENTRY,
                registration: crate::ModelRuntimeRegistration {
                    parser: Some(crate::ParserKey::new("test-parser")),
                    session: Some(crate::SessionKey::new("test-session")),
                },
            },
            crate::ModelCatalogEntry {
                registry: &OTHER_AERO_REGISTRY_ENTRY,
                registration: crate::ModelRuntimeRegistration {
                    parser: Some(crate::ParserKey::new("other-parser")),
                    session: Some(crate::SessionKey::new("other-session")),
                },
            },
        ];
        let catalog = crate::ModelCatalog::new(&entries);

        assert!(matches!(
            catalog.resolve_advertised_name("NF2557"),
            crate::CatalogModelResolution::Ambiguous
        ));
    }

    #[test]
    fn synthetic_catalog_scales_to_one_thousand_models() {
        const MODEL_COUNT: usize = 1_000;
        let entries: Vec<_> = (0..MODEL_COUNT).map(synthetic_catalog_entry).collect();
        let registry_entries: Vec<_> = entries.iter().map(|entry| entry.registry).collect();
        let catalog = crate::ModelCatalog::new(&entries);

        assert_eq!(crate::validate_registry_entries(&registry_entries), Ok(()));
        assert_eq!(crate::validate_model_catalog(&entries), Ok(()));
        assert_eq!(
            catalog
                .find_model_names("Synthetic", "Model0999")
                .map(|entry| entry.registration.session),
            Some(Some(crate::SessionKey::new("synthetic-session-0999")))
        );
        let crate::CatalogModelResolution::Matched(entry) =
            catalog.resolve_advertised_name("PEV-0999")
        else {
            panic!("unique synthetic advertised hint should resolve");
        };
        assert_eq!(entry.registry.model, "Model0999");
    }

    #[test]
    fn catalog_ambiguity_is_independent_of_registration_order() {
        let mut forward = vec![
            synthetic_catalog_entry_with_hint(1, "shared-hint"),
            synthetic_catalog_entry_with_hint(2, "shared-hint"),
        ];
        let mut reversed = forward.clone();
        reversed.reverse();

        assert!(matches!(
            crate::ModelCatalog::new(&forward).resolve_advertised_name("shared-hint"),
            crate::CatalogModelResolution::Ambiguous
        ));
        assert!(matches!(
            crate::ModelCatalog::new(&reversed).resolve_advertised_name("shared-hint"),
            crate::CatalogModelResolution::Ambiguous
        ));

        forward.reverse();
        assert_eq!(
            crate::validate_model_catalog(&forward),
            Ok(()),
            "shared advertised hints are ambiguous identity evidence, not invalid metadata"
        );
    }

    #[test]
    fn registry_validation_rejects_invalid_gatt_fingerprints() {
        const INVALID_GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
            service: GattChannel::from_bytes([0x11; 16]),
            characteristic: GattChannel::from_bytes([0x22; 16]),
            roles: crate::GattRoles::empty(),
            verification: VerificationStatus::SourceVerified,
        }];
        let mut entry = sample_registry_entry("NOSFET", "Aero");
        entry.gatt = &INVALID_GATT;

        assert_eq!(
            crate::validate_registry_entries(&[&entry]),
            Err(crate::RegistryValidationError::InvalidGattFingerprint {
                index: 0,
                fingerprint_index: 0,
            })
        );
    }

    #[test]
    fn registry_validation_rejects_equal_gatt_service_and_characteristic() {
        const INVALID_GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
            service: GattChannel::from_bytes([0x11; 16]),
            characteristic: GattChannel::from_bytes([0x11; 16]),
            roles: crate::GattRoles::empty()
                .with_write_without_response()
                .with_notify(),
            verification: VerificationStatus::SourceVerified,
        }];
        let mut entry = sample_registry_entry("NOSFET", "Aero");
        entry.gatt = &INVALID_GATT;

        assert_eq!(
            crate::validate_registry_entries(&[&entry]),
            Err(
                crate::RegistryValidationError::EqualGattServiceAndCharacteristic {
                    index: 0,
                    fingerprint_index: 0,
                }
            )
        );
    }

    #[test]
    fn bms_layout_spec_preserves_static_selector_map() {
        const SELECTORS: [crate::BmsPageSelectorSpec; 4] = [
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(0),
                kind: crate::BatteryPageKind::Metadata,
                verification: VerificationStatus::HardwareVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(1),
                kind: crate::BatteryPageKind::CellVoltage,
                verification: VerificationStatus::HardwareVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(3),
                kind: crate::BatteryPageKind::Raw,
                verification: VerificationStatus::SourceVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(8),
                kind: crate::BatteryPageKind::Raw,
                verification: VerificationStatus::SourceVerified,
            },
        ];
        let layout = crate::BmsLayoutSpec {
            series_cells: SeriesCount::new(30),
            parallel_packs: ParallelCount::new(2),
            cell_values_per_page: crate::BmsCellValuesPerPage::new(15),
            temperature_values_per_page: crate::BmsTemperatureValuesPerPage::new(6),
            selectors: &SELECTORS,
            verification: VerificationStatus::HardwareVerified,
        };

        assert_eq!(layout.selectors.len(), 4);
        assert_eq!(
            layout.selectors[2].selector,
            crate::ProtocolSelector::new(3)
        );
        assert_eq!(layout.selectors[2].kind, crate::BatteryPageKind::Raw);
        assert_eq!(
            layout.selectors[2].verification,
            VerificationStatus::SourceVerified
        );
    }

    #[test]
    fn installed_device_identity_uses_core_bluetooth_id_as_opaque_primary_key() {
        const GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
            service: GattChannel::from_bytes([
                0x00, 0x00, 0xff, 0xe0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]),
            characteristic: GattChannel::from_bytes([
                0x00, 0x00, 0xff, 0xe1, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]),
            roles: crate::GattRoles::empty()
                .with_write_without_response()
                .with_notify(),
            verification: VerificationStatus::HardwareVerified,
        }];
        let identity = crate::InstalledDeviceIdentity {
            platform_id: crate::InstalledDevicePlatformId {
                platform: crate::InstalledDevicePlatform::CoreBluetooth,
                value: "8de871ff-6aa1-a767-34dd-608e584b610e",
            },
            protocol_serial: Some(crate::VerifiedValue {
                value: "NF2557",
                verification: VerificationStatus::HardwareVerified,
            }),
            user_alias: Some("shop Aero"),
            resolved_model: Some(crate::InstalledDeviceModel {
                manufacturer: "NOSFET",
                model: "Aero",
                protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
                verification: VerificationStatus::HardwareVerified,
            }),
            gatt_fingerprints: &GATT,
        };

        assert_eq!(
            identity.platform_id.platform,
            crate::InstalledDevicePlatform::CoreBluetooth
        );
        assert_eq!(
            identity.platform_id.value,
            "8de871ff-6aa1-a767-34dd-608e584b610e"
        );
        assert_eq!(
            identity.protocol_serial.map(|serial| serial.value),
            Some("NF2557")
        );
        assert_eq!(identity.user_alias, Some("shop Aero"));
        assert_eq!(
            identity
                .resolved_model
                .map(|model| (model.manufacturer, model.model)),
            Some(("NOSFET", "Aero"))
        );
        assert!(identity.gatt_fingerprints[0].roles.supports_notify());
    }

    #[test]
    fn installed_device_identity_treats_android_identifier_as_platform_scoped_opaque_value() {
        let identity = crate::InstalledDeviceIdentity {
            platform_id: crate::InstalledDevicePlatformId {
                platform: crate::InstalledDevicePlatform::Android,
                value: "00:00:00:00:00:00",
            },
            protocol_serial: None,
            user_alias: None,
            resolved_model: Some(crate::InstalledDeviceModel {
                manufacturer: "Begode",
                model: "Falcon",
                protocol_family: crate::ProtocolFamily::BegodeGotway,
                verification: VerificationStatus::Inferred,
            }),
            gatt_fingerprints: &[],
        };

        assert_eq!(
            identity.platform_id.platform,
            crate::InstalledDevicePlatform::Android
        );
        assert_eq!(identity.platform_id.value, "00:00:00:00:00:00");
        assert_eq!(identity.protocol_serial, None);
        assert_eq!(identity.user_alias, None);
        assert_eq!(identity.gatt_fingerprints, &[]);
        assert_eq!(
            identity
                .resolved_model
                .map(|model| (model.protocol_family, model.verification)),
            Some((
                crate::ProtocolFamily::BegodeGotway,
                VerificationStatus::Inferred
            ))
        );
    }

    fn sample_registry_entry(
        manufacturer: &'static str,
        model: &'static str,
    ) -> crate::ModelRegistryEntry {
        const SAMPLE_GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
            service: GattChannel::from_bytes([0x11; 16]),
            characteristic: GattChannel::from_bytes([0x22; 16]),
            roles: crate::GattRoles::empty()
                .with_write_without_response()
                .with_notify(),
            verification: VerificationStatus::SourceVerified,
        }];

        crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new(manufacturer),
            model: crate::ModelKey::new(model),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &["NF"],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: &SAMPLE_GATT,
            capabilities: crate::Capabilities::from_supported_commands([
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::RequestTelemetry,
            ]),
            verification: VerificationStatus::Inferred,
        }
    }

    fn sample_registry_entry_with_bms(
        manufacturer: &'static str,
        model: &'static str,
        series_cells: u8,
        parallel_packs: u8,
    ) -> crate::ModelRegistryEntry {
        const SELECTORS: [crate::BmsPageSelectorSpec; 1] = [crate::BmsPageSelectorSpec {
            selector: crate::ProtocolSelector::new(1),
            kind: crate::BatteryPageKind::CellVoltage,
            verification: VerificationStatus::SourceVerified,
        }];
        let mut entry = sample_registry_entry(manufacturer, model);
        entry.bms = Some(crate::BmsLayoutSpec {
            series_cells: SeriesCount::new(series_cells),
            parallel_packs: ParallelCount::new(parallel_packs),
            cell_values_per_page: crate::BmsCellValuesPerPage::new(15),
            temperature_values_per_page: crate::BmsTemperatureValuesPerPage::new(6),
            selectors: &SELECTORS,
            verification: VerificationStatus::Inferred,
        });
        entry
    }

    fn synthetic_catalog_entry(index: usize) -> crate::ModelCatalogEntry {
        let hint = leak_static_str(format!("PEV-{index:04}"));
        synthetic_catalog_entry_with_hint(index, hint)
    }

    fn synthetic_catalog_entry_with_hint(
        index: usize,
        hint: &'static str,
    ) -> crate::ModelCatalogEntry {
        let model = leak_static_str(format!("Model{index:04}"));
        let hints = Box::leak(Box::new([hint]));
        let registry = Box::leak(Box::new(crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new("Synthetic"),
            model: crate::ModelKey::new(model),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: hints,
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: &STATIC_SAMPLE_GATT,
            capabilities: crate::Capabilities::from_supported_commands([
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::RequestTelemetry,
            ]),
            verification: VerificationStatus::Inferred,
        }));

        crate::ModelCatalogEntry {
            registry,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new(leak_static_str(format!(
                    "synthetic-parser-{index:04}"
                )))),
                session: Some(crate::SessionKey::new(leak_static_str(format!(
                    "synthetic-session-{index:04}"
                )))),
            },
        }
    }

    fn leak_static_str(value: String) -> &'static str {
        Box::leak(value.into_boxed_str())
    }

    const STATIC_SAMPLE_GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
        service: GattChannel::from_bytes([0x11; 16]),
        characteristic: GattChannel::from_bytes([0x22; 16]),
        roles: crate::GattRoles::empty()
            .with_write_without_response()
            .with_notify(),
        verification: VerificationStatus::SourceVerified,
    }];

    static STATIC_AERO_REGISTRY_ENTRY: crate::ModelRegistryEntry = crate::ModelRegistryEntry {
        manufacturer: crate::ManufacturerKey::new("NOSFET"),
        model: crate::ModelKey::new("Aero"),
        protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
        advertised_name_hints: &["NF"],
        wire_model_id: None,
        battery: None,
        bms: None,
        gatt: &STATIC_SAMPLE_GATT,
        capabilities: crate::Capabilities::from_supported_commands([
            crate::CommandKind::RequestIdentity,
            crate::CommandKind::RequestTelemetry,
        ]),
        verification: VerificationStatus::Inferred,
    };

    #[test]
    fn read_only_response_reports_matching_command_kind() {
        let firmware = crate::ReadOnlyResponse::Firmware(crate::FirmwareInfo::default());
        let battery = crate::ReadOnlyResponse::Battery(crate::BatteryPagePayload::Raw(
            crate::BatteryRawPage::new(
                crate::BatteryPageMetadata::raw(
                    crate::ProtocolSelector::new(8),
                    VerificationStatus::SourceVerified,
                ),
                crate::BatteryInfo::default(),
            ),
        ));
        let diagnostics = crate::ReadOnlyResponse::Diagnostics(crate::DiagnosticReadback {
            details: [None, None, None, None],
        });
        let settings = crate::ReadOnlyResponse::Settings(crate::SettingsReadback {
            entries: [None, None, None, None],
        });

        assert_eq!(
            firmware.command_kind(),
            crate::CommandKind::RequestFirmwareInfo
        );
        assert_eq!(
            battery.command_kind(),
            crate::CommandKind::RequestBatteryInfo
        );
        assert_eq!(
            diagnostics.command_kind(),
            crate::CommandKind::RequestDiagnostics
        );
        assert_eq!(settings.command_kind(), crate::CommandKind::RequestSettings);
    }

    #[test]
    fn read_only_response_can_be_emitted_as_device_event() {
        let firmware = crate::FirmwareInfo {
            firmware_major: Some(Measured::reported(43)),
            ..crate::FirmwareInfo::default()
        };

        assert_eq!(
            DeviceEvent::ReadOnlyResponse(crate::ReadOnlyResponse::Firmware(firmware)),
            DeviceEvent::ReadOnlyResponse(crate::ReadOnlyResponse::Firmware(crate::FirmwareInfo {
                firmware_major: Some(Measured::reported(43)),
                ..crate::FirmwareInfo::default()
            }))
        );
    }

    #[test]
    fn read_only_commands_have_queryable_metadata() {
        let command = DeviceCommand::RequestTelemetry;
        let metadata = command.metadata();

        assert_eq!(metadata.kind, command.kind());
        assert_eq!(metadata.safety_class, command.safety_class());
        assert_eq!(metadata.kind, crate::CommandKind::RequestTelemetry);
        assert_eq!(metadata.safety_class, crate::SafetyClass::ReadOnly);
    }

    #[test]
    fn read_only_probe_commands_have_distinct_metadata() {
        let probes = [
            (
                DeviceCommand::RequestFirmwareInfo,
                crate::CommandKind::RequestFirmwareInfo,
            ),
            (
                DeviceCommand::RequestBatteryInfo,
                crate::CommandKind::RequestBatteryInfo,
            ),
            (
                DeviceCommand::RequestDiagnostics,
                crate::CommandKind::RequestDiagnostics,
            ),
            (
                DeviceCommand::RequestSettings,
                crate::CommandKind::RequestSettings,
            ),
        ];

        for (command, kind) in probes {
            assert_eq!(
                command.metadata(),
                crate::CommandMetadata {
                    kind,
                    safety_class: crate::SafetyClass::ReadOnly,
                }
            );
        }
    }

    #[test]
    fn capabilities_accept_new_read_only_probe_commands() {
        let capabilities = crate::Capabilities::from_supported_commands([
            crate::CommandKind::RequestFirmwareInfo,
            crate::CommandKind::RequestBatteryInfo,
            crate::CommandKind::RequestDiagnostics,
            crate::CommandKind::RequestSettings,
        ]);

        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestFirmwareInfo),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestFirmwareInfo,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestBatteryInfo),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestBatteryInfo,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestDiagnostics),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestDiagnostics,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestSettings),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestSettings,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
    }

    #[test]
    fn read_only_probe_request_keys_are_distinct() {
        let keys = [
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestKey::new(crate::CommandKind::RequestFirmwareInfo),
            crate::RequestKey::new(crate::CommandKind::RequestBatteryInfo),
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestKey::new(crate::CommandKind::RequestSettings),
        ];

        for (index, key) in keys.into_iter().enumerate() {
            assert!(!keys[index + 1..].contains(&key));
        }
    }

    #[test]
    fn request_key_preserves_optional_transport_target() {
        let local = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let can = crate::RequestKey::for_target(
            crate::CommandKind::RequestTelemetry,
            crate::RequestTarget::VescCanController {
                controller_id: crate::VescControllerId::new(42),
            },
        );

        assert_eq!(local.command, crate::CommandKind::RequestTelemetry);
        assert_eq!(local.target, crate::RequestTarget::Local);
        assert_eq!(can.command, crate::CommandKind::RequestTelemetry);
        assert_eq!(
            can.target,
            crate::RequestTarget::VescCanController {
                controller_id: crate::VescControllerId::new(42),
            }
        );
        assert_ne!(local, can);
    }

    #[test]
    fn benign_controls_are_distinct_from_read_only_requests() {
        let lights = DeviceCommand::SetLights(crate::LightState::On);
        let horn = DeviceCommand::SoundHorn;

        assert_eq!(lights.kind(), crate::CommandKind::SetLights);
        assert_eq!(horn.kind(), crate::CommandKind::SoundHorn);
        assert_eq!(lights.safety_class(), crate::SafetyClass::BenignControl);
        assert_eq!(horn.safety_class(), crate::SafetyClass::BenignControl);
    }

    #[test]
    fn command_safety_classes_match_control_matrix() {
        let matrix = [
            (
                crate::SafetyClass::ReadOnly,
                &[
                    crate::CommandKind::RequestIdentity,
                    crate::CommandKind::RequestTelemetry,
                    crate::CommandKind::RequestFirmwareInfo,
                    crate::CommandKind::RequestBatteryInfo,
                    crate::CommandKind::RequestDiagnostics,
                    crate::CommandKind::RequestSettings,
                ][..],
            ),
            (
                crate::SafetyClass::BenignControl,
                &[crate::CommandKind::SetLights, crate::CommandKind::SoundHorn][..],
            ),
            (
                crate::SafetyClass::Actuation,
                &[crate::CommandKind::SetRawMotorCurrent][..],
            ),
        ];

        for (safety_class, commands) in matrix {
            for command in commands {
                assert_eq!(command.safety_class(), safety_class);
            }
        }
    }

    #[test]
    fn actuation_commands_are_not_supported_without_capability() {
        let capabilities = crate::Capabilities::default();
        let command = DeviceCommand::SetRawMotorCurrent {
            current: PhaseCurrent::from_milliamps(1_000),
        };

        assert_eq!(command.safety_class(), crate::SafetyClass::Actuation);
        assert_eq!(
            capabilities.check_command(command),
            Err(UnsupportedReason::CommandNotSupported(command.kind()))
        );
    }

    #[test]
    fn dangerous_actuation_policy_requires_arm_token() {
        let policy = crate::DangerousActuationPolicy {
            model: "Begode Falcon",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let command = DeviceCommand::SetRawMotorCurrent {
            current: PhaseCurrent::from_milliamps(1_000),
        };

        assert_eq!(
            policy.authorize(command, ms(42), None),
            Err(crate::DangerousActuationRefusal::MissingArm)
        );
    }

    #[test]
    fn dangerous_actuation_policy_rejects_expired_or_wrong_model_arms() {
        let falcon = crate::DangerousActuationPolicy {
            model: "Begode Falcon",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let aero = crate::DangerousActuationPolicy {
            model: "NOSFET Aero",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let command = DeviceCommand::SetRawMotorCurrent {
            current: PhaseCurrent::from_milliamps(1_000),
        };
        let falcon_arm = falcon.arm(ms(10));
        let aero_arm = aero.arm(ms(10));

        assert_eq!(
            falcon.authorize(command, ms(1_011), Some(falcon_arm)),
            Err(crate::DangerousActuationRefusal::ExpiredArm)
        );
        assert_eq!(
            falcon.authorize(command, ms(42), Some(aero_arm)),
            Err(crate::DangerousActuationRefusal::WrongModel)
        );
    }

    #[test]
    fn dangerous_actuation_policy_rejects_non_actuation_and_over_limit_commands() {
        let policy = crate::DangerousActuationPolicy {
            model: "Begode Falcon",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let arm = policy.arm(ms(10));

        assert_eq!(
            policy.authorize(DeviceCommand::SoundHorn, ms(42), Some(arm)),
            Err(crate::DangerousActuationRefusal::WrongSafetyClass)
        );
        assert_eq!(
            policy.authorize(
                DeviceCommand::SetRawMotorCurrent {
                    current: PhaseCurrent::from_milliamps(5_001)
                },
                ms(42),
                Some(arm)
            ),
            Err(crate::DangerousActuationRefusal::CurrentLimitExceeded)
        );
    }

    #[test]
    fn dangerous_actuation_policy_accepts_armed_in_limit_actuation() {
        let policy = crate::DangerousActuationPolicy {
            model: "Begode Falcon",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let command = DeviceCommand::SetRawMotorCurrent {
            current: PhaseCurrent::from_milliamps(-5_000),
        };
        let arm = policy.arm(ms(10));

        assert_eq!(
            policy.authorize(command, ms(1_010), Some(arm)),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::SetRawMotorCurrent,
                safety_class: crate::SafetyClass::Actuation,
            })
        );
    }

    #[test]
    fn hosts_can_query_support_before_writes() {
        let capabilities = crate::Capabilities::from_supported_commands([
            crate::CommandKind::RequestTelemetry,
            crate::CommandKind::SetLights,
        ]);

        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestTelemetry),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestTelemetry,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
        assert_eq!(
            capabilities.check_command(DeviceCommand::SoundHorn),
            Err(UnsupportedReason::CommandNotSupported(
                crate::CommandKind::SoundHorn
            ))
        );
    }

    #[test]
    fn parser_limits_reject_oversized_frame_lengths() {
        let limits = crate::ParserLimits {
            max_frame_len: frame_len(24),
            ..crate::ParserLimits::default()
        };

        assert_eq!(limits.validate_frame_len(frame_len(24)), Ok(()));
        assert_eq!(
            limits.validate_frame_len(frame_len(25)),
            Err(crate::ParserError::OversizedFrame {
                claimed: frame_len(25),
                max: frame_len(24),
            })
        );
    }

    #[test]
    fn parser_diagnostics_saturate_counters() {
        let mut diagnostics = crate::ParserDiagnostics {
            dropped_bytes: dropped_bytes(u64::MAX),
            ..crate::ParserDiagnostics::default()
        };

        diagnostics.add_dropped_bytes(dropped_bytes(10));
        diagnostics.record_resync();
        diagnostics.record_error(crate::ParserError::BadChecksum);

        assert_eq!(diagnostics.dropped_bytes, dropped_bytes(u64::MAX));
        assert_eq!(diagnostics.resyncs, diag_count(1));
        assert_eq!(diagnostics.bad_checksums, diag_count(1));
    }

    #[test]
    fn parser_diagnostics_merge_with_saturating_counts() {
        let mut left = crate::ParserDiagnostics {
            timeouts: diag_count(u64::MAX),
            malformed_frames: diag_count(2),
            ..crate::ParserDiagnostics::default()
        };
        let right = crate::ParserDiagnostics {
            timeouts: diag_count(1),
            unmatched_replies: diag_count(3),
            ..crate::ParserDiagnostics::default()
        };

        left.merge(right);

        assert_eq!(left.timeouts, diag_count(u64::MAX));
        assert_eq!(left.malformed_frames, diag_count(2));
        assert_eq!(left.unmatched_replies, diag_count(3));
    }

    #[test]
    fn parser_errors_map_to_expected_diagnostic_counters() {
        let mut diagnostics = crate::ParserDiagnostics::default();

        diagnostics.record_error(crate::ParserError::OversizedFrame {
            claimed: frame_len(4_097),
            max: frame_len(4_096),
        });
        diagnostics.record_error(crate::ParserError::MalformedFrame);
        diagnostics.record_error(crate::ParserError::Timeout {
            elapsed_ms: ms(1_500),
            timeout_ms: ms(1_000),
        });
        diagnostics.record_error(crate::ParserError::UnmatchedReply);

        assert_eq!(diagnostics.oversized_frames, diag_count(1));
        assert_eq!(diagnostics.malformed_frames, diag_count(1));
        assert_eq!(diagnostics.timeouts, diag_count(1));
        assert_eq!(diagnostics.unmatched_replies, diag_count(1));
    }

    #[test]
    fn parser_diagnostics_can_be_emitted_as_device_event() {
        let diagnostics = crate::ParserDiagnostics {
            bad_checksums: diag_count(2),
            resyncs: diag_count(1),
            ..crate::ParserDiagnostics::default()
        };

        assert_eq!(
            DeviceEvent::Diagnostics(diagnostics),
            DeviceEvent::Diagnostics(crate::ParserDiagnostics {
                bad_checksums: diag_count(2),
                resyncs: diag_count(1),
                ..crate::ParserDiagnostics::default()
            })
        );
    }

    #[test]
    fn diagnostic_error_can_be_emitted_as_device_event() {
        let error = crate::DiagnosticError::from_parser_error(crate::ParserError::Timeout {
            elapsed_ms: ms(1_500),
            timeout_ms: ms(1_000),
        });

        assert_eq!(
            DeviceEvent::DiagnosticError(error),
            DeviceEvent::DiagnosticError(crate::DiagnosticError {
                kind: crate::DiagnosticErrorKind::Timeout,
                claimed_len: None,
                max_len: None,
                elapsed_ms: Some(ms(1_500)),
                timeout_ms: Some(ms(1_000)),
            })
        );
    }

    #[test]
    fn diagnostic_snapshot_preserves_counter_fields() {
        let diagnostics = crate::ParserDiagnostics {
            dropped_bytes: dropped_bytes(1),
            resyncs: diag_count(2),
            bad_checksums: diag_count(3),
            timeouts: diag_count(4),
            oversized_frames: diag_count(5),
            malformed_frames: diag_count(6),
            unmatched_replies: diag_count(7),
        };

        assert_eq!(
            crate::DiagnosticSnapshot::from_parser_diagnostics(diagnostics),
            crate::DiagnosticSnapshot {
                dropped_bytes: dropped_bytes(1),
                resyncs: diag_count(2),
                bad_checksums: diag_count(3),
                timeouts: diag_count(4),
                oversized_frames: diag_count(5),
                malformed_frames: diag_count(6),
                unmatched_replies: diag_count(7),
            }
        );
    }

    #[test]
    fn diagnostic_error_preserves_oversized_frame_details() {
        let error = crate::DiagnosticError::from_parser_error(crate::ParserError::OversizedFrame {
            claimed: frame_len(4_097),
            max: frame_len(4_096),
        });

        assert_eq!(
            error,
            crate::DiagnosticError {
                kind: crate::DiagnosticErrorKind::OversizedFrame,
                claimed_len: Some(frame_len(4_097)),
                max_len: Some(frame_len(4_096)),
                elapsed_ms: None,
                timeout_ms: None,
            }
        );
    }

    #[test]
    fn diagnostic_error_preserves_timeout_details() {
        let error = crate::DiagnosticError::from_parser_error(crate::ParserError::Timeout {
            elapsed_ms: ms(1_500),
            timeout_ms: ms(1_000),
        });

        assert_eq!(
            error,
            crate::DiagnosticError {
                kind: crate::DiagnosticErrorKind::Timeout,
                claimed_len: None,
                max_len: None,
                elapsed_ms: Some(ms(1_500)),
                timeout_ms: Some(ms(1_000)),
            }
        );
    }

    #[test]
    fn diagnostic_snapshot_maps_from_device_event() {
        let diagnostics = crate::ParserDiagnostics {
            bad_checksums: diag_count(2),
            ..crate::ParserDiagnostics::default()
        };

        assert_eq!(
            crate::DiagnosticSnapshot::from_device_event(DeviceEvent::Diagnostics(diagnostics)),
            Some(crate::DiagnosticSnapshot {
                bad_checksums: diag_count(2),
                ..crate::DiagnosticSnapshot::default()
            })
        );
        assert_eq!(
            crate::DiagnosticSnapshot::from_device_event(DeviceEvent::LinkDown),
            None
        );
    }

    #[test]
    fn request_tracker_enforces_write_pacing() {
        let policy = crate::RequestPolicy {
            min_interval: Duration::from_milliseconds(100),
            ..crate::RequestPolicy::default()
        };
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);

        assert_eq!(tracker.start(key, policy, ms(1_000)), Ok(()));
        assert_eq!(
            tracker.correlate_reply(key, &mut crate::ParserDiagnostics::default()),
            crate::CorrelationResult::Matched { key, attempts: 1 }
        );
        assert_eq!(
            tracker.start(key, policy, ms(1_050)),
            Err(crate::RequestStartError::Pacing {
                ready_at_ms: ms(1_100)
            })
        );
        assert_eq!(tracker.start(key, policy, ms(1_100)), Ok(()));
    }

    #[test]
    fn request_tracker_reports_retry_after_timeout() {
        let policy = crate::RequestPolicy {
            timeout: Duration::from_milliseconds(250),
            max_retries: 2,
            ..crate::RequestPolicy::default()
        };
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestIdentity);

        tracker.start(key, policy, ms(10)).unwrap();

        assert_eq!(tracker.on_tick(ms(259)), crate::RequestTick::Waiting);
        assert_eq!(
            tracker.on_tick(ms(260)),
            crate::RequestTick::Retry { key, attempt: 1 }
        );
        assert_eq!(tracker.retry_started(ms(260)), Ok(()));
        assert_eq!(
            tracker.on_tick(ms(510)),
            crate::RequestTick::Retry { key, attempt: 2 }
        );
        assert_eq!(tracker.retry_started(ms(510)), Ok(()));
        assert_eq!(
            tracker.on_tick(ms(760)),
            crate::RequestTick::TimedOut { key, attempts: 3 }
        );
    }

    #[test]
    fn request_tracker_correlates_reply_and_clears_slot() {
        let policy = crate::RequestPolicy::default();
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);

        tracker.start(key, policy, ms(20)).unwrap();

        assert_eq!(
            tracker.correlate_reply(key, &mut crate::ParserDiagnostics::default()),
            crate::CorrelationResult::Matched { key, attempts: 1 }
        );
        assert_eq!(tracker.in_flight(), None);
        assert_eq!(tracker.start(key, policy, ms(21)), Ok(()));
    }

    #[test]
    fn request_tracker_counts_unmatched_replies() {
        let mut diagnostics = crate::ParserDiagnostics::default();
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);

        assert_eq!(
            tracker.correlate_reply(key, &mut diagnostics),
            crate::CorrelationResult::Unmatched { key }
        );
        assert_eq!(diagnostics.unmatched_replies, diag_count(1));
    }

    #[test]
    fn request_tracker_serializes_ambiguous_overlaps() {
        let policy = crate::RequestPolicy::default();
        let mut tracker = crate::RequestTracker::default();
        let telemetry = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let identity = crate::RequestKey::new(crate::CommandKind::RequestIdentity);

        tracker.start(telemetry, policy, ms(20)).unwrap();

        assert_eq!(
            tracker.start(identity, policy, ms(21)),
            Err(crate::RequestStartError::Busy { key: telemetry })
        );
    }

    #[test]
    fn request_tracker_correlates_can_target_separately_from_local_command() {
        let policy = crate::RequestPolicy::default();
        let mut tracker = crate::RequestTracker::default();
        let local = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let can = crate::RequestKey::for_target(
            crate::CommandKind::RequestTelemetry,
            crate::RequestTarget::VescCanController {
                controller_id: crate::VescControllerId::new(7),
            },
        );
        let mut diagnostics = crate::ParserDiagnostics::default();

        tracker.start(can, policy, ms(20)).unwrap();

        assert_eq!(
            tracker.correlate_reply(local, &mut diagnostics),
            crate::CorrelationResult::Unmatched { key: local }
        );
        assert_eq!(diagnostics.unmatched_replies, diag_count(1));
        assert_eq!(
            tracker.correlate_reply(can, &mut diagnostics),
            crate::CorrelationResult::Matched {
                key: can,
                attempts: 1
            }
        );
    }

    #[test]
    fn request_queue_pops_in_fifo_order() {
        let mut queue = crate::RequestQueue::<3>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
        );

        assert_eq!(queue.enqueue(telemetry), Ok(()));
        assert_eq!(queue.enqueue(identity), Ok(()));

        assert_eq!(queue.pop_next(), Some(telemetry));
        assert_eq!(queue.pop_next(), Some(identity));
        assert_eq!(queue.pop_next(), None);
    }

    #[test]
    fn request_queue_rejects_overflow() {
        let mut queue = crate::RequestQueue::<1>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
        );

        assert_eq!(queue.enqueue(telemetry), Ok(()));
        assert_eq!(
            queue.enqueue(identity),
            Err(crate::RequestQueueError::Full { capacity: 1 })
        );
    }

    #[test]
    fn request_queue_rejects_duplicate_keys() {
        let mut queue = crate::RequestQueue::<2>::new();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let request = crate::QueuedRequest::new(key, crate::RequestPolicy::default());

        assert_eq!(queue.enqueue(request), Ok(()));
        assert_eq!(
            queue.enqueue(request),
            Err(crate::RequestQueueError::DuplicateKey { key })
        );
    }

    #[test]
    fn request_queue_allows_reenqueue_after_dequeue() {
        let mut queue = crate::RequestQueue::<1>::new();
        let request = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );

        assert_eq!(queue.enqueue(request), Ok(()));
        assert_eq!(queue.pop_next(), Some(request));
        assert_eq!(queue.enqueue(request), Ok(()));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn request_queue_inserts_higher_urgency_before_routine_work() {
        let mut queue = crate::RequestQueue::<3>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(queue.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(queue.enqueue_by_urgency(identity), Ok(()));

        assert_eq!(queue.pop_next(), Some(identity));
        assert_eq!(queue.pop_next(), Some(telemetry));
    }

    #[test]
    fn request_queue_preserves_fifo_within_same_urgency() {
        let mut queue = crate::RequestQueue::<3>::new();
        let telemetry = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(queue.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(queue.enqueue_by_urgency(identity), Ok(()));

        assert_eq!(queue.pop_next(), Some(telemetry));
        assert_eq!(queue.pop_next(), Some(identity));
    }

    #[test]
    fn request_queue_refuses_duplicate_before_priority_insertion() {
        let mut queue = crate::RequestQueue::<2>::new();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let routine = crate::QueuedRequest::new(key, crate::RequestPolicy::default());
        let urgent = crate::QueuedRequest::with_urgency(
            key,
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(queue.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(
            queue.enqueue_by_urgency(urgent),
            Err(crate::RequestQueueError::DuplicateKey { key })
        );
        assert_eq!(queue.pop_next(), Some(routine));
    }

    #[test]
    fn request_queue_refuses_full_before_priority_insertion() {
        let mut queue = crate::RequestQueue::<1>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(queue.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(
            queue.enqueue_by_urgency(identity),
            Err(crate::RequestQueueError::Full { capacity: 1 })
        );
        assert_eq!(queue.pop_next(), Some(telemetry));
    }

    #[test]
    fn request_scheduler_counts_enqueue_and_dequeue_by_urgency() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );
        let diagnostics = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(scheduler.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(diagnostics), Ok(()));

        assert_eq!(
            scheduler.diagnostics().enqueued,
            crate::RequestUrgencyCounters {
                routine: 1,
                high: 1,
                critical: 1,
            }
        );
        assert_eq!(scheduler.pop_next(), Some(diagnostics));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.pop_next(), Some(telemetry));
        assert_eq!(
            scheduler.diagnostics().dequeued,
            crate::RequestUrgencyCounters {
                routine: 1,
                high: 1,
                critical: 1,
            }
        );
    }

    #[test]
    fn request_scheduler_preserves_fifo_within_same_urgency() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let telemetry = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(scheduler.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));

        assert_eq!(scheduler.pop_next(), Some(telemetry));
        assert_eq!(scheduler.pop_next(), Some(identity));
    }

    #[test]
    fn request_scheduler_inserts_between_higher_and_lower_urgency_work() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let critical = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let routine = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let high = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(scheduler.enqueue_by_urgency(critical), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(high), Ok(()));

        assert_eq!(scheduler.pop_next(), Some(critical));
        assert_eq!(scheduler.pop_next(), Some(high));
        assert_eq!(scheduler.pop_next(), Some(routine));
    }

    #[test]
    fn request_scheduler_counts_duplicate_and_overflow_refusals() {
        let mut scheduler = crate::RequestScheduler::<1>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
        );

        assert_eq!(scheduler.enqueue(telemetry), Ok(()));
        assert_eq!(
            scheduler.enqueue(telemetry),
            Err(crate::RequestQueueError::DuplicateKey { key: telemetry.key })
        );
        assert_eq!(
            scheduler.enqueue(identity),
            Err(crate::RequestQueueError::Full { capacity: 1 })
        );

        assert_eq!(scheduler.diagnostics().duplicate_refusals, 1);
        assert_eq!(scheduler.diagnostics().overflow_refusals, 1);
        assert_eq!(
            scheduler.diagnostics().enqueued,
            crate::RequestUrgencyCounters {
                routine: 1,
                high: 0,
                critical: 0,
            }
        );
    }

    #[test]
    fn request_scheduler_exposes_queue_len_and_empty_state() {
        let mut scheduler = crate::RequestScheduler::<1>::new();
        let request = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );

        assert!(scheduler.is_empty());
        assert_eq!(scheduler.len(), 0);
        assert_eq!(scheduler.enqueue(request), Ok(()));
        assert!(!scheduler.is_empty());
        assert_eq!(scheduler.len(), 1);
        assert_eq!(scheduler.pop_next(), Some(request));
        assert!(scheduler.is_empty());
        assert_eq!(scheduler.len(), 0);
    }

    #[test]
    fn request_scheduler_ages_skipped_routine_work_ahead_of_repeated_critical_work() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let routine = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let firmware = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestFirmwareInfo),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let diagnostics = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(scheduler.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(firmware), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(firmware));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.enqueue_by_urgency(diagnostics), Ok(()));

        assert_eq!(scheduler.pop_next(), Some(routine));
        assert_eq!(scheduler.diagnostics().starvation_aging_events, 1);
        assert_eq!(scheduler.pop_next(), Some(diagnostics));
    }

    #[test]
    fn request_scheduler_continues_after_aged_promotion_without_stale_skip_counts() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let routine = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let firmware = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestFirmwareInfo),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let diagnostics = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(scheduler.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.enqueue_by_urgency(firmware), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(firmware));
        assert_eq!(scheduler.enqueue_by_urgency(diagnostics), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(routine));

        assert_eq!(scheduler.pop_next(), Some(diagnostics));
        assert_eq!(scheduler.pop_next(), None);
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.diagnostics().starvation_aging_events, 1);
    }

    #[test]
    fn request_scheduler_does_not_age_new_middle_insert_after_promotion() {
        let mut scheduler = crate::RequestScheduler::<4>::new();
        let routine = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let firmware = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestFirmwareInfo),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let diagnostics = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let set_lights = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::SetLights),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(scheduler.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(firmware), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(firmware));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.enqueue_by_urgency(diagnostics), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(routine));
        assert_eq!(scheduler.enqueue_by_urgency(set_lights), Ok(()));

        assert_eq!(scheduler.pop_next(), Some(diagnostics));
        assert_eq!(scheduler.pop_next(), Some(set_lights));
        assert_eq!(scheduler.diagnostics().starvation_aging_events, 1);
    }

    #[test]
    fn request_scheduler_does_not_count_aging_when_fifo_front_is_selected() {
        let mut scheduler = crate::RequestScheduler::<2>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
        );

        assert_eq!(scheduler.enqueue(telemetry), Ok(()));
        assert_eq!(scheduler.enqueue(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(telemetry));
        assert_eq!(scheduler.pop_next(), Some(identity));

        assert_eq!(scheduler.diagnostics().starvation_aging_events, 0);
    }

    #[test]
    fn poll_request_converts_read_only_command_to_queued_request() {
        let policy = crate::RequestPolicy {
            timeout: Duration::from_milliseconds(250),
            max_retries: 2,
            min_interval: Duration::from_milliseconds(50),
        };
        let request = crate::PollRequest::new(
            crate::CommandKind::RequestIdentity,
            policy,
            crate::RequestUrgency::High,
        );

        assert_eq!(
            request.to_queued_request(),
            Ok(crate::QueuedRequest::with_urgency(
                crate::RequestKey::new(crate::CommandKind::RequestIdentity),
                policy,
                crate::RequestUrgency::High
            ))
        );
    }

    #[test]
    fn poll_request_rejects_non_read_only_command() {
        let request = crate::PollRequest::new(
            crate::CommandKind::SetLights,
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(
            request.to_queued_request(),
            Err(crate::PollingPlanError::UnsupportedCommand {
                kind: crate::CommandKind::SetLights,
                safety_class: crate::SafetyClass::BenignControl,
            })
        );
    }

    #[test]
    fn polling_plan_enqueues_requests_by_urgency() {
        let plan = crate::PollingPlan::new([
            crate::PollRequest::new(
                crate::CommandKind::RequestTelemetry,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::Routine,
            ),
            crate::PollRequest::new(
                crate::CommandKind::RequestIdentity,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::High,
            ),
        ]);
        let mut queue = crate::RequestQueue::<2>::new();

        assert_eq!(plan.enqueue_into(&mut queue), Ok(()));

        assert_eq!(
            queue.pop_next(),
            Some(crate::QueuedRequest::with_urgency(
                crate::RequestKey::new(crate::CommandKind::RequestIdentity),
                crate::RequestPolicy::default(),
                crate::RequestUrgency::High
            ))
        );
        assert_eq!(
            queue.pop_next(),
            Some(crate::QueuedRequest::new(
                crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
                crate::RequestPolicy::default()
            ))
        );
    }

    #[test]
    fn polling_plan_propagates_duplicate_queue_errors() {
        let plan = crate::PollingPlan::new([
            crate::PollRequest::new(
                crate::CommandKind::RequestTelemetry,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::Routine,
            ),
            crate::PollRequest::new(
                crate::CommandKind::RequestTelemetry,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::High,
            ),
        ]);
        let mut queue = crate::RequestQueue::<2>::new();

        assert_eq!(
            plan.enqueue_into(&mut queue),
            Err(crate::PollingPlanError::Queue(
                crate::RequestQueueError::DuplicateKey {
                    key: crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
                }
            ))
        );
    }

    #[test]
    fn polling_plan_propagates_capacity_errors() {
        let plan = crate::PollingPlan::new([
            crate::PollRequest::new(
                crate::CommandKind::RequestTelemetry,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::Routine,
            ),
            crate::PollRequest::new(
                crate::CommandKind::RequestIdentity,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::High,
            ),
        ]);
        let mut queue = crate::RequestQueue::<1>::new();

        assert_eq!(
            plan.enqueue_into(&mut queue),
            Err(crate::PollingPlanError::Queue(
                crate::RequestQueueError::Full { capacity: 1 }
            ))
        );
    }

    #[test]
    fn zero_capacity_request_queue_refuses_enqueue() {
        let mut queue = crate::RequestQueue::<0>::new();
        let request = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );

        assert_eq!(
            queue.enqueue(request),
            Err(crate::RequestQueueError::Full { capacity: 0 })
        );
        assert!(queue.is_empty());
    }

    proptest! {
        #[test]
        fn request_queue_preserves_order_up_to_capacity(input in proptest::collection::vec(0u8..5, 0..8)) {
            let mut queue = crate::RequestQueue::<3>::new();
            let mut expected = Vec::new();

            for value in input {
                let command = if value % 2 == 0 {
                    crate::CommandKind::RequestTelemetry
                } else {
                    crate::CommandKind::RequestIdentity
                };
                let request = crate::QueuedRequest::new(
                    crate::RequestKey::new(command),
                    crate::RequestPolicy::default(),
                );
                if expected.iter().any(|queued: &crate::QueuedRequest| queued.key == request.key) {
                    prop_assert_eq!(
                        queue.enqueue(request),
                        Err(crate::RequestQueueError::DuplicateKey { key: request.key })
                    );
                } else if expected.len() == 3 {
                    prop_assert_eq!(
                        queue.enqueue(request),
                        Err(crate::RequestQueueError::Full { capacity: 3 })
                    );
                } else {
                    prop_assert_eq!(queue.enqueue(request), Ok(()));
                    expected.push(request);
                }
            }

            let mut observed = Vec::new();
            while let Some(request) = queue.pop_next() {
                observed.push(request);
            }
            prop_assert_eq!(observed, expected);
        }
    }

    proptest! {
        #[test]
        fn poll_request_accepts_read_only_commands(value in 0u8..6) {
            let kind = match value {
                0 => crate::CommandKind::RequestIdentity,
                1 => crate::CommandKind::RequestTelemetry,
                2 => crate::CommandKind::RequestFirmwareInfo,
                3 => crate::CommandKind::RequestBatteryInfo,
                4 => crate::CommandKind::RequestDiagnostics,
                _ => crate::CommandKind::RequestSettings,
            };
            let request = crate::PollRequest::new(
                kind,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::Routine,
            );

            prop_assert_eq!(
                request.to_queued_request(),
                Ok(crate::QueuedRequest::new(
                    crate::RequestKey::new(kind),
                    crate::RequestPolicy::default()
                ))
            );
        }
    }

    proptest! {
        #[test]
        fn battery_response_keeps_unknown_distinct_from_zero(include_zero in any::<bool>()) {
            let level_reported = include_zero.then_some(Measured::reported(BatteryLevel::from_percent(0)));
            let response = crate::BatteryInfo {
                level_reported,
                ..crate::BatteryInfo::default()
            };

            if include_zero {
                prop_assert_eq!(
                    response.level_reported,
                    Some(Measured::reported(BatteryLevel::from_percent(0)))
                );
            } else {
                prop_assert_eq!(response.level_reported, None);
            }
        }
    }

    proptest! {
        #[test]
        fn request_queue_priority_order_is_monotonic(urgencies in proptest::collection::vec(0u8..3, 0..3)) {
            let commands = [
                crate::CommandKind::RequestTelemetry,
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::SetLights,
            ];
            let mut queue = crate::RequestQueue::<3>::new();

            for (index, urgency) in urgencies.into_iter().enumerate() {
                let request = crate::QueuedRequest::with_urgency(
                    crate::RequestKey::new(commands[index]),
                    crate::RequestPolicy::default(),
                    match urgency {
                        0 => crate::RequestUrgency::Routine,
                        1 => crate::RequestUrgency::High,
                        _ => crate::RequestUrgency::Critical,
                    },
                );
                prop_assert_eq!(queue.enqueue_by_urgency(request), Ok(()));
            }

            let mut last = crate::RequestUrgency::Critical;
            while let Some(request) = queue.pop_next() {
                prop_assert!(request.urgency <= last);
                last = request.urgency;
            }
        }
    }

    #[test]
    fn host_session_drives_link_events_and_drains_outputs() {
        let mut host = crate::HostSession::new(EchoSession::default());
        let link = LinkInfo {
            monotonic_ms: ms(10),
            max_write_len: Some(write_len(185)),
        };

        host.ingest_link_up(link)
            .expect("default host output capacity accepts link output");
        let drained = host.drain_outputs();

        assert_eq!(
            drained.as_slice(),
            &[SessionOutput::Event(DeviceEvent::LinkUp(link))]
        );
        assert!(host.drain_outputs().is_empty());
    }

    #[test]
    fn bounded_session_output_reports_overflow_without_allocating() {
        let mut output = crate::BoundedSessionOutput::<1>::new();
        let link = LinkInfo {
            monotonic_ms: ms(10),
            max_write_len: Some(write_len(185)),
        };

        assert_eq!(
            output.push(SessionOutput::Event(DeviceEvent::LinkUp(link))),
            Ok(())
        );
        assert_eq!(
            output.push(SessionOutput::Event(DeviceEvent::LinkDown)),
            Err(crate::SessionOutputError::Full {
                capacity: crate::SessionOutputCapacity::new(1),
            })
        );
        assert_eq!(
            output.as_slice(),
            &[SessionOutput::Event(DeviceEvent::LinkUp(link))]
        );
    }

    #[test]
    fn try_replay_capture_reports_bounded_output_overflow() {
        let mut host = crate::HostSession::<_, 1>::with_output_capacity(BurstSession);
        let records = [crate::CaptureRecord::LinkUp(LinkInfo {
            monotonic_ms: ms(10),
            max_write_len: Some(write_len(185)),
        })];

        assert_eq!(
            crate::try_replay_capture(&mut host, &records),
            Err(crate::SessionOutputError::Full {
                capacity: crate::SessionOutputCapacity::new(1),
            })
        );
    }

    #[test]
    fn host_session_ingests_owned_notifications_without_retaining_bytes() {
        let mut host = crate::HostSession::new(EchoSession::default());
        let channel = GattChannel::from_bytes([0xfe; 16]);

        host.ingest_notification_owned(channel, vec![0xdc, 0x5a, 0x5c], ms(20))
            .expect("default host output capacity accepts notification output");

        assert_eq!(
            host.drain_outputs().as_slice(),
            &[SessionOutput::NotificationIngest(
                crate::NotificationIngestOutcome::ignored_wrong_channel(
                    channel,
                    crate::NotificationByteLen::from_bytes(3),
                    ms(20)
                )
            )]
        );
    }

    #[test]
    fn host_session_ingests_borrowed_session_input_for_ffi_wrappers() {
        let mut host = crate::HostSession::new(EchoSession::default());
        let channel = GattChannel::from_bytes([0xa1; 16]);
        let bytes = [0xde, 0xad, 0xbe, 0xef];

        host.ingest(SessionInput::Notification {
            channel,
            bytes: &bytes,
            monotonic_ms: ms(42),
        })
        .expect("default host output capacity accepts borrowed notification output");

        assert_eq!(
            host.drain_outputs().as_slice(),
            &[SessionOutput::NotificationIngest(
                crate::NotificationIngestOutcome::ignored_wrong_channel(
                    channel,
                    crate::NotificationByteLen::from_bytes(4),
                    ms(42)
                )
            )]
        );
    }

    #[test]
    fn host_session_issues_commands_through_facade() {
        let mut host = crate::HostSession::new(EchoSession::default());

        host.issue_command(DeviceCommand::RequestTelemetry)
            .expect("default host output capacity accepts command output");

        assert_eq!(
            host.drain_outputs().as_slice(),
            &[SessionOutput::Transport(TransportAction::Write {
                channel: GattChannel::from_bytes([1; 16]),
                bytes: WritePayload::try_from_slice(b"telemetry").expect("test write payload fits"),
                mode: WriteMode::WithResponse,
            })]
        );
    }

    #[derive(Default)]
    struct StateSession;

    impl ProtocolSession for StateSession {
        fn handle(
            &mut self,
            input: SessionInput<'_>,
            output: &mut dyn SessionOutputSink,
        ) -> Result<(), crate::SessionOutputError> {
            match input {
                SessionInput::Command(DeviceCommand::RequestTelemetry) => {
                    output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                        TelemetryDelta {
                            at_ms: ms(40),
                            speed: Some(Measured::reported(Speed::from_millimetres_per_second(
                                1_200,
                            ))),
                            ..TelemetryDelta::empty(ms(40))
                        },
                    )))?;
                }
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                        crate::ParserDiagnostics {
                            timeouts: diag_count(monotonic_ms.get()),
                            ..crate::ParserDiagnostics::default()
                        },
                    )))?;
                }
                SessionInput::LinkUp(_)
                | SessionInput::LinkDown
                | SessionInput::Notification { .. }
                | SessionInput::Command(_) => {}
            }
            Ok(())
        }
    }

    #[test]
    fn host_session_updates_current_snapshot_from_events() {
        let mut host = crate::HostSession::new(StateSession);

        host.issue_command(DeviceCommand::RequestTelemetry)
            .expect("default host output capacity accepts telemetry output");

        assert_eq!(host.current_snapshot().at_ms, Some(ms(40)));
        assert_eq!(
            host.current_snapshot().speed,
            Some(Measured::reported(Speed::from_millimetres_per_second(
                1_200
            )))
        );
    }

    #[test]
    fn host_session_merges_diagnostics_from_events() {
        let mut host = crate::HostSession::new(StateSession);

        host.tick(ms(2))
            .expect("default host output capacity accepts diagnostic output");
        host.tick(ms(3))
            .expect("default host output capacity accepts diagnostic output");

        assert_eq!(host.diagnostics().timeouts, diag_count(5));
    }

    #[test]
    fn diagnostic_snapshot_maps_from_host_session_diagnostics() {
        let mut host = crate::HostSession::new(StateSession);

        host.tick(ms(2))
            .expect("default host output capacity accepts diagnostic output");

        assert_eq!(
            crate::DiagnosticSnapshot::from_parser_diagnostics(host.diagnostics()),
            crate::DiagnosticSnapshot {
                timeouts: diag_count(2),
                ..crate::DiagnosticSnapshot::default()
            }
        );
    }

    #[derive(Default)]
    struct FramedCaptureSession {
        sum: i32,
    }

    impl ProtocolSession for FramedCaptureSession {
        fn handle(
            &mut self,
            input: SessionInput<'_>,
            output: &mut dyn SessionOutputSink,
        ) -> Result<(), crate::SessionOutputError> {
            match input {
                SessionInput::LinkUp(info) => {
                    output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)))?;
                }
                SessionInput::LinkDown => {
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown))?;
                }
                SessionInput::Notification { bytes, .. } => {
                    for byte in bytes {
                        if *byte == 0xff {
                            output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                                TelemetryDelta {
                                    at_ms: ms(90),
                                    speed: Some(Measured::reported(
                                        Speed::from_millimetres_per_second(self.sum),
                                    )),
                                    ..TelemetryDelta::empty(ms(90))
                                },
                            )))?;
                            self.sum = 0;
                        } else {
                            self.sum += i32::from(*byte);
                        }
                    }
                }
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }))?;
                }
                SessionInput::Command(command) => {
                    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                        crate::ParserDiagnostics {
                            unmatched_replies: diag_count(command.kind() as u64),
                            ..crate::ParserDiagnostics::default()
                        },
                    )))?;
                }
            }
            Ok(())
        }
    }

    fn replay_events(records: &[crate::CaptureRecord]) -> Vec<DeviceEvent> {
        let mut host = crate::HostSession::new(FramedCaptureSession::default());
        crate::replay_capture(&mut host, records)
            .into_iter()
            .filter_map(|output| match output {
                SessionOutput::Event(event) => Some(event),
                SessionOutput::Transport(_) | SessionOutput::NotificationIngest(_) => None,
            })
            .collect()
    }

    #[test]
    fn capture_record_owns_notification_payloads() {
        let channel = GattChannel::from_bytes([0x11; 16]);
        let source = vec![1, 2, 0xff];
        let record = crate::CaptureRecord::notification(channel, source.clone(), ms(10));

        assert_eq!(
            record,
            crate::CaptureRecord::Notification {
                channel,
                bytes: source,
                monotonic_ms: ms(10),
            }
        );
    }

    #[test]
    fn capture_record_preserves_targeted_command_metadata() {
        let target = crate::RequestTarget::VescCanController {
            controller_id: crate::VescControllerId::new(7),
        };
        let record =
            crate::CaptureRecord::targeted_command(DeviceCommand::RequestTelemetry, target);

        assert_eq!(
            record,
            crate::CaptureRecord::TargetedCommand {
                command: DeviceCommand::RequestTelemetry,
                target,
            }
        );
    }

    #[test]
    fn replay_capture_drives_link_tick_command_and_notification_records() {
        let channel = GattChannel::from_bytes([0x22; 16]);
        let link = LinkInfo {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len(185)),
        };
        let records = [
            crate::CaptureRecord::LinkUp(link),
            crate::CaptureRecord::Tick {
                monotonic_ms: ms(2),
            },
            crate::CaptureRecord::Command(DeviceCommand::RequestIdentity),
            crate::CaptureRecord::notification(channel, vec![4, 5, 0xff], ms(3)),
            crate::CaptureRecord::LinkDown,
        ];

        assert_eq!(
            replay_events(&records).as_slice(),
            &[
                DeviceEvent::LinkUp(link),
                DeviceEvent::Tick {
                    monotonic_ms: ms(2)
                },
                DeviceEvent::Diagnostics(crate::ParserDiagnostics {
                    unmatched_replies: diag_count(crate::CommandKind::RequestIdentity as u64),
                    ..crate::ParserDiagnostics::default()
                }),
                DeviceEvent::Telemetry(TelemetryDelta {
                    at_ms: ms(90),
                    speed: Some(Measured::reported(Speed::from_millimetres_per_second(9),)),
                    ..TelemetryDelta::empty(ms(90))
                }),
                DeviceEvent::LinkDown,
            ]
        );
    }

    #[test]
    fn replay_capture_drives_targeted_command_as_underlying_command() {
        let target = crate::RequestTarget::VescCanController {
            controller_id: crate::VescControllerId::new(7),
        };
        let records = [crate::CaptureRecord::targeted_command(
            DeviceCommand::RequestTelemetry,
            target,
        )];

        assert_eq!(
            replay_events(&records).as_slice(),
            &[DeviceEvent::Diagnostics(crate::ParserDiagnostics {
                unmatched_replies: diag_count(crate::CommandKind::RequestTelemetry as u64),
                ..crate::ParserDiagnostics::default()
            })]
        );
    }

    #[test]
    fn targeted_command_survives_notification_chunking_helpers() {
        let target = crate::RequestTarget::VescCanController {
            controller_id: crate::VescControllerId::new(7),
        };
        let record =
            crate::CaptureRecord::targeted_command(DeviceCommand::RequestTelemetry, target);

        assert_eq!(
            record
                .clone()
                .split_notification_bytes(crate::NotificationChunkLen::from_bytes(1)),
            vec![record.clone()]
        );
        assert_eq!(
            record.clone().split_notification_by_lengths(&[
                crate::NotificationChunkLen::from_bytes(1),
                crate::NotificationChunkLen::from_bytes(2),
            ]),
            vec![record]
        );
    }

    #[test]
    fn one_byte_notification_replay_matches_whole_notification_replay() {
        let channel = GattChannel::from_bytes([0x33; 16]);
        let whole = [crate::CaptureRecord::notification(
            channel,
            vec![1, 2, 3, 0xff],
            ms(10),
        )];
        let one_byte = crate::CaptureRecord::notification(channel, vec![1, 2, 3, 0xff], ms(10))
            .split_notification_bytes(crate::NotificationChunkLen::from_bytes(1));

        assert_eq!(replay_events(&one_byte), replay_events(&whole));
    }

    #[test]
    fn replay_chunk_comparison_ignores_notification_boundaries() {
        let channel = GattChannel::from_bytes([0x66; 16]);
        let records = [crate::CaptureRecord::notification(
            channel,
            vec![1, 2, 3, 0xff],
            ms(10),
        )];

        let comparison = crate::compare_replay_capture_chunks(
            FramedCaptureSession::default,
            &records,
            &[
                crate::NotificationChunkLen::from_bytes(2),
                crate::NotificationChunkLen::from_bytes(1),
            ],
        );

        assert_eq!(
            comparison,
            crate::ReplayChunkComparison {
                whole_semantic_events: crate::SemanticEventCount::from_events(1),
                one_byte_semantic_events: crate::SemanticEventCount::from_events(1),
                arbitrary_semantic_events: crate::SemanticEventCount::from_events(1),
                one_byte_matches: true,
                arbitrary_matches: true,
            }
        );
    }

    #[test]
    fn replay_chunk_comparison_reports_semantic_mismatch() {
        #[derive(Default)]
        struct NotificationLengthSession;

        impl ProtocolSession for NotificationLengthSession {
            fn handle(
                &mut self,
                input: SessionInput<'_>,
                output: &mut dyn SessionOutputSink,
            ) -> Result<(), crate::SessionOutputError> {
                let SessionInput::Notification {
                    bytes,
                    monotonic_ms,
                    ..
                } = input
                else {
                    return Ok(());
                };
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    TelemetryDelta {
                        at_ms: monotonic_ms,
                        speed: Some(Measured::reported(Speed::from_millimetres_per_second(
                            i32::try_from(bytes.len()).unwrap_or(0),
                        ))),
                        ..TelemetryDelta::empty(monotonic_ms)
                    },
                )))
            }
        }

        let channel = GattChannel::from_bytes([0x77; 16]);
        let records = [crate::CaptureRecord::notification(
            channel,
            vec![1, 2, 3, 4],
            ms(10),
        )];

        let comparison = crate::compare_replay_capture_chunks(
            || NotificationLengthSession,
            &records,
            &[
                crate::NotificationChunkLen::from_bytes(2),
                crate::NotificationChunkLen::from_bytes(2),
            ],
        );

        assert!(!comparison.one_byte_matches);
        assert!(!comparison.arbitrary_matches);
    }

    #[test]
    fn replay_arbitrary_chunk_lengths_are_derived_from_capture_notifications() {
        let channel = GattChannel::from_bytes([0x78; 16]);
        let records = [
            crate::CaptureRecord::Tick {
                monotonic_ms: ms(1),
            },
            crate::CaptureRecord::notification(channel, vec![0; 4], ms(2)),
            crate::CaptureRecord::notification(channel, vec![0; 10], ms(3)),
            crate::CaptureRecord::LinkDown,
        ];

        assert_eq!(
            crate::replay_arbitrary_chunk_lengths(&records),
            vec![
                crate::NotificationChunkLen::from_bytes(2),
                crate::NotificationChunkLen::from_bytes(3),
                crate::NotificationChunkLen::from_bytes(5),
            ]
        );
    }

    #[test]
    fn replay_arbitrary_chunk_lengths_are_empty_without_notifications() {
        assert_eq!(
            crate::replay_arbitrary_chunk_lengths(&[crate::CaptureRecord::Tick {
                monotonic_ms: ms(1)
            }]),
            Vec::<crate::NotificationChunkLen>::new()
        );
    }

    #[test]
    fn notification_boundary_cases_cover_whole_bytewise_arbitrary_and_coalesced_replay() {
        let channel = GattChannel::from_bytes([0x79; 16]);
        let frame_a = [0xaa, 0xbb, 0xcc];
        let frame_b = [0xdd, 0xee];

        let cases = crate::notification_boundary_replay_cases(
            channel,
            &[frame_a.as_slice(), frame_b.as_slice()],
            ms(10),
            &[crate::NotificationChunkLen::from_bytes(2)],
        );

        assert_eq!(
            cases.iter().map(|case| case.name).collect::<Vec<_>>(),
            vec!["whole", "one-byte", "arbitrary", "coalesced"]
        );
        assert_eq!(
            cases
                .iter()
                .map(|case| case.records.len())
                .collect::<Vec<_>>(),
            vec![2, 5, 3, 1]
        );
    }

    #[test]
    fn notification_impairment_cases_cover_noisy_duplicate_missing_and_timeout_replay() {
        let channel = GattChannel::from_bytes([0x7a; 16]);
        let frame = [0xaa, 0xbb, 0xcc, 0xdd, 0xee];

        let cases = crate::notification_impairment_replay_cases(
            channel,
            frame.as_slice(),
            ms(10),
            &[0x00, 0x01],
            ms(99),
        );

        assert_eq!(
            cases.iter().map(|case| case.name).collect::<Vec<_>>(),
            vec![
                "garbage-prefix",
                "duplicate-first-chunk",
                "missing-final-byte",
                "timeout-after-partial",
            ]
        );
        assert_eq!(
            cases
                .iter()
                .map(|case| case.records.len())
                .collect::<Vec<_>>(),
            vec![1, 3, 1, 2]
        );
    }

    proptest! {
        #[test]
        fn arbitrary_chunk_notification_replay_matches_whole_notification_replay(
            payload_prefix in proptest::collection::vec(0u8..0xff, 0..16),
            lengths in proptest::collection::vec(0usize..6, 0..8),
        ) {
            let channel = GattChannel::from_bytes([0x44; 16]);
            let mut payload = payload_prefix;
            payload.push(0xff);
            let whole = [crate::CaptureRecord::notification(channel, payload.clone(), ms(20))];
            let chunk_lengths = lengths
                .into_iter()
                .map(crate::NotificationChunkLen::from_bytes)
                .collect::<Vec<_>>();
            let chunks = crate::CaptureRecord::notification(channel, payload, ms(20))
                .split_notification_by_lengths(&chunk_lengths);

            prop_assert_eq!(replay_events(&chunks), replay_events(&whole));
        }
    }

    #[test]
    fn replay_summary_preserves_output_order() {
        let channel = GattChannel::from_bytes([0x55; 16]);
        let records = [
            crate::CaptureRecord::Tick {
                monotonic_ms: ms(1),
            },
            crate::CaptureRecord::notification(channel, vec![9, 0xff], ms(2)),
            crate::CaptureRecord::Tick {
                monotonic_ms: ms(3),
            },
        ];
        let mut host = crate::HostSession::new(FramedCaptureSession::default());

        assert_eq!(
            crate::replay_capture(&mut host, &records).as_slice(),
            &[
                SessionOutput::Event(DeviceEvent::Tick {
                    monotonic_ms: ms(1)
                }),
                SessionOutput::Event(DeviceEvent::Telemetry(TelemetryDelta {
                    at_ms: ms(90),
                    speed: Some(Measured::reported(Speed::from_millimetres_per_second(9),)),
                    ..TelemetryDelta::empty(ms(90))
                })),
                SessionOutput::Event(DeviceEvent::Tick {
                    monotonic_ms: ms(3)
                }),
            ]
        );
    }
}
