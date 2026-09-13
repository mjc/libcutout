# CON-017: Lighting alias and vehicle association can fail silently

- Severity: P2
- Evidence status: source-confirmed defect
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Enter an oversized alias or vehicle identifier and tap Save details.

## Actual and expected behavior

**Actual:** Rust rejects invalid text, but both calls use try? and no error is displayed. One field can save while the other silently fails; editable fields retain their draft text, giving no reliable indication of the persisted result.

**Expected:** Validation failures should identify the affected field and leave a clear unsaved state.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:2055](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:2055) — saveAccessoryMetadata ignores both throwing setter errors.
- [crates/cutout-core/src/rgb_lighting.rs:195](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-core/src/rgb_lighting.rs:195) — Alias setter validates bounded text.
- [crates/cutout-core/src/rgb_lighting.rs:214](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-core/src/rgb_lighting.rs:214) — Vehicle identifier setter independently rejects invalid text.
- [swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:835](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:835) — Save details invokes the void method without result/error handling.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Handle existing typed validation errors and show field-specific feedback; avoid indicating a successful combined save after partial failure.

## Acceptance checks

- Oversized text is rejected with visible feedback.
- Mixed valid/invalid fields cannot masquerade as a complete successful save.
- Reopening details matches the last acknowledged values.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
