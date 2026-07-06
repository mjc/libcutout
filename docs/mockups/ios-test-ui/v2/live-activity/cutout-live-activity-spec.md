# CutOut Live Activity mockup

Purpose: project input for shaping an iOS Live Activity, Lock Screen, and Dynamic Island ride surface. This is a design direction and data-contract planning artifact, not final production UI.

Asset:
- `cutout-live-activity-card.jpg`
- Size: 1280x960
- SHA-256: `8a46a7e7d40d2100d3d543eee438022885222626b3b03b3d4984f5be23f65f52`

## Surfaces

1. Dynamic Island compact
   - Brand mark
   - Current speed
   - Battery percent
   - Status indicator

2. Dynamic Island expanded
   - Brand/session header
   - Connected device label
   - Speed hero
   - Battery, voltage, and PWM metrics
   - Ride mode, duration, and distance
   - Footer status chips for headroom, beeps, and temperature

3. Lock Screen Live Activity
   - Same glance hierarchy as the expanded Dynamic Island card
   - Large enough for speed and safety context without requiring app launch

## Visible fields

- Session/device identity: `Lynx-S connected` in the mockup, with a green connected indicator.
- Speed: `27 mph` as the hero riding value.
- Battery: `68%`.
- Pack voltage: `118.4 V`.
- PWM: `54%`.
- Mode: `Sport`.
- Duration: `18:42`.
- Distance: `7.8 mi`.
- Headroom status: `Headroom good`.
- Beep status: `Beeps armed`.
- Temperature: `34 C`.

## Product constraints

- This surface should consume typed ride/session state already shaped by the app, mobile FFI, and Rust domain contracts. The widget extension should not parse raw protocol bytes, raw CoreBluetooth state, or model-specific telemetry.
- Missing or stale data must render as unavailable, stale, or disconnected rather than as zeros.
- Compact Dynamic Island content should prioritize speed plus the single most important safety/energy indicator; expanded and Lock Screen content may show the wider metric set.
- Activity updates must be rate-limited and driven by explicit app/session state so the Lock Screen does not become a noisy dashboard.
- Safety-sensitive fields such as PWM, headroom, beeps, and temperature need the same typed applicability/provenance rules as the in-app ride screen.

## Tracker

- Parent Live Activity epic: `LIBCU-481`.
