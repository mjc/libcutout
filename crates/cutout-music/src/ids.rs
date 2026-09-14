//! Typed identities for asynchronous music-provider work.

use std::{fmt, hash::Hash, marker::PhantomData};

/// A Rust-owned identity whose marker prevents crossing lifecycle domains.
pub struct LifecycleId<K> {
    value: u64,
    marker: PhantomData<fn() -> K>,
}

impl<K> Copy for LifecycleId<K> {}

impl<K> Clone for LifecycleId<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> fmt::Debug for LifecycleId<K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("LifecycleId")
            .field(&self.value)
            .finish()
    }
}

impl<K> PartialEq for LifecycleId<K> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<K> Eq for LifecycleId<K> {}

impl<K> Hash for LifecycleId<K> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<K> LifecycleId<K> {
    /// Creates an identity from its platform representation.
    #[must_use]
    pub const fn from_raw(value: u64) -> Self {
        Self {
            value,
            marker: PhantomData,
        }
    }

    /// Returns the representation used at the FFI boundary.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.value
    }

    /// Advances this identity without allowing arithmetic overflow to reuse a value.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.value.checked_add(1) {
            Some(value) => Some(Self::from_raw(value)),
            None => None,
        }
    }
}

impl<K> From<u64> for LifecycleId<K> {
    fn from(value: u64) -> Self {
        Self::from_raw(value)
    }
}

impl<K> From<LifecycleId<K>> for u64 {
    fn from(value: LifecycleId<K>) -> Self {
        value.raw()
    }
}

macro_rules! lifecycle_id {
    ($marker:ident, $id:ident) => {
        /// Marker for the `$id` lifecycle identity.
        #[derive(Debug)]
        pub enum $marker {}
        /// Typed identity for the `$id` lifecycle domain.
        pub type $id = LifecycleId<$marker>;
    };
}

lifecycle_id!(MonitorGeneration, MonitorId);
lifecycle_id!(ProviderSessionGeneration, ProviderSessionId);
lifecycle_id!(AuthorizationGeneration, AuthorizationId);
lifecycle_id!(ConnectionAttempt, ConnectionAttemptId);
lifecycle_id!(EstablishedConnection, EstablishedConnectionId);
lifecycle_id!(PlayerStateRequest, PlayerStateRequestId);
lifecycle_id!(ArtworkRequest, ArtworkRequestId);
lifecycle_id!(ArtworkRetry, ArtworkRetryId);
lifecycle_id!(CommandFeedback, CommandFeedbackId);
lifecycle_id!(TransportRequest, TransportRequestId);
lifecycle_id!(HistoryTransition, HistoryTransitionId);

/// Monotonic revision of an authoritative player-state observation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ObservationRevision(u64);

impl ObservationRevision {
    /// Creates a revision from its platform representation.
    #[must_use]
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Returns the representation used at the FFI boundary.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Advances the revision without allowing arithmetic overflow to reuse a value.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

impl From<u64> for ObservationRevision {
    fn from(value: u64) -> Self {
        Self::from_raw(value)
    }
}

impl From<ObservationRevision> for u64 {
    fn from(value: ObservationRevision) -> Self {
        value.raw()
    }
}
