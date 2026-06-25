use std::{fmt, marker::PhantomData, num::NonZeroUsize, time::Duration};

use cutout_core::NotificationByteLen;

/// Bounded scan duration parsed at the caller boundary.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ScanWindow(Duration);

impl ScanWindow {
    /// Creates a scan window from seconds supplied by a caller.
    #[must_use]
    pub const fn from_secs(seconds: u64) -> Self {
        Self(Duration::from_secs(seconds))
    }

    /// Creates a scan window from milliseconds supplied by a caller.
    #[must_use]
    pub const fn from_millis(milliseconds: u64) -> Self {
        Self(Duration::from_millis(milliseconds))
    }

    pub(crate) const fn as_duration(self) -> Duration {
        self.0
    }
}

impl From<Duration> for ScanWindow {
    fn from(value: Duration) -> Self {
        Self(value)
    }
}

/// Passive notification collection duration parsed at the caller boundary.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NotificationWindow(Duration);

impl NotificationWindow {
    /// Creates a notification window from seconds supplied by a caller.
    #[must_use]
    pub const fn from_secs(seconds: u64) -> Self {
        Self(Duration::from_secs(seconds))
    }

    /// Creates a notification window from milliseconds supplied by a caller.
    #[must_use]
    pub const fn from_millis(milliseconds: u64) -> Self {
        Self(Duration::from_millis(milliseconds))
    }

    #[must_use]
    pub(crate) const fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    pub(crate) const fn as_duration(self) -> Duration {
        self.0
    }
}

impl From<Duration> for NotificationWindow {
    fn from(value: Duration) -> Self {
        Self(value)
    }
}

/// Poll interval used while waiting for a targeted scan match.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ScanPollInterval(Duration);

impl ScanPollInterval {
    pub(crate) const fn from_millis(milliseconds: u64) -> Self {
        Self(Duration::from_millis(milliseconds))
    }

    pub(crate) const fn as_duration(self) -> Duration {
        self.0
    }
}

/// Monotonic session timestamp in milliseconds.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct MonotonicMs(u64);

impl MonotonicMs {
    /// Creates a monotonic millisecond timestamp from an already parsed value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Creates a timestamp from elapsed milliseconds, saturating if necessary.
    #[must_use]
    pub fn from_elapsed_millis(milliseconds: u128) -> Self {
        Self(u64::try_from(milliseconds).unwrap_or(u64::MAX))
    }

    /// Returns the underlying millisecond value for protocol APIs that require it.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Converts this BTLE adapter timestamp into the protocol-core timestamp type.
    #[must_use]
    pub const fn into_core(self) -> cutout_core::MonotonicTimestamp {
        cutout_core::MonotonicTimestamp::new(self.0)
    }

    pub(crate) fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for MonotonicMs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Total notification payload bytes observed across a bridge report.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct NotificationByteTotal(usize);

impl NotificationByteTotal {
    /// Creates a total from an already counted byte value.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Creates a total from one typed notification length.
    #[must_use]
    pub const fn from_len(len: NotificationByteLen) -> Self {
        Self(len.get())
    }

    /// Returns the primitive value for rendering or serialization edges.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// Adds another typed byte total, saturating at `usize::MAX`.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// Adds one typed notification length, saturating at `usize::MAX`.
    #[must_use]
    pub const fn saturating_add_len(self, len: NotificationByteLen) -> Self {
        Self(self.0.saturating_add(len.get()))
    }
}

impl fmt::Display for NotificationByteTotal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Typed session report counter backed by a zero-sized semantic tag.
#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SessionCount<Tag> {
    value: usize,
    tag: PhantomData<fn() -> Tag>,
}

impl<Tag> Clone for SessionCount<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag> Copy for SessionCount<Tag> {}

impl<Tag> Default for SessionCount<Tag> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<Tag> SessionCount<Tag> {
    /// Creates a counter from an already parsed count value.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self {
            value,
            tag: PhantomData,
        }
    }

    /// Returns the primitive value for rendering or serialization edges.
    #[must_use]
    pub const fn get(self) -> usize {
        self.value
    }

    /// Returns true when the counter has no observed events.
    #[must_use]
    pub const fn has_no_events(self) -> bool {
        self.value == 0
    }

    /// Returns true when the counter has at least one observed event.
    #[must_use]
    pub const fn has_events(self) -> bool {
        !self.has_no_events()
    }

    /// Adds one observed event, saturating at `usize::MAX`.
    #[must_use]
    pub const fn increment(self) -> Self {
        Self::new(self.value.saturating_add(1))
    }

    /// Adds another typed event count, saturating at `usize::MAX`.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self::new(self.value.saturating_add(other.value))
    }
}

impl<Tag> fmt::Display for SessionCount<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

/// Zero-sized tag for protocol writes produced by a protocol session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProtocolWriteCountTag;

