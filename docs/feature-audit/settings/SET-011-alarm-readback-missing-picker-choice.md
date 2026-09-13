# SET-011: Speed-alarm picker cannot represent Off or PWM readback

- Severity: P2
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Connect a supported Begode wheel whose reported speed-alarm mode is Off or PWM tiltback, then open Tune. The Picker selection returns that mode, but only Both and Stage 1 only have tags.

## Expected behavior

The picker must represent every possible reported value even when some values are intentionally read-only.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1220-1255 declares only [.both,.stageOneOnly] but selection returns any current/readback documented kind. crates/cutout-core/src/lib.rs:288-299 includes Off/PwmTiltback; test begode_speed_alarm_settings_bits_use_documented_mapping at lib.rs:11220 verifies mapping. request_encoder.rs:412-413 intentionally only encodes the two offered writable modes.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Represent unsupported-to-write current modes as read-only selections/status rather than adding undocumented write commands.

## Acceptance and reproduction checks

Inject both Off and PwmTiltback readback. The picker must show their actual values without an invalid-selection warning or fallback to Both.

## Limits

A blank or incorrect rendered selection requires UI reproduction; the mismatch between possible selection values and tags is source-confirmed.

