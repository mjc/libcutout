#!/usr/bin/env python3
"""Reject server database clients in the iOS and Android library graph."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGETS = ("aarch64-apple-ios", "aarch64-linux-android")
BANNED = {
    "cutout-server",
    "libpq",
    "libpq-sys",
    "postgres",
    "postgres-native-tls",
    "postgres-protocol",
    "postgres-types",
    "postgis",
    "tokio-postgres",
}


def package_names(target: str) -> set[str]:
    result = subprocess.run(
        [
            "cargo", "tree", "--locked", "-p", "cutout-mobile-ffi", "--target", target,
            "--edges", "normal", "--prefix", "none", "--format", "{p}",
        ],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    names = set()
    for line in result.stdout.splitlines():
        name, separator, _version = line.partition(" ")
        if separator and name:
            names.add(name)
    return names


def main() -> int:
    found = []
    for target in TARGETS:
        banned = sorted(package_names(target) & BANNED)
        found.extend(f"{target}: {name}" for name in banned)
    if found:
        print("Phone library depends on a server database crate:", file=sys.stderr)
        print("\n".join(found), file=sys.stderr)
        return 1
    print("Phone library dependency graph excludes the server database crates.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        print(error.stderr or error, file=sys.stderr)
        raise SystemExit(1)
