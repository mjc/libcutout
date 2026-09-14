//! Identity for asynchronous provider callback lifecycles.

use std::marker::PhantomData;

use crate::ids::{AuthorizationGeneration, AuthorizationId, LifecycleId};

/// Whether a callback belongs to the active provider object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallbackEpochMatch {
    /// The callback belongs to the active epoch.
    Current,
    /// The callback belongs to a retired or superseded epoch.
    Stale,
}

/// Monotonic identity for one replaceable provider callback source.
#[derive(Debug)]
pub struct CallbackEpoch<K = ()> {
    last_id: Option<u64>,
    active: Option<LifecycleId<K>>,
    marker: PhantomData<fn() -> K>,
}

impl<K> Default for CallbackEpoch<K> {
    fn default() -> Self {
        Self {
            last_id: Some(0),
            active: None,
            marker: PhantomData,
        }
    }
}

/// Authorization operation associated with the current callback epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationTransactionKind {
    /// A user-initiated Spotify authorization.
    Authorizing,
    /// A silent renewal of a stored refresh-bearing session.
    Renewing,
}

/// Classification returned when an authorization operation terminates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationTransactionMatch {
    /// The current user-initiated authorization terminated.
    Authorizing,
    /// The current silent renewal terminated.
    Renewing,
    /// The callback belongs to an inactive or replaced operation.
    Stale,
}

/// Owns the identity and kind of the one active authorization operation.
#[derive(Debug, Default)]
pub struct AuthorizationTransaction {
    epoch: CallbackEpoch<AuthorizationGeneration>,
    kind: Option<AuthorizationTransactionKind>,
}

impl<K> CallbackEpoch<K> {
    /// Whether another distinct identity can be issued.
    #[must_use]
    pub fn can_begin(&self) -> bool {
        self.last_id.is_some_and(|last_id| last_id < u64::MAX)
    }

    /// Begins a new epoch and retires any predecessor.
    #[must_use]
    pub fn begin(&mut self) -> Option<LifecycleId<K>> {
        let next_id = self.last_id?.checked_add(1)?;
        self.last_id = Some(next_id);
        let id = LifecycleId::from_raw(next_id);
        self.active = Some(id);
        Some(id)
    }

    /// Classifies a callback without ending the epoch.
    #[must_use]
    pub fn classify(&self, id: LifecycleId<K>) -> CallbackEpochMatch {
        if self.active == Some(id) {
            CallbackEpochMatch::Current
        } else {
            CallbackEpochMatch::Stale
        }
    }

    /// Ends the epoch only when the terminal callback is current.
    #[must_use]
    pub fn finish(&mut self, id: LifecycleId<K>) -> CallbackEpochMatch {
        let outcome = self.classify(id);
        if outcome == CallbackEpochMatch::Current {
            self.active = None;
        }
        outcome
    }

    /// Retires the active callback source synchronously.
    pub fn invalidate(&mut self) {
        self.active = None;
    }
}

impl AuthorizationTransaction {
    /// Begins an authorization operation and retires any predecessor.
    #[must_use]
    pub fn begin(&mut self, kind: AuthorizationTransactionKind) -> Option<AuthorizationId> {
        let id = self.epoch.begin()?;
        self.kind = Some(kind);
        Some(id)
    }

    /// Classifies a callback without terminating the operation.
    #[must_use]
    pub fn classify(&self, id: AuthorizationId) -> AuthorizationTransactionMatch {
        if self.epoch.classify(id) == CallbackEpochMatch::Stale {
            return AuthorizationTransactionMatch::Stale;
        }
        match self.kind {
            Some(AuthorizationTransactionKind::Authorizing) => {
                AuthorizationTransactionMatch::Authorizing
            }
            Some(AuthorizationTransactionKind::Renewing) => AuthorizationTransactionMatch::Renewing,
            None => AuthorizationTransactionMatch::Stale,
        }
    }

    /// Terminates the operation only when the callback identity is current.
    #[must_use]
    pub fn finish(&mut self, id: AuthorizationId) -> AuthorizationTransactionMatch {
        let outcome = self.classify(id);
        if outcome != AuthorizationTransactionMatch::Stale {
            let _ = self.epoch.finish(id);
            self.kind = None;
        }
        outcome
    }

    /// Retires the active operation synchronously.
    pub fn invalidate(&mut self) {
        self.epoch.invalidate();
        self.kind = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthorizationTransaction, AuthorizationTransactionKind, AuthorizationTransactionMatch,
        CallbackEpoch, CallbackEpochMatch,
    };

    #[test]
    fn invalidation_and_terminal_callbacks_cannot_reopen_an_epoch() {
        let mut epoch: CallbackEpoch<()> = CallbackEpoch::default();
        let first = epoch.begin().expect("first epoch");
        epoch.invalidate();
        assert_eq!(epoch.classify(first), CallbackEpochMatch::Stale);

        let replacement = epoch.begin().expect("replacement epoch");
        assert_eq!(epoch.classify(first), CallbackEpochMatch::Stale);
        assert_eq!(epoch.finish(replacement), CallbackEpochMatch::Current);
        assert_eq!(epoch.classify(replacement), CallbackEpochMatch::Stale);
    }

    #[test]
    fn authorization_identity_preserves_the_operation_kind_until_termination() {
        let mut transaction = AuthorizationTransaction::default();
        let authorization = transaction
            .begin(AuthorizationTransactionKind::Authorizing)
            .expect("authorization");
        assert_eq!(
            transaction.classify(authorization),
            AuthorizationTransactionMatch::Authorizing
        );

        let renewal = transaction
            .begin(AuthorizationTransactionKind::Renewing)
            .expect("renewal");
        assert_eq!(
            transaction.finish(authorization),
            AuthorizationTransactionMatch::Stale
        );
        assert_eq!(
            transaction.finish(renewal),
            AuthorizationTransactionMatch::Renewing
        );
        assert_eq!(
            transaction.classify(renewal),
            AuthorizationTransactionMatch::Stale
        );
    }

    #[test]
    fn invalidation_rejects_a_queued_authorization_callback() {
        let mut transaction = AuthorizationTransaction::default();
        let generation = transaction
            .begin(AuthorizationTransactionKind::Renewing)
            .expect("renewal");
        transaction.invalidate();
        assert_eq!(
            transaction.finish(generation),
            AuthorizationTransactionMatch::Stale
        );
    }

    #[test]
    fn identity_exhaustion_does_not_reuse_the_last_epoch() {
        let mut epoch: CallbackEpoch<()> = CallbackEpoch {
            last_id: Some(u64::MAX),
            active: None,
            marker: std::marker::PhantomData,
        };

        assert!(!epoch.can_begin());
        assert_eq!(epoch.begin(), None);
        assert_eq!(epoch.last_id, Some(u64::MAX));
    }
}
