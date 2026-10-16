import importlib.util
import json
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[2] / "scripts/run-charging-service-ui-tests.py"
SPEC = importlib.util.spec_from_file_location("charging_service_ui_tests", SCRIPT)
HARNESS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS)


class ChargingServiceHarnessTests(unittest.TestCase):
    def test_run_uses_finite_default_and_preserves_explicit_timeout(self):
        completed = subprocess.CompletedProcess([], 0)
        with patch.object(HARNESS, "run_process_group", return_value=completed) as run:
            HARNESS.run(["cargo", "build"])
            self.assertEqual(run.call_args.kwargs["timeout"], 600)
            HARNESS.run(["cargo", "build"], timeout=900)
            self.assertEqual(run.call_args.kwargs["timeout"], 900)

    def test_shutdown_succeeds_when_simulator_is_already_shutdown(self):
        responses = [
            subprocess.CompletedProcess([], 149, stderr="device is not booted"),
            subprocess.CompletedProcess([], 0, stdout=json.dumps({"devices": {"iOS": [
                {"udid": "sim-1", "state": "Shutdown"}
            ]}})),
        ]
        with patch.object(HARNESS.subprocess, "run", side_effect=responses) as run:
            HARNESS.shutdown_simulator("sim-1")
        self.assertEqual([call.kwargs["timeout"] for call in run.call_args_list], [30, 30])

    def test_shutdown_surfaces_failure_when_simulator_remains_booted(self):
        responses = [
            subprocess.CompletedProcess([], 1, stderr="shutdown failed"),
            subprocess.CompletedProcess([], 0, stdout=json.dumps({"devices": {"iOS": [
                {"udid": "sim-1", "state": "Booted"}
            ]}})),
        ]
        with patch.object(HARNESS.subprocess, "run", side_effect=responses):
            with self.assertRaisesRegex(RuntimeError, "shutdown failed"):
                HARNESS.shutdown_simulator("sim-1")

    def test_timeout_kills_descendants_in_the_owned_process_group(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "descendant-finished"
            parent = (
                "import subprocess, sys, time; "
                f"child = subprocess.Popen([sys.executable, '-c', \"import pathlib, signal, time; "
                f"signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(1.8); "
                f"pathlib.Path({str(marker)!r}).touch()\"]); "
                "child.wait()"
            )
            with self.assertRaises(subprocess.TimeoutExpired) as failure:
                HARNESS.run_process_group([sys.executable, "-c", parent], timeout=0.4)
            self.assertAlmostEqual(failure.exception.timeout, 0.4, delta=0.01)
            time.sleep(0.8)
            self.assertFalse(marker.exists(), "timed-out descendant outlived the runner")


if __name__ == "__main__":
    unittest.main()
