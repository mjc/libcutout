# Protocol and dialect command ownership

This document describes the implemented wire-schema checks. The broader
[settings design review](settings-design-review.md) records remaining gaps in
semantic bindings, native transport outcomes, readback and physical testing.
Schema collision checks alone do not establish control correctness.

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
not declare duplicate destinations. Distinct protocols/dialects may legitimately
reuse bytes; there is no global ban on matching opcodes across unrelated devices.
New wire-layout kinds require an exhaustive overlap rule and serializer.

The compiler proves the declared mapping is internally non-colliding. It cannot
prove that a source-derived command matches a wheel's firmware or that hardware
applied it. Golden frames, fresh readback, and physical acceptance remain separate
checks; an unsuccessful write must not remove a setting's readback capability.

## Tests

The compiler tests compile the production schema checker in const contexts and
require the specific collision diagnostic. They cover binary-bank collisions,
padding aliases, literal opcodes, submenu widths, literal bypasses, inherited
collisions, and valid distinct/inherited layouts. Existing encoder and session
tests check complete frame bytes, CRCs, and timed transaction steps.
