# Native FFI warm-cache regression

From the repository root on native macOS arm64 with full Xcode installed:

```sh
devenv shell -- bash tests/fixtures/ffi-freshness/run.sh
```

Append `--xcode` to also assert A then B with `xcrun xcodebuild` on the same
package, sharing one separate DerivedData directory across both generations.

Uses the project's selected Xcode (`CUTOUT_DEVELOPER_DIR` can override it).
Builds a dependency-free Rust staticlib with Cargo, packages it with
`xcrun xcodebuild -create-xcframework`, and runs a Swift executable through
a C bridge using SwiftPM's default build system (`swiftbuild` with Swift 6.4),
matching production without a `--build-system` override. The same symbol returns A,
then B after only the Rust implementation changes. The consumer and bindings
stay identical; only the selector manifest's literal generation paths change.
Both builds share Cargo's target directory and SwiftPM's scratch/cache paths.
No linker flags, cache clearing, production FFI generation or repository build
artifacts are involved. All generated files and logs are retained in the
temporary directory printed by the script (including on failure).

This tests the immutable-path selector architecture with real native tooling,
not `cutout-dev`'s private publication/receipt functions. Their unit tests remain
the coverage for that policy. A/B are readable fixture generation identities.
