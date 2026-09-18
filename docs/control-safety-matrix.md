# Control Safety Matrix

User-visible confirmation, reversibility, disconnect, and accessibility policy
for settings controls is defined in
[`settings-safety-ux.md`](settings-safety-ux.md).

Cutout command support is split by safety class before a protocol encoder can
produce transport writes. Read-only sessions must fail closed for every
non-read command.

The required gates below are not a claim of complete end-to-end enforcement.
The [settings design review](settings-design-review.md) records the 2026-09-18
gaps in semantic bindings, native queues, discovery and the live harness.
“Benign” is an API classification, not a promise that an audible control is
harmless to exercise or that a legal setting value is safe to probe.

| Command class | Current commands | Required type bound | Required runtime gate | Live write status |
| --- | --- | --- | --- | --- |
| Read-only probes | `RequestIdentity`, `RequestTelemetry`, `RequestFirmwareInfo`, `RequestBatteryInfo`, `RequestDiagnostics`, `RequestSettings` | `SupportsReadRequests` / `ReadOnlyModelSpec` | Command kind must be in model `READ_CAPABILITIES`; unsupported read probes emit diagnostics without writes. Discovery must establish probe eligibility for all still-plausible protocols. | Session read gates exist; cross-protocol discovery safety and queued probe retirement remain open |
| Stationary-only settings writes | `InvokeAction` for reset/calibration and `SetSetting` with a profile-defined `SettingId`/value | `SupportsSettingsWrites` plus `StationarySettingsWriteSession` | Live arming requires a speed sample at most two seconds old and within the model limit (500 mm/s for Aero). The arm must be model-bound and expire. Motion, charging, failed rearm, stale speed, and disconnect must cancel pending work; rearming must not extend an older sequence's deadline. | Source-backed encoders and session guards exist. Native queued-effect identity, host receipts and delayed-send guard enforcement remain incomplete. A descriptor or accepted plan is neither host submission nor wheel confirmation. |
| Benign controls | `SetLights`, `SetTaillight`, `SoundHorn` | `SupportsBenignControls` plus a benign-control session shell | Model allow-list, exact captured command bytes, command capability, explicit unsupported diagnostics | Generic lights are emitted only by profiles that advertise them. Aero's Tune surface uses one canonical Headlight setting; unsupported lighting commands remain omitted. |
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

Live CLI `connect` and `capture` paths also require the selected Aero/Falcon
profile's registered GATT fingerprint before constructing a protocol session;
an unrelated BLE peripheral is rejected without a session probe or write.

That fingerprint does not distinguish Veteran from Begode on their shared
FFE0/FFE1 channel. The Mac discovery trace included Begode probes on NF2557,
including queued bytes flushed after identity resolution. Read-only session
classification alone does not establish safety of the discovery path. Before
delayed execution, Rust must revalidate the connection and applicable guard;
the native queue must retain operation identity and report overflow/failure
instead of silently dropping an old write or sequence fragment.

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

The historical NF2557 CLI capture used an explicit peripheral identifier, the
Aero model profile selecting the Veteran/NOSFET dialect, the discovered FFE0/FFE1
GATT fingerprint, and reported model id 43 before arming a write. Its tilt-back
readback changed 54 to 53 and then returned to 54; an alarm write did not change
the reported 55 value and was unconfirmed. This is evidence for those captured
interactions, not current acceptance of the Mac harness, all controls or all
firmware. Fresh matching telemetry establishes observed state, not causal
acknowledgment without an echoed operation ID. Use the
[coverage record](aero-settings-coverage.md#physical-evidence-and-open-acceptance)
for subsequent user reports and unresolved controls.
