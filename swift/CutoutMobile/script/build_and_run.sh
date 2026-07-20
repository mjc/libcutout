#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
APP_NAME="CutoutApp"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"

source "$ROOT_DIR/scripts/swift-package-common.sh"

HOST_OS="$(cutout_host_os)"
APP_BUNDLE=""
APP_BINARY=""
BUNDLE_ID=""

build_darwin_app() {
  APP_BUNDLE="$(cutout_build_ios_app_bundle)"
  local executable_name
  executable_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP_BUNDLE/Info.plist")"
  APP_BINARY="$APP_BUNDLE/Contents/MacOS/$executable_name"
  BUNDLE_ID="$(cutout_ios_app_bundle_identifier "$APP_BUNDLE")"
}

build_linux_app() {
  export LD_LIBRARY_PATH="$ROOT_DIR/target/debug:${LD_LIBRARY_PATH:-}"

  local swift_cmd package_dir
  swift_cmd=($(cutout_swift_runtime_command))
  package_dir="$ROOT_DIR/swift/CutoutMobile"

  cutout_prepare_swift_package_workspace

  "${swift_cmd[@]}" build \
    --package-path "$package_dir" \
    -Xlinker -L -Xlinker "$ROOT_DIR/target/debug" \
    -Xlinker -lcutout_mobile_ffi \
    --product "$APP_NAME"

  APP_BINARY="$("${swift_cmd[@]}" build --package-path "$package_dir" --show-bin-path)/$APP_NAME"
}

run_darwin_app() {
  /usr/bin/open -n "$APP_BUNDLE"
}

run_linux_app() {
  "$APP_BINARY"
}

stream_logs() {
  /usr/bin/log stream --info --style compact --predicate "process == \"$APP_NAME\""
}

pkill -x "$APP_NAME" >/dev/null 2>&1 || true

case "$HOST_OS" in
  Darwin)
    export DYLD_LIBRARY_PATH="$ROOT_DIR/target/debug:${DYLD_LIBRARY_PATH:-}"
    build_darwin_app
    ;;
  Linux)
    build_linux_app
    ;;
  *)
    echo "unsupported host OS for $APP_NAME: $HOST_OS" >&2
    exit 1
    ;;
esac

case "$MODE" in
  run)
    case "$HOST_OS" in
      Darwin) run_darwin_app ;;
      Linux) run_linux_app ;;
    esac
    ;;
  --debug|debug)
    lldb -- "$APP_BINARY"
    ;;
  --logs|logs)
    case "$HOST_OS" in
      Darwin) run_darwin_app ;;
      Linux) run_linux_app & ;;
    esac
    stream_logs
    ;;
  --telemetry|telemetry)
    case "$HOST_OS" in
      Darwin) run_darwin_app ;;
      Linux) run_linux_app & ;;
    esac
    stream_logs
    ;;
  --verify|verify)
    case "$HOST_OS" in
      Darwin) run_darwin_app ;;
      Linux) run_linux_app & app_pid=$! ;;
    esac
    sleep 2
    pgrep -x "$APP_NAME" >/dev/null
    if [[ "${app_pid:-}" != "" ]]; then
      kill "$app_pid" >/dev/null 2>&1 || true
      wait "$app_pid" >/dev/null 2>&1 || true
    elif [[ -n "$BUNDLE_ID" ]]; then
      osascript -e "tell application id \"$BUNDLE_ID\" to quit" >/dev/null 2>&1 || pkill -x "$APP_NAME" >/dev/null 2>&1 || true
    fi
    ;;
  *)
    echo "usage: $0 [run|--debug|--logs|--telemetry|--verify]" >&2
    exit 2
    ;;
esac
