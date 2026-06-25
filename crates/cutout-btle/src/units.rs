use std::{fmt, num::NonZeroUsize, time::Duration};

use cutout_core::{Count, Information, Quantity, Unit};

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

/// Notification payload byte total storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationTotalByte;

impl Unit for NotificationTotalByte {
    type Dimension = Information;
}

/// Total notification payload bytes observed across a bridge report.
pub type NotificationPayloadTotal = Quantity<Information, NotificationTotalByte, usize>;

/// Typed session report counter backed by a zero-sized semantic unit.
pub(crate) type SessionCount<Tag> = Quantity<Count, Tag, usize>;

/// Zero-sized tag for protocol writes produced by a protocol session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProtocolWriteCountTag;

impl Unit for ProtocolWriteCountTag {
    type Dimension = Count;
}

/// Zero-sized tag for transport writes executed by the BTLE bridge.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TransportWriteCountTag;

impl Unit for TransportWriteCountTag {
    type Dimension = Count;
}

/// Zero-sized tag for transport subscribe operations.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SubscribeCountTag;

impl Unit for SubscribeCountTag {
    type Dimension = Count;
}

/// Zero-sized tag for notification payloads relayed into a session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NotificationCountTag;

impl Unit for NotificationCountTag {
    type Dimension = Count;
}

/// Zero-sized tag for semantic telemetry events emitted by a session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TelemetryEventCountTag;

impl Unit for TelemetryEventCountTag {
    type Dimension = Count;
}

/// Zero-sized tag for read-only response events emitted by a session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReadOnlyResponseCountTag;

impl Unit for ReadOnlyResponseCountTag {
    type Dimension = Count;
}

/// Zero-sized tag for parser diagnostics events emitted by a session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticEventCountTag;

impl Unit for DiagnosticEventCountTag {
    type Dimension = Count;
}

/// Zero-sized tag for transport disconnect operations.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DisconnectCountTag;

impl Unit for DisconnectCountTag {
    type Dimension = Count;
}

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

/// Negotiated write byte storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NegotiatedWriteByte;

impl Unit for NegotiatedWriteByte {
    type Dimension = Information;
}

/// Negotiated write capacity exposed by the BTLE stack.
pub type NegotiatedWriteLimit = Quantity<Information, NegotiatedWriteByte, u16>;

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

/// Manufacturer advertisement byte storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManufacturerDataByte;

impl Unit for ManufacturerDataByte {
    type Dimension = Information;
}

/// Size of opaque manufacturer advertisement data.
pub type ManufacturerDataSize = Quantity<Information, ManufacturerDataByte, usize>;