/// Zero-sized tag for transport writes executed by the BTLE bridge.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TransportWriteCountTag;

/// Zero-sized tag for transport subscribe operations.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SubscribeCountTag;

/// Zero-sized tag for notification payloads relayed into a session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NotificationCountTag;

/// Zero-sized tag for semantic telemetry events emitted by a session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TelemetryEventCountTag;

/// Zero-sized tag for read-only response events emitted by a session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReadOnlyResponseCountTag;

/// Zero-sized tag for parser diagnostics events emitted by a session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticEventCountTag;

/// Zero-sized tag for transport disconnect operations.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DisconnectCountTag;

/// Protocol write actions emitted by a protocol session.
pub type ProtocolWriteCount = SessionCount<ProtocolWriteCountTag>;

/// Transport writes executed through the BTLE bridge.
pub type TransportWriteCount = SessionCount<TransportWriteCountTag>;

/// Transport subscribe operations executed through the BTLE bridge.
pub type SubscribeCount = SessionCount<SubscribeCountTag>;

/// Notification payloads relayed into a protocol session.
pub type NotificationCount = SessionCount<NotificationCountTag>;

/// Semantic telemetry events emitted by a protocol session.
pub type TelemetryEventCount = SessionCount<TelemetryEventCountTag>;

/// Semantic read-only response events emitted by a protocol session.
pub type ReadOnlyResponseCount = SessionCount<ReadOnlyResponseCountTag>;

/// Parser diagnostics events emitted by a protocol session.
pub type DiagnosticEventCount = SessionCount<DiagnosticEventCountTag>;

/// Transport disconnect operations executed through the BTLE bridge.
pub type DisconnectCount = SessionCount<DisconnectCountTag>;

/// Number of reconnect links a session may attempt.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MaxReconnectLinks(NonZeroUsize);

impl MaxReconnectLinks {
    /// Creates a reconnect limit when the caller supplied a non-zero count.
    #[must_use]
    pub const fn new(value: NonZeroUsize) -> Self {
        Self(value)
    }

    /// Parses a reconnect limit, treating zero as one link attempt.
    #[must_use]
    pub fn at_least_one(value: usize) -> Self {
        Self(NonZeroUsize::new(value).unwrap_or(NonZeroUsize::MIN))
    }

    pub(crate) fn attempts(self) -> impl Iterator<Item = ReconnectAttempt> {
        (1..=self.0.get()).map(ReconnectAttempt)
    }

    /// Returns true when more than one connected-link attempt is allowed.
    #[must_use]
    pub const fn has_multiple_links(self) -> bool {
        self.0.get() > 1
    }
}

impl From<NonZeroUsize> for MaxReconnectLinks {
    fn from(value: NonZeroUsize) -> Self {
        Self::new(value)
    }
}

/// One-based reconnect attempt index.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReconnectAttempt(usize);

impl ReconnectAttempt {
    /// Creates a one-based reconnect attempt index.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the one-based attempt number for diagnostics serialization.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    pub(crate) const fn is_first(self) -> bool {
        self.0 == 1
    }
}

/// Write provenance policy used while recording capture evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WriteProvenance {
    /// Writes came from stable protocol encoders.
    #[default]
    Stable,

    /// Writes came from provisional protocol encoders.
    Provisional,
}

impl WriteProvenance {
    pub(crate) const fn is_provisional(self) -> bool {
        matches!(self, Self::Provisional)
    }
}

/// Negotiated write length exposed by the BTLE stack.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NegotiatedWriteLen(u16);

impl NegotiatedWriteLen {
    /// Creates a negotiated write length from a backend MTU value.
    #[must_use]
    pub const fn from_mtu(mtu: u16) -> Self {
        Self(mtu)
    }

    /// Returns a chunk length suitable for transport writes.
    #[must_use]
    pub fn chunk_len(self) -> usize {
        usize::from(self.0).max(1)
    }

    /// Returns the raw negotiated value for capture provenance.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for NegotiatedWriteLen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Standard BLE Battery Level service value.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BtleBatteryLevel(u8);

impl BtleBatteryLevel {
    /// Parses the backend byte as a standard BLE battery percentage.
    ///
    /// Values outside 0..=100 are malformed and stay untyped.
    #[must_use]
    pub const fn from_backend_byte(value: u8) -> Option<Self> {
        if value <= 100 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Returns the battery level as a percentage value for display or serialization.
    #[must_use]
    pub const fn as_percent(self) -> u8 {
        self.0
    }
}

/// Length of opaque manufacturer advertisement data.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ManufacturerDataLen(usize);

impl ManufacturerDataLen {
    /// Creates a manufacturer data length from backend advertisement bytes.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the primitive value for display or serialization edges.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl fmt::Display for ManufacturerDataLen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
