# Battery current direction evidence

Shared battery current stores a nonnegative magnitude and a direction: in,
out, zero, or unknown. Protocol decoders retain the signed source value in
their decoded records; they assign a shared direction only when the source
defines its polarity. A raw zero normalizes to `Zero` for every polarity.

## Supported mappings

| Source | Mapping | Evidence |
| --- | --- | --- |
| VESC `COMM_GET_VALUES` input current | Positive is out; negative is in | At VESC firmware revision [`1aa0566`](https://github.com/vedderb/bldc/commit/1aa0566e6b3fc11470a0998b98756b668bad43d8), [`commands.c`](https://github.com/vedderb/bldc/blob/1aa0566e6b3fc11470a0998b98756b668bad43d8/comm/commands.c#L407) serializes `mc_interface_read_reset_avg_input_current()` as the input-current field. In [`mc_interface.c`](https://github.com/vedderb/bldc/blob/1aa0566e6b3fc11470a0998b98756b668bad43d8/motor/mc_interface.c#L2043-L2050), positive `current_in_filtered` accumulates uncharged amp-hours/watt-hours while negative values accumulate charged amp-hours/watt-hours. The same file applies separate `l_in_current_max` draw and `l_in_current_min` regen limits at [lines 2479-2505](https://github.com/vedderb/bldc/blob/1aa0566e6b3fc11470a0998b98756b668bad43d8/motor/mc_interface.c#L2479-L2505). |
| Refloat `battery_current` realtime field | Positive is out; negative is in | At Refloat revision [`663bbf7`](https://github.com/lukash/refloat/commit/663bbf77086b2e2aa207c2ac7314f895fe1b770d), the upstream [`REALTIME_DATA` reference](https://github.com/lukash/refloat/blob/663bbf77086b2e2aa207c2ac7314f895fe1b770d/doc/commands/REALTIME_DATA.md) names the field `battery_current`. [`motor_data.c`](https://github.com/lukash/refloat/blob/663bbf77086b2e2aa207c2ac7314f895fe1b770d/src/motor_data.c#L158-L169) copies VESC's `mc_get_tot_current_in_filtered()` and selects the regen limit when the value is negative. [`main.c`](https://github.com/lukash/refloat/blob/663bbf77086b2e2aa207c2ac7314f895fe1b770d/src/main.c#L2128-L2133) emits the same stored value as realtime battery current. |

These source references establish firmware conventions. They do not claim
that a particular wheel, controller installation, or physical current sensor
was tested on hardware.

## Unknown mappings

Begode/Gotway telemetry exposes signed values described as battery current,
including the Live Extra field and BMS summary. The repository's
[`Begode/Falcon protocol evidence`](begode-falcon-protocol-evidence.md)
documents the field, scale, and pack-voltage relationship, but does not define
which sign means charge or discharge. These values therefore retain their raw
signed form in protocol records and normalize to direction `Unknown`.

Veteran/LeaperKim/NOSFET realtime telemetry and Veteran/NOSFET BMS pages also
carry signed current values, but the available source captures and protocol
descriptions do not establish their polarity. They retain the raw value and
normalize to `Unknown`. No inference is made from whether a sample is positive
or negative, from the reported charge mode, or from a vehicle's expected
operating state.

## CLI presentation

Human-facing output reports the magnitude and an explicit direction. Machine
readable output preserves magnitude and direction as separate fields. Unknown
direction remains visible as unknown; it is not shown as charge or discharge.
Motor phase current remains a separate measurement and is never substituted
for battery current.

## Shared domain and consumers

`ReportedBatteryCurrent` is the signed protocol measurement converted to
milliamps. `normalize` maps that raw value once at the adapter boundary using
`BatteryCurrentPolarity`. It uses `unsigned_abs`, so the minimum signed value
retains its complete magnitude.

`BatteryCurrent` is an exhaustive enum: `Zero`, `In`, `Out`, or `Unknown`.
Nonzero variants carry `CurrentMagnitude`, a nonzero unsigned milliamp quantity.
Directed zero inputs normalize to `Zero`. Missing measurements remain absent;
unknown direction does not erase a measured magnitude. Measurement provenance
stays on `Measured<BatteryCurrent>`; knowledge of the magnitude is separate
from knowledge of its direction. Phase current has a separate type.

Core classifies contextual power flow from current and independently resolved
operating state. Out means discharge even while parked. In means regeneration
while riding, charging in an admitted charging state, and otherwise unknown.
Unknown direction remains unknown. Fresh connected-wheel movement at the
shared threshold, including reverse movement, overrides charging evidence.
FFI maps these results; Swift aliases the generated current enum and only
formats its magnitude and direction. It has no parallel current state machine
or normalization policy.

Pack power uses positive for power leaving the battery and negative for power
entering it. An unknown current direction cannot produce signed calculated
power. Independently reported power remains a separate source. Charge-rate
statistics contain unsigned magnitudes, not directed currents. Qualified
charging evidence may admit a known magnitude with unknown direction for a
charging estimate; outward current contradicts charging. The voltage-sag model
requires outward discharge, with zero current permitted as an unloaded baseline.
VESC consumed and regenerated charge/energy counters remain separate reported
counters; this migration does not derive those counters from unsigned magnitude.

## Observation times and recording

`battery_current_observed_at_ms` and `voltage_observed_at_ms` record when a
delta last supplied each electrical measurement. Unrelated packets do not renew
them. Sag requires both readings to be fresh and uses their observation clocks;
missing or stale readings clear the transient pair while retaining the learned
resistance model. Contextual flow is unknown for missing,
future, or expired observations, using the existing five-second freshness
window. Estimators also compare observation time against the actual request
clock. The timestamp is rebased alongside other clocks when captured.

New mobile semantic snapshots use schema version 2. Current values serialize
as `"zero"`, `{"in":{"milliamps":N}}`, `{"out":{"milliamps":N}}`, or
`{"unknown":{"milliamps":N}}`, inside the existing provenance wrapper.
Version 1 signed snapshots remain stored and exported verbatim. Storage and
PEVCAP import do not reinterpret a historical sign as a newly established
physical direction. Raw notification bytes and raw signed telemetry remain
unchanged; replay through the current decoder applies its documented mappings.

The material-recording comparator excludes the new observation clock just as
it excludes other root observation clocks. Clock-only changes do not cause a
write. Magnitude, direction, measurement evidence, and snapshot schema changes
remain material. Route distance and moving-time records do not carry battery
current or integrate it; this change does not rewrite route history or stored
ride distances.

## Regression coverage

- Core current tests normalize opposite wire conventions to identical physical
  directions, preserve the signed minimum without overflow, keep measured zero
  separate from unknown, reject negative magnitudes at compile time, and round
  trip direction and measurement evidence through FFI.
- Decoder tests exercise signed 16-bit extrema for both realtime and BMS
  Veteran/Begode fields. VESC projection tests preserve raw input while mapping
  both directions. Refloat tests reject nonfinite/out-of-range measurements
  without turning them into zero.
- Core and CLI tests cover motion overriding charge reports in either direction
  at the shared threshold, outward-current contradictions, unknown-direction
  charging admission, and clearing old signed power after an unknown reading.
- Sag tests cover stale voltage with fresh current, both orders of asynchronous
  field arrival, duplicate-pair suppression, stable cached display, clock
  regression, metadata downgrades, and recovery without learning across a gap.
- PEVCAP and SQLite tests retain both old signed snapshots and new directional
  snapshots without relabeling. Material-comparison tests suppress clock-only
  changes but retain direction changes at identical magnitude.
- Swift boundary tests cover unsigned magnitudes through `UInt32.max`, measured
  zero, explicit labels for each direction, and unknown current without
  manufactured signed power. Existing dashboard and Live Activity tests consume
  the same typed current values.
