# CON-008: Lighting Music has a Stop button but no clear way to start its displayed default

- Severity: P2
- Evidence status: UX inconsistency
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

From solid-color mode open Music and adjust microphone sensitivity expecting the displayed effect to activate.

## Actual and expected behavior

**Actual:** The page initializes a default effect, but opening it does not apply that mode. Sensitivity changes submit only when requestedPlayback is already music; the only unconditional start path is the effect-picker setter. Stop is offered even in solid mode.

**Expected:** Provide a clear Start/Stop or enabled control for the displayed microphone effect; editing inactive parameters must communicate that they are drafts.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:233](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:233) — musicEffect defaults to zero without activating playback.
- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:315](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:315) — Only the effect picker setter unconditionally calls applyMusic.
- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:338](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:338) — Sensitivity commits are discarded unless playback already is music.
- [swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:345](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/LightingPlaybackControls.swift:345) — Stop Music is rendered unconditionally on the Music page.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Add one explicit enable/start action for the selected effect and reflect whether microphone mode is requested.

## Acceptance checks

- Activate effect zero directly from solid without first selecting a different effect.
- Changing sensitivity while inactive is either clearly a draft or starts mode intentionally.
- Stop is disabled or absent when inactive.

## Limitations

Source confirms activation asymmetry. Whether reselecting the already-selected native Picker entry invokes its setter needs a UI check; do not depend on that behavior.
