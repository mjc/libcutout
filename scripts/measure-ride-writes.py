#!/usr/bin/env python3
"""Measure one ready/done write_amplification workload on ARM64 macOS.

Usage: python3 scripts/measure-ride-writes.py <harness> <scenario> <count>
The libproc delta excludes compilation, database setup, shutdown, and cleanup.
It measures process disk-I/O accounting, not device flash writes or phone I/O.
"""

import ctypes
import json
import platform
import subprocess
import sys

if sys.platform != "darwin" or platform.machine() != "arm64":
    raise SystemExit("This sampler requires ARM64 macOS.")
if len(sys.argv) != 4:
    raise SystemExit(__doc__)


class Usage(ctypes.Structure):
    # rusage_info_v2, as declared by the Darwin SDK's sys/resource.h.
    _fields_ = [("uuid", ctypes.c_ubyte * 16)] + [
        (name, ctypes.c_uint64)
        for name in (
            "user_time system_time pkg_idle_wkups interrupt_wkups pageins "
            "wired_size resident_size phys_footprint proc_start_abstime "
            "proc_exit_abstime child_user_time child_system_time child_pkg_idle_wkups "
            "child_interrupt_wkups child_pageins child_elapsed_abstime "
            "diskio_bytesread diskio_byteswritten"
        ).split()
    ]


lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
lib.proc_pid_rusage.restype = ctypes.c_int


def sample(pid):
    usage = Usage()
    if lib.proc_pid_rusage(pid, 2, ctypes.byref(usage)) != 0:
        raise OSError(ctypes.get_errno(), "proc_pid_rusage")
    return {
        name: getattr(usage, name)
        for name in (
            "user_time", "system_time", "diskio_bytesread",
            "diskio_byteswritten", "phys_footprint",
        )
    }


process = subprocess.Popen(
    sys.argv[1:], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True
)
try:
    ready = process.stdout.readline().strip()
    if not ready.startswith("READY "):
        raise RuntimeError("missing READY: " + ready)
    before = sample(process.pid)
    print(ready, flush=True)
    process.stdin.write("\n")
    process.stdin.flush()
    done = process.stdout.readline().strip()
    if not done.startswith("DONE "):
        raise RuntimeError("missing DONE: " + done)
    after = sample(process.pid)
    print(done, flush=True)
    print(json.dumps({
        "os_delta": {name: after[name] - before[name] for name in before},
        "pid": process.pid,
    }), flush=True)
    process.stdin.write("\n")
    process.stdin.flush()
    if process.wait() != 0:
        raise RuntimeError("harness failed")
finally:
    if process.poll() is None:
        process.terminate()
        process.wait()
