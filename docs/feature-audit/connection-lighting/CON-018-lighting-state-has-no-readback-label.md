# CON-018: Initial Lighting controls look like observed device state

- Severity: P2
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Connect to an accessory that is already on with a color chosen in another app.

## Actual and expected behavior

**Actual:** The model initializes power Off, red, and 100% brightness (or prior requested values). Readiness enables those controls without reading physical power/color/brightness; the evidence card says Idle, but the controls themselves look like current device settings.

**Expected:** Distinguish last requested values from observed state, particularly on first connection and after another app changes the accessory.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1658](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1658) — Initial requestedState is Off/red/100%.
- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1690](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1690) — Persisted requested/confirmed values initialize controls without readback.
- [swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:196](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:196) — Power toggle binds requestedPowerOn as a normal enabled toggle.
- [swift/CutoutMobile/Sources/CutoutMobile/MelkLightingSession.swift:424](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Sources/CutoutMobile/MelkLightingSession.swift:424) — Notifications are forwarded as raw bytes, not decoded into requested control values.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Label the surface as last requested or unknown until an explicit command/observation supplies state; keep actions direct and understandable.

## Acceptance checks

- Connect to an already-lit accessory and do not display a definitive observed Off claim.
- A previous request does not become telemetry merely because BLE is ready.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
