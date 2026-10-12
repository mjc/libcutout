# Rust nightly compatibility trial, 2026-10-12

The development toolchain is pinned to `nightly-2026-10-12`: rustc and Cargo
1.101.0-nightly, compiler commit `080cc27146389dbb5c8772c476b1937be9c6110f`,
LLVM 23.1.3. Only the rust-overlay input was updated in `devenv.lock`; nixpkgs,
the other inputs and Cargo.lock were retained. The workspace MSRV declaration is
still Rust 1.99. No nightly-only language or library APIs were added.

The trial addresses the [never-type stabilization](https://github.com/rust-lang/rust/pull/155499)
and [Cargo intermediate-layout changes](https://github.com/rust-lang/cargo/issues/15010)
planned for Rust 1.100. This is validation of the dated 1.101 nightly, which
contains those changes; it is not an exact beta/stable 1.100 run.

## Findings and changes

- Both project `Infallible` callback results in `cutout-core/src/pevcap.rs` are
  explicitly typed. No `From<Infallible>` implementation or conversion/coherence
  assumption was found. The workspace already uses edition 2024.
- Cargo's new layout was observed at
  `target/debug/build/cutout-dev/<unit>/out/`. Source audits found no dependency
  on Cargo's private deps/fingerprint/build-script layout. The registry build
  script uses Cargo's `OUT_DIR`. The shared `cutout-dev` FFI ensure boundary
  delegates invalidation to Cargo and hashes published Swift package/static
  library outputs. Toolchain and lock files are included in its source fingerprint.
- Strict nightly Clippy required `FromStr` for `VehicleIdentity`. The new trait
  delegates to existing `TryFrom<&str>` validation. A RED/GREEN regression checks
  normalization and empty/Unicode-whitespace rejection through both entry points.
- Two Option map/default expressions were simplified while preserving their
  zero/default values and Rust 1.99 API availability.
- The connection test module moved intact after production items to satisfy
  `items_after_test_module`. Sem reported 17 reordered entities, zero modified
  bodies. The moved block SHA-256 was
  `2e268ca86d912bdf99469491b1aaadc9f64cc74e9a19f22e89dc740241949ca6`.
- Four unused workspace dependency declarations were removed: mio, smol,
  tracing-log and zerocopy. No member manifest referenced them; transitive lock
  entries were retained.
- Nightly rustfmt changed six macro-invocation layouts in `settings_wire.rs` and
  formatted the Option-expression change. No setting values or encodings changed.

## Validation

| Gate | Result |
| --- | --- |
| Final Rust nextest | 2,386 passed; two configured subprocess helper skips |
| Rust doctests | Three passed |
| Workspace all-target/all-feature Clippy, warnings denied | Passed |
| Rust lint policy | Passed |
| Formatting | 384 files, zero remaining changes |
| Dependency policy | Passed; existing duplicate/unmatched-advisory-ignore warnings retained |
| Shared-boundary native ARM64 Swift package | 1,129 passed: 642 library, five live-validator and 482 app tests |
| Shared-boundary ARM64 iOS Simulator | App build/link and three selected UI tests passed in 161 seconds |
| ARM64 FFI freshness fixture | Passed: Swift executed A, then B after a Rust-only implementation change in reused build directories |

All Rust source is frozen before Apple builds. Validation scope is the native
Apple Silicon host and Simulator. No phone access, deployment, Android builds,
x86 builds or remote Git publication are part of this trial. The unchanged
Rust 1.99 MSRV declaration is not a claim that the final patch was re-tested on
1.99; this trial runs nightly.

## Infrastructure and reproduction

The configured private Nix cache returned HTTP522 and stalled the initial
environment evaluation. The public-cache retry succeeded. These command-local
options bypassed that outage; repository/global cache settings were not changed:

```sh
devenv --no-eval-cache --nix-option substituters https://cache.nixos.org \
  --nix-option connect-timeout 5 tasks run project:test
```

The same prefix was used for `project:lint`, `test:rust-lint-policy`,
`project:dependency-policy` and the supported Apple tasks. Standard Devenv
commands remain the normal workflow when the cache is reachable. Old target
artifacts were retained; no cargo clean or generated-binding commit was used.

Raw logs, compiler/manifest versions, source hashes and patch evidence are in
`target/nightly-20261012/`. Full Rust and Swift logs live under `target/test-logs/`.
The final Rust run is `rust-workspace-20261012T115713-96902.log`; the native Swift
run is `swift-package-20261012T120315-813.log`. The shared FFI wrapper generated
and selected generation
`0af500e8350a119126a9f14baaefa71de0fdb45c0c601e6385387e6d0227d917`.
All nine frozen build-input hashes still matched after native Swift validation.
The Simulator result bundle is
`target/xcode-ui-tests/TestResults/run.I4Ruij/Result.xcresult`. Selected UI tests
covered confirmed-idle home map, unavailable ride storage and map/lifecycle
actions from the picker surface. The run used iOS 27 on iPhone 18 Pro with
`arch=arm64`, `ARCHS=arm64` and `ONLY_ACTIVE_ARCH=YES`. Existing iOS 27
`canOpenURL` deprecation and signed-extension stripping warnings remain.
The result bundle also records three QoS priority-inversion runtime warnings.
These did not fail the tests; this trial does not establish their cause or
measure their performance impact.

The existing `tests/fixtures/ffi-freshness/run.sh` passed with the realized
nightly compiler. Rust implementations A and B used the same Cargo and SwiftPM
build directories with identical exported headers and bridge source. Swift
executed A, then B, without cleaning. This checks the final static-library
location and warm-build freshness across the Cargo layout change. Fixture
outputs are retained at `/tmp/cutout-ffi-freshness.JwSa0M`; the copied run log is
`target/nightly-20261012/ffi-freshness.log`.

Lific MCP and its public endpoint were unavailable during the run, and no local
Lific CLI was installed. `target/nightly-20261012/tracker-pending.md` records the
tracker update to make when the service returns; this repository report is the
durable fallback. The canonical operating guide had already been read in this
conversation; its current read also failed with HTTP522.
