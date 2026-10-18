"""Device routing and real instrumentation protocol acceptance, without a phone."""

import importlib.util
from pathlib import Path
import unittest

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
