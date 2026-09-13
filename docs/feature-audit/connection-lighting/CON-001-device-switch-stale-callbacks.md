# CON-001: Switching devices can let the previous connection mutate the new session

- Severity: P1
- Evidence status: source-confirmed defect
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Wheel connection/navigation

## Trigger

Tap device A, then tap a different device B before A finishes connecting.

## Actual and expected behavior

**Actual:** The second selection replaces the active peripheral without cancelling A. A later didConnect or GATT discovery callback from A can change the phase, overwrite the shared service-discovery set, bind characteristics, or start detection against A while the selection identifies B.

**Expected:** Only the active connection attempt may publish identity, readiness, GATT inventory, or failures.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:2499](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:2499) — pair permits a different selection during connecting/retrying/connected.
- [swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:1567](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:1567) — connectForProtocolDetection replaces self.peripheral and calls connect without cancelling the previous peripheral or clearing its delegate.
- [swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2586](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2586) — didConnect does not check the callback peripheral against self.peripheral.
- [swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2622](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2622) — service and characteristic discovery callbacks mutate shared state without an active-peripheral guard.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Cancel the old attempt when replacing it and reject callbacks outside the current peripheral/attempt. Reuse the existing identity guard used by handleDisconnect and write-ready handling.

## Acceptance checks

- Select A then B; deliver late success, service discovery, characteristic discovery, and error callbacks for A; B remains the sole active identity.
- Verify A is cancelled and cannot start telemetry or writes under B.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
