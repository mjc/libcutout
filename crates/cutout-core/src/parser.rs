//! Parser limits, diagnostics, and notification ingest evidence.

use std::{fmt, marker::PhantomData};

use crate::{
    Count, DeviceEvent, Duration, GattChannel, Information, MonotonicTimestamp,
    NotificationChunkByte, NotificationPayloadByte, ParserBufferByte, ParserDiagnosticEvent,
    ParserDroppedByte, ParserFrameByte, ParserQueuedOutput, PayloadBodyByte, ProtocolFamily,
    Quantity, SemanticEvent, VerificationStatus,
};

/// Transport-independent parser resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserLimits {
    /// Maximum accepted logical frame length in bytes.
    pub max_frame_len: ParserFrameLen,

    /// Maximum buffered input length in bytes before a parser should shed data.
    pub max_buffered_len: ParserBufferedLen,

    /// Maximum queued outputs a parser should retain before yielding to host code.
    pub max_queued_outputs: ParserQueuedOutputCount,

    /// Parser timeout threshold.
    pub timeout: Duration,
}

impl Default for ParserLimits {
    fn default() -> Self {
        Self {
            max_frame_len: ParserFrameLen::from_bytes(4_096),
            max_buffered_len: ParserBufferedLen::from_bytes(8_192),
            max_queued_outputs: ParserQueuedOutputCount::from_outputs(128),
            timeout: Duration::from_seconds(1),
        }
    }
}

impl ParserLimits {
    /// Validates that a claimed frame length is within the configured limit.
    ///
    /// # Errors
    ///
    /// Returns [`ParserError::OversizedFrame`] when `claimed` exceeds
    /// [`Self::max_frame_len`].
    pub const fn validate_frame_len(self, claimed: ParserFrameLen) -> Result<(), ParserError> {
        if claimed.is_at_most(self.max_frame_len) {
            Ok(())
        } else {
            Err(ParserError::OversizedFrame {
                claimed,
                max: self.max_frame_len,
            })
        }
    }
}

enum ProtocolSelectorUnit {}
enum ProtocolTagUnit {}
enum VescControllerIdUnit {}
enum BmsCellValuesPerPageUnit {}
enum BmsTemperatureValuesPerPageUnit {}
enum BmsPackIndexUnit {}
enum BmsHalfIndexUnit {}
enum BmsCellIndexUnit {}

macro_rules! typed_protocol_value {
    ($name:ident, $unit:ident, $inner:ty, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name {
            value: $inner,
            _unit: PhantomData<fn() -> $unit>,
        }

        impl $name {
            /// Creates the typed protocol value.
            #[must_use]
            pub const fn new(value: $inner) -> Self {
                Self {
                    value,
                    _unit: PhantomData,
                }
            }

            /// Returns the underlying primitive value for FFI/rendering edges.
            #[must_use]
            pub const fn get(self) -> $inner {
                self.value
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.value).finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.value.fmt(f)
            }
        }
    };
}

/// Bytes dropped while recovering from malformed or excessive parser input.
pub type ParserDroppedBytes = Quantity<Information, ParserDroppedByte, u64>;

impl ParserDroppedBytes {
    /// Creates a dropped parser byte count from bytes.
    #[must_use]
    pub const fn from_bytes(value: u64) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this dropped parser byte count in bytes.
    #[must_use]
    pub const fn as_bytes(self) -> u64 {
        self.unit_value()
    }

    /// Adds dropped bytes, saturating at `u64::MAX`.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self::from_bytes(self.as_bytes().saturating_add(other.as_bytes()))
    }
}

/// Saturating count for one class of parser diagnostic event.
pub type ParserDiagnosticCount = Quantity<Count, ParserDiagnosticEvent, u64>;

impl ParserDiagnosticCount {
    /// Creates a parser diagnostic count from event count.
    #[must_use]
    pub const fn from_events(value: u64) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this parser diagnostic count as event count.
    #[must_use]
    pub const fn as_events(self) -> u64 {
        self.unit_value()
    }

    /// Adds one diagnostic event, saturating at `u64::MAX`.
    #[must_use]
    pub const fn next(self) -> Self {
        Self::from_events(self.as_events().saturating_add(1))
    }

