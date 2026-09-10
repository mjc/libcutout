use std::sync::{Mutex, MutexGuard, PoisonError};

use cutout_music::callback_epoch::{
    AuthorizationTransaction, AuthorizationTransactionKind, AuthorizationTransactionMatch,
    CallbackEpoch, CallbackEpochMatch,
};

/// Whether a provider callback belongs to the active SDK object.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicCallbackEpochMatch {
    /// The callback belongs to the active epoch.
    Current,
    /// The callback belongs to a retired or superseded epoch.
    Stale,
}

/// Kind of Spotify authorization operation begun by the platform adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicAuthorizationKind {
    Authorizing,
    Renewing,
}

/// Current terminal authorization operation, or a stale callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicAuthorizationMatch {
    Authorizing,
    Renewing,
    Stale,
}

/// Rust-owned identity for one replaceable provider callback source.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileMusicCallbackEpoch {
    inner: Mutex<CallbackEpoch>,
}

/// Rust-owned identity and kind for the active Spotify authorization operation.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileMusicAuthorizationTransaction {
    inner: Mutex<AuthorizationTransaction>,
}

#[uniffi::export]
impl MobileMusicCallbackEpoch {
    /// Creates an idle callback epoch.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Begins a new epoch and returns its identity.
    #[must_use]
    pub fn begin(&self) -> u64 {
        self.lock_inner().begin()
    }

    /// Classifies a callback without ending its epoch.
    #[must_use]
    pub fn classify(&self, id: u64) -> MobileMusicCallbackEpochMatch {
        Self::map(self.lock_inner().classify(id))
    }

    /// Ends the epoch only for its current terminal callback.
    #[must_use]
    pub fn finish(&self, id: u64) -> MobileMusicCallbackEpochMatch {
        Self::map(self.lock_inner().finish(id))
    }

    /// Retires the active callback source synchronously.
    pub fn invalidate(&self) {
        self.lock_inner().invalidate();
    }
}

#[uniffi::export]
impl MobileMusicAuthorizationTransaction {
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn begin(&self, kind: MobileMusicAuthorizationKind) -> u64 {
        self.lock_inner().begin(kind.into())
    }

    #[must_use]
    pub fn classify(&self, id: u64) -> MobileMusicAuthorizationMatch {
        self.lock_inner().classify(id).into()
    }

    #[must_use]
    pub fn finish(&self, id: u64) -> MobileMusicAuthorizationMatch {
        self.lock_inner().finish(id).into()
    }

    pub fn invalidate(&self) {
        self.lock_inner().invalidate();
    }
}

impl MobileMusicCallbackEpoch {
    fn lock_inner(&self) -> MutexGuard<'_, CallbackEpoch> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn map(outcome: CallbackEpochMatch) -> MobileMusicCallbackEpochMatch {
        match outcome {
            CallbackEpochMatch::Current => MobileMusicCallbackEpochMatch::Current,
            CallbackEpochMatch::Stale => MobileMusicCallbackEpochMatch::Stale,
        }
    }
}

impl MobileMusicAuthorizationTransaction {
    fn lock_inner(&self) -> MutexGuard<'_, AuthorizationTransaction> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl From<MobileMusicAuthorizationKind> for AuthorizationTransactionKind {
    fn from(value: MobileMusicAuthorizationKind) -> Self {
        match value {
            MobileMusicAuthorizationKind::Authorizing => Self::Authorizing,
            MobileMusicAuthorizationKind::Renewing => Self::Renewing,
        }
    }
}

impl From<AuthorizationTransactionMatch> for MobileMusicAuthorizationMatch {
    fn from(value: AuthorizationTransactionMatch) -> Self {
        match value {
            AuthorizationTransactionMatch::Authorizing => Self::Authorizing,
            AuthorizationTransactionMatch::Renewing => Self::Renewing,
            AuthorizationTransactionMatch::Stale => Self::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MobileMusicAuthorizationKind, MobileMusicAuthorizationMatch,
        MobileMusicAuthorizationTransaction, MobileMusicCallbackEpoch,
        MobileMusicCallbackEpochMatch,
    };

    #[test]
    fn binding_rejects_callbacks_queued_before_invalidation() {
        let epoch = MobileMusicCallbackEpoch::new();
        let retired = epoch.begin();
        epoch.invalidate();
        let current = epoch.begin();
        assert_eq!(
            epoch.classify(retired),
            MobileMusicCallbackEpochMatch::Stale
        );
        assert_eq!(
            epoch.finish(current),
            MobileMusicCallbackEpochMatch::Current
        );
        assert_eq!(
            epoch.classify(current),
            MobileMusicCallbackEpochMatch::Stale
        );
    }

    #[test]
    fn authorization_binding_returns_the_terminal_operation_kind_once() {
        let transaction = MobileMusicAuthorizationTransaction::new();
        let generation = transaction.begin(MobileMusicAuthorizationKind::Renewing);
        assert_eq!(
            transaction.finish(generation),
            MobileMusicAuthorizationMatch::Renewing
        );
        assert_eq!(
            transaction.finish(generation),
            MobileMusicAuthorizationMatch::Stale
        );
    }
}
