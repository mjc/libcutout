#!/usr/bin/env python3
"""Shared Kotlin smoke build and opt-in ARM64 Android instrumentation runner."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import traceback
from datetime import datetime, timezone
from xml.etree import ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
ANDROID = ROOT / "android"
BINDINGS = TARGET / "uniffi-smoke/kotlin"
REPLAY_SOURCE = TARGET / "uniffi-smoke/replay/EucReplayFixture.kt"
REPLAY_CAPTURE = ROOT / "crates/cutout-cli/fixtures/pevcap/aero-veteran-live.jsonl"
APP = "lol.cutout"
RUNNER = f"{APP}.test/androidx.test.runner.AndroidJUnitRunner"
GRADLE_CACHE = TARGET / "android-tests/gradle-cache"
SHARED_LABEL_PREFIXES = ("settings.", "actions.", "controls.", "euc.", "vesc.",
                         "ride.", "status.", "footpad.")


def select_device(output: str, selector: str | None) -> str:
    devices = {}
    for line in output.splitlines():
        fields = line.split()
        if len(fields) >= 2 and fields[1] in ("device", "offline", "unauthorized"):
            devices[fields[0]] = fields[1]
    if selector is None:
        if len(devices) != 1:
            raise ValueError("Select one phone explicitly with CUTOUT_ANDROID_DEVICE or --device")
        selector = next(iter(devices))
    state = devices.get(selector)
    if state != "device":
        raise ValueError(f"Android device {selector!r} is {state or 'missing'}")
    return selector


def instrumentation_results(output: str) -> list[dict[str, str]]:
    status = {}
    results = []
    expected = None
    final_code = None
    for line in output.splitlines():
        if line.startswith("INSTRUMENTATION_STATUS: "):
            key, _, value = line.removeprefix("INSTRUMENTATION_STATUS: ").partition("=")
            status[key] = value
        elif line.startswith("INSTRUMENTATION_STATUS_CODE: "):
            code = int(line.split(": ", 1)[1])
            if "numtests" in status:
                expected = int(status["numtests"])
            if "class" in status and "test" in status and code != 1:
                results.append({"class": status["class"], "test": status["test"],
                                "failure": "" if code == 0 else status.get("stack", str(status))})
            status = {}
        elif line.startswith("INSTRUMENTATION_CODE: "):
            final_code = int(line.split(": ", 1)[1])
    identities = {(r["class"], r["test"]) for r in results}
    if final_code != -1 or not results or len(results) != expected or len(identities) != len(results):
        raise ValueError("Instrumentation did not finish the advertised tests; inspect instrumentation.log")
    return results


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def prepare_replay() -> None:
    """Compile the existing public capture into Kotlin inputs, without a JSON dependency."""
    rows = [json.loads(line) for line in REPLAY_CAPTURE.read_text().splitlines()]
    if "capture_privacy=public" not in rows[0]["header"]["annotations"]:
        raise ValueError("Only a public capture may be packaged in the test app")
    kinds = {"LinkUp": "LINK_UP", "Inbound": "NOTIFICATION", "LinkDown": "LINK_DOWN"}
    lines = [f"// Generated from {REPLAY_CAPTURE.relative_to(ROOT).as_posix()}.",
             "package lol.cutout.smoke", "import uniffi.cutout_mobile_ffi.*",
             "internal fun eucReplayInputs(): List<MobileSessionInputDto> = listOf("]
    for row in rows[1:]:
        record = row["record"]
        kind = kinds.get(record["direction"])
        if kind is None:
            continue  # Captured outbound commands are observations, never requests to send.
        arrays = []
        for field in ("characteristic", "bytes"):
            values = (str(b if b < 128 else b - 256) for b in record[field])
            arrays.append("byteArrayOf(" + ", ".join(values) + ")")
        limit = record.get("link_max_write_len")
        limit_source = "null" if limit is None else f"MobileTransportWriteLimitDto({limit}U)"
        lines.append(f"    MobileSessionInputDto(MobileSessionInputKindDto.{kind}, "
                     f"MobileMonotonicMillisDto({record['monotonic_ms']}UL), {limit_source}, "
                     f"{arrays[0]}, {arrays[1]}),")
    lines.extend([")", "internal fun eucReplayGatt(): List<MobileGattFingerprintDto> = listOf("])
    for fingerprint in rows[0]["header"]["gatt_fingerprints"]:
        arrays = ["byteArrayOf(" + ", ".join(str(b if b < 128 else b - 256) for b in fingerprint[field]) + ")"
                  for field in ("service", "characteristic")]
        roles = {"Read": "READ", "Write": "WRITE", "WriteWithoutResponse": "WRITE_WITHOUT_RESPONSE",
                 "Notify": "NOTIFY", "Indicate": "INDICATE"}
        role_source = ", ".join("MobileGattRoleDto." + roles[role] for role in fingerprint["roles"]["roles"])
        lines.append(f"    MobileGattFingerprintDto({arrays[0]}, {arrays[1]}, listOf({role_source}), "
                     "MobileVerificationStatusDto.HARDWARE_VERIFIED),")
    source = "\n".join([*lines, ")", ""])
    REPLAY_SOURCE.parent.mkdir(parents=True, exist_ok=True)
    if not REPLAY_SOURCE.exists() or REPLAY_SOURCE.read_text() != source:
        REPLAY_SOURCE.write_text(source)


def prepare_shared_labels() -> None:
    labels = {}
    for source in (ROOT / "swift/CutoutMobile/Sources/CutoutMobile/Localizable.xcstrings",
                   ROOT / "swift/CutoutMobile/Apps/CutoutApp/Localizable.xcstrings"):
        for key, entry in json.loads(source.read_text())["strings"].items():
            if key.startswith(SHARED_LABEL_PREFIXES):
                for language, value in entry.get("localizations", {}).items():
                    text = value.get("stringUnit", {}).get("value")
                    if text is not None:
                        labels.setdefault(language, {})[key] = text
    resource = TARGET / "android-tests/resources/raw/shared_labels.json"
    resource.parent.mkdir(parents=True, exist_ok=True)
    contents = json.dumps(labels, sort_keys=True, ensure_ascii=False)
    if not resource.exists() or resource.read_text() != contents:
        resource.write_text(contents)


def validate_device(device: dict[str, str], expected_serial: str | None) -> None:
    if not device["serial"] or device["abi"] != "arm64-v8a" or int(device["api"]) < 33 or device["emulator"] == "1":
        raise ValueError(f"Expected a physical ARM64 Android 33+ phone: {device}")
    if expected_serial and device["serial"] != expected_serial:
        raise ValueError("Physical serial does not match CUTOUT_ANDROID_SERIAL; nothing installed")


class Run:
    def __init__(self, mode: str):
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
        self.logs = TARGET / "test-logs" / f"mobile-ffi-{mode}-{stamp}"
        self.logs.mkdir(parents=True)
        self.count = 0
        self.env = dict(os.environ, CARGO_BUILD_JOBS="1")
        self.receipt = {"mode": mode, "status": "running", "logs": str(self.logs),
                        "rust_jobs": 1, "gradle_workers": 2, "build_status": "not_run",
                        "install_status": "not_run", "test_status": "not_run", "launch_status": "not_run"}
        self.selector = ""
        self.last_returncode = 0

    def command(self, args: list[str], name: str, timeout: int = 1800,
                allow_failure: bool = False) -> str:
        self.count += 1
        log = self.logs / f"{self.count:02d}-{name}.log"
        print(f"{name}: {log}", flush=True)
        with log.open("w") as output:
            result = subprocess.run(args, cwd=ROOT, env=self.env, stdout=output,
                                    stderr=subprocess.STDOUT, timeout=timeout)
        self.last_returncode = result.returncode
        text = log.read_text(errors="replace")
        if result.returncode and not allow_failure:
            raise RuntimeError(f"{name} failed ({result.returncode}):\n" + "\n".join(text.splitlines()[-25:]))
        return text

    def adb(self, args: list[str], name: str, timeout: int = 30) -> str:
        return self.command(["adb", "-s", self.selector, *args], name, timeout)

    def prepare(self, android: bool) -> Path:
        prepare_replay()
        prepare_shared_labels()
        self.receipt["replay_capture_sha256"] = digest(REPLAY_CAPTURE)
        args = ["cargo", "build", "--locked", "--jobs", "1", "-p", "cutout-mobile-ffi", "--lib"]
        folder = TARGET / "debug"
        if android:
            args.extend(["--target", "aarch64-linux-android"])
            folder = TARGET / "aarch64-linux-android/debug"
        self.command(args, "rust-library")
        extension = "dylib" if not android and platform.system() == "Darwin" else "so"
        library = folder / f"libcutout_mobile_ffi.{extension}"
        self.command(["cargo", "run", "--locked", "--jobs", "1", "-p", "cutout-uniffi-bindgen", "--",
                      "generate", "--library", str(library), "--language", "kotlin", "--no-format",
                      "--out-dir", str(BINDINGS)], "generate-kotlin")
        self.receipt["rust_library_sha256"] = digest(library)
        return library

    def host(self):
        self.receipt["build_status"] = "running"
        library = self.prepare(False)
        classpath = os.pathsep.join([self.env["JNA_JAR"], self.env["KOTLIN_COROUTINES_JAR"]])
        jar = TARGET / "uniffi-smoke/kotlin-smoke.jar"
        self.command(["kotlinc", str(BINDINGS / "uniffi/cutout_mobile_ffi/cutout_mobile_ffi.kt"),
                      str(ANDROID / "src/main/java/lol/cutout/GattOperationQueue.kt"),
                      str(ANDROID / "src/main/java/lol/cutout/PevSession.kt"),
                      str(ROOT / "tests/mobile-ffi/shared/MobileFfiSmoke.kt"),
                      str(ROOT / "tests/mobile-ffi/shared/AndroidTransportSmoke.kt"),
                      str(ROOT / "tests/mobile-ffi/shared/EucReplaySmoke.kt"), str(REPLAY_SOURCE),
                      str(ROOT / "tests/mobile-ffi/kotlin-smoke.kt"), "-cp", classpath,
                      "-include-runtime", "-d", str(jar)], "compile-host")
        self.receipt["build_status"] = "passed"
        self.receipt["test_status"] = "running"
        print(self.command(["java", f"-Djava.library.path={library.parent}",
                            f"-Djna.library.path={library.parent}", "-cp", str(jar) + os.pathsep + classpath,
                            "Kotlin_smokeKt"], "host-tests"), end="")
        self.receipt["test_status"] = "passed"

    def build(self) -> list[Path]:
        self.receipt["build_status"] = "running"
        library = self.prepare(True)
        link = TARGET / "android-tests/jniLibs/arm64-v8a" / library.name
        link.parent.mkdir(parents=True, exist_ok=True)
        if not link.is_symlink() or link.resolve() != library:
            if link.exists() or link.is_symlink():
                link.unlink()
            link.symlink_to(library)
        self.command(["gradle", "--no-daemon", "--max-workers=2", "--console=plain",
                      "--project-cache-dir", str(TARGET / "android-tests/gradle-cache"),
                      "-p", str(ANDROID), "assembleDebug", "assembleDebugAndroidTest"], "android-apks")
        apks = []
        for variant, package in (("debug", APP), ("androidTest/debug", APP + ".test")):
            folder = TARGET / "android-tests/build/outputs/apk" / variant
            metadata = json.loads((folder / "output-metadata.json").read_text())
            elements = metadata["elements"]
            if len(elements) != 1:
                raise ValueError(f"Expected exactly one {variant} APK, found {elements}")
            apk = folder / elements[0]["outputFile"]
            aapt = Path(self.env["ANDROID_HOME"]) / "build-tools" / self.env["CUTOUT_ANDROID_BUILD_TOOLS"] / "aapt"
            info = self.command([str(aapt), "dump", "badging", str(apk)], "apk-identity")
            if not info.startswith(f"package: name='{package}' "):
                raise ValueError(f"Refusing unexpected APK identity: {info.splitlines()[0]}")
            apks.append(apk)
        self.receipt["apks"] = {str(apk): digest(apk) for apk in apks}
        with zipfile.ZipFile(apks[0]) as apk:
            natives = [name for name in apk.namelist() if name.startswith("lib/") and name.endswith(".so")]
            if not natives or any(not n.startswith("lib/arm64-v8a/") for n in natives):
                raise ValueError(f"Unexpected APK native ABIs: {natives}")
            self.receipt["packaged_libraries"] = {n: hashlib.sha256(apk.read(n)).hexdigest() for n in natives}
        self.receipt["build_status"] = "passed"
        return apks

    def local(self) -> None:
        if platform.system() != "Darwin" or platform.machine().lower() not in ("arm64", "aarch64"):
            raise ValueError("Local Robolectric UI tests require ARM64 macOS")
        self.receipt["build_status"] = "running"
        library = self.prepare(False)
        self.receipt["rust_build_status"] = "passed"
        self.receipt["test_status"] = "running"
        artifacts = self.logs / "ui"
        artifacts.mkdir()
        result_dir = TARGET / "android-tests/build/test-results/testDebugUnitTest"
        shutil.rmtree(result_dir, ignore_errors=True)
        gradle = ["gradle", "--no-daemon", "--max-workers=2", "--console=plain",
                  "--project-cache-dir", str(GRADLE_CACHE),
                  f"-Pcutout.native.library.dir={library.parent}",
                  f"-Pcutout.ui.artifacts={artifacts}",
                  "-p", str(ANDROID), "testDebugUnitTest"]
        self.command(gradle, "android-local-ui-tests", allow_failure=True)
        if self.last_returncode == 0:
            self.receipt["build_status"] = "passed"
        result_files = sorted(result_dir.glob("TEST-*.xml"))
        results = []
        skipped = []
        for result_file in result_files:
            try:
                root = ET.parse(result_file).getroot()
            except ET.ParseError as error:
                raise ValueError(f"Invalid Gradle JUnit report {result_file}: {error}") from error
            for case in root.iter("testcase"):
                failure = case.find("failure")
                if failure is None:
                    failure = case.find("error")
                skip = case.find("skipped")
                detail = "" if failure is None else (failure.get("message", "") + "\n" + (failure.text or "")).strip()
                result = {"class": case.get("classname", ""), "test": case.get("name", ""),
                          "failure": detail}
                if skip is not None:
                    result["skipped"] = skip.get("message", "") or (skip.text or "skipped")
                    skipped.append(result)
                results.append(result)
        if not results:
            raise ValueError("Gradle reported no local UI test cases; inspect android-local-ui-tests.log")
        suite = ET.Element("testsuite", name="cutout-android-local-ui",
                           tests=str(len(results)),
                           failures=str(sum(bool(test["failure"]) for test in results)),
                           skipped=str(len(skipped)))
        for result in results:
            case = ET.SubElement(suite, "testcase", classname=result["class"], name=result["test"])
            if result["failure"]:
                ET.SubElement(case, "failure").text = result["failure"]
            if "skipped" in result:
                ET.SubElement(case, "skipped", message=result["skipped"])
        ET.ElementTree(suite).write(self.logs / "junit.xml", encoding="utf-8", xml_declaration=True)
        self.receipt["tests"] = results
        self.receipt["test_count"] = len(results)
        self.receipt["failure_count"] = sum(bool(test["failure"]) for test in results)
        self.receipt["skipped_count"] = len(skipped)
        self.receipt["junit_xml"] = str(self.logs / "junit.xml")
        self.receipt["ui_artifacts"] = str(artifacts)
        if self.last_returncode:
            raise ValueError(f"Gradle local UI tests failed with exit code {self.last_returncode}; inspect junit.xml")
        if self.receipt["failure_count"] or skipped:
            raise ValueError("Local UI tests contain failures or skips; inspect junit.xml")
        self.receipt["test_status"] = "passed"
        print(f"PASS {len(results)} local Android UI tests; screenshots: {artifacts}", flush=True)

    def identity(self) -> dict[str, str]:
        fields = {"serial": "ro.serialno", "model": "ro.product.model", "api": "ro.build.version.sdk",
                  "abi": "ro.product.cpu.abi", "emulator": "ro.kernel.qemu"}
        return {key: self.adb(["shell", "getprop", prop], key).strip() for key, prop in fields.items()}

    def device(self, selector: str | None, expected_serial: str | None, launch: bool = False):
        output = self.command(["adb", "devices", "-l"], "adb-devices", 30)
        self.selector = select_device(output, selector)
        device = self.identity()
        validate_device(device, expected_serial)
        self.receipt["device"] = dict(device, transport=self.selector)
        apks = self.build()
        if self.identity() != device:
            raise ValueError("Device identity changed during build; nothing installed")
        self.receipt["install_status"] = "running"
        for apk in apks:
            self.adb(["install", "-r", "-t", str(apk)], "install", 180)
        self.receipt["install_status"] = "passed"
        if self.identity() != device:
            raise ValueError("Device identity changed before instrumentation")
        if launch:
            self.receipt["launch_status"] = "running"
            result = self.adb(["shell", "am", "start", "-W", "-n", APP + "/.MainActivity"], "launch")
            if "Status: ok" not in result:
                raise ValueError("Android did not confirm activity launch")
            self.receipt["launch_status"] = "passed"
            return
        self.receipt["test_status"] = "running"
        output = self.adb(["shell", "am", "instrument", "-w", "-r", RUNNER], "instrumentation", 180)
        # Keep the raw protocol even when the runner exits zero after a test crash.
        (self.logs / "instrumentation.log").write_text(output)
        for pid in sorted(set(re.findall(r"INSTRUMENTATION_STATUS: cutoutPid=(\d+)", output))):
            self.adb(["logcat", "-d", "-t", "200", "--pid", pid], "test-logcat")
        results = instrumentation_results(output)
        suite = ET.Element("testsuite", name="cutout-mobile-ffi", tests=str(len(results)),
                           failures=str(sum(bool(t["failure"]) for t in results)))
        for result in results:
            case = ET.SubElement(suite, "testcase", classname=result["class"], name=result["test"])
            if result["failure"]:
                ET.SubElement(case, "failure").text = result["failure"]
        ET.ElementTree(suite).write(self.logs / "junit.xml", encoding="utf-8", xml_declaration=True)
        self.receipt["tests"] = results
        if any(t["failure"] for t in results):
            raise ValueError("Android instrumentation assertions failed; inspect junit.xml")
        self.receipt["test_status"] = "passed"
        print(f"PASS {len(results)} Android tests over {self.selector}", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("host", "build", "device", "app", "local"))
    parser.add_argument("--device", default=os.environ.get("CUTOUT_ANDROID_DEVICE"))
    parser.add_argument("--expected-serial", default=os.environ.get("CUTOUT_ANDROID_SERIAL"))
    args = parser.parse_args()
    if args.mode == "local" and (platform.system() != "Darwin" or
                                  platform.machine().lower() not in ("arm64", "aarch64")):
        parser.error("Local Robolectric UI tests require ARM64 macOS")
    environment_root = os.environ.get("DEVENV_ROOT")
    if not environment_root or Path(environment_root).resolve() != ROOT:
        parser.error("Run from this repository's devenv")
    lock_path = TARGET / "android-tests/runner.lock"
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error("Another mobile FFI test run is active; reuse its job handle")
        run = Run(args.mode)
        try:
            run.receipt["commit"] = run.command(["git", "rev-parse", "HEAD"], "revision").strip()
            diff = run.command(["git", "diff", "--no-ext-diff", "--no-textconv", "HEAD"], "source-diff")
            run.receipt["tracked_diff_sha256"] = hashlib.sha256(diff.encode()).hexdigest()
            run.receipt["tools"] = {tool: run.command([tool, "--version"], tool).strip()
                                    for tool in ("rustc", "java", "gradle")}
            if args.mode == "local":
                run.local()
            elif args.mode in ("device", "app"):
                run.device(args.device, args.expected_serial, launch=args.mode == "app")
            elif args.mode == "build":
                run.build()
            else:
                run.host()
            run.receipt["status"] = "passed"
            return 0
        except Exception as error:
            run.receipt["status"] = "failed"
            run.receipt["error"] = str(error)
            for field in ("build_status", "install_status", "test_status", "launch_status"):
                if run.receipt[field] == "running":
                    run.receipt[field] = "failed"
            (run.logs / "failure.log").write_text(traceback.format_exc())
            print(error, file=sys.stderr)
            return 1
        finally:
            receipt = run.logs / "receipt.json"
            receipt.write_text(json.dumps(run.receipt, indent=2) + "\n")
            print(f"Receipt: {receipt}", flush=True)


if __name__ == "__main__":
    sys.exit(main())
