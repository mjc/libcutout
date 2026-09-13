# CON-010: Browsing effect groups highlights a pattern that was never sent

- Severity: P2
- Evidence status: source-confirmed defect
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Run an effect, then choose a different pattern group without tapping a pattern.

## Actual and expected behavior

**Actual:** The group onChange replaces the local selected pattern with the first ID in that group but never sends it. The grid and picker highlight that ID; the speed slider then refuses to submit because the active pattern differs from the highlighted one.

**Expected:** Browsing groups must not change the displayed active selection or silently disable speed control.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:353](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:353) — A group change changes pattern to the first group ID only.
- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:261](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:261) — Effect grids use local pattern for selected highlighting.
- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:297](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:297) — Speed submission requires the active pattern to equal local pattern.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Keep browsed group separate from active pattern; display the real active pattern and bind speed to it.

## Acceptance checks

- Switch groups without choosing a card; active highlighting does not claim another mode.
- Speed changes either apply to the active effect or are explicitly unavailable with a reason.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
