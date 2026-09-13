# RID-025: Switching vehicles can attribute the new wheel's telemetry to the old ride

- Severity: P1
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Start a ride on wheel A, disconnect without stopping the map ride, then connect wheel B. The still-active ride remains associated with A, yet B's notifications refresh that ride's telemetry timestamp. New route points can therefore claim fresh associated telemetry for A while the rider is using B.

## Evidence

`CutoutSessionCore.swift:1974-1981` discards the result of observeVehicleConnection and always calls observeTelemetry. Rust's `recording.rs:983-988` correctly returns IdentityMismatch when a ride already belongs to another vehicle. Its observe_telemetry at 1003-1030 accepts only a timestamp and cannot verify the source. The automatic ensure path at `lib.rs:10151-10162` keeps an existing Active ride rather than starting a B ride.

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:1974`
- `crates/cutout-ride-maps/src/recording.rs:983`
- `crates/cutout-ride-maps/src/recording.rs:1003`
- `crates/cutout-mobile-ffi/src/lib.rs:10151`
- `crates/cutout-mobile-ffi/src/lib.rs:10647`

## Expected behavior and smallest remedy

Make confirmed telemetry observation carry/validate vehicle identity, and handle association mismatch explicitly. Either finish/split the ride when switching vehicles or keep A's ride clearly stale while offering the correct action; do not silently relabel B as A.

## Acceptance / reproduction

Use two deterministic vehicle identities. After A disconnects and B connects, verify the recorded association, telemetry freshness, displayed vehicle and new GPS points remain mutually consistent. Keep the existing first-association preservation test, and add the production caller sequence that currently ignores its mismatch result.

## Comparison and limitations

Competitor multi-wheel ride switching behavior was not independently verified. Attribution to the actual vehicle is an internal correctness requirement.

Source confirms the ignored mismatch and identity-free telemetry path. No two-wheel physical session was reproduced. See RID-005 for the separate policy problem that leaves map recording active on explicit disconnect.

