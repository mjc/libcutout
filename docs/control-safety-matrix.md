# Control Safety Matrix

Cutout command support is split by safety class before a protocol encoder can
produce transport writes. Read-only sessions must fail closed for every
non-read command.

| Command class | Current commands | Required type bound | Required runtime gate | Live write status |
| --- | --- | --- | --- | --- |
| Read-only probes | `RequestIdentity`, `RequestTelemetry`, `RequestFirmwareInfo`, `RequestBatteryInfo`, `RequestDiagnostics`, `RequestSettings` | `SupportsReadRequests` / `ReadOnlyModelSpec` | Command kind must be in model `READ_CAPABILITIES`; unsupported read probes emit diagnostics without writes | Enabled for capture-backed read-only paths |
| Stationary-only settings writes | `ResetTripMeter`, `SetAeroTiltbackSpeed`, `SetAeroPwmPercent`, `SetAeroAlarmSpeed`, `SetAeroAngleAdjustment`, `SetAeroHighBeam`, `SetPedalMode`, `SetRollAngle`, `SetSpeedAlarmMode`, `SetBegodeMaxSpeed`, `SetBegodeBeeperVolume`, `SetBegodeLedMode`, `SetAccelerationAssist` | `SupportsSettingsWrites` plus `StationarySettingsWriteSession` | A `StationarySettingsPolicy` may arm from `Parked` or `Standing` (or a model-specific bounded low-speed window); the session requires matching model/capability, seeds its monotonic clock from the arm, rejects expiry, and clears the arm on link-down | NOSFET Aero is registered for the explicit Rust `aero-write` path. Typed speed writes must match a reported value before success; other settings remain write-only/unverified until a typed readback exists |
| Benign controls | `SetLights`, `SetTaillight`, `SoundHorn` | `SupportsBenignControls` plus a benign-control session shell | Model allow-list, exact captured command bytes, command capability, explicit unsupported diagnostics | Aero headlight/high-beam writes are exposed through the guarded Rust/mobile wrappers; effect proof remains separate from encoder proof |
| Dangerous actuation | `SetRawMotorCurrent` | `SupportsDangerousActuation` plus dangerous-control feature and session shell | Non-default build feature, explicit runtime arming token, short expiry, current limits, stationary/no-probing validation where applicable | Not enabled |
| Firmware operations | No `DeviceCommand` variant yet | Separate firmware operation marker, not a control trait | Explicit firmware mode, image provenance, rollback/failsafe plan, hardware-specific acceptance | Not enabled |

## Current Enforcement

`CommandKind::safety_class` is the source of truth for command classification.
Protocol sessions add type-level gates through zero-sized operation markers:
`ReadOnlyOperation`, `SettingsWriteOperation`, `BenignControlOperation`, and
`DangerousActuationOperation`.

Current `ReadOnlySession` implementations only accept commands classified as
`ReadOnly` and present in model read capabilities. Benign controls, settings
writes, dangerous actuation, and future firmware writes must be implemented in
separate session shells so read-only probing cannot accidentally gain write
behavior.

## Before Enabling Live Controls

Each model-specific control issue must provide capture-backed request bytes,
capability tests, refusal tests for unsupported states, and hardware evidence
for the exact device/firmware variant. Dangerous actuation also needs a
non-default `dangerous-controls` feature and the core
`DangerousActuationPolicy` / `DangerousActuationArm` gate before any transport
write can be produced. The policy checks model binding, token expiry, command
safety class, and absolute raw-current limit. The feature-gated
`DangerousControlSession` emits typed refusal events and still has no
model-specific encoder, so even authorized commands cannot reach a transport
until a capture-backed control issue adds that path.

The current NF2557 proof path uses an explicit peripheral identifier, the Aero
protocol profile, the discovered FFE0/FFE1 GATT fingerprint, and reported model
id 43 before arming a write. A live tiltback write was confirmed by the typed
readback changing 54 to 53 and then restored to 54. A live alarm write emitted
one protocol write but did not change the reported 55 value, so the CLI rejects
it as unconfirmed instead of treating transport success as setting success.
