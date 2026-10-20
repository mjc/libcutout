#![cfg_attr(test, allow(clippy::disallowed_macros))]

use cutout_application::charging::account::{
    AccountDeletionError, AccountDeletionPort, AccountRemoteError, AccountStorageError,
    delete_community_account,
};
use libcutout_persistence::CommunityAccountState;
use std::{
    future::Future,
    pin::Pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
    time::Duration,
};

#[derive(Default)]
struct Port {
    calls: Mutex<Vec<&'static str>>,
    remote_calls: Mutex<usize>,
    completed: Mutex<Option<u64>>,
}

impl AccountDeletionPort for Port {
    fn begin_deletion(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<CommunityAccountState, AccountStorageError>> + Send + '_>>
    {
        Box::pin(async {
            self.calls.lock().unwrap().push("fence");
            Ok(self.completed.lock().unwrap().map_or(
                CommunityAccountState::Deleting,
                |completed_at_milliseconds| CommunityAccountState::Deleted {
                    completed_at_milliseconds,
                },
            ))
        })
    }
    fn delete_remote_exclusive<'a>(
        &'a self,
        _: &'a str,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<u64, AccountRemoteError>> + Send + 'a>> {
        Box::pin(async move {
            assert_eq!(timeout, Duration::from_secs(20));
            self.calls.lock().unwrap().push("remote");
            let mut count = self.remote_calls.lock().unwrap();
            *count += 1;
            // First request completes remotely but loses its receipt. Retrying is idempotent.
            if *count == 1 {
                Err(AccountRemoteError::Network)
            } else {
                Ok(100)
            }
        })
    }
    fn complete_deletion(
        &self,
        completed_at_ms: u64,
    ) -> Pin<Box<dyn Future<Output = Result<CommunityAccountState, AccountStorageError>> + Send + '_>>
    {
        Box::pin(async move {
            self.calls.lock().unwrap().push("settle");
            *self.completed.lock().unwrap() = Some(completed_at_ms);
            Ok(CommunityAccountState::Deleted {
                completed_at_milliseconds: completed_at_ms,
            })
        })
    }
}

fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("mock effects complete immediately"),
    }
}

#[test]
fn lost_remote_receipt_preserves_fence_and_retry_settles_once() {
    let port = Port::default();
    let token = "a".repeat(64);
    assert!(matches!(
        ready(delete_community_account(&port, &token)),
        Err(AccountDeletionError::Remote(AccountRemoteError::Network))
    ));
    assert_eq!(*port.calls.lock().unwrap(), ["fence", "remote"]);
    assert_eq!(ready(delete_community_account(&port, &token)).unwrap(), 100);
    assert_eq!(
        *port.calls.lock().unwrap(),
        ["fence", "remote", "fence", "remote", "settle"]
    );
    assert_eq!(ready(delete_community_account(&port, &token)).unwrap(), 100);
    assert_eq!(*port.remote_calls.lock().unwrap(), 2);
    assert_eq!(port.calls.lock().unwrap().last(), Some(&"fence"));
}

#[test]
fn invalid_credential_has_no_durable_or_remote_effect() {
    let port = Port::default();
    assert!(matches!(
        ready(delete_community_account(&port, "invalid")),
        Err(AccountDeletionError::Unauthorized)
    ));
    assert_eq!(*port.calls.lock().unwrap(), Vec::<&str>::new());
}
