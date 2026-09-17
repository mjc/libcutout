# libcutout agent guide

## Development environment

- Use `devenv shell -- ...` for repository commands.
- Keep generated Swift FFI output in the ignored `target/swift-ffi/CutoutMobileFFI` package. Prepare it with the repository ensure boundary or `devenv tasks run build:swift-ffi-package`; never commit generated bindings, headers, module maps, fingerprints, or static libraries.
- Keep iOS signing values in the OS keyring through SecretSpec's `development` profile; configure them with `devenv shell -- secretspec set --profile development NAME --provider keyring` and never put values in `devenv.nix`, documentation, or commits. Use `deploy:ios-device` to build, install, and launch on a connected iPhone, or `export:ios-ad-hoc-ipa` to produce an ad-hoc IPA.
- Format Rust and Nix with `devenv shell -- treefmt`. The built-in treefmt Git hook uses the same configuration. `treefmt --ci` formats files and fails if they change; it is not read-only.
- Use `devenv tasks run project:quality-gate` for the canonical non-device gate; `devenv test` only checks devenv's lightweight environment lifecycle.
- Use `devenv tasks run validate:aero-live-connection` or `devenv tasks run validate:melk-live-controller` for the opt-in macOS CoreBluetooth validators. Set their documented `CUTOUT_*_VALIDATION_*` environment variables when changing timeout or target identity.
- Keep protocol/FFI assertions in the normal Swift and Rust test suites; do not add separate smoke executables or shell validators. Use `devenv tasks run test:swift-package` for Swift tests and `devenv shell -- cargo cutout ios captures` to retrieve device captures.

## Code navigation and edits

- Use MCPLS for semantic search, symbol inspection, structural edits, and LSP validation when available.
- Reuse the shared Swift FFI ensure boundary instead of adding consumer-specific generation or linker workarounds.

## Music scope

- Read [the music scope contract](docs/music-integration.md) before changing or reviewing music, listening history, or its ride-replay integration. Ride replay displays historical song metadata; it does not replay music or capture background audio.
