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
- **Unconfirmed**: transport accepted the write but the protocol has no usable
  readback. The UI must say this explicitly and must not present it as current.
- **Refused/failed/expired**: no state change is implied. Keep the last reported
  value and show the typed refusal or transport error.

Controls must not default an unknown value to `Off`, `false`, or a numeric
default. Render `Unavailable`/`Unknown` until Rust supplies a current value.
While pending, show the requested value as pending only; readback rows continue
to show the prior current value or an explicit pending label.

## Before submission

Rust must reject a settings write unless all applicable checks pass:

1. The resolved protocol/model capability explicitly supports that command.
2. The session is live and the command is in the settings-write session, never a
   read-only session.
3. Telemetry is fresh and within the model's stationary limit (Aero: 500 mm/s).
4. The wheel is not charging and the arm has not expired.
5. The value is a typed, range-checked value for the resolved dialect.

The UI disables controls while these checks are false and surfaces the reason
through localized text and accessibility hints. Validation-mode writes remain
visibly marked as source-backed but unverified.

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

Every control has a stable accessibility identifier, a localized label, its
unit/range, and a hint describing whether the result is confirmed or
unconfirmed. Error and refusal text must explain the next safe action without
claiming a physical effect. Large text and unavailable/unknown states are part
of simulator/UI coverage.

The complete command classification and runtime enforcement live in
[`control-safety-matrix.md`](control-safety-matrix.md). This document defines
the user-visible policy; it does not promote simulator or source evidence to
hardware verification.
