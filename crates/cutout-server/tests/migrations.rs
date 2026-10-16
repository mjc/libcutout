use cutout_server::{Error, Store};
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn a_foreign_database_is_not_migrated_or_repurposed() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("foreign.sqlite");
    let connection = Connection::open(&path).expect("database");
    connection
        .execute_batch(
            "CREATE TABLE another_app (value TEXT); INSERT INTO another_app VALUES ('preserve');",
        )
        .expect("foreign schema");
    assert_eq!(
        Store::open(&path).expect_err("foreign database must be rejected"),
        Error::Unavailable
    );
    let value: String = connection
        .query_row("SELECT value FROM another_app", [], |row| row.get(0))
        .expect("foreign record");
    assert_eq!(value, "preserve");
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("version");
    assert_eq!(version, 0);
}

#[test]
fn an_unknown_future_schema_is_rejected_without_downgrading() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("future.sqlite");
    let connection = Connection::open(&path).expect("database");
    connection
        .pragma_update(None, "user_version", 4)
        .expect("future version");
    assert_eq!(
        Store::open(&path).expect_err("future schema rejected"),
        Error::Unavailable
    );
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("version");
    assert_eq!(version, 4);
}

#[test]
fn schema_one_receives_the_additive_account_deletion_migration() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("schema-one.sqlite");
    let connection = Connection::open(&path).expect("database");
    connection
        .execute_batch(include_str!("../migrations/001_directory.sql"))
        .expect("schema one");
    drop(connection);

    let store = Store::open(&path).expect("migrate schema one");
    drop(store);
    let connection = Connection::open(&path).expect("migrated database");
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("version");
    assert_eq!(version, 3);
    let (token_hash, revoked_at_ms): (String, Option<u64>) = connection
        .query_row(
            "SELECT token_hash, revoked_at_ms FROM contributors
             WHERE id = '00000000-0000-4000-8000-000000000001'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("anonymous system principal");
    assert_eq!(token_hash, "anonymous-system-principal-no-login");
    assert!(revoked_at_ms.is_some());
    let receipts: u64 = connection
        .query_row("SELECT COUNT(*) FROM deletion_receipts", [], |row| {
            row.get(0)
        })
        .expect("deletion receipts table");
    assert_eq!(receipts, 0);
}

#[test]
fn schema_two_adds_private_capture_capability_without_granting_moderators() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("schema-two.sqlite");
    let connection = Connection::open(&path).expect("database");
    connection
        .execute_batch(include_str!("../migrations/001_directory.sql"))
        .expect("schema one");
    connection
        .execute_batch(include_str!("../migrations/002_account_deletion.sql"))
        .expect("schema two");
    connection.execute("INSERT INTO contributors(id,token_hash,role,created_at_ms) VALUES('moderator','retained-hash','moderator',1)",[]).expect("moderator");
    drop(connection);
    assert!(
        Store::open_existing(&path).is_err(),
        "existing mode must not migrate an old schema"
    );
    drop(Store::open(&path).expect("migration"));
    let connection = Connection::open(&path).expect("database");
    let (role, capability): (String, bool) = connection
        .query_row(
            "SELECT role,private_capture_review FROM contributors WHERE id='moderator'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("retained moderator");
    assert_eq!(role, "moderator");
    assert!(!capability);
    assert!(Store::open_existing(&path).is_ok());
}
