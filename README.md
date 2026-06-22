# Cutout

Rust library and CLI for BTLE communication with PEVs, starting with balance PEV read-only support.

## CLI

`cutout` is the hardware-facing command-line tool. It is currently focused on
Bluetooth discovery, GATT endpoint inspection, and read-only protocol fixture
capture.

```console
cutout scan --seconds 10
cutout connect --name-contains Aero
cutout connect --address AA:BB:CC:DD:EE:FF --seconds 8
cutout capture --name-contains NF2557 --seconds 20
cutout capture --name-contains Falcon --profile falcon --probe identity --probe firmware
```

For dashboard work under bacon, use the dev-shell wrapper so bacon runs
headless and does not draw over the dashboard TUI:

```console
cutout-dashboard-watch --device NF2557 --seconds 30
```

Target selection can use `--address`, `--name-contains`, or neither. When no
target filter is supplied, the first matching peripheral from the scan results
is used.

`capture` prints raw protocol evidence for fixture work. Review capture
logs before sharing them, because they may contain device identifiers and raw
notification payloads.
