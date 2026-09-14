//! Identity for asynchronous provider callback lifecycles.

use crate::ids::{AuthorizationId, LifecycleId};

/// Whether a callback belongs to the active provider object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallbackEpochMatch {
    /// The callback belongs to the active epoch.
    Current,
    /// The callback belongs to a retired or superseded epoch.
    Stale,
}

/// Whether a replaceable callback source currently owns an identity.
#[derive(Debug, Default)]
enum CallbackEpochState<K> {
    #[default]
    Idle,
    Active(LifecycleId<K>),
}

/// Monotonic identity for one replaceable provider callback source.
#[derive(Debug)]
pub struct CallbackEpoch<K = ()> {
    last_id: LifecycleId<K>,
    state: CallbackEpochState<K>,
}

impl<K> Default for CallbackEpoch<K> {
    fn default() -> Self {
        Self {
            last_id: LifecycleId::from_raw(0),
            state: CallbackEpochState::Idle,
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

#[derive(Debug, Default)]
enum AuthorizationTransactionState {
    #[default]
    Idle,
    Authorizing(AuthorizationId),
    Renewing(AuthorizationId),
}

/// Owns the identity and kind of the one active authorization operation.
#[derive(Debug)]
pub struct AuthorizationTransaction {
    last_id: AuthorizationId,
    state: AuthorizationTransactionState,
}

impl Default for AuthorizationTransaction {
    fn default() -> Self {
        Self {
            last_id: AuthorizationId::from_raw(0),
            state: AuthorizationTransactionState::Idle,
        }
    }
}

impl<K> CallbackEpoch<K> {
    /// Begins a new epoch and retires any predecessor.
    #[must_use]
    pub fn begin(&mut self) -> Option<LifecycleId<K>> {
        let id = self.last_id.next()?;
        self.last_id = id;
        self.state = CallbackEpochState::Active(id);
        Some(id)
    }

    /// Classifies a callback without ending the epoch.
    #[must_use]
    pub fn classify(&self, id: LifecycleId<K>) -> CallbackEpochMatch {
        match self.state {
            CallbackEpochState::Active(active) if active == id => CallbackEpochMatch::Current,
            CallbackEpochState::Idle | CallbackEpochState::Active(_) => CallbackEpochMatch::Stale,
        }
    }

    /// Ends the epoch only when the terminal callback is current.
    #[must_use]
    pub fn finish(&mut self, id: LifecycleId<K>) -> CallbackEpochMatch {
        let outcome = self.classify(id);
        if outcome == CallbackEpochMatch::Current {
            self.state = CallbackEpochState::Idle;
        }
        outcome
    }

    /// Retires the active callback source synchronously.
    pub fn invalidate(&mut self) {
        self.state = CallbackEpochState::Idle;
    }
}

impl AuthorizationTransaction {
    /// Begins an authorization operation and retires any predecessor.
    #[must_use]
    pub fn begin(&mut self, kind: AuthorizationTransactionKind) -> Option<AuthorizationId> {
        let id = self.last_id.next()?;
        self.last_id = id;
        self.state = match kind {
            AuthorizationTransactionKind::Authorizing => {
                AuthorizationTransactionState::Authorizing(id)
            }
            AuthorizationTransactionKind::Renewing => AuthorizationTransactionState::Renewing(id),
        };
        Some(id)
    }

    /// Classifies a callback without terminating the operation.
    #[must_use]
    pub fn classify(&self, id: AuthorizationId) -> AuthorizationTransactionMatch {
        match self.state {
            AuthorizationTransactionState::Authorizing(active) if active == id => {
                AuthorizationTransactionMatch::Authorizing
            }
            AuthorizationTransactionState::Renewing(active) if active == id => {
                AuthorizationTransactionMatch::Renewing
            }
            AuthorizationTransactionState::Idle
            | AuthorizationTransactionState::Authorizing(_)
            | AuthorizationTransactionState::Renewing(_) => AuthorizationTransactionMatch::Stale,
        }
    }

    /// Terminates the operation only when the callback identity is current.
    #[must_use]
    pub fn finish(&mut self, id: AuthorizationId) -> AuthorizationTransactionMatch {
        let outcome = self.classify(id);
        if outcome != AuthorizationTransactionMatch::Stale {
            self.state = AuthorizationTransactionState::Idle;
        }
        outcome
    }

    /// Retires the active operation synchronously.
    pub fn invalidate(&mut self) {
        self.state = AuthorizationTransactionState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthorizationTransaction, AuthorizationTransactionKind, AuthorizationTransactionMatch,
        CallbackEpoch, CallbackEpochMatch, CallbackEpochState,
    };
    use crate::ids::LifecycleId;

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
            last_id: LifecycleId::from_raw(u64::MAX),
            state: CallbackEpochState::Idle,
        };

        assert_eq!(epoch.begin(), None);
        assert_eq!(epoch.last_id.raw(), u64::MAX);
    }
}
