use serde_json::Value;
use tempfile::TempDir;

#[test]
fn operator_reads_refuse_missing_or_uninitialized_source_databases() {
    let binary = env!("CARGO_BIN_EXE_cutout-server");
    for existing in [false, true] {
        for operation in ["check", "backup", "revoke"] {
            let directory = TempDir::new().expect("directory");
            let database = directory.path().join("source.sqlite");
            let backup = directory.path().join("backup.sqlite");
            if existing {
                std::fs::write(&database, []).expect("uninitialized database");
            }
            let mut command = std::process::Command::new(binary);
            command.arg("--database").arg(&database).arg(operation);
            match operation {
                "backup" => {
                    command.arg("--output").arg(&backup);
                }
                "revoke" => {
                    command.args(["--identity", "00000000-0000-4000-8000-000000000001"]);
                }
                _ => {}
            }
            let result = command.output().expect("operator command");
            assert!(
                !result.status.success(),
                "{operation} accepted an absent schema"
            );
            assert!(String::from_utf8_lossy(&result.stderr).contains("service unavailable"));
            assert_eq!(database.exists(), existing, "{operation} created a source");
            if existing {
                assert_eq!(
                    std::fs::read(&database)
                        .expect("preserved source")
                        .as_slice(),
                    b""
                );
            }
            assert!(!backup.exists(), "{operation} created a backup");
        }
    }
}

#[test]
fn operator_identity_is_explicit_and_backup_does_not_overwrite_existing_files() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("directory.sqlite");
    let binary = env!("CARGO_BIN_EXE_cutout-server");
    let result = std::process::Command::new(binary)
        .arg("--database")
        .arg(&database)
        .args(["identity", "--moderator"])
        .output()
        .expect("identity command");
    assert!(
        result.status.success(),
        "operator failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let identity: Value = serde_json::from_slice(&result.stdout).expect("identity output");
    assert_eq!(identity["token"].as_str().expect("token").len(), 64);
    assert!(
        !String::from_utf8_lossy(&result.stderr)
            .contains(identity["token"].as_str().expect("token"))
    );
    let backup = directory.path().join("backup.sqlite");
    let backed_up = std::process::Command::new(binary)
        .arg("--database")
        .arg(&database)
        .arg("backup")
        .arg("--output")
        .arg(&backup)
        .output()
        .expect("backup command");
    assert!(backed_up.status.success());
    let source = std::fs::read(&backup).expect("backup contents");
    let repeated = std::process::Command::new(binary)
        .arg("--database")
        .arg(&database)
        .arg("backup")
        .arg("--output")
        .arg(&backup)
        .output()
        .expect("repeat backup");
    assert!(!repeated.status.success());
    assert_eq!(std::fs::read(&backup).expect("preserved backup"), source);
    let restored = cutout_server::Store::open(&backup);
    assert!(restored.is_ok());
}
