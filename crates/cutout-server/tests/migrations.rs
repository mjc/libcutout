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
        .pragma_update(None, "user_version", 9)
        .expect("future version");
    assert_eq!(
        Store::open(&path).expect_err("future schema rejected"),
        Error::Unavailable
    );
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("version");
    assert_eq!(version, 9);
}

#[test]
fn schema_one_receives_signup_account_deletion_sync_and_removal_migrations() {
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
    assert_eq!(version, 8);
    assert_capture_byte_ceiling(&connection);
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
    let outbox: u64 = connection
        .query_row("SELECT COUNT(*) FROM alpha_signup_outbox", [], |row| {
            row.get(0)
        })
        .expect("signup outbox table");
    assert_eq!(outbox, 0);
    let removal_requests: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM alpha_signup_removal_requests",
            [],
            |row| row.get(0),
        )
        .expect("removal request table");
    assert_eq!(removal_requests, 0);
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

fn signup_schema(path: &std::path::Path, version: u32) -> Connection {
    let connection = Connection::open(path).expect("database");
    for migration in [
        include_str!("../migrations/001_directory.sql"),
        include_str!("../migrations/002_account_deletion.sql"),
        include_str!("../migrations/003_alpha_signup.sql"),
    ] {
        connection
            .execute_batch(migration)
            .expect("signup schema three");
    }
    if version >= 4 {
        connection
            .execute_batch(include_str!("../migrations/004_alpha_signup_sync.sql"))
            .expect("signup schema four");
    }
    if version >= 5 {
        connection
            .execute_batch(include_str!("../migrations/005_alpha_signup_removal.sql"))
            .expect("signup schema five");
    }
    connection
}

const RETAINED_SPOT_BODY: &str = r#"{"coordinate":{"latitude_e7":400000000,"longitude_e7":-1050000000},"access_note":"Retained outlet","cost":"free"}"#;

