# Settings safety UX policy

This policy applies to the Tune/settings surface and keeps a submitted command
distinct from a wheel-reported state. It is deliberately shared by the CLI,
simulator, and Swift UI; Swift renders the Rust-owned state and does not invent
protocol behavior.

## State and confirmation

Every setting uses these states when the protocol exposes them:

- **Current**: the last value reported by the wheel (with source and age).
- **Pending**: a typed request was accepted locally and is awaiting a result.
- **Confirmed**: a matching, fresh readback arrived after submission.
- **Unconfirmed**: the request has no usable confirming readback. Retain this
  distinction in the model; do not overwrite current state or show a success claim.
- **Refused/failed/expired**: no state change is implied. Keep the last reported
  value and show the typed refusal or transport error.

Controls must not default an unknown value to `Off`, `false`, or a numeric
default. Compose ordinary screens from typed availability: omit absent optional
readbacks and unsupported controls, and omit groups left empty by that filtering.
Real zero, `false`, and `Off` values remain visible. Do not filter by matching
localized placeholder text.

A writable control remains actionable even if its protocol cannot report its
current value. Do not add a limitation paragraph, a verification badge, or a
`Current: Unavailable` diagnostic row. Direct On/Off commands
need no draft picker or separate Apply step. Numeric edits retain their draft
until the rider explicitly submits it; telemetry must not overwrite an edit.
While pending, show the requested value as pending only and retain any known
reported value separately. Protocol acceptance alone is not evidence that the
native Bluetooth write was delivered or that the physical state changed.

Requested-operation errors stay beside the affected control. If the whole
screen has no usable controls, show one concise explanation. Loss of previously
live safety telemetry warrants a clear warning, not a wall of missing-value rows.

## Before submission

Rust must reject a settings write unless all applicable checks pass:

1. The resolved protocol/model capability explicitly supports that command.
2. The session is live and the command is in the settings-write session, never a
   read-only session.
3. Telemetry is fresh and within the model's stationary limit (Aero: 500 mm/s).
4. The wheel is not charging and the arm has not expired.
5. The value is a typed, range-checked value for the resolved dialect.

The session enforces these checks and returns actionable refusal reasons.
Ordinary supported settings do not require a rider-facing validation mode.
Source and hardware verification remain engineering evidence, not settings
screen copy. Unsupported commands do not acquire an encoder through UI policy.

## Reversibility and retries

Numeric settings and toggles are reversible when a current value is known. The
UI should offer the previous value for an explicit retry or restore action; it
must not silently undo a user request. Trip reset, calibration, transport, and
power actions are non-idempotent: require confirmation where exposed, serialize
them, and never retry automatically after timeout or disconnect.

A repeated idempotent setting request may be submitted again, but success still
requires a matching post-write readback when one exists. A prior value matching
the request is not proof that the new command was accepted.

## Disconnects and failsafe behavior

Disconnect, stale telemetry, charging, model changes, and reconnect invalidate
pending settings work and the old capability/state projection. No queued write
may execute after its arm expires or after the session owner changes. Read-only
and capture sessions cannot schedule writes. Dangerous actuation stays behind
its separate feature and runtime gate.

## Accessibility and copy

The production screen contains grouped setting labels, values, and controls.
No validation toggle, verification badges, provenance/details disclosure,
raw diagnostic card, permanent request-status paragraph, or instructional
subtitle belongs on this surface. All supported editable settings appear in
their normal groups. Apply appears only for an explicit local draft.

Every control has a stable accessibility identifier, a localized label, and
accessible units and adjustment bounds. Error and refusal text appears when
an operation fails and explains the next action without claiming a physical
effect. Large text and unknown-value states are part of simulator/UI coverage.

The complete command classification and runtime enforcement live in
[`control-safety-matrix.md`](control-safety-matrix.md). This document defines
the user-visible policy; it does not promote simulator or source evidence to
hardware verification.
