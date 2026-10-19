"""Device routing and real instrumentation protocol acceptance, without a phone."""

import importlib.util
from pathlib import Path
import unittest
import tempfile
import io
from unittest.mock import patch
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "mobile_ffi_runner", Path(__file__).resolve().parents[2] / "scripts/test-mobile-ffi.py"
)
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


def status(name, code=0, failure=""):
    return (
        "INSTRUMENTATION_STATUS: class=lol.cutout.ffitest.MobileFfiTest\n"
        f"INSTRUMENTATION_STATUS: test={name}\n"
        "INSTRUMENTATION_STATUS: numtests=2\n"
        f"INSTRUMENTATION_STATUS: stack={failure}\n"
        f"INSTRUMENTATION_STATUS_CODE: {code}\n"
    )


class DeviceSelectionTests(unittest.TestCase):
    def test_capture_picker_sections_are_exported_for_android(self):
        labels = runner.collect_shared_labels()["en"]
        self.assertEqual(labels["picker.section.detected_devices"], "Detected devices")
        self.assertEqual(labels["picker.section.probe_first"], "Probe first")
        self.assertEqual(labels["picker.section.record_only"], "Record only")

    def test_requires_supported_physical_arm64_android(self):
        valid = {"serial": "PHONE", "abi": "arm64-v8a", "api": "35", "emulator": ""}
        for override in ({"abi": "x86_64"}, {"api": "32"}, {"emulator": "1"}, {"serial": ""}):
            with self.subTest(override=override), self.assertRaises(ValueError):
                runner.validate_device(dict(valid, **override), "PHONE")
        runner.validate_device(valid, "PHONE")

    def test_network_endpoint_cannot_substitute_a_different_physical_serial(self):
        device = {"serial": "OTHER", "abi": "arm64-v8a", "api": "35", "emulator": ""}
        with self.assertRaisesRegex(ValueError, "Physical serial"):
            runner.validate_device(device, "EXPECTED")
        runner.validate_device(device, "OTHER")

    def test_explicit_network_transport_is_not_replaced_by_usb(self):
        devices = "List of devices attached\nUSB device usb:1\n192.0.2.1:5555 device\n"
        self.assertEqual(runner.select_device(devices, "192.0.2.1:5555"), "192.0.2.1:5555")
        with self.assertRaisesRegex(ValueError, "explicitly"):
            runner.select_device(devices, None)

    def test_unavailable_explicit_target_never_falls_back_to_another_phone(self):
        for state in ("offline", "unauthorized"):
            with self.subTest(state=state), self.assertRaisesRegex(ValueError, state):
                runner.select_device(f"WANTED {state}\nOTHER device\n", "WANTED")
        with self.assertRaisesRegex(ValueError, "missing"):
            runner.select_device("OTHER device\n", "WANTED")

    def test_exactly_one_authorized_device_can_be_selected(self):
        self.assertEqual(runner.select_device("List of devices attached\nUSB device usb:1\n", None), "USB")
        with self.assertRaises(ValueError):
            runner.select_device("List of devices attached\n", None)


