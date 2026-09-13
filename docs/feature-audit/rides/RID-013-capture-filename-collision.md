# RID-013: Two capture starts in one second can overwrite the earlier capture

- Severity: P1
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Start two captures within the same wall-clock second through a rapid connection/mode transition. Both use the same path. The second Rust writer truncates the first file; if the earlier writer is still finishing, writes may overlap.

## Evidence

CutoutSessionCore.swift:2102-2103 builds a filename from integer Unix seconds. CaptureWriter::start at lib.rs:12520-12525 accepts an existing path. write_capture_stream at 12693 opens it with File::create, which truncates existing content. Old writers finish asynchronously at CutoutSessionCore.swift:2168-2181.

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2102`
- `crates/cutout-mobile-ffi/src/lib.rs:12520`
- `crates/cutout-mobile-ffi/src/lib.rs:12693`
- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2168`

## Expected behavior and smallest remedy

Allocate a collision-resistant capture name and atomically create a new file. Pass that opened file to the writer instead of reopening a shared path.

## Acceptance / reproduction

Under a fixed wall-clock second, start/finish two real writers and verify distinct artifacts preserve their own records. Also overlap finalization of the first with starting the second.

## Comparison and limitations

Competitor artifact naming unverified; preserving prior recordings is the expected storage contract.

Source-confirmed overwrite path; no user capture was modified and no phone corruption was reproduced.

