# SET-017: Unknown-topology screen drops reported temperature-sensor counts

- Severity: P2
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open unknown-topology BMS with decoded pack temperature readings. The temperature-sensor count can show unavailable even though diagnostics know the number of readings.

## Expected behavior

The sensor count should use the actual reported temperature sensors without assuming a sensor per cell group.

## Evidence

CutoutMobile.swift:5111-5119 counts only groups.compactMap(temperature). Mobile FFI lib.rs:11631-11651 sets every projected cell group temperature to None while lib.rs:11654-11661 places pack sensors in the separate temperatures array. CutoutMobile.swift:4721-4722 maps it to temperatureReadings; BmsUnknownLayout.swift:39-44 uses the incorrect count.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Use the proper observed sensor collection and preserve identities when combining pages.

## Acceptance and reproduction checks

Provide six temperatureReadings and groups with no per-cell temperature. Unknown topology should report six sensors, consistent with diagnostics. Empty readback should remain unavailable.

## Limits

Sensor-to-cell association is not established; do not fabricate group temperatures.

