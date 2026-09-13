# CON-019: Lighting Music does not explain which microphone it uses

- Severity: P3
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Open Lighting Music while phone music is playing and try to make the lights react.

## Actual and expected behavior

**Actual:** The screen says Music effect, Sensitivity and Microphone sensitivity, without a visible explanation that sensing occurs at the lighting controller. Elsewhere the app offers actual phone music-provider controls, making the two music features easy to conflate.

**Expected:** Explain locally that the accessory listens to nearby sound and that phone provider playback is a separate feature.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:304](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:304) — Music control section contains effect, mic sensitivity, and Stop with no source explanation.
- [crates/cutout-protocols/src/melk_lighting.rs:671](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-protocols/src/melk_lighting.rs:671) — Playback emits accessory sensitivity/music-effect/microphone frames.
- [swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:175](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingRouteView.swift:175) — Music is a page of standalone accessory controls.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Add one short helper sentence naming the controller microphone and keep it separate from provider controls.

## Acceptance checks

- A first-time user can identify where sound must be audible and whether the phone microphone is involved.
- No phone audio capture or provider-audio analysis is introduced.

## Limitations

Static source trace only; no physical iPhone or MELK controller was exercised in this audit.
