#!/usr/bin/env python3
"""Run charging-service UI acceptance against a disposable local server."""

from __future__ import annotations

import argparse
import hashlib
import http.client
import http.server
import json
import os
import platform
import signal
import shutil
import socket
import sqlite3
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SERVICE_PORT = 18473
PROXY_PORT = 18474
TEST_NAME = "testTwoClientsConvergeThroughService"
SELECTOR = f"CutoutAppUITests/ChargingMapDeviceUITests/{TEST_NAME}"
UI_FINAL_NOTE = "Service sync outlet. Use the north entrance."
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))
HOP_HEADERS = {"connection", "keep-alive", "proxy-authenticate", "proxy-authorization",
               "te", "trailers", "transfer-encoding", "upgrade", "host", "content-length"}


def run(command: list[str], **kwargs) -> subprocess.CompletedProcess:
    environment = {**os.environ, "CARGO_BUILD_JOBS": "2", "CARGO_INCREMENTAL": "0"}
    kwargs.setdefault("timeout", 600)
    return run_process_group(command, cwd=ROOT, check=True, env=environment, **kwargs)


def run_process_group(command: list[str], *, timeout: float, check: bool = False, **kwargs) -> subprocess.CompletedProcess:
    process = subprocess.Popen(command, start_new_session=True, **kwargs)
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        time.sleep(1)
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.communicate()
        raise
    result = subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    if check and process.returncode != 0:
        raise subprocess.CalledProcessError(process.returncode, command, output=stdout, stderr=stderr)
    return result


def shutdown_simulator(udid: str) -> None:
    shutdown = subprocess.run(["xcrun", "simctl", "shutdown", udid], cwd=ROOT,
                              timeout=30, capture_output=True, text=True)
    if shutdown.returncode == 0:
        return

    devices = subprocess.run(["xcrun", "simctl", "list", "devices", "--json"], cwd=ROOT,
                             timeout=30, capture_output=True, text=True)
    if devices.returncode == 0:
        try:
            groups = json.loads(devices.stdout)["devices"].values()
            if any(device.get("udid") == udid and device.get("state") == "Shutdown"
                   for group in groups for device in group):
                return
        except (KeyError, TypeError, ValueError):
            pass
    detail = shutdown.stderr.strip() or f"exit status {shutdown.returncode}"
    if devices.returncode != 0:
        detail += "; simulator state check failed: " + (devices.stderr.strip() or
                                                         f"exit status {devices.returncode}")
    raise RuntimeError(f"Could not shut down Simulator {udid}: {detail}")


def check_environment(udid: str | None) -> dict:
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise RuntimeError("This harness requires an arm64 Darwin host.")
    devenv_root = os.environ.get("DEVENV_ROOT")
    if not devenv_root or Path(devenv_root).resolve() != ROOT:
        raise RuntimeError(f"Run through this repository's Devenv shell (DEVENV_ROOT={ROOT}).")
    for executable in ("cargo", "rustc", "xcrun"):
        if shutil.which(executable) is None:
            raise RuntimeError(f"Required Devenv command is missing: {executable}")
    device = None
    if udid is not None:
        devices = json.loads(subprocess.check_output(
            ["xcrun", "simctl", "list", "devices", "available", "--json"], text=True, timeout=30
        ))
        device = next((item for runtime, group in devices["devices"].items() if ".iOS-" in runtime
                       for item in group if item["udid"] == udid and item["isAvailable"]), None)
        if device is None:
            raise RuntimeError(f"Simulator UDID is not available: {udid}")
    free_gib = shutil.disk_usage(ROOT).free / (1024 ** 3)
    if free_gib < 10:
        raise RuntimeError(f"Need at least 10 GiB free for the existing build caches; found {free_gib:.1f} GiB.")
    for port in (SERVICE_PORT, PROXY_PORT):
        with socket.socket() as sock:
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            try:
                sock.bind(("127.0.0.1", port))
            except OSError as error:
                raise RuntimeError(f"Port {port} is occupied; stop the owning service yourself and retry.") from error
    return {"simulator": device, "free_disk_gib": round(free_gib, 1)}


