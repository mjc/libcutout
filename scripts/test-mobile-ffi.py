#!/usr/bin/env python3
"""Shared Kotlin smoke build and opt-in physical/headless Android instrumentation runner."""

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
import socket
import time
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
ANDROID_TARGETS = {"arm64-v8a": "aarch64-linux-android", "x86_64": "x86_64-linux-android"}
EMULATOR_TESTS = "lol.cutout.ffitest.MobileFfiTest,lol.cutout.ffitest.ProductionAppTest"
GRADLE_CACHE = TARGET / "android-tests/gradle-cache"
SHARED_LABEL_PREFIXES = ("device.", "settings.", "actions.", "controls.", "euc.", "vesc.",
                         "ride.", "status.", "footpad.", "music.", "bms.", "lighting.",
                         "charging.", "setup.", "navigation.", "ride_map.", "camera.", "captures.", "capture.")


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


def validate_emulator(device: dict[str, str], selector: str, avd: str, expected_avd: str, expected_api: str) -> None:
    if (not re.fullmatch(r"emulator-[0-9]+", selector) or device["emulator"] != "1"
            or device["abi"] != "x86_64" or int(device["api"]) < 33 or device["api"] != expected_api
            or not device["serial"] or avd != expected_avd):
        raise ValueError(f"Expected the owned x86_64 Android 33+ emulator {expected_avd}: {device}, {avd}")


def source_fingerprint() -> str:
    """Content identity also covers new task sources, independent of commit/checkout path."""
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT)
    identity = hashlib.sha256()
    for name in sorted(set(names.split(b"\0")) - {b""}):
        path = ROOT / os.fsdecode(name)
        identity.update(name + b"\0")
        identity.update((digest(path) if path.is_file() else "deleted").encode() + b"\0")
    return identity.hexdigest()