    /// Adds diagnostic events, saturating at `u64::MAX`.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self::from_events(self.as_events().saturating_add(other.as_events()))
    }
}

/// Size of one parser frame or claimed parser frame.
pub type ParserFrameLen = Quantity<Information, ParserFrameByte, usize>;

impl ParserFrameLen {
    /// Returns true when this frame length is less than or equal to another.
    #[must_use]
    pub const fn is_at_most(self, other: Self) -> bool {
        self.as_bytes() <= other.as_bytes()
    }
}

/// Maximum buffered parser input size.
pub type ParserBufferedLen = Quantity<Information, ParserBufferByte, usize>;

/// Maximum queued parser output count.
pub type ParserQueuedOutputCount = Quantity<Count, ParserQueuedOutput, usize>;

impl ParserQueuedOutputCount {
    /// Creates a parser queued-output count from output count.
    #[must_use]
    pub const fn from_outputs(value: usize) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this parser queued-output count as output count.
    #[must_use]
    pub const fn as_outputs(self) -> usize {
        self.unit_value()
    }
}

/// Parser failure reason that can be counted without tying core to a protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParserError {
    /// A frame claimed or accumulated more bytes than allowed.
    OversizedFrame {
        /// Claimed or observed frame length.
        claimed: ParserFrameLen,

        /// Configured maximum accepted frame length.
        max: ParserFrameLen,
    },

    /// A frame checksum did not match its payload.
    BadChecksum,

    /// Input bytes could not form a valid frame.
    MalformedFrame,

    /// A parser deadline elapsed before the expected data arrived.
    Timeout {
        /// Elapsed time.
        elapsed: Duration,

        /// Timeout threshold.
        timeout: Duration,
    },

    /// A reply could not be matched to an in-flight request.
    UnmatchedReply,
}

/// Saturating parser diagnostics counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParserDiagnostics {
    /// Bytes dropped while recovering from malformed or excessive input.
    pub dropped_bytes: ParserDroppedBytes,

    /// Parser resynchronization attempts.
    pub resyncs: ParserDiagnosticCount,

    /// Frames rejected because their checksum did not match.
    pub bad_checksums: ParserDiagnosticCount,

    /// Parser deadlines that elapsed before expected data arrived.
    pub timeouts: ParserDiagnosticCount,

    /// Frames rejected because they exceeded parser limits.
    pub oversized_frames: ParserDiagnosticCount,

    /// Frames rejected because their structure was invalid.
    pub malformed_frames: ParserDiagnosticCount,

    /// Replies that could not be matched to an in-flight request.
    pub unmatched_replies: ParserDiagnosticCount,
}

impl ParserDiagnostics {
    /// Adds dropped bytes using saturating arithmetic.
    pub const fn add_dropped_bytes(&mut self, count: ParserDroppedBytes) {
        self.dropped_bytes = self.dropped_bytes.saturating_add(count);
    }

    /// Records one parser resynchronization attempt.
    pub const fn record_resync(&mut self) {
        self.resyncs = self.resyncs.next();
    }

    /// Records one parser error in the corresponding diagnostics counter.
    pub const fn record_error(&mut self, error: ParserError) {
        match error {
            ParserError::OversizedFrame { .. } => {
                self.oversized_frames = self.oversized_frames.next();
            }
            ParserError::BadChecksum => {
                self.bad_checksums = self.bad_checksums.next();
            }
            ParserError::MalformedFrame => {
                self.malformed_frames = self.malformed_frames.next();
            }
            ParserError::Timeout { .. } => {
                self.timeouts = self.timeouts.next();
            }
            ParserError::UnmatchedReply => {
                self.unmatched_replies = self.unmatched_replies.next();
            }
        }
    }

    /// Merges another diagnostics snapshot using saturating arithmetic.
    pub const fn merge(&mut self, other: Self) {
        self.dropped_bytes = self.dropped_bytes.saturating_add(other.dropped_bytes);
        self.resyncs = self.resyncs.saturating_add(other.resyncs);
        self.bad_checksums = self.bad_checksums.saturating_add(other.bad_checksums);
        self.timeouts = self.timeouts.saturating_add(other.timeouts);
        self.oversized_frames = self.oversized_frames.saturating_add(other.oversized_frames);
        self.malformed_frames = self.malformed_frames.saturating_add(other.malformed_frames);
        self.unmatched_replies = self
            .unmatched_replies
            .saturating_add(other.unmatched_replies);
    }
}

