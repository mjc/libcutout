# Control Safety Matrix

Cutout command support is split by safety class before a protocol encoder can
produce transport writes. Read-only sessions must fail closed for every
non-read command.

| Command class | Current commands | Required type bound | Required runtime gate | Live write status |
| --- | --- | --- | --- | --- |
| Read-only probes | `RequestIdentity`, `RequestTelemetry`, `RequestFirmwareInfo`, `RequestBatteryInfo`, `RequestDiagnostics`, `RequestSettings` | `SupportsReadRequests` / `ReadOnlyModelSpec` | Command kind must be in model `READ_CAPABILITIES`; unsupported read probes emit diagnostics without writes | Enabled for capture-backed read-only paths |
| Stationary-only settings writes | No `DeviceCommand` variant yet | `SupportsSettingsWrites` plus a settings-write session shell | Model allow-list, stationary-state evidence where available, explicit command capability, no probing overlap | Not enabled |
| Benign controls | `SetLights`, `SoundHorn` | `SupportsBenignControls` plus a benign-control session shell | Model allow-list, exact captured command bytes, command capability, explicit unsupported diagnostics | Not enabled |
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
non-default feature and an expiring runtime arming token before any transport
write can be produced.
