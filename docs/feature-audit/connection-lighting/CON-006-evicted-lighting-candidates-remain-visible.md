# CON-006: Some nearby accessory rows remain selectable after the core has forgotten them

- Severity: P2
- Evidence status: source-confirmed defect
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Leave first-pairing scan running in a place with more than 32 named BLE devices, then select an early row.

## Actual and expected behavior

**Actual:** The Rust candidate map evicts an older non-MELK candidate, but emits no removal to Swift. Swift appends each candidate indefinitely; selecting the stale row silently does nothing because Rust no longer has it.

**Expected:** The visible list and selectable candidate set must agree, and expired rows should disappear or explain their state.

## Evidence

- [crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:414](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:414) — The bounded map removes an entry at capacity.
- [crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:180](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-mobile-ffi/src/rgb/melk/session/reducer.rs:180) — select silently returns if candidates.get cannot find the identifier.
- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1774](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1774) — onCandidate removes only duplicate IDs, appends the new row, and never processes eviction.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Publish the bounded candidate snapshot or explicit removals so the UI shares the reducer list.

## Acceptance checks

- Discover 33 or more candidates and assert that all visible rows are selectable or explicitly unavailable.
- The Swift list stays bounded as advertisements continue.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