/// Stable host-facing diagnostic counter snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticSnapshot {
    /// Bytes dropped while recovering from malformed or excessive input.
    pub dropped_bytes: ParserDroppedBytes,

    /// Parser resynchronization attempts.
    pub resyncs: ParserDiagnosticCount,

    /// Frames rejected because their checksum did not match.
    pub bad_checksums: ParserDiagnosticCount,

    /// Parser deadlines that elapsed before expected data arrived.
    pub timeouts: ParserDiagnosticCount,

    /// Frames rejected because they exceeded parser limits.
    pub oversized_frames: ParserDiagnosticCount,

    /// Frames rejected because their structure was invalid.
    pub malformed_frames: ParserDiagnosticCount,

    /// Replies that could not be matched to an in-flight request.
    pub unmatched_replies: ParserDiagnosticCount,
}

impl DiagnosticSnapshot {
    /// Creates a stable host-facing snapshot from parser diagnostics.
    #[must_use]
    pub const fn from_parser_diagnostics(diagnostics: ParserDiagnostics) -> Self {
        Self {
            dropped_bytes: diagnostics.dropped_bytes,
            resyncs: diagnostics.resyncs,
            bad_checksums: diagnostics.bad_checksums,
            timeouts: diagnostics.timeouts,
            oversized_frames: diagnostics.oversized_frames,
            malformed_frames: diagnostics.malformed_frames,
            unmatched_replies: diagnostics.unmatched_replies,
        }
    }

    /// Creates a diagnostic snapshot when the event carries diagnostics.
    #[must_use]
    pub const fn from_device_event(event: DeviceEvent) -> Option<Self> {
        match event {
            DeviceEvent::Diagnostics(diagnostics) => {
                Some(Self::from_parser_diagnostics(diagnostics))
            }
            DeviceEvent::LinkUp(_)
            | DeviceEvent::LinkDown
            | DeviceEvent::Tick { .. }
            | DeviceEvent::Telemetry(_)
            | DeviceEvent::ReadOnlyResponse(_)
            | DeviceEvent::ControlRefusal(_)
            | DeviceEvent::DiagnosticError(_) => None,
        }
    }
}

/// Stable host-facing parser error kind.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticErrorKind {
    /// A frame claimed or accumulated more bytes than allowed.
    OversizedFrame,

    /// A frame checksum did not match its payload.
    BadChecksum,

    /// Input bytes could not form a valid frame.
    MalformedFrame,

    /// A parser deadline elapsed before expected data arrived.
    Timeout,

    /// A reply could not be matched to an in-flight request.
    UnmatchedReply,
}

/// Stable host-facing parser error details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticError {
    /// Stable diagnostic error discriminator.
    pub kind: DiagnosticErrorKind,

    /// Claimed or observed frame length for oversized-frame errors.
    pub claimed_len: Option<ParserFrameLen>,

    /// Configured maximum frame length for oversized-frame errors.
    pub max_len: Option<ParserFrameLen>,

    /// Elapsed time for timeout errors.
    pub elapsed: Option<Duration>,

    /// Timeout threshold for timeout errors.
    pub timeout: Option<Duration>,
}

impl DiagnosticError {
    /// Creates stable host-facing error details from a parser error.
    #[must_use]
    pub const fn from_parser_error(error: ParserError) -> Self {
        match error {
            ParserError::OversizedFrame { claimed, max } => Self {
                kind: DiagnosticErrorKind::OversizedFrame,
                claimed_len: Some(claimed),
                max_len: Some(max),
                elapsed: None,
                timeout: None,
            },
            ParserError::BadChecksum => Self::without_details(DiagnosticErrorKind::BadChecksum),
            ParserError::MalformedFrame => {
                Self::without_details(DiagnosticErrorKind::MalformedFrame)
            }
            ParserError::Timeout { elapsed, timeout } => Self {
                kind: DiagnosticErrorKind::Timeout,
                claimed_len: None,
                max_len: None,
                elapsed: Some(elapsed),
                timeout: Some(timeout),
            },
            ParserError::UnmatchedReply => {
                Self::without_details(DiagnosticErrorKind::UnmatchedReply)
            }
        }
    }

