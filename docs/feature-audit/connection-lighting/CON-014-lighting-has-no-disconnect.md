# CON-014: A remembered lighting accessory cannot be temporarily disconnected

- Severity: P2
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

After pairing Lighting, try to release the accessory so another controller app can connect while keeping saved scenes.

## Actual and expected behavior

**Actual:** Leaving Lighting intentionally keeps the remembered connection alive. Details has Connect and Forget, but no Disconnect. Forget removes all accessory restore/preset state, so the available in-app way to release the link is destructive.

**Expected:** Offer a temporary Disconnect that preserves pairing and scenes, with a clear reconnect action.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1722](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1722) — stopIfUnpaired stops only sessions with no remembered identifier.
- [swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:692](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:692) — Details offers Connect, disabled when ready.
- [swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:749](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:749) — The only connection-removal action is Forget.
- [swift/CutoutMobile/Sources/CutoutMobile/LightingAccessoryPersistence.swift:212](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Sources/CutoutMobile/LightingAccessoryPersistence.swift:212) — forget removes the complete accessory record.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Expose the existing session stop path as Disconnect and define when automatic reconnection resumes.

## Acceptance checks

- Disconnect releases BLE while alias, scene presets, and restore preference remain saved.
- The app does not immediately reconnect until the user requests it or the documented policy applies.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
