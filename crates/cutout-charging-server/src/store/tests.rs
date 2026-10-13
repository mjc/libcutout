use super::identity::registration_quota;
use super::{Error, QuotaWindow, Registration, Role, Store, hash, quota, state};
use crate::model::SpotStatus;

#[tokio::test]
async fn moderator_credentials_are_rechecked_after_authentication() {
    let directory = tempfile::TempDir::new().expect("directory");
    let path = directory.path().join("moderators.sqlite");
    let store = Store::open(&path).expect("store");
    let identity = store
        .identity(Registration::Operator {
            role: Role::Moderator,
        })
        .await
        .expect("identity");
    let moderator = store
        .authenticate(identity.token.clone())
        .await
        .expect("authenticate")
        .into_moderator()
        .expect("moderator role");
    let connection = rusqlite::Connection::open(&path).expect("database");
    connection
        .execute(
            "UPDATE contributors SET role = 'contributor' WHERE id = ?1",
            [&identity.id],
        )
        .expect("demote moderator");
    assert_moderation_rejects(&store, &moderator, Error::Forbidden).await;
    assert_eq!(
        store
            .authenticate(identity.token.clone())
            .await
            .expect("contributor")
            .into_moderator()
            .expect_err("contributor cannot moderate"),
        Error::Forbidden
    );
    store.revoke(identity.id).await.expect("revoke identity");
    assert_moderation_rejects(&store, &moderator, Error::Unauthorized).await;
}

async fn assert_moderation_rejects(store: &Store, actor: &super::ModeratorActor, expected: Error) {
    use crate::model::{
        ChangesQuery, ModerationAction, ModerationRequest, OperationId, ReportsQuery,
        ResolveRequest,
    };
    let reports = ReportsQuery {
        after: None,
        status: None,
        limit: None,
    }
    .into_validated()
    .expect("reports query");
    assert_eq!(
        store
            .reports(actor.clone(), reports)
            .await
            .expect_err("reports forbidden"),
        expected
    );
    let changes = ChangesQuery {
        after: None,
        limit: None,
    }
    .into_validated()
    .expect("audit query");
    assert_eq!(
        store
            .audit(actor.clone(), changes)
            .await
            .expect_err("audit forbidden"),
        expected
    );
    let target = "00000000-0000-4000-8000-000000000001";
    assert_eq!(
        store
            .moderator_spot(actor.clone(), target.into())
            .await
            .expect_err("spot forbidden"),
        expected
    );
    let moderation = ModerationRequest {
        operation_id: OperationId::parse(target).expect("operation"),
        expected_revision: 1,
        note: "Check access.".into(),
        action: ModerationAction::Hide,
    }
    .into_validated()
    .expect("moderation request");
    assert_eq!(
        store
            .moderate(actor.clone(), target.into(), moderation)
            .await
            .expect_err("moderation forbidden"),
        expected
    );
    let resolution = ResolveRequest {
        operation_id: OperationId::parse(target).expect("operation"),
        expected_revision: 1,
        note: "Check access.".into(),
    }
    .into_validated()
    .expect("resolution request");
    assert_eq!(
        store
            .resolve(actor.clone(), target.into(), resolution)
            .await
            .expect_err("resolution forbidden"),
        expected
    );
}

#[test]
fn published_content_is_checked_at_readback_and_hidden_content_stays_private() {
    let connection = rusqlite::Connection::open_in_memory().expect("database");
    connection
        .execute_batch(include_str!("../../migrations/001_directory.sql"))
        .expect("schema");
    connection
        .execute(
            "INSERT INTO contributors (id, token_hash, role, created_at_ms)
         VALUES ('owner', 'token-hash', 'contributor', 1)",
            [],
        )
        .expect("owner");
    let body = r#"{"coordinate":{"latitude_e7":400000000,"longitude_e7":-1050000000},"access_note":"Ask staff","cost":"free","name":null,"venue":null,"hours":null,"paid_terms":null,"price":null,"supplies":[]}"#;
    connection
        .execute(
            "INSERT INTO spots (id, owner_id, revision, status, latitude_e7, longitude_e7,
            cost, body, created_at_ms, updated_at_ms, change_sequence)
         VALUES ('spot', 'owner', 1, 'published', 400000000, -1050000000,
            'free', ?1, 1, 1, 1)",
            [body],
        )
        .expect("spot");
    let published = state(&connection, "spot").expect("published state");
    assert_eq!(
        published.spot.expect("public content").input.access_note,
        "Ask staff"
    );

    connection
        .execute("UPDATE spots SET body = NULL WHERE id = 'spot'", [])
        .expect("missing body");
    assert_eq!(
        state(&connection, "spot").expect_err("published body required"),
        Error::Unavailable
    );
    connection
        .execute("UPDATE spots SET body = 'malformed' WHERE id = 'spot'", [])
        .expect("malformed body");
    assert_eq!(
        state(&connection, "spot").expect_err("published body must decode"),
        Error::Unavailable
    );

    connection
        .execute("UPDATE spots SET status = 'hidden' WHERE id = 'spot'", [])
        .expect("hidden body");
    let hidden = state(&connection, "spot").expect("hidden state");
    assert_eq!(hidden.status, SpotStatus::Hidden);
    assert!(hidden.spot.is_none());
}

