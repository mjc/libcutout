# CON-002: Wheel connection and GATT discovery can remain Connecting indefinitely

- Severity: P2
- Evidence status: source-confirmed defect
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Wheel connection/navigation

## Trigger

Select a wheel that stops advertising before connection, or a peripheral that connects but produces an empty service inventory.

## Actual and expected behavior

**Actual:** The app starts the connection without a deadline. The only general detection timer is armed after discovery has advanced to protocol detection. An empty services array executes no characteristic callbacks, so detection never begins and no error is published.

**Expected:** Each connection/discovery attempt reaches readiness, a recoverable failure, or a bounded retry; a user can cancel an in-progress attempt.

## Evidence

- [swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:1567](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:1567) — Connection setup contains no timer or attempt deadline.
- [swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2622](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2622) — didDiscoverServices loops services; an empty list has no terminal branch.
- [swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:3156](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:3156) — The two-second protocol detection expiry is a later phase and does not bound connect/GATT discovery.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Add one attempt deadline spanning connection and discovery and explicitly fail an empty inventory. Expose cancellation from the picker.

## Acceptance checks

- A never-connecting peripheral exits Connecting within the chosen deadline.
- An empty successful services response fails deterministically.
- A late callback after deadline cannot revive the attempt.

## Limitations

The missing deadline and empty-list stall are source-confirmed. Actual CoreBluetooth callback timing requires device validation; this is not a reproduced phone crash.
