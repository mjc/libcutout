# CON-004: Home Lighting changes navigation ownership with wheel selection

- Severity: P2
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Wheel connection/navigation

## Trigger

Open Lighting from Home before automatic wheel selection starts, then let the saved-wheel connection begin.

## Actual and expected behavior

**Actual:** Lighting uses a standalone screen only when selectedConnectionRoute is nil. The same destination becomes a TabView in the wheel shell when a wheel selection becomes available (including during connection, not only once live); surrounding navigation therefore depends on an unrelated connection.

**Expected:** A Home Lighting entry should keep its standalone navigation and accessory connection context for its lifetime.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:101](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:101) — Home opens the same .lighting(.euc) route used by ride navigation.
- [swift/CutoutMobile/Apps/CutoutApp/ContentView.swift:135](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/ContentView.swift:135) — Standalone rendering is conditional on selectedConnectionRoute == nil.
- [swift/CutoutMobile/Apps/CutoutApp/ContentView.swift:155](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/ContentView.swift:155) — Otherwise the destination is wrapped in the connected TabView.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Represent Home Lighting entry ownership explicitly and render it consistently while ride connectivity changes.

## Acceptance checks

- Open Home Lighting before auto-pair completes; connecting/disconnecting a wheel does not change Lighting navigation chrome.
- Ride-origin Lighting remains reachable with an intentional return destination.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