#[tokio::test]
async fn registration_has_a_global_backstop_without_storing_raw_addresses() {
    let directory = tempfile::TempDir::new().expect("directory");
    let store = Store::open(&directory.path().join("quota.sqlite")).expect("store");
    store.call(|connection| {
        for suffix in 1..=200 {
            let transaction = connection.transaction().expect("transaction");
            registration_quota(&transaction, std::net::Ipv4Addr::new(192, 0, 2, suffix).into(), 1_000)?;
            transaction.commit().expect("commit");
        }
        let transaction = connection.transaction().expect("transaction");
        assert_eq!(registration_quota(&transaction, std::net::Ipv4Addr::new(192, 0, 2, 201).into(), 1_000), Err(Error::Throttled));
        drop(transaction);
        let raw: u64 = connection.query_row("SELECT COUNT(*) FROM quotas WHERE principal LIKE '%192.0.2.%'", [], |row| row.get(0)).expect("private principals");
        assert_eq!(raw, 0);
        let global: u64 = connection.query_row("SELECT count FROM quotas WHERE principal='registration-global' AND period_ms=60000", [], |row| row.get(0)).expect("global budget");
        assert_eq!(global, 200, "a rejected registration must not consume another budget");
        Ok(())
    }).await.expect("backstop");
}

#[tokio::test]
async fn quota_budgets_persist_and_reset_only_for_a_new_window() {
    let directory = tempfile::TempDir::new().expect("directory");
    let path = directory.path().join("quota.sqlite");
    let store = Store::open(&path).expect("store");
    let windows = [
        (QuotaWindow::Minute, 60_000),
        (QuotaWindow::Hour, 3_600_000),
        (QuotaWindow::Day, 86_400_000),
    ];
    store
        .call(move |connection| {
            for (window, period) in windows {
                assert_eq!(window.period_ms(), period);
                quota(connection, "contributor", window, 2, 1_000)?;
                quota(connection, "contributor", window, 2, 2_000)?;
            }
            Ok(())
        })
        .await
        .expect("two allowed writes in each window");
    drop(store);
    let reopened = Store::open(&path).expect("reopen");
    reopened
        .call(move |connection| {
            for (window, boundary) in windows {
                assert_eq!(
                    quota(connection, "contributor", window, 2, 3_000),
                    Err(Error::Throttled)
                );
                quota(connection, "contributor", window, 2, boundary)?;
                quota(connection, "contributor", window, 2, boundary + 1)?;
                assert_eq!(
                    quota(connection, "contributor", window, 2, boundary + 2),
                    Err(Error::Throttled)
                );
            }
            Ok(())
        })
        .await
        .expect("each window resets at its boundary");
}

#[tokio::test]
async fn authentication_rejects_unknown_persisted_roles() {
    let directory = tempfile::TempDir::new().expect("directory");
    let path = directory.path().join("roles.sqlite");
    let store = Store::open(&path).expect("store");
    let identity = store
        .identity(Registration::Operator {
            role: Role::Contributor,
        })
        .await
        .expect("identity");
    let connection = rusqlite::Connection::open(&path).expect("inspect database");
    connection
        .execute_batch("PRAGMA ignore_check_constraints=ON")
        .expect("simulate damaged role");
    connection
        .execute(
            "UPDATE contributors SET role='unknown' WHERE id=?1",
            [&identity.id],
        )
        .expect("damaged role");
    assert_eq!(
        store
            .authenticate(identity.token)
            .await
            .expect_err("unknown role rejected"),
        Error::Unavailable
    );
}

#[tokio::test]
async fn contributor_tokens_are_hashed_in_storage_and_redacted_in_debug() {
    let directory = tempfile::TempDir::new().expect("directory");
    let path = directory.path().join("tokens.sqlite");
    let store = Store::open(&path).expect("store");
    let identity = store
        .identity(Registration::Operator {
            role: Role::Contributor,
        })
        .await
        .expect("identity");
    assert!(!format!("{identity:?}").contains(&identity.token));
    let connection = rusqlite::Connection::open(&path).expect("inspect durable record");
    let stored: String = connection
        .query_row(
            "SELECT token_hash FROM contributors WHERE id=?1",
            [&identity.id],
            |row| row.get(0),
        )
        .expect("stored hash");
    assert_eq!(stored, hash(identity.token.as_bytes()));
    assert_ne!(stored, identity.token);
}
