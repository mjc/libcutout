# CON-007: Saved lighting timers reappear as disabled defaults

- Severity: P2
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Save an enabled lighting timer, switch to Color, then return to Schedule.

## Actual and expected behavior

**Actual:** The Schedule view is conditionally destroyed and recreates local @State with 18:00/23:00, all days, and disabled toggles. No timer values are stored in the model or read back. The enabled controls look like current settings even though they are fresh drafts.

**Expected:** Retain the last submitted drafts and label their source; show unknown controller state explicitly when readback is unavailable.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:45](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:45) — LightingScheduleControls exists only while page == .schedule.
- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:389](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:389) — Times, day masks, and enabled flags are local defaults on each new view.
- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1926](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1926) — setSchedule sends clock/slot commands and records only command status, not schedule values.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Keep per-accessory timer drafts in the existing model/persistence record and distinguish last requested values from controller readback.

## Acceptance checks

- Save both timers, change page, reopen Lighting, and relaunch; the last requested values remain visible with their evidence state.
- Opening Schedule issues no writes.
- Never present an unknown timer as definitively disabled.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
