# CRH-008: Cutout produces unusually large disk-write resource reports

- Severity: P2
- Evidence status: Phone-confirmed resource reports; producer attribution pending
- Status: OPEN — no current phone acceptance test
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Observed behavior

Three September 12 reports each record about 1,074 MB of file-backed memory dirtied over approximately 2.2–3.0 hours. All report Action taken: none. These counters are not the size of exported recordings and must not be added as proven unique bytes.

## Expected behavior

Long rides should have understandable storage growth and bounded write amplification.

## Evidence and current source

The phone file listing shows a 1,139,482,624-byte ride database. Investigate `crates/cutout-mobile-ffi/src/lib.rs` write_capture_stream and ride persistence, `crates/libcutout-persistence/src`, and `CutoutSessionCore.swift:2107` capture creation. Raw report stack samples must be correlated before assigning the excess to SQLite or JSONL.

Reports (hashes and binary UUIDs in [phone evidence](../phone-evidence.md)):

- `CutoutApp.diskwrites_resource-2026-09-12-124347.ips`
- `CutoutApp.diskwrites_resource-2026-09-12-173825.ips`
- `CutoutApp.diskwrites_resource-2026-09-12-211748.ips`

## Minimal remedy direction

Measure writes per telemetry event/route point and identify duplicate storage or over-frequent flushes before changing retention or durability. Provide a truthful storage-management path where needed.

## Acceptance checks

Replay a representative long capture into an isolated store; measure logical data size, actual writes, retained files and recoverability after interruption. Verify on phone for the same duration.

## Comparison and limits

Keeping the app responsive is a baseline expectation, independent of competitor design. This is not a reported disk-full crash or a claim that the database is corrupt. Never delete the user’s recordings to make the test pass.