class ProxyState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.offline = False
        self.forwarded = 0
        self.failed_while_offline = 0
        self.last_published_spot = None

    def snapshot(self) -> dict:
        with self.lock:
            return {"offline": self.offline, "forwarded_requests": self.forwarded,
                    "requests_failed_while_offline": self.failed_while_offline,
                    "last_published_spot": self.last_published_spot}

    def remember_published_feed(self, request_path: str, status: int, body: bytes) -> None:
        parsed_path = urllib.parse.urlsplit(request_path)
        if status != 200 or parsed_path.path != "/v1/charging-spots/changes" or not parsed_path.query:
            return
        spot = published_spot_from_feed(body)
        if spot is None:
            return
        with self.lock:
            previous = self.last_published_spot
            if previous is None or spot["change_sequence"] >= previous["change_sequence"]:
                self.last_published_spot = spot


def published_spot_from_feed(body: bytes) -> dict | None:
    try:
        page = json.loads(body)
    except (ValueError, UnicodeDecodeError):
        return None
    if not isinstance(page, dict) or not isinstance(page.get("items"), list):
        return None
    published = []
    for item in page["items"]:
        if isinstance(item, dict) and item.get("status") == "published":
            spot = item.get("spot")
            sequence = spot.get("change_sequence") if isinstance(spot, dict) else None
            if isinstance(sequence, int) and not isinstance(sequence, bool):
                published.append(spot)
    return max(published, key=lambda spot: spot["change_sequence"]) if published else None


def make_proxy_handler(state: ProxyState):
    class ProxyHandler(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, _format: str, *_args) -> None:
            # Default access logs can contain credential-bearing URLs and headers.
            return

        def reply(self, status: int, payload: dict) -> None:
            body = json.dumps(payload, separators=(",", ":")).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self) -> None:
            if self.path == "/__test/offline":
                length = int(self.headers.get("Content-Length", "0"))
                try:
                    payload = json.loads(self.rfile.read(length))
                except (ValueError, UnicodeDecodeError):
                    self.reply(400, {"error": "invalid_json"})
                    return
                if not isinstance(payload, dict) or not isinstance(payload.get("offline"), bool):
                    self.reply(400, {"error": "offline_boolean_required"})
                    return
                with state.lock:
                    state.offline = payload["offline"]
                self.reply(200, {"offline": payload["offline"]})
                return
            self.forward()

        def do_GET(self) -> None:
            if self.path == "/__test/state":
                self.reply(200, state.snapshot())
                return
            self.forward()

        def do_PUT(self) -> None:
            self.forward()

        def do_PATCH(self) -> None:
            self.forward()

        def do_DELETE(self) -> None:
            self.forward()

        def do_OPTIONS(self) -> None:
            self.forward()

        def forward(self) -> None:
            with state.lock:
                offline = state.offline
                if offline:
                    state.failed_while_offline += 1
                else:
                    state.forwarded += 1
            if offline:
                self.reply(503, {"error": "test_service_offline"})
                return
            length = int(self.headers.get("Content-Length", "0"))
            body = self.rfile.read(length) if length else None
            headers = [(key, value) for key, value in self.headers.items()
                       if key.lower() not in HOP_HEADERS]
            headers.append(("X-Real-IP", "127.0.0.1"))
            connection = http.client.HTTPConnection("127.0.0.1", SERVICE_PORT, timeout=10)
            try:
                connection.request(self.command, self.path, body=body, headers=dict(headers))
                response = connection.getresponse()
                response_body = response.read()
                if self.command == "GET":
                    state.remember_published_feed(self.path, response.status, response_body)
                self.send_response(response.status)
                for key, value in response.getheaders():
                    if key.lower() not in HOP_HEADERS:
                        self.send_header(key, value)
                self.send_header("Content-Length", str(len(response_body)))
                self.send_header("Connection", "close")
                self.end_headers()
                self.wfile.write(response_body)
            except OSError:
                self.reply(503, {"error": "local_service_unavailable"})
            finally:
                connection.close()

    return ProxyHandler


