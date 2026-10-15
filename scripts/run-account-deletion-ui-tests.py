#!/usr/bin/env python3
"""Run the account-deletion UI tests against a disposable local directory DB."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import socket
import sqlite3
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PORT = 45987
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))
ANONYMOUS_ID = "00000000-0000-4000-8000-000000000001"
FIXTURES = [
    ("11111111-1111-4111-8111-111111111111", "c" * 64, "normal"),
    ("22222222-2222-4222-8222-222222222222", "e" * 64, "accessibility"),
    ("33333333-3333-4333-8333-333333333333", "f" * 64, "unrelated"),
]
BODY = {
    "coordinate": {"latitude_e7": 397392000, "longitude_e7": -1049903000},
    "access_note": "Account deletion test outlet",
    "cost": "free",
    "name": None,
    "venue": None,
    "hours": None,
    "paid_terms": None,
    "price": None,
    "supplies": [],
}


def run(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def api(path: str, token: str | None = None, payload: dict | None = None):
    headers = {"Accept": "application/json"}
    data = None
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode()
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(f"http://127.0.0.1:{PORT}{path}", data, headers)
    try:
        with HTTP.open(request, timeout=5) as response:
            body = response.read()
            return response.status, json.loads(body) if body else None
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode(errors="replace")


def check_environment(udid: str) -> dict:
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise RuntimeError("This harness requires an arm64 Darwin host.")
    if Path(os.environ.get("DEVENV_ROOT", "")).resolve() != ROOT:
        raise RuntimeError(f"Run through this repository's Devenv shell (DEVENV_ROOT={ROOT}).")
    for executable in ("cargo", "rustc", "xcrun"):
        if shutil.which(executable) is None:
            raise RuntimeError(f"Required Devenv command is missing: {executable}")
    # The repository owns this literal version pin; avoid a Python 3.11-only
    # TOML dependency in a harness that also runs with Xcode's Python.
    channel = re.search(r'(?m)^\s*channel\s*=\s*"([^"\n]+)"\s*(?:#.*)?$',
                        (ROOT / "rust-toolchain.toml").read_text())
    if channel is None:
        raise RuntimeError("Expected a literal channel version in rust-toolchain.toml.")
    expected = channel[1]
    actual = subprocess.check_output(["rustc", "--version"], text=True).strip()
    if not expected or not actual.startswith(f"rustc {expected} "):
        raise RuntimeError(f"Configured Rust is {expected!r}, active compiler is {actual!r}.")
    devices = json.loads(subprocess.check_output(
        ["xcrun", "simctl", "list", "devices", "available", "--json"], text=True
    ))
    device = next((d for runtime, group in devices["devices"].items() if ".iOS-" in runtime for d in group
                   if d["udid"] == udid and d["isAvailable"]), None)
    if not device:
        raise RuntimeError(f"Simulator UDID is not available: {udid}")
    free_gib = shutil.disk_usage(ROOT).free / (1024 ** 3)
    if free_gib < 10:
        raise RuntimeError(f"Need at least 10 GiB free for the existing build caches; found {free_gib:.1f} GiB.")
    with socket.socket() as sock:
        try:
            sock.bind(("127.0.0.1", PORT))
        except OSError as error:
            raise RuntimeError(f"Port {PORT} is occupied; stop the owning service yourself and retry.") from error
    return {"rust": actual, "simulator": device, "free_disk_gib": round(free_gib, 1)}


def seed_database(binary: Path, database: Path) -> None:
    # Initialize the schema with the server's own CLI, then replace its throwaway identity.
    initialized = subprocess.run([str(binary), "--database", str(database), "identity"],
                                 cwd=ROOT, check=True, capture_output=True, text=True)
    identity = json.loads(initialized.stdout)
    with sqlite3.connect(database) as db:
        db.execute("DELETE FROM contributors WHERE id = ?", (identity["id"],))
        anonymous = db.execute("SELECT token_hash, revoked_at_ms FROM contributors WHERE id=?", (ANONYMOUS_ID,)).fetchone()
        if anonymous != ("anonymous-system-principal-no-login", 0):
            raise RuntimeError("Server migration did not create the non-login Anonymous principal.")
        for contributor_id, token, _ in FIXTURES:
            db.execute("INSERT INTO contributors (id, token_hash, role, created_at_ms) VALUES (?, ?, 'contributor', 0)",
                       (contributor_id, hashlib.sha256(token.encode()).hexdigest()))


def wait_ready(server: subprocess.Popen, log) -> None:
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


def create_points() -> list[dict]:
    points = []
    for contributor_id, token, label in FIXTURES:
        point_body = {**BODY, "access_note": f"Account deletion test outlet {label[0]}"}
        status, response = api("/v1/charging-spots", token, {
            "operation_id": str(uuid.uuid4()), "spot": point_body
        })
        if status != 201:
            raise RuntimeError(f"Creating {label} fixture failed: HTTP {status}: {response}")
        state = response.get("state", response)
        points.append({"label": label, "owner_id": contributor_id,
                       "token": token, "body": point_body,
                       "id": state["id"], "revision": state["revision"],
                       "status": state["status"],
                       "change_sequence": state.get("change_sequence")})
    return points


def run_ui_test(evidence: Path, udid: str, test: str, size: str, log_name: str) -> None:
    selector = f"CutoutAppUITests/CutoutAppUITests/{test}"
    command = ["scripts/run-ios-ui-tests.sh", "--timeout", "1200", "--appearance", "light",
               "--content-size", size, "-destination", f"platform=iOS Simulator,id={udid}",
               "-only-testing:" + selector]
    with (evidence / log_name).open("w") as log:
        run(command, stdout=log, stderr=subprocess.STDOUT)


def verify(database: Path, points: list[dict]) -> dict:
    with sqlite3.connect(database) as db:
        contributors = {row[0] for row in db.execute("SELECT id FROM contributors")}
        receipt_hashes = {row[0] for row in db.execute("SELECT token_hash FROM deletion_receipts")}
        rows = {row[0]: row[1:] for row in db.execute(
            "SELECT id, owner_id, revision, status, body, change_sequence FROM spots")}
    deleted = {FIXTURES[0][0], FIXTURES[1][0]}
    if contributors != {ANONYMOUS_ID, FIXTURES[2][0]}:
        raise RuntimeError("Expected only Anonymous and the unrelated contributor; deletion or registration changed the set.")
    expected_receipts = {hashlib.sha256(token.encode()).hexdigest() for _, token, _ in FIXTURES[:2]}
    if receipt_hashes != expected_receipts:
        raise RuntimeError("Deletion receipt hashes do not match the two deleted accounts.")
    checks = []
    for point in points:
        row = rows.get(point["id"])
        expected_owner = ANONYMOUS_ID if point["label"] in ("normal", "accessibility") else point["owner_id"]
        if not row or row[0] != expected_owner or json.loads(row[3]) != point["body"]:
            raise RuntimeError(f"Database body or owner changed unexpectedly for {point['label']}.")
        if row[2] != point["status"]:
            raise RuntimeError(f"Database status changed unexpectedly for {point['label']}.")
        if point["label"] == "unrelated" and (row[1] != point["revision"] or
                                                row[4] != point["change_sequence"]):
            raise RuntimeError("The unrelated point's revision or change sequence changed.")
        status, result = api(f"/v1/charging-spots/{point['id']}")
        if status != 200 or any(result.get(key) != value for key, value in point["body"].items()):
            raise RuntimeError(f"Public readback failed for {point['label']}.")
        checks.append({"label": point["label"], "id": point["id"],
                       "owner_id": expected_owner, "status": row[2], "revision": row[1],
                       "change_sequence": row[4]})
    for _, token, label in FIXTURES:
        status, _ = api("/v1/moderation/audit", token)
        expected = 403 if label == "unrelated" else 401
        if status != expected:
            raise RuntimeError(f"{label} token moderation readback was HTTP {status}, expected {expected}.")
    if len(receipt_hashes) != 2:
        raise RuntimeError("Expected exactly two deletion receipts.")
    return {"deleted_contributor_ids": sorted(deleted), "receipt_count": len(receipt_hashes),
            "points": checks, "moderation_status": {"deleted": 401, "unrelated": 403}}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--simulator-udid", required=True,
                        help="one available iOS Simulator used for both sequential settings cells")
    args = parser.parse_args()
    environment = check_environment(args.simulator_udid)
    evidence = ROOT / "target/account-deletion-ui-tests" / (
        "run-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:8]
    )
    evidence.mkdir(parents=True)
    database = evidence / "directory.sqlite"
    (evidence / "run.json").write_text(json.dumps({
        "started_at": datetime.now(timezone.utc).isoformat(),
        "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "rust": environment["rust"], "simulator": environment["simulator"],
        "free_disk_gib": environment["free_disk_gib"], "database": str(database), "port": PORT,
    }, indent=2) + "\n")
    print(f"Evidence: {evidence}", flush=True)
    server = None
    try:
        with (evidence / "cargo-build.log").open("w") as log:
            run(["cargo", "build", "-p", "cutout-server", "--locked"], stdout=log, stderr=subprocess.STDOUT)
        target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
        binary = target / "debug/cutout-server"
        if not binary.is_file():
            raise RuntimeError(f"Built cutout-server binary not found: {binary}")
        receipt = json.loads((evidence / "run.json").read_text())
        receipt["binary"] = str(binary)
        receipt["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        (evidence / "run.json").write_text(json.dumps(receipt, indent=2) + "\n")
        seed_database(binary, database)
        with (evidence / "server.log").open("w") as log:
            server = subprocess.Popen([str(binary), "--database", str(database), "serve",
                                       "--listen", f"127.0.0.1:{PORT}"], cwd=ROOT,
                                      stdout=log, stderr=subprocess.STDOUT)
            wait_ready(server, log)
            points = create_points()
            (evidence / "seeded-public-spots.json").write_text(json.dumps(
                [{k: v for k, v in point.items() if k != "token"} for point in points], indent=2) + "\n")
            run_ui_test(evidence, args.simulator_udid,
                        "testPickerSurfaceHomeMapRouteAccountDeletionRetainsLocalData", "large",
                        "ui-test-normal.log")
            run_ui_test(evidence, args.simulator_udid,
                        "testPickerSurfaceHomeMapRouteAccountDeletionAtAccessibilityDynamicTypeRetainsLocalData",
                        "accessibility-extra-extra-extra-large", "ui-test-accessibility.log")
            result = verify(database, points)
            (evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    finally:
        if server is not None:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
    print(f"Passed; evidence retained at {evidence}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"account-deletion harness failed: {error}", file=sys.stderr)
        raise SystemExit(1)
