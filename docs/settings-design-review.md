# Settings correctness: design review and proposed contract

Status: proposed design, 2026-09-18. Implementation is incomplete. This review
reopens the settings/control boundary on PR #106; a clean build or a committed
checkpoint does not close the physical failures. It applies to every supported
protocol, not just the NF2557 fixture.

Tracker mirror: [LIBCU-DOC-8](https://lific.mjc.lol/LIBCU/pages/30).
[LIBCU-664](https://lific.mjc.lol/LIBCU/issues/LIBCU-664) and
[LIBCU-PLAN-16](https://lific.mjc.lol/LIBCU/plans/68) own delivery. The
[reconciliation index](settings-tracker-reconciliation.md) maps the existing
repair owners and records which documents, plans and tickets were updated.

## Correct the evidence first

The recent Mac inventory logged the first nonempty *descriptor* snapshot during
subscription, before usable telemetry. The validator then exited as soon as ride
telemetry was ready. Its `current=nil` rows do not establish that the device has
no settings readback. Its bounds and `confirmationSupported` fields are our
library's declarations, not capabilities negotiated from the wheel. Existing
NF2557 fixtures and the source inventory describe settings readback.

Likewise, the reported value “34.8” did not specify a unit. The native display
converts canonical speed through `SpeedReadout`. Rejecting canonical 348
deci-km/h is a useful representability test, but does not explain a displayed
34.8 mph (approximately 56 km/h). Reproduce the actual display-to-request path.
Voltage correction -15..15 at precision 1 means -1.5..1.5 percent.

The noise stopped after the user power-cycled the wheel. That establishes that
the symptom ceased; it does not identify the command that caused it or prove
that every setting was restored. No additional hardware writes are needed to
review this design.

## Concrete gaps in the reviewed checkpoint

The first LIBCU-836 containment change removes the mutating sweep and rejects
its legacy opt-ins before constructing a Bluetooth session. Its offline tests
cover that gate, finite argument bounds, one connection attempt, and verdicts
that never equate connection success with settings acceptance. The harness rows
below describe the removed implementation, not remaining callable test cases.
The replacement settings observation/case runner is still unimplemented; active
discovery and native queued-write cancellation remain separate open problems.
The protocol checkpoint now contains three bounded repairs: exact speed inputs
are rejected by the low-level NOSFET and Falcon encoders instead of being
truncated; the selected decoder is fenced to its subscription channel even
through the legacy accept-any session path; and incompatible Rust-side probe
state is retired after Veteran, VESC or conflict evidence. The detector still
needs a candidate-wide safe probe plan, and retiring Rust state does not cancel
bytes already queued in the native transport.
The terminal native-tool coordinator now uses Unix exec; offline subprocess
tests prove PID preservation, inherited FFI lock ownership, signal exit and
failed-exec cleanup. Full launcher-chain and queued-write cancellation remain
unproven.

| Boundary | Source evidence | Consequence |
| --- | --- | --- |
| Control definition | `device_settings.rs`, `device_settings/aero.rs`, `request_encoder.rs`, and `control_wire/` separately define membership, bounds, mappings and confirmation | A collision-free wire schema can still encode the wrong semantic control or accept a differently scaled value. |
| Encoding | `request_encoder.rs` now requires canonical deci-km/h inputs to be divisible by 10 before checked wire-value construction | The low-level NOSFET and Falcon encoders no longer silently truncate an inexact speed value; display-unit conversion and end-to-end request provenance remain open. |
| Transport outcome | `device_connection/settings.rs::submit_setting` records acceptance; Swift `DeviceSessionTransport.process` publishes controls before executing native operations | An accepted plan can appear sent before the host actually submits it. |
| Native backpressure | `CutoutSessionCore.writeWithoutResponse` drops the oldest queued write when full; queue entries contain a characteristic and bytes | A dropped command or partial sequence lacks a request-specific failure; captured write intent is not evidence of host submission. |
| Live harness | `start` repeats the suite after a false result, and `waitForSettingResult` accepts any non-idle state | Pending writes cause premature timeout reporting and can trigger repeated settings/actions. |
| Harness verdict | `runSettingsValidation` ignores skips and unconfirmed counts when deciding success | An incomplete run can report `validation=ok`. |
| Probe selection | `validNumericProbes` invents values; `testUnknownBoolean` chooses false/true/false; invalid-value tests call the live submission API | A legal range does not establish a safe experiment; a broken validator can turn a negative test into a real write. |
| Detection | `identification.rs` now suppresses Begode probes once Veteran/VESC/conflict evidence resolves the protocol and retires their pending Rust state; the historical NF2557 run sent Begode N/V/M probes and flushed queued bytes after Veteran identity resolved | Shared GATT characteristics cannot by themselves establish that a probe is harmless. Native queued-write cancellation and safe probing while multiple protocols remain plausible are still open. |

Paths above are under `crates/cutout-protocols/src/` or
`swift/CutoutMobile/Sources/CutoutMobile/`; harness findings refer to
`swift/CutoutMobile/Tests/CutoutMobileLiveValidator/main.swift`.

## Proposed ownership

Keep the existing Rust session as the sole control owner. Swift, Kotlin and CLI
clients select semantic controls and render snapshots. They do not select wire
banks, decide confirmation policy, invent defaults, or implement retry policy.

The UI-facing generic facade is `CutoutSessionStateHandle.settings()`, which
returns one atomic `MobileSettingsDto` containing the connection identity,
descriptors, requested/observed setting state, actions, and charge basis. Swift
exposes that same projection as `CutoutSessionCore.settings` and `DeviceSettings`.
Protocol and dialect adapters remain behind this boundary. This is the current
in-repository presentation facade, not a compatibility layer for older clients.

Identity keeps protocol, dialect, manufacturer, model and firmware separate:

- Veteran protocol -> NOSFET dialect -> Aero model; add an Aero-specific dialect
  only when evidence requires different wire behavior.
- Begode protocol -> Falcon dialect/model selection.
- VESC protocol -> Refloat package dialect, retaining the controller/firmware
  and package versions needed to resolve behavior. Read-only support does not
  imply writable capability.

Use one typed control definition per dialect binding. It supplies semantic ID,
canonical quantity/value domain, write encoding, readable observation mapping,
completion strategy and applicability. A model/firmware profile selects these
definitions and constraints. Public descriptors are projections of that
selection. Preserve this semantic public API; add no compatibility facade or
parallel generic framework.

The write domain and observation domain may differ: firmware can report a value
that is not selectable for writing. Model those domains explicitly instead of
discarding the observation or making it writable by accident.

## What must be enforced structurally

Typed constructors and exhaustive definitions must make these inconsistent
declarations impossible to compile:

- A model choosing a dialect from a different protocol.
- A writable control lacking a checked encoder for its canonical value type.
- Duplicate wire destinations inside one effective dialect, except explicit
  semantic aliases of the same operation.
- A readback-confirmed control lacking an observation mapping and comparison
  rule for its value type. A bare confirmation boolean is insufficient.
- A request/queued effect losing its connection and operation identity.

Dynamic FFI input still needs runtime validation: range, representability,
exact units, firmware applicability, current connection and fresh operating
conditions. Private checked value constructors prevent bypass through lower
encoder entry points. Reject inexact speed conversion; never silently truncate.
The compiler cannot prove that a published opcode is correct for real firmware.
Independent source evidence, captured frames and physical tests must prove that.

## Execution and observations

Assign each request an operation ID within its connection attempt. Carry it
through preparation, queued transport effects, actual host submission and
completion. A native adapter reports submission or failure to Rust. For BLE
without response, host submission is the strongest transport receipt available;
it is not a device acknowledgment.

Rust owns serialization, deadlines, cancellation and retry rules. Revalidate
the connection and applicable operating guard when a delayed effect is sent.
Queue overflow must fail the affected operation explicitly; never silently
discard a fragment. Identity resolution retires incompatible detection work.
Probe eligibility must cover all still-plausible protocols sharing the channel.

Use a typed completion strategy: write-only submission, protocol acknowledgment,
matching post-submission readback, or explicit multi-step procedure. Preserve
the strength of each outcome. A fresh matching observation is evidence that the
desired value was observed; absent an echoed request ID, do not claim causal
acknowledgment from telemetry alone. Start applicable deadlines at the specified
transport/response boundary, not merely when a client asks to enqueue a command.

Observations retain protocol provenance, source, age and validity separately from
requests. The current readback normalization tags observations with the selected
adapter's protocol; that is a useful binding invariant, not independent proof of
packet origin or device acknowledgment. Distinguish
not-yet-observed, explicitly unavailable, stale and undecodable evidence without
inventing a current value. Known factory defaults require independent provenance
and never become current device readings automatically. The public UI should
remain the approved production design; these distinctions belong primarily in
the library contract and diagnostics, with concise feedback for actual user
operations. The checkpoint UI changes are not renewed visual approval.

## Replace the live-test procedure

The replacement default harness must inventory descriptors and observe notifications for a
bounded interval sufficient to see the relevant settings pages. It reports
which pages/fields were actually seen and distinguishes library declarations
from observations. It must not infer unsupported readback from an early sample.

Mutating tests use an explicit reviewed case: exact device identity, exact
setting/action and target, expected physical effect, expected evidence, and
restoration policy. Remove guessed probe values and boolean sweeps. Audible or
irreversible effects require an explicit case, not a broad environment switch.
Invalid-input tests run against a fake transport and assert zero emitted writes.

Execute each case once. Await its terminal outcome before any restoration or
next case. Stop on unexpected results. Log intent before execution and stream
transport/readback evidence durably as it occurs. Cancellation must stop the
actual child process and cancel queued work. Restoration is a separately tracked
operation whose failure leaves an explicit outstanding recovery obligation.
Skipped, submitted-unconfirmed and failed cases cannot count as confirmed tests.

## Implementation order and acceptance

1. Repair the harness offline first. Fake-transport tests must prove exactly-once
   execution, genuine deadline waiting, cancellation, failure exit, no writes in
   inventory mode, no generated probe values and honest incomplete results.
2. Consolidate typed control definitions and exact conversions. Compile-fail
   cases cover protocol mismatch, missing encoder/readback bindings and schema
   collisions; boundary tests cover units, sentinels and inexact inputs.
3. Connect operation identity and native transport receipts to Rust lifecycle.
   Test backpressure, overflow, partial sequences, replacement connections,
   stale guards, late callbacks and cancellation before every delayed step.
4. Repair and verify settings observation acquisition with captured page cycles,
   including fragmentation, unavailable fields and firmware applicability. Use
   independently sourced golden frames; simulator agreement with its own encoder
   is not independent protocol evidence.
5. Revisit native presentation against the approved comps using unknown, pending,
   observed and failed snapshots. Verify display-unit round trips explicitly.
6. Run reviewed physical cases individually on NF2557. Headlight On/Off is
   physically confirmed with the modern literal commands; the separate AERO
   brightness-cycle control is deferred outside this acceptance baseline. Keep
   sound, lateral tilt, speed alarm, brake alarm, pedal angle, mode switches
   and trip reset open until their required effects and evidence are
   established.

This is the design baseline for further work, not a claim that those repairs
have already been implemented or that #106 is ready to merge.
