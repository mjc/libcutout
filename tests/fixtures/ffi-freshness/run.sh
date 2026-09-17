#!/usr/bin/env bash
set -euo pipefail
[[ $# == 0 || ($# == 1 && "$1" == --xcode) ]] || { echo 'Usage: run.sh [--xcode]' >&2; exit 2; }
fixture="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$fixture/../../.." && pwd)"
[[ "${DEVENV_ROOT:-}" == "$root" ]] || { echo 'Run from the project devenv shell' >&2; exit 1; }
[[ "$(uname -sm)" == 'Darwin arm64' ]] || { echo 'Requires native macOS arm64 and Xcode' >&2; exit 1; }
scratch="$(mktemp -d "${TMPDIR:-/tmp}/cutout-ffi-freshness.XXXXXX")"
exec > >(tee "$scratch/run.log") 2>&1
echo "Retained fixture, caches and log: $scratch"
echo "DEVENV_ROOT=$DEVENV_ROOT DEVELOPER_DIR=${DEVELOPER_DIR:-}"
command -v cargo rustc
cargo --version
/usr/bin/xcrun swift --version
/usr/bin/xcrun xcodebuild -version
export CARGO_HOME="$scratch/cargo-home" CARGO_TARGET_DIR="$scratch/rust-target"
export CLANG_MODULE_CACHE_PATH="$scratch/clang-cache" SWIFTPM_MODULECACHE_OVERRIDE="$scratch/swift-cache"
export XDG_CACHE_HOME="$scratch/cache" TMPDIR="$scratch/tmp"
mkdir -p "$TMPDIR" "$scratch/rust/src" "$scratch/headers" "$scratch/bridge/include" "$scratch/consumer/Sources/Probe"
cat > "$scratch/rust/Cargo.toml" <<'EOF'
[package]
name = "ffi_freshness"
version = "0.0.0"
edition = "2024"
[lib]
crate-type = ["staticlib"]
EOF
printf '#include <stdint.h>\nuint8_t fixture_value(void);\n' > "$scratch/headers/native.h"
printf 'module NativeFFI { header "native.h" export * }\n' > "$scratch/headers/module.modulemap"
printf '#include <stdint.h>\nuint8_t bridge_value(void);\n' > "$scratch/bridge/include/bridge.h"
printf '#include "bridge.h"\n#include <native.h>\nuint8_t bridge_value(void) { return fixture_value(); }\n' > "$scratch/bridge/bridge.c"
printf 'import Bridge\nprint(String(UnicodeScalar(bridge_value())))\n' > "$scratch/consumer/Sources/Probe/main.swift"
cat > "$scratch/consumer/Package.swift" <<'EOF'
// swift-tools-version: 6.0
import PackageDescription
let package = Package(
    name: "Probe", platforms: [.macOS(.v15)],
    dependencies: [.package(name: "Selector", path: "../selector")],
    targets: [.executableTarget(name: "Probe", dependencies: [.product(name: "Bridge", package: "Selector")])]
)
EOF
swift_args=(--package-path "$scratch/consumer" --scratch-path "$scratch/swift-build"
    --cache-path "$scratch/cache" --config-path "$scratch/config" --security-path "$scratch/security")
for version in A B; do
    echo "=== Rust implementation $version (same Cargo and SwiftPM build directories) ==="
    cp "$fixture/$version.rs" "$scratch/rust/src/lib.rs"
    (cd "$scratch/rust" && cargo build --offline --target aarch64-apple-darwin)
    generation="$scratch/selector/generations/$version"
    mkdir -p "$generation"
    cp -R "$scratch/bridge" "$generation/Bridge"
    /usr/bin/xcrun xcodebuild -create-xcframework \
        -library "$CARGO_TARGET_DIR/aarch64-apple-darwin/debug/libffi_freshness.a" \
        -headers "$scratch/headers" -output "$generation/NativeFFI.xcframework"
    # Literal paths change the build graph; published generation A is never overwritten.
    cat > "$scratch/selector/Package.swift.next" <<EOF
// swift-tools-version: 6.0
import PackageDescription
let package = Package(
    name: "Selector", platforms: [.macOS(.v15)],
    products: [.library(name: "Bridge", targets: ["Bridge"])],
    targets: [
        .binaryTarget(name: "NativeFFI", path: "generations/$version/NativeFFI.xcframework"),
        .target(name: "Bridge", dependencies: ["NativeFFI"], path: "generations/$version/Bridge")
    ]
)
EOF
    mv "$scratch/selector/Package.swift.next" "$scratch/selector/Package.swift"
    /usr/bin/xcrun swift run "${swift_args[@]}" Probe > "$scratch/actual-$version.txt"
    actual="$(< "$scratch/actual-$version.txt")"
    echo "SwiftPM runtime: expected=$version actual=$actual"
    [[ "$actual" == "$version" ]] || { echo 'FAIL: stale native FFI' >&2; exit 1; }
    if [[ "${1:-}" == --xcode ]]; then
        (cd "$scratch/consumer" && /usr/bin/xcrun xcodebuild -scheme Probe \
            -configuration Debug -destination 'platform=macOS,arch=arm64' \
            -derivedDataPath "$scratch/xcode-build" \
            -clonedSourcePackagesDirPath "$scratch/xcode-packages" build) \
            > "$scratch/xcode-$version.log" 2>&1 || { cat "$scratch/xcode-$version.log"; exit 1; }
        actual="$("$scratch/xcode-build/Build/Products/Debug/Probe")"
        echo "Xcode runtime: expected=$version actual=$actual"
        [[ "$actual" == "$version" ]] || { echo 'FAIL: stale Xcode FFI' >&2; exit 1; }
    fi
done
diff -r "$scratch/selector/generations/A/Bridge" "$scratch/selector/generations/B/Bridge"
diff -r "$scratch/selector/generations/A/NativeFFI.xcframework/macos-arm64/Headers" \
    "$scratch/selector/generations/B/NativeFFI.xcframework/macos-arm64/Headers"
echo 'PASS: default SwiftPM A -> B, identical interface, warm build directory, no cleaning'
