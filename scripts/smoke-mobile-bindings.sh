#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

case "$(uname -s)" in
  Darwin)
    lib_path="target/debug/libcutout_mobile_ffi.dylib"
    export DYLD_LIBRARY_PATH="$root/target/debug:${DYLD_LIBRARY_PATH:-}"
    swiftc_cmd=(env -u SDKROOT -u DEVELOPER_DIR /usr/bin/swiftc)
    ;;
  Linux)
    lib_path="target/debug/libcutout_mobile_ffi.so"
    export LD_LIBRARY_PATH="$root/target/debug:${LD_LIBRARY_PATH:-}"
    swiftc_cmd=(swiftc)
    ;;
  *)
    echo "unsupported host OS for mobile FFI smoke: $(uname -s)" >&2
    exit 1
    ;;
esac

cargo build -p cutout-mobile-ffi
rm -rf target/uniffi-smoke
cargo run -p cutout-uniffi-bindgen -- generate \
  --library "$lib_path" \
  --language swift \
  --no-format \
  --out-dir target/uniffi-smoke/swift
cargo run -p cutout-uniffi-bindgen -- generate \
  --library "$lib_path" \
  --language kotlin \
  --no-format \
  --out-dir target/uniffi-smoke/kotlin

"${swiftc_cmd[@]}" \
  -I target/uniffi-smoke/swift \
  -Xcc -fmodule-map-file=target/uniffi-smoke/swift/cutout_mobile_ffiFFI.modulemap \
  -L target/debug \
  -lcutout_mobile_ffi \
  target/uniffi-smoke/swift/cutout_mobile_ffi.swift \
  tests/mobile-ffi/swift-smoke.swift \
  -o target/uniffi-smoke/swift-smoke
target/uniffi-smoke/swift-smoke

jna_jar="${JNA_JAR:-}"
if [[ -z "$jna_jar" ]]; then
  for candidate in \
    "${JNA_HOME:-}/share/java/jna.jar" \
    /nix/store/*-jna-*/share/java/jna.jar \
    /usr/share/java/jna.jar
  do
    if [[ -f "$candidate" ]]; then
      jna_jar="$candidate"
      break
    fi
  done
fi
if [[ -z "$jna_jar" ]]; then
  echo "jna.jar not found; set JNA_JAR or install jna in the dev shell" >&2
  exit 1
fi

kotlinc \
  target/uniffi-smoke/kotlin/uniffi/cutout_mobile_ffi/cutout_mobile_ffi.kt \
  tests/mobile-ffi/kotlin-smoke.kt \
  -cp "$jna_jar" \
  -include-runtime \
  -d target/uniffi-smoke/kotlin-smoke.jar
java \
  -Djava.library.path="$root/target/debug" \
  -cp "target/uniffi-smoke/kotlin-smoke.jar:$jna_jar" \
  Kotlin_smokeKt
