//! Deterministic capture replay helpers.

use crate::{
    DeviceCommand, DeviceEvent, GattChannel, HostSession, LinkInfo, MonotonicTimestamp,
    NotificationChunkLen, ProtocolSession, RequestTarget, SemanticEventCount, SessionInput,
    SessionOutput, SessionOutputError,
};

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
///
/// # Errors
///
/// Returns [`SessionOutputError::Full`] when the host output buffer fills.
pub fn replay_capture<S>(
    host: &mut HostSession<S>,
    records: &[CaptureRecord],
) -> Result<Vec<SessionOutput>, SessionOutputError>
where
    S: ProtocolSession,
{
    try_replay_capture(host, records)
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
///
/// # Errors
///
/// Returns [`SessionOutputError::Full`] when the host output buffer fills.
pub fn replay_capture_semantic_events<S>(
    host: &mut HostSession<S>,
    records: &[CaptureRecord],
) -> Result<Vec<DeviceEvent>, SessionOutputError>
where
    S: ProtocolSession,
{
    Ok(replay_capture(host, records)?
        .into_iter()
        .filter_map(|output| match output {
            SessionOutput::Transport(_) | SessionOutput::NotificationIngest(_) => None,
            SessionOutput::Event(event) => Some(event),
        })
        .collect())
}

/// Compares whole-notification replay against one-byte and arbitrary
/// notification chunk replay.
///
/// # Errors
///
/// Returns [`SessionOutputError::Full`] when the host output buffer fills.
pub fn compare_replay_capture_chunks<S, F>(
    mut make_session: F,
    records: &[CaptureRecord],
    arbitrary_lengths: &[NotificationChunkLen],
) -> Result<ReplayChunkComparison, SessionOutputError>
where
    S: ProtocolSession,
    F: FnMut() -> S,
{
    let whole = replay_capture_semantic_events(&mut HostSession::new(make_session()), records)?;
    let one_byte_records =
        split_capture_notifications_by_len(records, NotificationChunkLen::from_bytes(1));
    let one_byte =
        replay_capture_semantic_events(&mut HostSession::new(make_session()), &one_byte_records)?;
    let arbitrary_records = split_capture_notifications_by_lengths(records, arbitrary_lengths);
    let arbitrary =
        replay_capture_semantic_events(&mut HostSession::new(make_session()), &arbitrary_records)?;

    Ok(ReplayChunkComparison {
        whole_semantic_events: SemanticEventCount::from_events(whole.len()),
        one_byte_semantic_events: SemanticEventCount::from_events(one_byte.len()),
        arbitrary_semantic_events: SemanticEventCount::from_events(arbitrary.len()),
        one_byte_matches: one_byte == whole,
        arbitrary_matches: arbitrary == whole,
    })
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
