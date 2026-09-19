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
- A paired binary command whose primary or companion field collides with any
  declared destination.

Aliases for one physical operation must resolve to one canonical wire command,
not declare duplicate destinations. Some Veteran settings are intentionally
ordered frame pairs: lateral cutoff sends the `LkAp` field followed by an
`LdAp` companion with discriminator `01 00`; both fields belong to one semantic
setting and are declared together in the checked schema. Distinct
protocols/dialects may legitimately reuse bytes; there is no global ban on
matching opcodes across unrelated devices. New wire-layout kinds require an
exhaustive overlap rule and serializer.

The compiler proves the declared mapping is internally non-colliding, including
both destinations of a paired command. It cannot
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

The schema checks and adapter definitions still do not make semantic membership,
canonical domains, encoding, observation comparison, and completion policy one
literal declaration. The checked request path does require a concrete encoder for
every writable command, and readback observations are normalized through the
selected adapter, but compile-time missing-binding diagnostics and one-definition
projection remain follow-up work. A checked dialect marker is not proof that
every control binding is complete.

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
tests check complete frame bytes, CRCs, and timed transaction steps.

Further acceptance requires compile-fail cases for missing encoder/readback
bindings and boundary tests for exact units, sentinels and inexact inputs across
supported dialects. Native receipt/queue tests and independently sourced captured
page cycles are separate requirements; green encoder tests do not close them.
