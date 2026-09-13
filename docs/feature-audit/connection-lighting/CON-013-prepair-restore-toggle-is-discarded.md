# CON-013: Enabling Restore before pairing is silently undone

- Severity: P2
- Evidence status: source-confirmed defect
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Open first-pairing details, turn on Restore last lighting settings, then connect the first accessory.

## Actual and expected behavior

**Actual:** The toggle updates model.restoreEnabled but persistence has no record and drops the write. Once the accessory becomes Ready, ensureRecord creates a record with Restore disabled and the model resets the toggle to that value.

**Expected:** Either retain the user preference until pairing succeeds or disable this preference with a clear reason until an accessory exists.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:736](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:736) — The Restore toggle is always enabled, including before pairing.
- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:2069](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:2069) — setRestoreEnabled updates the local flag and calls persistence.
- [swift/CutoutMobile/Sources/CutoutMobile/LightingAccessoryPersistence.swift:192](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Sources/CutoutMobile/LightingAccessoryPersistence.swift:192) — setRestoreEnabled mutates an optional record and otherwise stores nothing.
- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:2132](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:2132) — A newly created accessory record replaces restoreEnabled with its persisted default.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Disable record-scoped preferences until paired, or explicitly carry the pending preference into record creation.

## Acceptance checks

- Turn on Restore before first pairing; the resulting setting is retained or the pre-pair action is clearly unavailable.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
