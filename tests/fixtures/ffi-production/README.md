# Production FFI A/B regression

From the project devenv on native macOS arm64:

```sh
devenv shell -- bash tests/fixtures/ffi-production/run.sh
```

The script runs the supported `cargo cutout swift -- test` path twice with the
same SwiftPM build directories, filtering the generic `DeviceControlsTests`
coverage. It changes only a Rust source implementation between runs, then
verifies that the selector changes to a new immutable generation and that the
real `VerifyRustArtifact` plugin accepts and executes the selected package. The
tracked source is restored on exit.