fn seed_signup_data(connection: &Connection, version: u32) {
    connection.execute_batch("INSERT INTO contributors(id,token_hash,role,created_at_ms)
        VALUES('00000000-0000-4000-8000-000000000002','existing-moderator-hash','moderator',1);
        INSERT INTO alpha_signup_requests(id,email,email_hash,pev_type,pev_model,requested_accessories,locale,disclosure_revision,received_at_ms)
        VALUES('00000000-0000-4000-8000-000000000004','rider@example.invalid','retained-email-hash','euc','Unverified wheel','[]','en','alpha-1',1);
        INSERT INTO alpha_signup_idempotency(idempotency_key,payload_hash,request_id,receipt_id)
        VALUES('retained-operation','retained-payload-hash','00000000-0000-4000-8000-000000000004','retained-receipt');
        INSERT INTO alpha_signup_outbox(request_id,state,attempts,last_error)
        VALUES('00000000-0000-4000-8000-000000000004','failed',2,'unavailable');").expect("existing signup data");
    connection.execute("INSERT INTO spots(id,owner_id,revision,status,latitude_e7,longitude_e7,cost,body,created_at_ms,updated_at_ms,change_sequence)
        VALUES('00000000-0000-4000-8000-000000000003','00000000-0000-4000-8000-000000000002',1,'published',400000000,-1050000000,'free',?1,1,1,1)",
        [RETAINED_SPOT_BODY]).expect("existing charging spot");
    if version == 5 {
        connection.execute_batch("INSERT INTO alpha_signup_removal_requests(id,email_hash,signup_id,status,requested_at_ms)
            VALUES('retained-removal','retained-email-hash','00000000-0000-4000-8000-000000000004','pending',1);").expect("existing removal");
    }
}

fn assert_retained_signup_data(connection: &Connection, version: u32) {
    let actual: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("version");
    assert_eq!(actual, 7);
    let (role,review):(String,bool)=connection.query_row("SELECT role,private_capture_review FROM contributors WHERE id='00000000-0000-4000-8000-000000000002'",[],|row|Ok((row.get(0)?,row.get(1)?))).expect("moderator");
    assert_eq!(role, "moderator");
    assert!(
        !review,
        "signup moderators must not gain private capture review"
    );
    let body: String = connection
        .query_row("SELECT body FROM spots", [], |row| row.get(0))
        .expect("charging content");
    assert_eq!(body, RETAINED_SPOT_BODY);
    let email: String = connection
        .query_row("SELECT email FROM alpha_signup_requests", [], |row| {
            row.get(0)
        })
        .expect("signup content");
    assert_eq!(email, "rider@example.invalid");
    let (state, attempts, operation): (String, u32, String) = connection
        .query_row(
            "SELECT state,attempts,operation FROM alpha_signup_outbox",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("outbox");
    assert_eq!(
        (state.as_str(), attempts, operation.as_str()),
        ("failed", 2, "sync")
    );
    let receipt: String = connection
        .query_row(
            "SELECT receipt_id FROM alpha_signup_idempotency",
            [],
            |row| row.get(0),
        )
        .expect("idempotency");
    assert_eq!(receipt, "retained-receipt");
    let removals: u64 = connection
        .query_row(
            "SELECT count(*) FROM alpha_signup_removal_requests",
            [],
            |row| row.get(0),
        )
        .expect("removals");
    assert_eq!(removals, u64::from(version == 5));
    let captures: u64 = connection
        .query_row("SELECT count(*) FROM capture_submissions", [], |row| {
            row.get(0)
        })
        .expect("new capture store");
    assert_eq!(captures, 0);
}

#[test]
fn signup_schemas_three_four_and_five_gain_capture_without_losing_existing_data() {
    for version in 3..=5 {
        let directory = TempDir::new().expect("directory");
        let path = directory.path().join(format!("schema-{version}.sqlite"));
        let connection = signup_schema(&path, version);
        seed_signup_data(&connection, version);
        drop(connection);
        assert!(
            Store::open_existing(&path).is_err(),
            "schema {version} must not migrate in existing-only mode"
        );
        drop(Store::open(&path).expect("migrate to combined schema"));
        let connection = Connection::open(&path).expect("migrated database");
        assert_retained_signup_data(&connection, version);
        drop(connection);
        assert!(Store::open_existing(&path).is_ok());
        assert!(Store::open(&path).is_ok(), "migration must not repeat");
    }
}

#[test]
fn unpublished_capture_schema_three_is_refused_without_mutating_its_data() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("old-capture-schema.sqlite");
    let connection = Connection::open(&path).expect("database");
    connection
        .execute_batch(include_str!("../migrations/001_directory.sql"))
        .expect("schema one");
    connection
        .execute_batch(include_str!("../migrations/002_account_deletion.sql"))
        .expect("schema two");
    // This is the actual former capture schema: the additive SQL is unchanged,
    // but that unpublished branch assigned it version three instead of six.
    connection
        .execute_batch(include_str!(
            "../migrations/006_private_capture_submissions.sql"
        ))
        .expect("former capture tables");
    connection
        .pragma_update(None, "user_version", 3)
        .expect("former capture version");
    connection.execute_batch("INSERT INTO contributors(id,token_hash,role,created_at_ms,private_capture_review)
        VALUES('00000000-0000-4000-8000-000000000002','old-private-reviewer-hash','contributor',1,1);
        INSERT INTO capture_submissions(operation_id,owner_id,deleted)
        VALUES('00000000-0000-4000-8000-000000000003','00000000-0000-4000-8000-000000000002',1);").expect("retained cancellation");
    drop(connection);
    assert_eq!(
        Store::open(&path).expect_err("capture schema three is not signup schema three"),
        Error::Unavailable
    );
    assert!(Store::open_existing(&path).is_err());
    let connection = Connection::open(&path).expect("preserved database");
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("version");
    assert_eq!(version, 3);
    let (operation, owner, deleted): (String, String, bool) = connection
        .query_row(
            "SELECT operation_id,owner_id,deleted FROM capture_submissions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("preserved cancellation");
    assert_eq!(operation, "00000000-0000-4000-8000-000000000003");
    assert_eq!(owner, "00000000-0000-4000-8000-000000000002");
    assert!(deleted);
    let (token_hash, review): (String, bool) = connection
        .query_row(
            "SELECT token_hash,private_capture_review FROM contributors WHERE id=?1",
            [owner],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("preserved reviewer");
    assert_eq!(token_hash, "old-private-reviewer-hash");
    assert!(review);
    let signup_tables: u32 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='alpha_signup_outbox'",
            [],
            |row| row.get(0),
        )
        .expect("no partial migration");
    assert_eq!(signup_tables, 0);
}

fn schema_six_with_capture_blobs(path: &std::path::Path) {
    let connection = Connection::open(path).expect("database");
    for migration in [
        include_str!("../migrations/001_directory.sql"),
        include_str!("../migrations/002_account_deletion.sql"),
        include_str!("../migrations/003_alpha_signup.sql"),
        include_str!("../migrations/004_alpha_signup_sync.sql"),
        include_str!("../migrations/005_alpha_signup_removal.sql"),
        include_str!("../migrations/006_private_capture_submissions.sql"),
    ] {
        connection.execute_batch(migration).expect("schema six");
    }
    connection
        .execute_batch(
            "INSERT INTO contributors(id,token_hash,role,created_at_ms)
             VALUES('00000000-0000-4000-8000-000000000002','retained-hash','contributor',1);
             INSERT INTO capture_submissions(
                operation_id,owner_id,metadata,receipt,summary,bytes,payload)
             VALUES('00000000-0000-4000-8000-000000000009',
                '00000000-0000-4000-8000-000000000002','{}','{}','{}',4,X'01020304');
             INSERT INTO capture_submissions(
                operation_id,owner_id,metadata,receipt,summary,bytes,payload)
             VALUES('00000000-0000-4000-8000-00000000000b',
                '00000000-0000-4000-8000-000000000002','{}','{}','{}',3,X'0a0b0c');
             INSERT INTO capture_submissions(operation_id,owner_id,deleted)
             VALUES('00000000-0000-4000-8000-00000000000a',
                '00000000-0000-4000-8000-000000000002',1);",
        )
        .expect("schema six captures");
}

fn capture_object_directory(database: &std::path::Path) -> std::path::PathBuf {
    let mut objects = database
        .canonicalize()
        .expect("database path")
        .into_os_string();
    objects.push(".capture-objects");
    std::path::PathBuf::from(objects)
}

#[test]
fn schema_six_capture_blobs_move_into_files_and_a_finished_file_is_kept() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("schema-six.sqlite");
    schema_six_with_capture_blobs(&path);
    let objects = capture_object_directory(&path);
    let kept = objects.join("00000000-0000-4000-8000-000000000009");
    std::fs::create_dir(&objects).expect("objects from an interrupted extract");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&objects, std::fs::Permissions::from_mode(0o700))
            .expect("private object directory");
    }
    std::fs::write(&kept, b"\x01\x02\x03\x04").expect("already extracted bytes");
    assert!(
        Store::open_existing(&path).is_err(),
        "schema six must not migrate in existing-only mode"
    );

    drop(Store::open(&path).expect("move blobs to files"));
    let connection = Connection::open(&path).expect("migrated database");
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("version");
    assert_eq!(version, 8);
    assert_capture_byte_ceiling(&connection);
    let payload_column: u32 = connection
        .query_row(
            "SELECT count(*) FROM pragma_table_info('capture_submissions') WHERE name='payload'",
            [],
            |row| row.get(0),
        )
        .expect("payload column");
    assert_eq!(payload_column, 0);
    let (bytes, deleted_rows): (u64, u64) = connection
        .query_row(
            "SELECT coalesce(sum(bytes),0), coalesce(sum(deleted),0) FROM capture_submissions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("retained rows");
    assert_eq!((bytes, deleted_rows), (7, 1));
    assert_eq!(
        std::fs::read(&kept).expect("capture file"),
        b"\x01\x02\x03\x04"
    );
    assert_eq!(
        std::fs::read(objects.join("00000000-0000-4000-8000-00000000000b"))
            .expect("extracted blob"),
        b"\x0a\x0b\x0c"
    );
    assert!(
        !objects
            .join("00000000-0000-4000-8000-00000000000a")
            .exists()
    );
    drop(connection);
    assert!(Store::open(&path).is_ok(), "file migration must not repeat");
    assert_eq!(
        std::fs::read(kept).expect("file survives reopen"),
        b"\x01\x02\x03\x04"
    );
}

fn assert_capture_byte_ceiling(connection: &Connection) {
    let sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name = 'capture_submissions'",
            [],
            |row| row.get(0),
        )
        .expect("capture table");
    assert!(sql.contains("20000000000"), "{sql}");
    assert!(!sql.contains("536870912"), "{sql}");
}
