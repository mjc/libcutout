#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

case "$(uname -s)" in
  Darwin) lib_path="target/debug/libcutout_mobile_ffi.dylib" ;;
  Linux) lib_path="target/debug/libcutout_mobile_ffi.so" ;;
  *)
    echo "unsupported host OS for Kotlin FFI smoke: $(uname -s)" >&2
    exit 1
    ;;
esac

cargo build -p cutout-mobile-ffi
rm -rf target/uniffi-smoke
cargo run -p cutout-uniffi-bindgen -- generate \
  --library "$lib_path" \
  --language kotlin \
  --no-format \
  --out-dir target/uniffi-smoke/kotlin

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

coroutines_jar="${KOTLINX_COROUTINES_JAR:-}"
if [[ -z "$coroutines_jar" && -f /usr/share/java/kotlinx-coroutines-core-jvm.jar ]]; then
  coroutines_jar=/usr/share/java/kotlinx-coroutines-core-jvm.jar
fi
if [[ -z "$coroutines_jar" ]]; then
  echo "kotlinx-coroutines-core-jvm.jar not found; enter the Nix dev shell" >&2
  exit 1
fi

kotlin_classpath="$jna_jar:$coroutines_jar"
library_override="$root/$lib_path"

kotlinc \
  target/uniffi-smoke/kotlin/uniffi/cutout_mobile_ffi/cutout_mobile_ffi.kt \
  tests/mobile-ffi/kotlin-smoke.kt \
  -cp "$kotlin_classpath" \
  -include-runtime \
  -d target/uniffi-smoke/kotlin-smoke.jar
java \
  -Djava.library.path="$root/target/debug" \
  -Duniffi.component.cutout_mobile_ffi.libraryOverride="$library_override" \
  -cp "target/uniffi-smoke/kotlin-smoke.jar:$kotlin_classpath" \
  Kotlin_smokeKt
