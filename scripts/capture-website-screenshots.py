#!/usr/bin/env python3
"""Run the five website screenshot UI cases and export their native PNG attachments."""

from __future__ import annotations

import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs/mockups/cutout-site/assets/screenshots/en"
CASES = {
    "euc-main": ("testEucMain", "fictional music: Dusk on Colfax by Mira Sol"),
    "rgb": ("testRgbControls", "local fake accessory: MELK-OC21 6A"),
    "tune": ("testTune", "EUC fixture with high-beam setting controls"),
    "ride-tracking": (
        "testRideTracking",
        "51 synthetic samples over five minutes on Denver's public Platte River Trail route",
    ),
    "charging-map": (
        "testChargingMap",
        "three fictional local spots: Juniper Corner Library, Bluebird Coffee Room, Riverglass Charge Stop",
    ),
}
RESULT_PATTERN = re.compile(r"\(([^()]*\.xcresult)\)")
GIT_STATUS_PATHS = ["--", ".", ":(exclude)docs/mockups/cutout-site/assets/screenshots/en"]


def run(command: list[str], *, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
        check=True,
    )


def png_dimensions(path: Path) -> tuple[int, int]:
    with path.open("rb") as image:
        header = image.read(24)
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
        raise ValueError(f"Not a PNG screenshot: {path.name}")
    return struct.unpack(">II", header[16:24])


def attachment_records(manifest: list[dict]) -> dict[str, dict]:
    selected: dict[str, dict] = {}
    for test in manifest:
        identifier = test.get("testIdentifier", "")
        if "CutoutWebsiteScreenshotTests" not in identifier:
            continue
        for attachment in test.get("attachments", []):
            name = attachment.get("suggestedHumanReadableName", "")
            if name.startswith("website-"):
                scene = name.removeprefix("website-")
                if scene not in CASES or attachment.get("isAssociatedWithFailure"):
                    raise ValueError(f"Unexpected or failed screenshot attachment: {name}")
                if CASES[scene][0] not in identifier or scene in selected:
                    raise ValueError(f"Screenshot attachment has an unexpected test owner: {name}")
                selected[scene] = {"testIdentifier": identifier, **attachment}
    if set(selected) != set(CASES):
        raise ValueError(f"Expected five named screenshot attachments; found {sorted(selected)}")
    return selected


def device_metadata(summary: dict) -> tuple[dict, str | None]:
    configuration = summary.get("devicesAndConfigurations", {})
    device = configuration.get("device")
    if not isinstance(device, dict) or not device:
        raise ValueError("xcresult summary did not include its simulator device metadata")
    return device, summary.get("environmentDescription")


def main() -> int:
    if sys.platform != "darwin" or os.uname().machine != "arm64":
        raise SystemExit("Screenshot capture requires the supported Apple Silicon Devenv host.")
    devenv_root = os.environ.get("DEVENV_ROOT")
    if not devenv_root or Path(devenv_root).resolve() != ROOT:
        raise SystemExit("Run from the repository Devenv: devenv shell -- python3 scripts/capture-website-screenshots.py")
    revision = run(["git", "rev-parse", "HEAD"], capture=True).stdout.strip()
    dirty_source = run(
        ["git", "status", "--porcelain", "--untracked-files=all", *GIT_STATUS_PATHS], capture=True
    ).stdout.strip()
    if dirty_source:
        raise SystemExit("Refusing capture from a dirty source tree; commit the harness and fixtures first.")

    try:
        runner = run(
            [
                "scripts/run-ios-ui-tests.sh",
                "--timeout",
                "1200",
                "--appearance",
                "dark",
                "--increase-contrast",
                "disabled",
                "--content-size",
                "large",
                "-only-testing:CutoutAppUITests/CutoutWebsiteScreenshotTests",
            ],
            capture=True,
        )
    except subprocess.CalledProcessError as error:
        if error.stdout:
            print(error.stdout, end="", file=sys.stderr)
        raise SystemExit(error.returncode) from error
    print(runner.stdout, end="")
    matches = RESULT_PATTERN.findall(runner.stdout)
    if not matches:
        raise SystemExit("The UI test runner passed without reporting its result bundle path.")
    result_bundle = Path(matches[-1])

    with tempfile.TemporaryDirectory(prefix="cutout-website-screenshots-") as temporary:
        temporary_path = Path(temporary)
        exported = temporary_path / "attachments"
        run(["xcrun", "xcresulttool", "export", "attachments", "--path", str(result_bundle), "--output-path", str(exported), "--filter", "*.png"])
        records = json.loads((exported / "manifest.json").read_text())
        selected = attachment_records(records)
        summary = json.loads(
            run(["xcrun", "xcresulttool", "get", "test-results", "summary", "--path", str(result_bundle)], capture=True).stdout
        )
        device, environment = device_metadata(summary)
        current_revision = run(["git", "rev-parse", "HEAD"], capture=True).stdout.strip()
        dirty_source = run(
            ["git", "status", "--porcelain", "--untracked-files=all", *GIT_STATUS_PATHS], capture=True
        ).stdout.strip()
        if current_revision != revision or dirty_source:
            raise SystemExit("Source changed during capture; refusing to publish screenshots from mixed revisions.")

        staged = temporary_path / "ready"
        staged.mkdir()
        screenshots = []
        for scene, (_, synthetic_data) in CASES.items():
            record = selected[scene]
            filename = Path(record["exportedFileName"]).name
            if filename != record["exportedFileName"]:
                raise ValueError("xcresult attachment filename was not a simple file name")
            source = exported / filename
            width, height = png_dimensions(source)
            destination_name = f"{scene}.png"
            shutil.copyfile(source, staged / destination_name)
            screenshots.append(
                {
                    "scene": scene,
                    "file": destination_name,
                    "testIdentifier": record["testIdentifier"],
                    "timestamp": record.get("timestamp"),
                    "dimensionsPixels": {"width": width, "height": height},
                    "syntheticData": synthetic_data,
                **(
                    {
                        "routeSource": "https://github.com/JasonSanford/denver-bike-routes/blob/master/routes.geojson",
                        "routeFeature": "Littleton to REI Flagship - Downtown Denver",
                    }
                    if scene == "ride-tracking"
                    else {}
                ),
                }
            )

        manifest = {
            "sourceRevision": revision,
            "device": device,
            "environmentDescription": environment,
            "captureSettings": {
                "appearance": "dark",
                "increaseContrast": "disabled",
                "contentSize": "large",
                "orientation": "portrait",
                "language": "en",
                "locale": "en_US",
            },
            "screenshots": screenshots,
        }
        (staged / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")

        OUTPUT.mkdir(parents=True, exist_ok=True)
        for scene in CASES:
            os.replace(staged / f"{scene}.png", OUTPUT / f"{scene}.png")
        os.replace(staged / "manifest.json", OUTPUT / "manifest.json")
    print(f"Exported five full-screen native PNGs and manifest to {OUTPUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
