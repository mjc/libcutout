# libcutout

## Development

Use the pinned Devenv environment for repository commands:

```console
devenv shell -- <command>
```

The main checks and workflows are exposed as named tasks:

```console
devenv tasks run project:quality-gate
devenv tasks run test:swift-package
devenv tasks run build:swift-ffi-package
devenv tasks run validate:aero-live-connection
devenv tasks run validate:melk-live-controller
```

Format Rust and Nix with `devenv shell -- treefmt`. The treefmt hook uses the
same configuration. `treefmt --ci` formats files and fails if it changes them;
it is not a read-only check.

For iOS work, use `deploy:ios-device` to build, install, and launch on a phone,
or `export:ios-ad-hoc-ipa` to produce an ad-hoc IPA. Keep signing values in the
OS keyring through SecretSpec's `development` profile:

```console
devenv shell -- secretspec set --profile development NAME --provider keyring
```

See [the mobile FFI guide](docs/mobile-ffi.md) for Swift, Kotlin, Xcode, and
capture workflows. `devenv test` only checks Devenv's lightweight lifecycle;
the project quality gate is the canonical non-device check.
