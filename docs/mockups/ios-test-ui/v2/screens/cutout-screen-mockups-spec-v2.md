# CutOut example screen set

Purpose: disposable implementation mockups for wiring real telemetry, not final visual design.

Screens:
1. Device picker
   - Add EUC
   - Add VESC Onewheel
   - Nearby device list
   - Privacy/local logging notice

2. EUC ride screen
   Primary model: rider safety margin for a self-balancing wheel.
   Hero: speed.
   Primary safety fields: PWM headroom, sag-adjusted energy, voltage sag warning, pack voltage, power, thermal state, limp-home range.

3. EUC garage/pack screen
   Primary model: stationary wheel health and vendor/device configuration.
   Fields: battery percent, pack voltage, beep margin, tiltback, pedal mode/hardness, cell group high/low/delta, last fault.

4. VESC Onewheel ride screen
   Primary model: board/nose authority and VESC duty/current limits.
   Hero: board speed.
   Primary safety fields: duty headroom, pushback proximity, battery current, motor/phase current, board angle, controller/motor temp.

5. VESC Onewheel debug screen
   Primary model: VESC state/config/logging, not glanceable riding UI.
   Fields: profile, firmware/bridge, duty cycle, pack voltage, battery current limit, motor current limit, last fault, app/input channel, CAN status, logging state, write guardrails.

Intentional differentiation:
- No generic circular speedometer.
- Battery percentage is secondary to sag-adjusted energy and usable margin.
- Riding screens are sparse and glance-oriented.
- Garage/debug screens are dense and explicit.
- EUC and VESC Onewheel do not share the same safety hierarchy yet.


## Device picker v2

The launch picker is a scan/discovery list, not a manufacturer chooser. It groups nearby devices into:

- Supported now: selectable devices with enough identification to choose a known EUC or VESC Onewheel connection path.
- Looks like a PEV, unsupported for launch: disabled rows for scooters, hoverboards, Segways, ebikes, or unknown self-balancing devices that advertise like rideable hardware.
- Manual add / record unknown device: intentionally disabled placeholder for the later capture flow that records the data needed to add support.

Do not put manufacturer lists on this screen. Use vendor/model details only when discovered or manually entered.
