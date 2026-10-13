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
        .pragma_update(None, "user_version", 2)
        .expect("future version");
    assert_eq!(
        Store::open(&path).expect_err("future schema rejected"),
        Error::Unavailable
    );
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("version");
    assert_eq!(version, 2);
}
