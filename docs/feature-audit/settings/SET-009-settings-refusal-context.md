# SET-009: Tune hides why a setting write was refused

- Severity: P2
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Try a setting while moving above its permitted speed or while another command is busy. The control stays enabled, submission results are discarded, and a distant capability row can only say Refused.

## Expected behavior

The control should explain the actionable cause next to the attempted change, such as stop moving or wait for the current write.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:785-808 discards results; 1490-1506 capability rendering passes only state kind and time; 1540-1561 maps all refusals to one string. CutoutSessionCore.swift:1010-1030 returns typed refusal reasons, and CutoutMobile.swift setting-state types retain them. The trip-reset status implementation at CutoutRouteViews.swift:1510-1537 already distinguishes stationary, expired and busy causes.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Reuse typed refusal reasons and existing trip-reset presentation pattern at the affected control; disable obviously unavailable interactions based on current state.

## Acceptance and reproduction checks

Inject missingArm, busy and transport failure; each must produce visible actionable feedback without scrolling through the full capability inventory.

## Limits

Rust still enforces stationary policy. This is not an assertion that above-speed writes bypass its guard.

