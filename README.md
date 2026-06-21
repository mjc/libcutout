# Cutout

Rust library and CLI for BTLE communication with PEVs, starting with balance PEV read-only support.

## CLI

`cutout` is the hardware-facing command-line tool. It is currently focused on
Bluetooth discovery, GATT endpoint inspection, and read-only Aero/Veteran-family
fixture capture.

```console
cutout scan --seconds 10
cutout connect --name-contains Aero
cutout connect --address AA:BB:CC:DD:EE:FF --seconds 8
cutout capture-aero --name-contains NF2557 --seconds 20
```

Target selection can use `--address`, `--name-contains`, or neither. When no
target filter is supplied, the first matching peripheral from the scan results
is used.

`capture-aero` prints raw protocol evidence for fixture work. Review capture
logs before sharing them, because they may contain device identifiers and raw
notification payloads.
