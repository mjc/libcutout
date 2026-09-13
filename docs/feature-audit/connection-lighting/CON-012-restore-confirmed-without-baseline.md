# CON-012: Lighting says Confirmed while Restore has no usable baseline

- Severity: P2
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Pair a new accessory, enable Restore, change solid color and mark it confirmed; optionally confirm power and brightness separately, then reconnect.

## Actual and expected behavior

**Actual:** The UI says Confirmed, but partial confirmation cannot create a complete baseline. Restore remains unavailable and silently does nothing. The initial color/power/brightness controls all use partial scopes, so the ordinary solid-color workflow never establishes the required baseline.

**Expected:** Explain restore readiness and provide a clear way to apply and confirm a complete scene without guessing unobserved fields.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1944](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1944) — markConfirmed updates UI status before attempting partial persistence.
- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1968](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1968) — Failed partial confirmation marks persistence unconfirmed without reflecting that in commandStatus.
- [crates/cutout-core/src/rgb_lighting.rs:252](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-core/src/rgb_lighting.rs:252) — confirm_partial_state immediately returns false without a preexisting complete baseline.
- [swift/CutoutMobile/Tests/CutoutAppTests/CutoutAppRouteTests.swift:1440](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Tests/CutoutAppTests/CutoutAppRouteTests.swift:1440) — Existing test explicitly expects first color confirmation to leave restoreCandidate nil.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Expose whether Restore has a complete confirmed scene and provide an explicit complete-state apply/confirm action. Preserve conservative per-field confirmation.

## Acceptance checks

- Fresh accessory can establish a restore baseline through a discoverable flow.
- Partial-only confirmation never implies that reconnect restore is ready.
- Restore feedback names what is missing instead of silently doing nothing.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
