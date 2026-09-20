# Protocol and dialect command ownership

This document describes the implemented wire-schema checks. The broader
[settings design review](settings-design-review.md) records remaining gaps in
semantic bindings, native transport outcomes, readback and physical testing.
Schema collision checks alone do not establish control correctness.
The corresponding tracker baseline is
[LIBCU-DOC-8](https://lific.mjc.lol/LIBCU/pages/30); its broader contract remains
proposed, not fully implemented by this schema.

Manufacturers, device models, base protocols, and dialects are separate concepts:

- Veteran is the community name for the protocol shared by Leaperkim and NOSFET.
  NOSFET has a dialect of that protocol. Aero is a model selecting that dialect;
  an additional Aero dialect requires evidence of a model-specific difference.
- Begode is both a manufacturer name and the community name of its protocol.
  Falcon is a model with a dialect of the Begode protocol.
- VESC is motor-controller firmware and its wire protocol. Refloat is a VESC
  package with a package-specific dialect on VESC, not a manufacturer or a new
  base protocol.

## Write contract

Every writable model selects a base-protocol type and a checked dialect of that
same type. Model implementations declare capabilities and operating constraints;
they cannot implement their own benign/settings serialization hooks. Sessions
dispatch to the selected dialect. Dialects return opaque selections from the
checked schema, not arbitrary byte buffers. A selection carries its protocol in
its type, so a Veteran dialect cannot return a Begode command. Only the shared
serializer constructs control frames and timed write sequences. Read-only
VESC/Refloat support does not acquire write authority through this change.

The shared `control_wire` schema is the source for both collision checking and
serialization, rather than an audit table maintained beside hand-written bytes.
Its declarations are evaluated at compile time even if the dialect is unused.
Dialect extensions are checked against all inherited command destinations as
well as each other. The current NOSFET dialect adds the brake-overpressure field
to the Veteran schema; Falcon currently inherits the Begode layouts unchanged.

Compilation rejects:

- Two binary fields with the same header, bank, and value offset, including
  equivalent bank padding.
- Duplicate literal commands.
- Duplicate submenu selectors, even with different numeric payload widths.
- Literal commands that occupy a declared binary field or submenu entry point.
- A dialect extension that collides with an inherited command.
- A model selecting a dialect of a different base-protocol type.

Aliases for one physical operation must resolve to one canonical wire command,
not declare duplicate destinations. Aero lateral cutoff sends one 22-byte `LkAp`
frame, matching the official NOSFET and EUC World setters; it does not require an
`LdAp` companion. The unsupported paired layout and its collision-check fixture
have been removed. See the [reference comparison](eucworld-aero-settings-re.md#lateral-correction-and-send-lifecycle-comparison-2026-09-20)
for primary artifact paths and line references. Splitting that frame into 20+2
bytes at the current link limit is transport chunking, not a second command. Distinct
protocols/dialects may legitimately reuse bytes; there is no global ban on
matching opcodes across unrelated devices. New wire-layout kinds require an
exhaustive overlap rule and serializer.

The compiler proves the declared mapping is internally non-colliding. It cannot
prove that a source-derived command matches a wheel's firmware or that hardware
applied it. Golden frames, fresh readback, and physical acceptance remain separate
checks; an unsuccessful write must not remove a setting's readback capability.

## Typed-contract status

The public settings contract now exposes a typed completion strategy selected by
the protocol adapter. `MatchingReadback` is owned by the adapter's semantic
setting definitions; `SubmissionOnly` means that host submission is the
strongest evidence currently available. There is no public confirmation boolean
or vendor-specific settings facade for native consumers to interpret. A failed
write therefore cannot remove a setting's readback capability, and a submitted
write is not presented as device confirmation unless the descriptor requires a
matching observation.

The current Aero/NOSFET and Falcon adapter binding co-locates semantic control
shape, write access, the typed observation field, and the derived completion
strategy with an explicit NOSFET, Falcon, or read-only encoder binding. Checked
requests are now encoded through that setting binding; the adapter no longer
has a separate vendor-wide write switch that can accidentally grant authority
to an unrelated setting. The concrete semantic-to-wire mapping inside each
dialect and the decoder's value comparison are still separate implementation
details, so compile-time missing-mapping diagnostics remain follow-up work.
A dialect binding is not proof of physical acceptance.

Accepted settings writes also carry the Rust request operation identity through
the core/mobile FFI boundary and into correlated CoreBluetooth planned writes,
including each chunk of a bounded write. Swift preserves that identity through
the native queue and receipt path before advancing host transport state; it
still must not interpret that receipt as wheel confirmation. Clearing a pending
settings write now produces a typed cancellation receipt, distinct from
rejection. Read-only polling, multi-step operation identity beyond the current
write chunks, and cancellation receipts for other operation classes remain
separate follow-up work.

Write and observation domains may differ, so reported values must not be rejected
solely because they cannot be selected for writing. Keep the semantic public API;
no compatibility facade or parallel settings framework is required.

Private checked constructors must protect lower encoder entry points as well as
FFI requests. In particular, dividing speed by 10 before checking its wire value
can truncate an inexact canonical input today. Range checks alone cannot enforce
exact representability or correct display-unit conversion. Runtime identity,
firmware applicability and fresh operating guards remain necessary alongside
compile-time structure.

## Tests

The compiler tests compile the production schema checker in const contexts and
require the specific collision diagnostic. They cover binary-bank collisions,
padding aliases, literal opcodes, submenu widths, literal bypasses, inherited
collisions, and valid distinct/inherited layouts. Existing encoder and session
tests check complete frame bytes, CRCs, and timed transaction steps. Lateral
regressions require the exact single source-backed frame and no follow-up write.

Further acceptance requires compile-fail cases for missing encoder/readback
bindings and boundary tests for exact units, sentinels and inexact inputs across
supported dialects. Native receipt/queue tests and independently sourced captured
page cycles are separate requirements; green encoder tests do not close them.
