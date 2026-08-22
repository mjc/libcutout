use std::fs;

use cutout_mobile_ffi::RideDatabaseHandle;

#[test]
fn mobile_clients_open_the_rust_owned_database() {
    let path = std::env::temp_dir().join(format!(
        "cutout-mobile-ffi-ride-database-{}-{}.sqlite",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let handle = RideDatabaseHandle::open(path.to_string_lossy().into_owned())
        .expect("Rust should own the mobile database service");
    let capabilities = handle
        .capabilities()
        .expect("SQLite capabilities should be observable through FFI");
    assert!(capabilities.major >= 3);
    handle.shutdown().expect("database worker should shut down");
    let _ = fs::remove_file(path);
}
