# CON-015: Stop Music never sends the protocol microphone-disable command

- Severity: P2
- Evidence status: hypothesis needing device validation
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Start a controller microphone effect, then tap Stop Music.

## Actual and expected behavior

**Actual:** The model requests solid playback. The solid plan sends power/color/brightness but no Microphone(false); music previously sent Microphone(true). Whether a mode write itself disables controller reactivity is not established for this hardware.

**Expected:** Stop Music should visibly stop microphone-driven effects and leave the accessory in a predictable static state.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1889](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1889) — stopMusic changes playback to solid and applies the state.
- [crates/cutout-protocols/src/melk_lighting.rs:654](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-protocols/src/melk_lighting.rs:654) — Music plan explicitly enables microphone; solid plan has no disabling command.
- [crates/cutout-protocols/src/melk_lighting.rs:616](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-protocols/src/melk_lighting.rs:616) — The encoder can represent microphone false.
- [swift/CutoutMobile/Tests/CutoutAppTests/CutoutAppRouteTests.swift:1213](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Tests/CutoutAppTests/CutoutAppRouteTests.swift:1213) — The test asserts the state request, not physical microphone behavior.

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Capture Start/Stop behavior on the exact OC21; if mode selection does not disable microphone reactivity, use the existing disable command in the transition.

## Acceptance checks

- Enable music, clap/play nearby sound, then stop; the accessory remains static under subsequent sound.
- Capture exact Start/Stop BLE writes and record firmware/profile identity.

## Limitations

No claim that Stop currently fails on the phone or controller. The source has an explicit state transition whose hardware semantics need verification.