class EmulatorSelectionTests(unittest.TestCase):
    def test_emulator_requires_its_explicit_transport_avd_and_guest_abi(self):
        valid = {"serial": "EMULATOR", "abi": "x86_64", "api": "35", "emulator": "1"}
        runner.validate_emulator(valid, "emulator-5554", "cutout-api35-x86_64", "cutout-api35-x86_64", "35")
        for override in ({"abi": "arm64-v8a"}, {"api": "32"}, {"api": "34"}, {"emulator": ""}, {"serial": ""}):
            with self.subTest(override=override), self.assertRaises(ValueError):
                runner.validate_emulator(dict(valid, **override), "emulator-5554", "owned", "owned", "35")
        for selector, avd in (("PHONE", "owned"), ("emulator-5554", "other")):
            with self.subTest(selector=selector, avd=avd), self.assertRaises(ValueError):
                runner.validate_emulator(valid, selector, avd, "owned", "35")
        # An emulator remains forbidden in physical-device mode.
        with self.assertRaises(ValueError):
            runner.validate_device(valid, None)

    def test_nonbuild_modes_cannot_silently_ignore_an_explicit_abi(self):
        for mode in ("device", "app", "emulator", "host", "local"):
            with self.subTest(mode=mode), patch("sys.argv", ["runner", mode, "--abi", "x86_64"]), \
                    patch("sys.stderr", io.StringIO()), self.assertRaises(SystemExit) as result:
                runner.main()
            self.assertEqual(result.exception.code, 2)

    def test_reuse_requires_both_unchanged_apks_matching_source_and_abi(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            apks = [root / "app.apk", root / "test.apk"]
            for apk in apks:
                apk.write_bytes(b"built artifact")
            receipt = {"build_status": "passed", "android_abi": "x86_64", "source_sha256": "source",
                       "artifacts": [{"path": apk.name, "sha256": runner.digest(apk)} for apk in apks]}
            self.assertEqual(runner.reusable_artifacts(receipt, "x86_64", "source", root), apks)
            for change in ({"build_status": "failed"}, {"android_abi": "arm64-v8a"},
                           {"source_sha256": "old"}, {"artifacts": receipt["artifacts"][:1]}):
                with self.subTest(change=change), self.assertRaises(ValueError):
                    runner.reusable_artifacts(dict(receipt, **change), "x86_64", "source", root)
            apks[1].write_bytes(b"replaced instrumentation")
            with self.assertRaisesRegex(ValueError, "hash changed"):
                runner.reusable_artifacts(receipt, "x86_64", "source", root)

    def test_packaged_jni_rejects_wrong_abi_or_missing_rust_library(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "app.apk"
            test = root / "test.apk"
            test.write_bytes(b"instrumentation")
            run = runner.Run.__new__(runner.Run)
            run.receipt = {}
            with patch.object(runner, "ROOT", root):
                for libraries in ({"lib/arm64-v8a/libcutout_mobile_ffi.so": b"rust"},
                                  {"lib/x86_64/libjnidispatch.so": b"jna"}):
                    with zipfile.ZipFile(app, "w") as archive:
                        for name, contents in libraries.items():
                            archive.writestr(name, contents)
                    with self.subTest(libraries=libraries), self.assertRaises(ValueError):
                        run.check_apks([app, test], "x86_64")
                with zipfile.ZipFile(app, "w") as archive:
                    archive.writestr("lib/x86_64/libcutout_mobile_ffi.so", b"rust")
                run.check_apks([app, test], "x86_64")
                self.assertIn("lib/x86_64/libcutout_mobile_ffi.so", run.receipt["packaged_libraries"])


class InstrumentationTests(unittest.TestCase):
    def test_accepts_completed_tests_and_ignores_custom_pid_status(self):
        output = (
            "INSTRUMENTATION_STATUS: cutoutPid=123\nINSTRUMENTATION_STATUS_CODE: 2\n"
            + status("boundary") + status("storage") + "INSTRUMENTATION_CODE: -1\n"
        )
        results = runner.instrumentation_results(output)
        self.assertEqual([t["test"] for t in results], ["boundary", "storage"])
        self.assertTrue(all(t["failure"] == "" for t in results))

    def test_retains_assertion_failures_even_with_successful_adb_exit(self):
        results = runner.instrumentation_results(
            status("boundary", -2, "java.lang.AssertionError: native result")
            + status("storage") + "INSTRUMENTATION_CODE: -1\n"
        )
        self.assertIn("AssertionError", results[0]["failure"])

    def test_rejects_crash_zero_tests_incomplete_run_and_duplicate_results(self):
        cases = (
            "INSTRUMENTATION_FAILED: process crashed\nINSTRUMENTATION_CODE: 0\n",
            "OK (0 tests)\nINSTRUMENTATION_CODE: -1\n",
            status("boundary") + "INSTRUMENTATION_CODE: -1\n",
            status("boundary") + status("boundary") + "INSTRUMENTATION_CODE: -1\n",
            status("boundary") + status("storage"),
        )
        for output in cases:
            with self.subTest(output=output), self.assertRaises(ValueError):
                runner.instrumentation_results(output)


if __name__ == "__main__":
    unittest.main()