    const fn without_details(kind: DiagnosticErrorKind) -> Self {
        Self {
            kind,
            claimed_len: None,
            max_len: None,
            elapsed: None,
            timeout: None,
        }
    }
}

/// Size of one transport notification payload after capture/parser admission.
pub type NotificationByteLen = Quantity<Information, NotificationPayloadByte, usize>;

/// Replay split size for one notification chunk; zero preserves whole-notification replay.
pub type NotificationChunkLen = Quantity<Information, NotificationChunkByte, usize>;

impl NotificationChunkLen {
    /// Returns true when replay should preserve whole notifications.
    #[must_use]
    pub const fn is_whole(self) -> bool {
        self.as_bytes() == 0
    }
}

/// Size of a protocol payload body after selector/tag framing bytes are removed.
pub type PayloadBodyLen = Quantity<Information, PayloadBodyByte, usize>;

/// Number of semantic events emitted from one protocol ingest operation.
pub type SemanticEventCount = Quantity<Count, SemanticEvent, usize>;

typed_protocol_value!(
    ProtocolSelector,
    ProtocolSelectorUnit,
    u8,
    "Protocol selector or page identifier carried by a parsed family payload."
);

typed_protocol_value!(
    ProtocolTag,
    ProtocolTagUnit,
    u16,
    "Protocol tag or opcode carried by a parsed family payload."
);

typed_protocol_value!(
    VescControllerId,
    VescControllerIdUnit,
    u8,
    "VESC CAN controller identifier used for forwarded read-only requests."
);

typed_protocol_value!(
    BmsCellValuesPerPage,
    BmsCellValuesPerPageUnit,
    u8,
    "Cell-voltage value count decoded from a full BMS cell page."
);

typed_protocol_value!(
    BmsTemperatureValuesPerPage,
    BmsTemperatureValuesPerPageUnit,
    u8,
    "Temperature value count decoded from a full BMS temperature page."
);

typed_protocol_value!(
    BmsPackIndex,
    BmsPackIndexUnit,
    u8,
    "Zero-based BMS pack index inferred from protocol page metadata."
);

typed_protocol_value!(
    BmsHalfIndex,
    BmsHalfIndexUnit,
    u8,
    "Zero-based BMS half-pack index inferred from protocol page metadata."
);

typed_protocol_value!(
    BmsCellIndex,
    BmsCellIndexUnit,
    u16,
    "Zero-based BMS cell index represented by a decoded cell page."
);

/// Bounded notification evidence shared by protocol ingest outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationEvidence {
    /// Protocol family that accepted or classified the bytes, when known.
    pub family: Option<ProtocolFamily>,

    /// Logical protocol channel used for session ingest.
    pub channel: GattChannel,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MonotonicTimestamp,

    /// Number of notification bytes observed.
    pub len: NotificationByteLen,
}

impl NotificationEvidence {
    /// Creates bounded notification evidence without retaining raw bytes.
    #[must_use]
    pub const fn new(
        family: Option<ProtocolFamily>,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self {
            family,
            channel,
            monotonic_ms,
            len,
        }
    }
}

/// Bounded evidence for protocol payloads that are known but intentionally not
/// decoded as stable telemetry yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadClassifier {
    /// Protocol selector/page id.
    Selector(ProtocolSelector),

    /// Protocol tag/opcode.
    Tag(ProtocolTag),
}

impl PayloadClassifier {
    /// Creates selector-based payload evidence.
    #[must_use]
    pub const fn selector(selector: ProtocolSelector) -> Self {
        Self::Selector(selector)
    }

    /// Creates tag-based payload evidence.
    #[must_use]
    pub const fn tag(tag: ProtocolTag) -> Self {
        Self::Tag(tag)
    }

    /// Returns the selector value when this evidence is selector-based.
    #[must_use]
    pub const fn selector_value(self) -> Option<ProtocolSelector> {
        match self {
            Self::Selector(selector) => Some(selector),
            Self::Tag(_) => None,
        }
    }

