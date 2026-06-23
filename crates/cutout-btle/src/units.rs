use std::{fmt, num::NonZeroUsize, time::Duration};

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

    pub(crate) fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for MonotonicMs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

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

/// Standard BLE Battery Level percentage.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BatteryLevelPercent(u8);

impl BatteryLevelPercent {
    /// Parses the backend byte as a clamped BLE battery percentage.
    #[must_use]
    pub const fn from_backend_byte(value: u8) -> Self {
        Self(if value > 100 { 100 } else { value })
    }

    /// Returns the percentage value for display or serialization.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}
