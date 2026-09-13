# CON-009: Effect thumbnails show invented colors and geometry

- Severity: P2
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Choose an effect by its colorful preview card.

## Actual and expected behavior

**Actual:** Thumbnail hue is computed as (ID * 37) modulo 360 and shapes are generic functions of group/ID. They are not derived from the named pattern or observed controller output, yet no in-product caption says they are illustrations.

**Expected:** A visual preview should correspond to the mode, or be visibly described as an illustration.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:49](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:49) — Preview points use generic group shapes and ID parity.
- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:102](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:102) — Colors are calculated from arbitrary ID arithmetic.
- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:185](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:185) — The generated thumbnail is placed directly above the pattern name in a selectable effect card.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Use a neutral group icon or clearly mark illustrations until a mode can be previewed faithfully.

## Acceptance checks

- A red-only or directional named mode does not imply unsupported thumbnail colors/direction.
- Any illustrative preview has visible wording explaining its limitation.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
