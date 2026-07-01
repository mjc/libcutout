# Cutout iOS Test UI Mockups v2

These are disposable implementation mockups for wiring CutoutApp test screens,
not final visual design.

## Source

- Original archive: `source/cutout-screen-mockups-v2.zip`
- Extracted assets and source spec: `screens/`
- Spec file: `screens/cutout-screen-mockups-spec-v2.md`
- Combined sheet: `screens/cutout-screen-mockups-v2-sheet.png`

The mockups were supplied as project input and stored here so the app, fixture,
and tracker work can reference stable repo paths.

## Screens

| Screen | PNG size | Tracker epic |
| --- | ---: | --- |
| `cutout-00-device-picker-v2.png` | 780x1688 | `LIBCU-418` |
| `cutout-01-euc-ride.png` | 390x844 | `LIBCU-419` |
| `cutout-02-euc-garage.png` | 390x844 | `LIBCU-420` |
| `cutout-03-vesc-onewheel-ride.png` | 390x844 | `LIBCU-421` |
| `cutout-04-vesc-onewheel-debug.png` | 390x844 | `LIBCU-422` |
| `cutout-screen-mockups-v2-sheet.png` | 2118x956 | `LIBCU-417` |

## Tracker Map

- Parent screen-set epic: `LIBCU-417`
- App shell, session routing, and mockup fixtures: `LIBCU-423`
- Typed discovery candidates for the picker: `LIBCU-412`
- Typed EUC ride safety dashboard contract: `LIBCU-413`
- Typed EUC pack health/settings readback contract: `LIBCU-414`
- Typed VESC Onewheel ride dashboard contract: `LIBCU-415`
- Typed VESC debug/config/logging contract: `LIBCU-416`

The screen epics intentionally link each visible datum back to the Rust,
mobile FFI, and Swift API work needed to make the UI hard to misuse. Mockup-only
or unproven live fields should render as unavailable until their linked data
contract and validation issues are complete.
