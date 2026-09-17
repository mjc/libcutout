#!/usr/bin/env bash
set -euo pipefail

swift_args=("$@")
swift_command=""
package_path="$PWD"
skip_build=false
prepare=false

# Inspect SwiftPM commands without changing the arguments passed to Swift.
while [[ $# -gt 0 ]]; do
  case "$1" in
    --package-path)
      [[ $# -ge 2 ]] || break
      package_path="$2"
      shift
      ;;
    --package-path=*) package_path="${1#*=}" ;;
    build|test|run)
      if [[ -z "$swift_command" ]]; then
        swift_command="$1"
        prepare=true
      elif [[ "$swift_command" == run ]]; then
        break
      fi
      ;;
    --skip-build) skip_build=true ;;
    --help|-help|-h)
      prepare=false
      break
      ;;
    package|help)
      [[ -n "$swift_command" ]] || prepare=false
      break
      ;;
    --) break ;;
    # Skip option operands so names like "build" cannot become commands,
    # and run's executable arguments (including --help) stay opaque.
    # Audited against Swift 6.4 run/test/build --help.
    -c|--configuration|-j|--jobs|--scratch-path|--cache-path|--config-path|--security-path|\
    --swift-sdks-path|--toolset|--pkg-config-path|--manifest-cache|--netrc-file|\
    --resolver-fingerprint-checking|--resolver-signing-entity-checking|--default-registry-url|\
    --sdk|--swift-sdk|--triple|--toolchain|--destination|--arch|--sanitize|\
    --explicit-target-dependency-import-check|--build-system|-debug-info-format|\
    --experimental-codesize-profile-output-dir|--traits|--product|--target|\
    --sbom-spec|--sbom-output-dir|--sbom-filter|--attachments-path|--num-workers|\
    -s|--specifier|--filter|--skip|--xunit-output|-Xswiftc|-Xcc|-Xcxx|-Xlinker|-Xmanifest)
      [[ $# -ge 2 ]] || break
      shift
      ;;
    --*|-v) ;;
    -*)
      [[ -n "$swift_command" ]] || break
      ;;
    *)
      [[ -n "$swift_command" && "$swift_command" != run ]] || break
      ;;
  esac
  shift
done

if $prepare; then
  root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
  # SwiftPM searches parents from the chosen directory. Stop at the first
  # manifest, so nested and unrelated packages are never intercepted.
  package_path="$(cd "$package_path" 2>/dev/null && pwd -P)" || package_path=""
  while [[ -n "$package_path" && "$package_path" != / && ! -f "$package_path/Package.swift" ]]; do
    package_path="${package_path%/*}"
  done
  if [[ "$package_path" == "$root/swift/CutoutMobile" ]]; then
    if $skip_build && [[ "$swift_command" == test || "$swift_command" == run ]]; then
      echo "CutoutMobile $swift_command cannot use --skip-build: rebuild to avoid stale FFI executables" >&2
      exit 2
    fi
    source "$root/scripts/swift-package-common.sh"
    cutout_ensure_swift_ffi_build_input "$root"
  fi
fi

# Preserve the shell's chosen Xcode and bypass this wrapper unconditionally.
exec /usr/bin/xcrun swift ${swift_args[@]+"${swift_args[@]}"}
