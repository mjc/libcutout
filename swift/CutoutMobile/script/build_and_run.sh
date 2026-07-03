#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
APP_NAME="CutoutApp"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"

source "$ROOT_DIR/scripts/swift-package-common.sh"

case "$(cutout_host_os)" in
  Darwin)
    export DYLD_LIBRARY_PATH="$ROOT_DIR/target/debug:${DYLD_LIBRARY_PATH:-}"
    ;;
  Linux)
    export LD_LIBRARY_PATH="$ROOT_DIR/target/debug:${LD_LIBRARY_PATH:-}"
    ;;
  *)
    echo "unsupported host OS for $APP_NAME: $(cutout_host_os)" >&2
    exit 1
    ;;
esac

swift_cmd=($(cutout_swift_runtime_command))
package_dir="$ROOT_DIR/target/swift-package-app/CutoutMobile"

cutout_prepare_swift_package_workspace "$package_dir"

pkill -x "$APP_NAME" >/dev/null 2>&1 || true

"${swift_cmd[@]}" build \
  --package-path "$package_dir" \
  -Xlinker -L -Xlinker "$ROOT_DIR/target/debug" \
  -Xlinker -lcutout_mobile_ffi \
  --product "$APP_NAME"

APP_BINARY="$("${swift_cmd[@]}" build --package-path "$package_dir" --show-bin-path)/$APP_NAME"

run_app() {
  "$APP_BINARY"
}

case "$MODE" in
  run)
    run_app
    ;;
  --debug|debug)
    lldb -- "$APP_BINARY"
    ;;
  --logs|logs)
    run_app &
    /usr/bin/log stream --info --style compact --predicate "process == \"$APP_NAME\""
    ;;
  --telemetry|telemetry)
    run_app &
    /usr/bin/log stream --info --style compact --predicate "process == \"$APP_NAME\""
    ;;
  --verify|verify)
    run_app &
    app_pid=$!
    sleep 2
    pgrep -x "$APP_NAME" >/dev/null
    kill "$app_pid" >/dev/null 2>&1 || true
    wait "$app_pid" >/dev/null 2>&1 || true
    ;;
  *)
    echo "usage: $0 [run|--debug|--logs|--telemetry|--verify]" >&2
    exit 2
    ;;
esac
