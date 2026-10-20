use std::{future::Future, pin::Pin, time::Duration};

use super::reports::charging_account_token_is_well_formed;
use libcutout_persistence::{CommunityAccountState, StorageError};

/// Validates the service's unsigned millisecond timestamp for durable local storage.
#[must_use]
pub const fn charging_deletion_timestamp_is_valid(timestamp_ms: u64) -> bool {
    timestamp_ms > 0 && timestamp_ms <= i64::MAX as u64
}

/// Remote deletion failures retained across the UI boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountRemoteError {
    Unauthorized,
    Conflict,
    Rejected,
    Throttled,
    Network,
}

/// Persistence effect failure, including a worker that stopped before returning.
#[derive(Debug, thiserror::Error)]
pub enum AccountStorageError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("persistence worker stopped before returning")]
    WorkerStopped,
}

/// Failure returned by the portable account-deletion workflow.
#[derive(Debug, thiserror::Error)]
pub enum AccountDeletionError {
    #[error("invalid account token")]
    Unauthorized,
    #[error("account deletion transport failed")]
    Network,
    #[error("account deletion storage failed: {0:?}")]
    Storage(#[from] AccountStorageError),
    #[error("account deletion state did not settle")]
    Unsettled,
    #[error("account deletion service failed: {0:?}")]
    Remote(AccountRemoteError),
}

/// Effect port for the durable account deletion sequence.
pub trait AccountDeletionPort {
    fn begin_deletion(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<CommunityAccountState, AccountStorageError>> + Send + '_>>;
    /// Waits for sync ownership and performs deletion while retaining the exclusive guard.
    fn delete_remote_exclusive<'a>(
        &'a self,
        token: &'a str,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<u64, AccountRemoteError>> + Send + 'a>>;
    fn complete_deletion(
        &self,
        completed_at_ms: u64,
    ) -> Pin<Box<dyn Future<Output = Result<CommunityAccountState, AccountStorageError>> + Send + '_>>;
}

/// Executes community deletion with the durable outbox fence committed before waiting for sync.
///
/// # Errors
/// Returns an error for a malformed token, unavailable durable state, remote failure, or failed
/// completion receipt. The deletion fence remains committed when the remote outcome is uncertain.
pub async fn delete_community_account<P: AccountDeletionPort>(
    port: &P,
    token: &str,
) -> Result<u64, AccountDeletionError> {
    if !charging_account_token_is_well_formed(token) {
        return Err(AccountDeletionError::Unauthorized);
    }
    // Fence first: older sync receipts cannot regain ownership after this call.
    let state = port.begin_deletion().await?;
    if let CommunityAccountState::Deleted {
        completed_at_milliseconds,
    } = state
    {
        return Ok(completed_at_milliseconds);
    }
    let completed_at_ms = port
        .delete_remote_exclusive(token, Duration::from_secs(20))
        .await
        .map_err(AccountDeletionError::Remote)?;
    if !charging_deletion_timestamp_is_valid(completed_at_ms) {
        return Err(AccountDeletionError::Network);
    }
    match port.complete_deletion(completed_at_ms).await? {
        CommunityAccountState::Deleted {
            completed_at_milliseconds,
        } => Ok(completed_at_milliseconds),
        CommunityAccountState::Active | CommunityAccountState::Deleting => {
            Err(AccountDeletionError::Unsettled)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletion_timestamp_must_fit_the_signed_persistence_clock() {
        assert!(!charging_deletion_timestamp_is_valid(0));
        assert!(charging_deletion_timestamp_is_valid(1));
        assert!(charging_deletion_timestamp_is_valid(i64::MAX as u64));
        assert!(!charging_deletion_timestamp_is_valid(i64::MAX as u64 + 1));
    }
}
