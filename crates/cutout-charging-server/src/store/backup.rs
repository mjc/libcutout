use super::{create_empty, db_error};
use crate::Error;
use rusqlite::Connection;
use std::{fs, path::Path};
use uuid::Uuid;

pub(super) fn backup_database(connection: &Connection, output: &Path) -> Result<(), Error> {
    if output.try_exists().map_err(|_| Error::Unavailable)? {
        return Err(Error::Conflict);
    }
    TemporaryBackup::create(output)?
        .capture(connection)?
        .publish(output)
}

/// Owns the staging file from creation until cleanup, including failed capture.
struct TemporaryBackup {
    path: String,
}

impl TemporaryBackup {
    fn create(output: &Path) -> Result<Self, Error> {
        let temporary = output.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
        let path = temporary.to_str().ok_or(Error::Invalid)?.to_owned();
        create_empty(Path::new(&path)).map_err(|_| Error::Unavailable)?;
        Ok(Self { path })
    }

    fn capture(self, connection: &Connection) -> Result<BackupSnapshot, Error> {
        connection
            .execute("VACUUM INTO ?1", [&self.path])
            .map_err(db_error)?;
        sync_path(Path::new(&self.path))?;
        Ok(BackupSnapshot(self))
    }
}

impl Drop for TemporaryBackup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Publication requires a complete, synced SQLite snapshot.
struct BackupSnapshot(TemporaryBackup);

impl BackupSnapshot {
    fn publish(self, output: &Path) -> Result<(), Error> {
        fs::hard_link(&self.0.path, output).map_err(|_| Error::Unavailable)?;
        let parent = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        sync_path(parent)
    }
}

fn sync_path(path: &Path) -> Result<(), Error> {
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| Error::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::{TemporaryBackup, backup_database};
    use crate::Error;
    use rusqlite::Connection;

    #[test]
    fn publishing_a_snapshot_cannot_replace_a_destination_created_after_capture() {
        let directory = tempfile::TempDir::new().expect("directory");
        let output = directory.path().join("backup.sqlite");
        let connection = Connection::open_in_memory().expect("database");
        let snapshot = TemporaryBackup::create(&output)
            .expect("staging file")
            .capture(&connection)
            .expect("complete snapshot");
        std::fs::write(&output, b"existing output").expect("competing destination");
        assert_eq!(snapshot.publish(&output), Err(Error::Unavailable));
        assert_eq!(
            std::fs::read(&output).expect("preserved output"),
            b"existing output"
        );
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("directory entries")
                .count(),
            1
        );
    }

    #[test]
    fn publishing_a_snapshot_preserves_data_and_removes_its_staging_file() {
        let directory = tempfile::TempDir::new().expect("directory");
        let output = directory.path().join("backup.sqlite");
        let connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch("CREATE TABLE sample (value INTEGER); INSERT INTO sample VALUES (42)")
            .expect("source data");
        backup_database(&connection, &output).expect("backup");
        let backup = Connection::open(&output).expect("snapshot");
        let value: u32 = backup
            .query_row("SELECT value FROM sample", [], |row| row.get(0))
            .expect("backed-up data");
        assert_eq!(value, 42);
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("directory entries")
                .count(),
            1
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::metadata(&output)
                .expect("output metadata")
                .permissions();
            assert_eq!(permissions.mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn failed_snapshot_leaves_no_output_or_staging_file() {
        let directory = tempfile::TempDir::new().expect("directory");
        let output = directory.path().join("backup.sqlite");
        let connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch("BEGIN")
            .expect("active transaction");
        assert_eq!(
            backup_database(&connection, &output),
            Err(Error::Unavailable)
        );
        assert!(!output.exists());
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("directory entries")
                .count(),
            0
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_backup_paths_are_rejected_before_creating_a_file() {
        use std::os::unix::ffi::OsStringExt;
        let directory = tempfile::TempDir::new().expect("directory");
        let output = directory
            .path()
            .join(std::ffi::OsString::from_vec(vec![0xff, b'.', b'd', b'b']));
        let connection = Connection::open_in_memory().expect("database");
        assert_eq!(backup_database(&connection, &output), Err(Error::Invalid));
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("directory entries")
                .count(),
            0
        );
    }
}