    /// Returns the tag value when this evidence is tag-based.
    #[must_use]
    pub const fn tag_value(self) -> Option<ProtocolTag> {
        match self {
            Self::Selector(_) => None,
            Self::Tag(tag) => Some(tag),
        }
    }
}

/// Bounded evidence for protocol payloads that are known but intentionally not
/// decoded as stable telemetry yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservedPayloadEvidence {
    /// Typed payload classifier for the family.
    pub classifier: PayloadClassifier,

    /// Length of the classified body, without retaining raw bytes.
    pub body_len: PayloadBodyLen,

    /// Verification status for this reserved-payload classification.
    pub verification: VerificationStatus,
}

/// Bounded evidence for a known-family payload that still has no stable parser
/// mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserGapEvidence {
    /// Typed payload classifier for the family.
    pub classifier: PayloadClassifier,

    /// Length of the unparsed body, without retaining raw bytes.
    pub body_len: PayloadBodyLen,
}

/// Typed result of feeding one transport notification into a protocol decoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationIngestOutcome {
    /// The notification produced one or more semantic session events.
    SemanticEvents {
        /// Bounded notification evidence.
        notification: NotificationEvidence,

        /// Number of semantic events produced by this ingest step.
        event_count: SemanticEventCount,
    },

    /// The protocol accepted the bytes but is still waiting for a complete
    /// frame/message.
    BufferedFragment(NotificationEvidence),

    /// The protocol produced parser diagnostics for the notification.
    ParserDiagnostic {
        /// Bounded notification evidence.
        notification: NotificationEvidence,

        /// Parser error emitted for this ingest step.
        error: ParserError,
    },

    /// The protocol recognized the payload as known/reserved evidence.
    KnownReserved {
        /// Bounded notification evidence.
        notification: NotificationEvidence,

        /// Reserved payload evidence without raw bytes.
        payload: ReservedPayloadEvidence,
    },

    /// The protocol family accepted the notification but lacks a stable mapping
    /// for the payload.
    ParserGap {
        /// Bounded notification evidence.
        notification: NotificationEvidence,

        /// Parser-gap evidence without raw bytes.
        gap: ParserGapEvidence,
    },

    /// The session ignored the notification, usually because it arrived on the
    /// wrong logical channel for the selected protocol model.
    Ignored(NotificationEvidence),
}

impl NotificationIngestOutcome {
    /// Creates a semantic-events outcome.
    #[must_use]
    pub const fn semantic_events(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
        event_count: SemanticEventCount,
    ) -> Self {
        Self::SemanticEvents {
            notification: NotificationEvidence::new(Some(family), channel, len, monotonic_ms),
            event_count,
        }
    }

    /// Creates an accepted buffered-fragment outcome.
    #[must_use]
    pub const fn buffered_fragment(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::BufferedFragment(NotificationEvidence::new(
            Some(family),
            channel,
            len,
            monotonic_ms,
        ))
    }

    /// Creates a parser-diagnostic outcome.
    #[must_use]
    pub const fn parser_diagnostic(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
        error: ParserError,
    ) -> Self {
        Self::ParserDiagnostic {
            notification: NotificationEvidence::new(Some(family), channel, len, monotonic_ms),
            error,
        }
    }

    /// Creates a known-reserved payload outcome.
    #[must_use]
    pub const fn known_reserved(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
        payload: ReservedPayloadEvidence,
    ) -> Self {
        Self::KnownReserved {
            notification: NotificationEvidence::new(Some(family), channel, len, monotonic_ms),
            payload,
        }
    }

    /// Creates a parser-gap outcome.
    #[must_use]
    pub const fn parser_gap(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
        gap: ParserGapEvidence,
    ) -> Self {
        Self::ParserGap {
            notification: NotificationEvidence::new(Some(family), channel, len, monotonic_ms),
            gap,
        }
    }

    /// Creates an ignored wrong-channel/unsupported notification outcome.
    #[must_use]
    pub const fn ignored_wrong_channel(
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored(NotificationEvidence::new(None, channel, len, monotonic_ms))
    }
}
