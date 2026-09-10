//! Identity for asynchronous provider callback lifecycles.

/// Whether a callback belongs to the active provider object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallbackEpochMatch {
    /// The callback belongs to the active epoch.
    Current,
    /// The callback belongs to a retired or superseded epoch.
    Stale,
}

/// Monotonic identity for one replaceable provider callback source.
#[derive(Debug, Default)]
pub struct CallbackEpoch {
    last_id: u64,
    active: Option<u64>,
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
    epoch: CallbackEpoch,
    kind: Option<AuthorizationTransactionKind>,
}

impl CallbackEpoch {
    /// Begins a new epoch and retires any predecessor.
    #[must_use]
    pub fn begin(&mut self) -> u64 {
        self.last_id = self.last_id.wrapping_add(1);
        self.active = Some(self.last_id);
        self.last_id
    }

    /// Classifies a callback without ending the epoch.
    #[must_use]
    pub fn classify(&self, id: u64) -> CallbackEpochMatch {
        if self.active == Some(id) {
            CallbackEpochMatch::Current
        } else {
            CallbackEpochMatch::Stale
        }
    }

    /// Ends the epoch only when the terminal callback is current.
    #[must_use]
    pub fn finish(&mut self, id: u64) -> CallbackEpochMatch {
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
    pub fn begin(&mut self, kind: AuthorizationTransactionKind) -> u64 {
        self.kind = Some(kind);
        self.epoch.begin()
    }

    /// Classifies a callback without terminating the operation.
    #[must_use]
    pub fn classify(&self, id: u64) -> AuthorizationTransactionMatch {
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
    pub fn finish(&mut self, id: u64) -> AuthorizationTransactionMatch {
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
        let mut epoch = CallbackEpoch::default();
        let first = epoch.begin();
        epoch.invalidate();
        assert_eq!(epoch.classify(first), CallbackEpochMatch::Stale);

        let replacement = epoch.begin();
        assert_eq!(epoch.classify(first), CallbackEpochMatch::Stale);
        assert_eq!(epoch.finish(replacement), CallbackEpochMatch::Current);
        assert_eq!(epoch.classify(replacement), CallbackEpochMatch::Stale);
    }

    #[test]
    fn authorization_identity_preserves_the_operation_kind_until_termination() {
        let mut transaction = AuthorizationTransaction::default();
        let authorization = transaction.begin(AuthorizationTransactionKind::Authorizing);
        assert_eq!(
            transaction.classify(authorization),
            AuthorizationTransactionMatch::Authorizing
        );

        let renewal = transaction.begin(AuthorizationTransactionKind::Renewing);
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
        let generation = transaction.begin(AuthorizationTransactionKind::Renewing);
        transaction.invalidate();
        assert_eq!(
            transaction.finish(generation),
            AuthorizationTransactionMatch::Stale
        );
    }
}
