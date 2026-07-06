# Cutout iOS Test UI Mockups v2

These are disposable implementation mockups for wiring CutoutApp test screens,
not final visual design.

## Source

- Original archive: `source/cutout-screen-mockups-v2.zip`
- BMS archive: `source/cutout-bms-mockups.zip`
- Extracted assets and source spec: `screens/`
- Spec file: `screens/cutout-screen-mockups-spec-v2.md`
- Combined sheet: `screens/cutout-screen-mockups-v2-sheet.png`
- BMS assets and source spec: `bms/`
- BMS spec file: `bms/cutout-bms-mockups-spec.md`
- BMS combined sheet: `bms/cutout-bms-mockups-sheet.png`
- Live Activity assets and source spec: `live-activity/`
- Live Activity spec file: `live-activity/cutout-live-activity-spec.md`

The mockups were supplied as project input and stored here so the app, fixture,
and tracker work can reference stable repo paths.

## Screens

| Screen | Asset size | Tracker epic |
| --- | ---: | --- |
| `cutout-00-device-picker-v2.png` | 780x1688 | `LIBCU-418` |
| `cutout-01-euc-ride.png` | 390x844 | `LIBCU-419` |
| `cutout-02-euc-garage.png` | 390x844 | `LIBCU-420` |
| `cutout-03-vesc-onewheel-ride.png` | 390x844 | `LIBCU-421` |
| `cutout-04-vesc-onewheel-debug.png` | 390x844 | `LIBCU-422` |
| `cutout-screen-mockups-v2-sheet.png` | 2118x956 | `LIBCU-417` |

## BMS Screens

| Screen | Asset size | Tracker epic |
| --- | ---: | --- |
| `cutout-05-bms-overview.png` | 390x844 | `LIBCU-440` |
| `cutout-06-bms-cell-map-6s.png` | 390x844 | `LIBCU-441` |
| `cutout-07-bms-cell-map-40s.png` | 390x844 | `LIBCU-442` |
| `cutout-08-bms-cell-detail-popover.png` | 390x844 | `LIBCU-443` |
| `cutout-09-bms-unknown-topology.png` | 390x844 | `LIBCU-444` |
| `cutout-10-dumb-bms-no-data.svg` | 390x844 | `LIBCU-446` |
| `cutout-bms-mockups-sheet.png` | 1468x663 | `LIBCU-438` |

## Live Activity

| Screen | Asset size | Tracker epic |
| --- | ---: | --- |
| `cutout-live-activity-card.jpg` | 1280x960 | `LIBCU-481` |

## Tracker Map

- Parent screen-set epic: `LIBCU-417`
- App shell, session routing, and mockup fixtures: `LIBCU-423`
- Typed discovery candidates for the picker: `LIBCU-412`
- Typed EUC ride safety dashboard contract: `LIBCU-413`
- Typed EUC pack health/settings readback contract: `LIBCU-414`
- Typed VESC Onewheel ride dashboard contract: `LIBCU-415`
- Typed VESC debug/config/logging contract: `LIBCU-416`
- Parent BMS screen-set epic: `LIBCU-438`
- Typed BMS topology and per-group readback contract: `LIBCU-439`
- Controller-only / non-smart BMS no-data screen: `LIBCU-446`
- Parent Live Activity epic: `LIBCU-481`

The screen epics intentionally link each visible datum back to the Rust,
mobile FFI, and Swift API work needed to make the UI hard to misuse. Mockup-only
or unproven live fields should render as unavailable until their linked data
contract and validation issues are complete.
