# RID-006: Manual Resume after a phone reboot does not rebase the ride clock

- Severity: P1
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Record a ride at a high device uptime, reboot the phone, reopen the interrupted ride and tap Resume without connecting its vehicle. The new raw monotonic clock can be below stored points, causing new route points to be rejected as out of order and duration to stall.

## Evidence

lib.rs:9669 initializes monotonic_epoch_offset_milliseconds to zero. Automatic same-vehicle recovery explicitly rebases to the old watermark at 10140-10149. Manual resume at 10237-10241 delegates directly to transition_at; 11061-11074 only applies the current offset. recording.rs:1062-1064 compares incoming sample time with retained points.

- `crates/cutout-mobile-ffi/src/lib.rs:9669`
- `crates/cutout-mobile-ffi/src/lib.rs:10140`
- `crates/cutout-mobile-ffi/src/lib.rs:10237`
- `crates/cutout-mobile-ffi/src/lib.rs:11061`
- `crates/cutout-ride-maps/src/recording.rs:1062`

## Expected behavior and smallest remedy

Put recovered-clock rebasing in the shared resume boundary so manual and automatic resume obey the same clock contract.

## Acceptance / reproduction

Persist a ride with last monotonic time 1,000,000 ms, reopen at 1,000 ms, manually resume and ingest valid later samples. Points must append and duration must exclude the reboot gap.

## Comparison and limitations

Competitor reboot recovery details unverified; expectation follows durable resume semantics.

Source-confirmed asymmetry and rejection path; actual reboot not reproduced. Existing automatic-resume and reopen tests do not exercise this manual lower-epoch case.

