# CON-011: Preset summaries use the inverse native speed instead of the speed shown in controls

- Severity: P3
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Set effect speed to 100%, save a scene, then read its summary.

## Actual and expected behavior

**Actual:** The slider displays 100% for native byte zero, while the preset summary prints speed 0. At the other end a slow 0% setting prints speed 255.

**Expected:** The same speed should have the same direction, unit, and scale everywhere it is shown.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:285](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:285) — Speed percentage is (255 - speed) * 100 / 255.
- [swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:454](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:454) — Effect preset summary interpolates the raw speed byte.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Reuse the percentage conversion for scene summaries and label it with percent.

## Acceptance checks

- Native 0 renders 100% in both slider and preset; native 255 renders 0%.
- Midrange speed rounds consistently.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
