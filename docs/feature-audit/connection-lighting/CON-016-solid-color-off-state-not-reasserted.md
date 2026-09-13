# CON-016: Changing solid color while off bypasses the off-preserving state plan

- Severity: P2
- Evidence status: hypothesis needing device validation
- Reviewed revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Scope: Standalone MELK lighting accessory; distinct from built-in Aero lighting

## Trigger

Turn the accessory off while in solid mode, then drag the color wheel or choose a quick color.

## Actual and expected behavior

**Actual:** The direct solid-color path keeps requestedPowerOn false but sends only a color frame. The complete state planner documents that mode writes can turn output on and reasserts Off afterward; the direct path bypasses that protection.

**Expected:** Editing color while the UI says Off should either retain Off or visibly and intentionally turn the accessory On.

## Evidence

- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1812](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1812) — When already solid, setSolidColor calls session.setSolidColor directly without a final power command.
- [swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1839](/Users/mjc/.codex/worktrees/21ec/libcutout/swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1839) — Preview uses the same direct path after switching to solid.
- [crates/cutout-protocols/src/melk_lighting.rs:647](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-protocols/src/melk_lighting.rs:647) — plan_state documents mode-write power behavior and its final Off safeguard.
- [crates/cutout-protocols/src/melk_lighting.rs:683](/Users/mjc/.codex/worktrees/21ec/libcutout/crates/cutout-protocols/src/melk_lighting.rs:683) — An explicitly off complete state appends SetPower(Off).

## Familiar-app comparison

No feature-specific DarknessBot, EUC World, or WheelLog behavior is asserted here. The expectation follows the app's own controls and state contract; consult the audit's central comparison notes for verified comparator references.

## Minimal remedy direction

Validate direct OC21 color writes while off; if they illuminate output, route through the existing off-preserving plan or reflect intentional power-on behavior.

## Acceptance checks

- Off→drag, Off→quick preset, and effect-off→solid maintain agreement between the toggle and physical output.
- Check final transmitted frame and actual light output.

## Limitations

Physical OC21 response was not tested. The bypass is source-confirmed; unexpected illumination remains a hardware hypothesis.