def api(path: str) -> tuple[int, object]:
    request = urllib.request.Request(f"http://127.0.0.1:{PROXY_PORT}{path}", headers={"Accept": "application/json"})
    try:
        with HTTP.open(request, timeout=5) as response:
            body = response.read()
            return response.status, json.loads(body) if body else None
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode(errors="replace")


def wait_ready(server: subprocess.Popen) -> None:
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if server.poll() is not None:
            raise RuntimeError("Local cutout-server exited before readiness; see server.log.")
        try:
            if api("/readyz")[0] == 200:
                return
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(0.25)
    raise RuntimeError("Local cutout-server did not become ready within 30 seconds.")


def stop_process(process: subprocess.Popen | None) -> None:
    if process is None:
        return
    if process.poll() is None:
        process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def readback(database: Path) -> dict:
    with sqlite3.connect(database) as db:
        identities = [{"role": role, "count": count} for role, count in db.execute(
            "SELECT role, COUNT(*) FROM contributors WHERE revoked_at_ms IS NULL GROUP BY role ORDER BY role")]
        spots = []
        for spot_id, owner_id, revision, status, body, sequence in db.execute(
            "SELECT id, owner_id, revision, status, body, change_sequence FROM spots ORDER BY change_sequence"):
            public_status, public = api(f"/v1/charging-spots/{spot_id}")
            spots.append({"id": spot_id, "owner_id": owner_id, "revision": revision, "status": status,
                          "change_sequence": sequence, "body": json.loads(body) if body else None,
                          "public_readback_status": public_status,
                          "public_readback": public if public_status == 200 else None})
        votes = [{"spot_id": spot_id, "contributor_id": contributor_id, "revision": revision, "value": value}
                 for spot_id, contributor_id, revision, value in db.execute(
                     "SELECT spot_id, contributor_id, revision, value FROM votes ORDER BY spot_id, contributor_id")]
        anonymous = db.execute("SELECT COUNT(*) FROM contributors WHERE id=? AND revoked_at_ms IS NOT NULL",
                               ("00000000-0000-4000-8000-000000000001",)).fetchone()[0]
        active_votes = db.execute("SELECT COUNT(*) FROM votes WHERE value IS NOT NULL").fetchone()[0]
        return {"active_identity_counts_by_role": identities, "anonymous_tombstone_count": anonymous,
                "spots": spots, "votes": votes, "active_vote_count": active_votes,
                "withdrawn_spot_count": sum(spot["status"] == "withdrawn" for spot in spots)}


