#!/usr/bin/env python3
"""Exercise the built dashboard's real Termina reader and terminal cleanup in a PTY.

Run from the repository root through Devenv after building cutout-cli:
    devenv shell -- python3 tests/scripts/dashboard-input.py target/debug/cutout

The panic case builds/discovers the library test executable and runs one ignored
test under the PTY. Normal Rust test runs never open a real terminal for this case.
"""

import fcntl
import json
import os
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time


def identity(pid):
    return subprocess.check_output(
        ["ps", "-p", str(pid), "-o", "lstart=,command="], text=True
    ).strip()


def supervise(command):
    # Keep the controlling session alive until after the Rust process drops its
    # terminal. Darwin detaches the slave when its session leader exits.
    original = termios.tcgetattr(0)
    child = subprocess.Popen(command)
    print(f"CUTOUT_PTY_CHILD={child.pid}", flush=True)
    status = child.wait()
    current = termios.tcgetattr(0)
    # Darwin sets PENDIN when canonical input is restored. It is kernel input
    # state, reproduced by a plain setraw/tcsetattr round trip without Cutout.
    if sys.platform == "darwin":
        current[3] &= ~termios.PENDIN
        original[3] &= ~termios.PENDIN
    restored = current == original
    if not restored:
        print(f"CUTOUT_PTY_ATTRIBUTES={original!r} -> {current!r}", flush=True)
    print(f"CUTOUT_PTY_RESTORED={int(restored)}", flush=True)
    return status


def run_case(binary, mode, command=None):
    command = command or [binary, "dashboard", "--demo"]
    master, slave = os.openpty()
    original = termios.tcgetattr(slave)
    rows, cols = (0, 0) if mode == "init_failure" else (36, 120)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    environment = {**os.environ, "TERM": "xterm-256color"}
    environment.pop("LINES", None)
    environment.pop("COLUMNS", None)

    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    process = subprocess.Popen(
        [sys.executable, os.path.abspath(__file__), "--supervise", *command],
        stdin=slave,
        stdout=slave,
        stderr=slave,
        preexec_fn=controlling_terminal,
        env=environment,
    )
    expected_identity = identity(process.pid)
    output = bytearray()
    child_pid = None
    child_identity = None

    def send_signal(value):
        assert process.poll() is None, "dashboard exited before signal"
        assert identity(process.pid) == expected_identity, "PID identity changed"
        process.send_signal(value)

    def drain(seconds):
        deadline = time.monotonic() + seconds
        before = len(output)
        while time.monotonic() < deadline:
            ready, _, _ = select.select([master], [], [], max(0, deadline - time.monotonic()))
            if ready:
                output.extend(os.read(master, 65536))
        return len(output) - before

    try:
        if mode == "init_failure":
            deadline = time.monotonic() + 5
            while process.poll() is None and time.monotonic() < deadline:
                drain(0.05)
            assert process.poll() == 1, bytes(output).decode(errors="replace")
            drain(0.05)
            assert b"cannot read non-zero cols/rows" in output
            assert b"CUTOUT_PTY_RESTORED=1" in output, f"failed init left raw mode: {bytes(output)!r}"
            for enabled in [b"\x1b[?1049h", b"\x1b[?25l", b"\x1b[?2004h"]:
                assert enabled not in output, "failed sizing enabled terminal modes"
            print("dashboard PTY init_failure: PASS")
            return
        deadline = time.monotonic() + 5
        while b"\x1b[?1049h" not in output and time.monotonic() < deadline:
            drain(0.1)
            assert process.poll() is None, bytes(output).decode(errors="replace")
        assert b"\x1b[?1049h" in output, f"alternate screen not entered: {bytes(output)!r}"
        child_pid = int(re.search(rb"CUTOUT_PTY_CHILD=(\d+)", output).group(1))
        child_identity = identity(child_pid)
        assert b"\x1b[?2004h" in output, "bracketed paste not enabled"
        assert not termios.tcgetattr(slave)[3] & termios.ICANON, "raw mode not entered"
        if mode != "panic":
            assert drain(0.4) > 0, "quiet input stopped dashboard ticks"

        if mode == "panic":
            os.write(master, b"p")
        elif mode == "input":
            os.write(master, b"\x1b[")
            drain(0.005)
            os.write(master, b"C")
            drain(0.05)
            os.write(master, b"\x1b[200~qjkb\x1b[201~")
            drain(0.1)
            assert process.poll() is None, "pasted q quit the dashboard"
            os.write(master, b"\x1b")
            drain(0.1)
            assert process.poll() is None, "lone Escape quit the dashboard"
            os.write(master, b"\x1b[")
            drain(0.1)
            assert drain(0.35) > 0, "incomplete Escape stopped dashboard ticks"
            # Finish the pending CSI before sending a normal quit key.
            os.write(master, b"Dq")
        elif mode == "render_failure":
            # Startup and regular ticks already succeeded. The next draw must
            # observe invalid backend dimensions and take the error exit path.
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 0, 0, 0, 0))
        else:
            assert identity(child_pid) == child_identity, "dashboard PID identity changed"
            os.kill(child_pid, signal.SIGTERM)

        deadline = time.monotonic() + 3
        while process.poll() is None and time.monotonic() < deadline:
            drain(0.05)
        assert process.poll() is not None, "dashboard did not exit within three seconds"
        drain(0.05)
        expected_status = 1 if mode == "render_failure" else 0
        assert process.returncode == expected_status, bytes(output).decode(errors="replace")
        if mode == "render_failure":
            assert b"cannot read non-zero cols/rows" in output
        for restored in [b"\x1b[?1049l", b"\x1b[?25h", b"\x1b[?2004l"]:
            assert restored in output, f"terminal mode not restored: {restored!r}"
        assert b"CUTOUT_PTY_RESTORED=1" in output, "terminal attributes not restored"
        print(f"dashboard PTY {mode}: PASS ({len(output)} output bytes)")
    finally:
        if process.poll() is None:
            if child_pid is not None:
                try:
                    if identity(child_pid) == child_identity:
                        os.kill(child_pid, signal.SIGKILL)
                except subprocess.CalledProcessError:
                    pass
            send_signal(signal.SIGKILL)
            drain(0.1)
            process.wait(timeout=3)
        os.close(master)
        os.close(slave)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--supervise":
        sys.exit(supervise(sys.argv[2:]))
    executable = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "target/debug/cutout")
    run_case(executable, "init_failure")
    run_case(executable, "input")
    run_case(executable, "signal")
    run_case(executable, "render_failure")
    # The ignored Rust unit test exercises the real init path and panics after a
    # PTY handshake. The production CLI has no test-only panic option or env var.
    build = subprocess.run(
        ["cargo", "test", "-p", "cutout-cli", "--lib", "--locked", "--no-run", "--message-format=json"],
        check=True, text=True, stdout=subprocess.PIPE,
    )
    artifacts = [json.loads(line) for line in build.stdout.splitlines()]
    test_binary = next(
        artifact["executable"] for artifact in artifacts
        if artifact.get("reason") == "compiler-artifact"
        and artifact["target"]["name"] == "cutout_cli"
        and artifact["profile"]["test"]
        and artifact.get("executable")
    )
    run_case(executable, "panic", [
        test_binary, "--exact", "dashboard::tests::dashboard_terminal_panic_restores_modes",
        "--ignored", "--nocapture", "--test-threads=1",
    ])
