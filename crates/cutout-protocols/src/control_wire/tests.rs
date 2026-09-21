#![allow(clippy::needless_raw_string_hashes)]

use std::io::Write;
use std::process::{Command, Output, Stdio};

// Compile the production checker, not a duplicate implementation, in a const
// context. Metadata goes to stdout: no fixture files or build-tree mutation.
fn compile(layouts: &str) -> Output {
    compile_declarations(&format!(
        "const LAYOUTS: &[Layout] = &[{layouts}]; const CHECK: Schema = Schema::new(LAYOUTS);"
    ))
}

fn compile_declarations(declarations: &str) -> Output {
    let source = format!(
        "mod schema {{\n{}\n{declarations}\n}}",
        include_str!("schema.rs")
    );
    let mut child = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args([
            "--crate-name",
            "wire_schema_probe",
            "--crate-type",
            "lib",
            "--emit=metadata",
            "-o",
            "-",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Rust compiler available in the repository test environment");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn dialect_cannot_reuse_an_inherited_destination() {
    let output = compile_declarations(
        r#"
        const BASE_FIELDS: &[Layout] = &[Layout::DecimalMenu { selector: b'Y', digits: 2 }];
        const BASE: Schema = Schema::new(BASE_FIELDS);
        const EXTENSION: &[Layout] = &[Layout::DecimalMenu { selector: b'Y', digits: 1 }];
        const CHECK: Schema = Schema::extend(&BASE, EXTENSION);
    "#,
    );
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains("E0080") && error.contains("control wire destinations overlap"),
        "{error}"
    );
}

#[test]
fn dialect_can_add_a_distinct_destination() {
    let output = compile_declarations(
        r#"
        const BASE_FIELDS: &[Layout] = &[Layout::Literal(b"Q")];
        const BASE: Schema = Schema::new(BASE_FIELDS);
        const EXTENSION: &[Layout] = &[Layout::Literal(b"E")];
        const CHECK: Schema = Schema::extend(&BASE, EXTENSION);
        const _: () = assert!(CHECK.len() == 2);
    "#,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn colliding_protocol_destinations_fail_at_compile_time() {
    for (name, layouts) in [
        (
            "binary field",
            r#"
            Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 12 },
            Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 12 }
        "#,
        ),
        (
            "equivalent padding",
            r#"
            Layout::Field { magic: *b"LkAp", bank: &[1], offset: 17 },
            Layout::Field { magic: *b"LkAp", bank: &[1, 0x80], offset: 17 }
        "#,
        ),
        (
            "literal opcode",
            r#"Layout::Literal(b"Q"), Layout::Literal(b"Q")"#,
        ),
        (
            "menu selector despite different value widths",
            r#"
            Layout::DecimalMenu { selector: b'Y', digits: 1 },
            Layout::DecimalMenu { selector: b'Y', digits: 2 }
        "#,
        ),
        (
            "literal bypass of binary field",
            r#"
            Layout::Field { magic: *b"LkAp", bank: &[1], offset: 7 },
            Layout::Literal(b"LkAp\x0c\x01\x80\x01\x00\x00\x00\x00")
        "#,
        ),
        (
            "menu prefix reused as single write",
            r#"
            Layout::DecimalMenu { selector: b'Y', digits: 2 }, Layout::Literal(b"W")
        "#,
        ),
    ] {
        let output = compile(layouts);
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            error.contains("E0080") && error.contains("control wire destinations overlap"),
            "{name}: {error}"
        );
    }
}

#[test]
fn distinct_banks_fields_and_opcodes_compile() {
    let output = compile(
        r#"
        Layout::Field { magic: *b"LkAp", bank: &[1], offset: 12 },
        Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 12 },
        Layout::Field { magic: *b"LkAp", bank: &[0], offset: 12 },
        Layout::Field { magic: *b"LkAp", bank: &[1], offset: 17 },
        Layout::Literal(b"Q"), Layout::Literal(b"E"),
        Layout::DecimalMenu { selector: b'Y', digits: 2 },
        Layout::DecimalMenu { selector: b'B', digits: 1 }
    "#,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
