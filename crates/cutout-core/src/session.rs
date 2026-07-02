//! Transport writes, protocol session IO, and host session facade.

use arrayvec::ArrayVec;
use thiserror::Error;

use crate::{
    ControlRefusal, DeviceCommand, DiagnosticError, GattChannel, LinkInfo, MonotonicTimestamp,
    NotificationIngestOutcome, ParserDiagnostics, ReadOnlyResponse, TelemetryDelta,
    TelemetrySnapshot,
};

/// Maximum payload bytes accepted for a single GATT write value.
pub const MAX_TRANSPORT_WRITE_LEN: usize = 512;

/// Payload bytes stored inline before falling back to an explicit large write.
pub const MAX_INLINE_TRANSPORT_WRITE_LEN: usize = 32;

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