def verify_readback(result: dict, *, native_only: bool) -> None:
    if result["proxy"]["forwarded_requests"] < 1 or result["proxy"]["requests_failed_while_offline"] < 1:
        raise RuntimeError("Expected app traffic to reach the service and fail during the offline interval.")
    spot_rows = result["spots"]
    if len(spot_rows) != 1 or spot_rows[0]["status"] != "withdrawn":
        raise RuntimeError("Expected exactly one service spot, withdrawn by its owner.")
    withdrawn = spot_rows[0]
    if withdrawn["body"] is not None or withdrawn["public_readback_status"] != 404:
        raise RuntimeError("Expected withdrawal to clear the service body and make detail readback return 404.")
    active_identity_count = sum(item["count"] for item in result["active_identity_counts_by_role"])
    if active_identity_count != 2 or result["anonymous_tombstone_count"] != 1:
        raise RuntimeError("Expected two registered contributors and the Anonymous system principal.")
    if result["active_vote_count"] != 0:
        raise RuntimeError("Expected withdrawal to leave no active votes.")
    if result["public_list_status"] != 200:
        raise RuntimeError(f"Public discovery readback failed with HTTP {result['public_list_status']}.")
    if result["public_list"].get("items") != []:
        raise RuntimeError("Expected the fresh public discovery list to contain no withdrawn spot.")
    if result["changes_status"] != 200:
        raise RuntimeError(f"Public change-feed readback failed with HTTP {result['changes_status']}.")
    changes = result["changes"].get("items", [])
    if not any(item.get("status") == "withdrawn" and item.get("id") == withdrawn["id"] for item in changes):
        raise RuntimeError("Expected the public change feed to expose the withdrawal tombstone.")
    published = result["proxy"].get("last_published_spot")
    if not isinstance(published, dict) or published.get("id") != withdrawn["id"]:
        raise RuntimeError("Expected the proxy to retain the matching published body observed before withdrawal.")
    if not native_only:
        if published.get("access_note") != UI_FINAL_NOTE:
            raise RuntimeError("Expected the published feed snapshot to contain the UI test's offline edit.")
        if published.get("cost") != "free" or published.get("supplies") != []:
            raise RuntimeError("Expected the public body to retain only the required free-charging fields.")
        optional_fields = ("name", "venue", "hours", "paid_terms", "price")
        metadata_fields = {"id", "revision", "created_at_ms", "updated_at_ms",
                           "change_sequence", "last_verified_at_ms", "votes"}
        expected_fields = metadata_fields | {"coordinate", "access_note", "cost", *optional_fields, "supplies"}
        if set(published) != expected_fields or not isinstance(published.get("coordinate"), dict):
            raise RuntimeError("Expected the published feed snapshot to contain exactly public metadata and body fields.")
        if any(published.get(field) is not None for field in optional_fields):
            raise RuntimeError("Expected the UI service spot to keep optional body fields empty.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--simulator-udid", help="one existing available iOS Simulator UDID")
    parser.add_argument("--native-only", action="store_true",
                        help="run the charging sync Swift package integration test without a Simulator")
    parser.add_argument("--evidence-root", type=Path,
                        default=Path.home() / "Library/Logs/Cutout/charging-service-ui-tests",
                        help="directory for the run receipt, disposable database and logs")
    args = parser.parse_args()
    if args.native_only == (args.simulator_udid is not None):
        parser.error("provide --simulator-udid for UI acceptance, or --native-only for host-native acceptance")
    environment = check_environment(args.simulator_udid)
    evidence = args.evidence_root.expanduser().resolve() / (
        "run-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:8]
    )
    evidence.mkdir(parents=True)
    database = evidence / "charging-service.sqlite"
    receipt = {"started_at": datetime.now(timezone.utc).isoformat(),
               "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT,
                                                           text=True, timeout=30).strip(),
               "source_sha256": {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                                 for path in [Path(__file__), ROOT / "swift/CutoutMobile/UITests/CutoutAppUITests.swift",
                                              ROOT / "swift/CutoutMobile/Tests/CutoutAppTests/ChargingServiceSyncTests.swift",
                                              ROOT / "swift/CutoutMobile/Apps/CutoutApp/ChargingCommunitySync.swift",
                                              ROOT / "crates/cutout-mobile-ffi/src/charging.rs",
                                              ROOT / "crates/cutout-mobile-ffi/src/charging/sync.rs"] if path.is_file()},
               "simulator": environment["simulator"], "free_disk_gib": environment["free_disk_gib"],
               "database": str(database), "service_port": SERVICE_PORT, "proxy_port": PROXY_PORT,
               "test_selector": "ChargingServiceSyncTests" if args.native_only else SELECTOR,
               "mode": "native" if args.native_only else "ui"}
    receipt_path = evidence / "run.json"
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"Evidence: {evidence}", flush=True)

    server = None
    proxy = None
    proxy_thread = None
    state = None
    original_error: BaseException | None = None
    try:
        with (evidence / "cargo-build.log").open("w") as log:
            run(["cargo", "build", "--locked", "-p", "cutout-server"], stdout=log,
                stderr=subprocess.STDOUT, timeout=600)
        binary = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug/cutout-server"
        if not binary.is_file():
            raise RuntimeError(f"Built cutout-server binary not found: {binary}")
        receipt.update({"server_binary": str(binary), "server_binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()})
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
        state = ProxyState()
        proxy = http.server.ThreadingHTTPServer(("127.0.0.1", PROXY_PORT), make_proxy_handler(state))
        proxy_thread = threading.Thread(target=proxy.serve_forever, name="charging-sync-proxy", daemon=True)
        proxy_thread.start()
        with (evidence / "server.log").open("w") as log:
            server = subprocess.Popen([str(binary), "--database", str(database), "serve", "--listen",
                                       f"127.0.0.1:{SERVICE_PORT}"], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
            wait_ready(server)
            origin = f"http://127.0.0.1:{PROXY_PORT}/"
            test_environment = {
                **os.environ,
                "CARGO_BUILD_JOBS": "2",
                "CARGO_INCREMENTAL": "0",
                "CUTOUT_UI_TEST_CHARGING_SERVICE_BASE_URL": origin,
                "TEST_RUNNER_CUTOUT_UI_TEST_CHARGING_SERVICE_BASE_URL": origin,
            }
            if args.native_only:
                command = ["bash", "scripts/run-swift-package-tests.sh", "--filter", "ChargingServiceSyncTests"]
                log_name = "native-test.log"
                test_timeout = 1800
            else:
                command = ["bash", "scripts/run-ios-ui-tests.sh", "--verbose", "--timeout", "1200", "--appearance", "light",
                           "--content-size", "large",
                           "-destination", f"platform=iOS Simulator,id={args.simulator_udid}", "-jobs", "2",
                           "-only-testing:" + SELECTOR]
                log_name = "ui-test.log"
                test_timeout = 1500
            with (evidence / log_name).open("w") as test_log:
                run_process_group(command, cwd=ROOT, check=True, env=test_environment,
                                  stdout=test_log, stderr=subprocess.STDOUT, timeout=test_timeout)
            result = readback(database)
            result["proxy"] = state.snapshot()
            result["public_list_status"], result["public_list"] = api("/v1/charging-spots")
            result["changes_status"], result["changes"] = api("/v1/charging-spots/changes?after=0&limit=100")
            (evidence / "readback.json").write_text(json.dumps(result, indent=2) + "\n")
            verify_readback(result, native_only=args.native_only)
        receipt["completed_at"] = datetime.now(timezone.utc).isoformat()
        receipt["outcome"] = "passed"
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    except BaseException as error:
        original_error = error
        receipt["completed_at"] = datetime.now(timezone.utc).isoformat()
        receipt["outcome"] = "failed"
        receipt["error"] = str(error)
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    finally:
        cleanup_errors = []

        def cleanup(action) -> None:
            try:
                action()
            except BaseException as cleanup_error:
                cleanup_errors.append(str(cleanup_error))

        readback_path = evidence / "readback.json"
        if server is not None and not readback_path.exists():
            def write_readback() -> None:
                readback_result = readback(database)
                readback_result["proxy"] = state.snapshot() if state is not None else None
                readback_result["public_list_status"], readback_result["public_list"] = api("/v1/charging-spots")
                readback_result["changes_status"], readback_result["changes"] = api(
                    "/v1/charging-spots/changes?after=0&limit=100")
                readback_path.write_text(json.dumps(readback_result, indent=2) + "\n")
            cleanup(write_readback)
        cleanup(lambda: stop_process(server))
        if proxy is not None:
            cleanup(proxy.shutdown)
            cleanup(proxy.server_close)
        if proxy_thread is not None:
            cleanup(lambda: proxy_thread.join(timeout=5))
        if args.simulator_udid is not None:
            cleanup(lambda: shutdown_simulator(args.simulator_udid))
        if cleanup_errors:
            receipt["cleanup_errors"] = cleanup_errors
            receipt["outcome"] = "failed"
            receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
            if original_error is None:
                original_error = RuntimeError("Cleanup failed: " + "; ".join(cleanup_errors))
    if original_error is not None:
        print(f"Charging-service UI harness failed: {original_error}", file=sys.stderr)
        return 1
    print(f"Passed; evidence retained at {evidence}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