def reusable_artifacts(receipt: dict, abi: str, fingerprint: str, root: Path = ROOT) -> list[Path]:
    if (receipt.get("build_status") != "passed" or receipt.get("android_abi") != abi
            or receipt.get("source_sha256") != fingerprint):
        raise ValueError("APK receipt does not match this source and requested ABI; build matching artifacts")
    artifacts = receipt.get("artifacts", [])
    if len(artifacts) != 2:
        raise ValueError("Receipt must identify both production and instrumentation APKs")
    paths = []
    for artifact in artifacts:
        path = root / artifact["path"]
        if digest(path) != artifact["sha256"]:
            raise ValueError(f"APK hash changed: {path}")
        paths.append(path)
    return paths


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

    def prepare(self, android: bool, abi: str = "arm64-v8a") -> Path:
        prepare_replay()
        prepare_shared_labels()
        self.receipt["replay_capture_sha256"] = digest(REPLAY_CAPTURE)
        args = ["cargo", "build", "--locked", "--jobs", "1", "-p", "cutout-mobile-ffi", "--lib"]
        folder = TARGET / "debug"
        if android:
            target = ANDROID_TARGETS[abi]
            args.extend(["--target", target])
            folder = TARGET / target / "debug"
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

    def build(self, abi: str = "arm64-v8a") -> list[Path]:
        self.receipt["build_status"] = "running"
        self.receipt["android_abi"] = abi
        library = self.prepare(True, abi)
        link = TARGET / "android-tests/jniLibs" / abi / library.name
        link.parent.mkdir(parents=True, exist_ok=True)
        if not link.is_symlink() or link.resolve() != library:
            if link.exists() or link.is_symlink():
                link.unlink()
            link.symlink_to(library)
        self.command(["gradle", "--no-daemon", "--max-workers=2", "--console=plain",
                      "--project-cache-dir", str(TARGET / "android-tests/gradle-cache"),
                      f"-Pcutout.android.abi={abi}",
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
        self.check_apks(apks, abi)
        self.receipt["build_status"] = "passed"
        return apks

    def check_apks(self, apks: list[Path], abi: str) -> None:
        with zipfile.ZipFile(apks[0]) as apk:
            natives = [name for name in apk.namelist() if name.startswith("lib/") and name.endswith(".so")]
            if not natives or any(not n.startswith(f"lib/{abi}/") for n in natives):
                raise ValueError(f"Unexpected APK native ABIs: {natives}")
            self.receipt["packaged_libraries"] = {n: hashlib.sha256(apk.read(n)).hexdigest() for n in natives}
        if f"lib/{abi}/libcutout_mobile_ffi.so" not in natives:
            raise ValueError("APK lacks the selected Rust JNI library")
        self.receipt["artifacts"] = [{"path": str(apk.relative_to(ROOT)), "sha256": digest(apk)} for apk in apks]

    def artifacts(self, abi: str, receipt_path: str | None) -> list[Path]:
        if receipt_path is None:
            return self.build(abi)
        receipt = json.loads(Path(receipt_path).read_text())
        apks = reusable_artifacts(receipt, abi, self.receipt["source_sha256"])
        self.check_apks(apks, abi)
        self.receipt.update(build_status="passed", android_abi=abi, reused_receipt=str(Path(receipt_path).resolve()))
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
            if self.last_returncode:
                raise ValueError(
                    f"Android UI compilation or test startup failed ({self.last_returncode}); "
                    "inspect android-local-ui-tests.log"
                )
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

    def device(self, selector: str | None, expected_serial: str | None, launch: bool = False,
               artifacts: str | None = None):
        output = self.command(["adb", "devices", "-l"], "adb-devices", 30)
        self.selector = select_device(output, selector)
        device = self.identity()
        validate_device(device, expected_serial)
        self.receipt["device"] = dict(device, transport=self.selector)
        apks = self.artifacts("arm64-v8a", artifacts)
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
        self.instrument()

    def instrument(self, test_class: str | None = None) -> None:
        self.receipt["test_status"] = "running"
        selection = ["-e", "class", test_class] if test_class else []
        self.receipt["test_selection"] = test_class or "all"
        output = self.adb(["shell", "am", "instrument", "-w", "-r", *selection, RUNNER], "instrumentation", 600)
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


    def emulator(self, artifacts: str | None = None, test_class: str = EMULATOR_TESTS) -> None:
        if platform.system() != "Linux" or platform.machine() != "x86_64":
            raise ValueError("Headless emulator mode requires x86_64 Linux with KVM")
        if not os.access("/dev/kvm", os.R_OK | os.W_OK):
            raise ValueError("Headless emulator mode requires read/write /dev/kvm access")
        self.command(["emulator", "-accel-check"], "emulator-acceleration", 30)
        self.command(["adb", "start-server"], "adb-server", 30)
        apks = self.artifacts("x86_64", artifacts)
        avd_home = TARGET / "android-tests/avd"
        avd_home.mkdir(parents=True, exist_ok=True)
        self.env["ANDROID_AVD_HOME"] = str(avd_home)
        # The owned emulator never connects to the user's gaming/display/audio session.
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "PULSE_SERVER"):
            self.env.pop(key, None)
        avd_name = "cutout-api" + self.env["CUTOUT_ANDROID_API"] + "-x86_64"
        image = f"system-images;android-{self.env['CUTOUT_ANDROID_API']};google_apis;x86_64"
        image_dir = Path(self.env["ANDROID_HOME"]) / "system-images" / ("android-" + self.env["CUTOUT_ANDROID_API"]) / "google_apis/x86_64"
        if not (image_dir / "system.img").is_file():
            raise ValueError("The devenv SDK must provide the configured emulator system image")
        if not (avd_home / (avd_name + ".ini")).exists():
            # Explicit device profile avoids avdmanager's interactive custom-device prompt.
            self.command(["avdmanager", "create", "avd", "--name", avd_name, "--package", image,
                          "--device", "pixel_2", "--path", str(avd_home / (avd_name + ".avd"))], "create-avd", 90)
        port = None
        for candidate in range(5554, 5584, 2):
            try:
                with socket.socket() as console, socket.socket() as adb:
                    console.bind(("127.0.0.1", candidate))
                    adb.bind(("127.0.0.1", candidate + 1))
                port = candidate
                break
            except OSError:
                continue
        if port is None:
            raise ValueError("No available emulator console/adb port pair")
        self.selector = f"emulator-{port}"
        args = ["emulator", "-avd", avd_name, "-port", str(port), "-no-window", "-no-audio",
                "-no-boot-anim", "-no-snapshot", "-accel", "on", "-gpu", "swiftshader",
                "-cores", "2", "-memory", "2048", "-camera-back", "none", "-camera-front", "none"]
        log_path = self.logs / "emulator.log"
        self.receipt["emulator"] = {"avd": avd_name, "image": image, "image_directory": str(image_dir),
                                    "transport": self.selector, "command": args, "log": str(log_path),
                                    "stop_status": "not_started"}
        with log_path.open("w") as output:
            process = subprocess.Popen(args, cwd=ROOT, env=self.env, stdout=output, stderr=subprocess.STDOUT)
            self.receipt["emulator"].update(pid=process.pid, started_at=datetime.now(timezone.utc).isoformat())
            try:
                deadline = time.monotonic() + 240
                while True:
                    if process.poll() is not None:
                        raise ValueError(f"Emulator exited ({process.returncode}); inspect {log_path}")
                    result = subprocess.run(["adb", "-s", self.selector, "shell", "getprop", "sys.boot_completed"],
                                            env=self.env, capture_output=True, text=True, timeout=10)
                    if result.returncode == 0 and result.stdout.strip() == "1":
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("Emulator did not finish booting within 240 seconds")
                    time.sleep(2)
                device = self.identity()
                avd = self.adb(["emu", "avd", "name"], "avd-name").splitlines()[0].strip()
                validate_emulator(device, self.selector, avd, avd_name, self.env["CUTOUT_ANDROID_API"])
                self.receipt["device"] = dict(device, transport=self.selector, avd=avd)
                self.receipt["install_status"] = "running"
                # Exact identity is rechecked after build, boot and immediately before install.
                if self.identity() != device or process.poll() is not None:
                    raise ValueError("Owned emulator identity changed before installation")
                for apk in apks:
                    self.adb(["install", "-r", "-t", str(apk)], "install", 180)
                self.receipt["install_status"] = "passed"
                if self.identity() != device:
                    raise ValueError("Owned emulator identity changed before instrumentation")
                self.instrument(test_class)
                self.receipt["launch_status"] = "running"
                result = self.adb(["shell", "am", "start", "-W", "-n", APP + "/.MainActivity"], "launch")
                if "Status: ok" not in result:
                    raise ValueError("Owned emulator did not confirm production activity launch")
                self.receipt["launch_status"] = "passed"
                time.sleep(2)
            finally:
                if process.poll() is None and "device" in self.receipt:
                    # Preserve native output on success and assertion failures, before teardown.
                    try:
                        self.adb(["logcat", "-d", "-t", "500"], "emulator-logcat")
                        screenshot = self.logs / "emulator-screen.png"
                        with screenshot.open("wb") as image:
                            capture = subprocess.run(["adb", "-s", self.selector, "exec-out", "screencap", "-p"],
                                                     env=self.env, stdout=image, stderr=subprocess.PIPE, timeout=30)
                        if capture.returncode == 0 and screenshot.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"):
                            self.receipt["screenshot"] = str(screenshot)
                        else:
                            self.receipt["screenshot_error"] = capture.stderr.decode(errors="replace")
                    except Exception as error:
                        self.receipt["capture_error"] = str(error)
                if process.poll() is None:
                    # Popen is the retained ownership handle; never signal a rediscovered PID.
                    process.terminate()
                    try:
                        process.wait(timeout=30)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=10)
                self.receipt["emulator"].update(stop_status="stopped", returncode=process.returncode,
                                                stopped_at=datetime.now(timezone.utc).isoformat())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("host", "build", "device", "app", "local", "emulator"))
    parser.add_argument("--device", default=os.environ.get("CUTOUT_ANDROID_DEVICE"))
    parser.add_argument("--expected-serial", default=os.environ.get("CUTOUT_ANDROID_SERIAL"))
    parser.add_argument("--abi", choices=tuple(ANDROID_TARGETS),
                        help="JNI ABI for build mode; emulator mode always selects x86_64")
    parser.add_argument("--artifacts", help="Reuse APKs from a matching source/ABI/hash receipt without rebuilding")
    parser.add_argument("--test-class", default=EMULATOR_TESTS,
                        help="Emulator instrumentation classes (comma separated)")
    args = parser.parse_args()
    if args.abi and args.mode != "build":
        parser.error("--abi is supported only by build mode")
    if args.mode == "emulator" and (args.device or args.expected_serial):
        parser.error("Emulator mode owns its target; unset physical-device selectors")
    if args.artifacts and args.mode not in ("device", "app", "emulator"):
        parser.error("--artifacts is supported by device, app and emulator modes")
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
            run.receipt["source_sha256"] = source_fingerprint()
            run.receipt["tools"] = {tool: run.command([tool, "--version"], tool).strip()
                                    for tool in ("rustc", "java", "gradle")}
            if args.mode == "local":
                run.local()
            elif args.mode in ("device", "app"):
                run.device(args.device, args.expected_serial, launch=args.mode == "app", artifacts=args.artifacts)
            elif args.mode == "emulator":
                run.emulator(args.artifacts, args.test_class)
            elif args.mode == "build":
                run.build(args.abi or "arm64-v8a")
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
