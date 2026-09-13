# CON-005: Lighting can become stuck discovering after the BLE connection succeeds

- Severity: P2
- Evidence status: source-confirmed defect
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Pair a lighting controller whose service discovery, characteristic discovery, or notification subscription never completes.

## Actual and expected behavior

**Actual:** The connection timer is cleared on Connected and no discovery/subscription deadline replaces it. The first timer after that requires a successful subscription and initialization write. Controls stay unavailable with Connecting/Discovering indefinitely.

**Expected:** A stalled discovery phase should time out and offer a retry without restarting the app.

## Evidence

- [crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:486](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:486) — Connected clears self.timer and transitions to Discovering.
- [crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:528](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:528) — Characteristic discovery advances to Subscribe but arms no timeout.
- [crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:593](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:593) — Only a notification-state response advances initialization.
- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:2077](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:2077) — canReconnect is false in discovering; start is already guarded by isRunning.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Keep a bounded initialization deadline until Ready, cancelling it on a terminal transition.

## Acceptance checks

- Omit each discovery/subscription callback in turn; the reducer reaches retry or failure.
- A successful initialization cancels the deadline.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
