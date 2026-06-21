# AGENTS.md

- Use `nix develop` when a flake exists.
- If no flake exists, check `~/cfg/devshells` before falling back to the pinned Rust toolchain in `rust-toolchain.toml`.
- Keep the workspace naming on the `cutout-*` line used by the Cargo workspace.
- Treat the repository as dual-licensed MIT OR Apache-2.0. Keep source provenance explicit. Do not copy GPL implementation code; when porting permissive code or using captures/docs as references, preserve attribution and separate observed behavior from implementation.
- Keep the protocol engine synchronous, bounded, and transport-independent.
- Keep async runtimes and BLE stacks in adapter crates only.
- Start from failing tests. Use property tests for parser and state-machine work, and mutation tests where they add signal.
- Prefer functional style and low-allocation data structures when they do not materially complicate implementation.
- Source-scanning tests are never appropriate.
- For hardware-backed work, record device model, firmware, battery, app/library version, GATT inventory, labels, provenance, and any redaction notes. Once a device or protocol is fully specced, verify it against actual Bluetooth hardware before closing the issue.
- Keep Beads issue state current when setup or implementation changes land.
- Required validation before merge is `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo deny check`, and `cargo miri test` from a nightly toolchain when available.
