# CON-003: A failed wheel reconnect discards the current Tune or Pack page

- Severity: P2
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Wheel connection/navigation

## Trigger

Open Tune or a detailed Pack screen, then lose Bluetooth availability or let reconnect attempts exhaust.

## Actual and expected behavior

**Actual:** An exhausted failure or Bluetooth-unavailable transition navigates back to the picker for every route except Map and Lighting. Ordinary retrying transitions preserve the route. Reconnection from the picker opens the main Ride screen, losing the previous inspection context.

**Expected:** A recoverable connection failure should preserve the selected feature with a disconnected state, or restore that feature after reconnecting.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/ContentView.swift:68](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/ContentView.swift:68) — returnToPicker navigation replaces the path unless the route preserves navigation.
- [swift/CutoutMobile/Apps/CutoutApp/CutoutAppRoute.swift:121](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutAppRoute.swift:121) — preservesNavigationOnConnectionLoss includes only Map, map detail, and Lighting.
- [swift/CutoutMobile/Apps/CutoutApp/ContentView.swift:73](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/ContentView.swift:73) — openRide from the picker opens the generic Ride route.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Preserve the current feature during reconnect and disable live-only actions with the shared connection state.

## Acceptance checks

- Exhaust retries or toggle Bluetooth on Tune and BMS cell detail; their context is preserved or restored after recovery.
- Explicit Disconnect still returns Home.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
