# Settings correctness tracker reconciliation

Reconciled 2026-09-18 for PR #106. This records documentation, planning and the
bounded protocol repairs on the settings branch; it does not claim complete
settings acceptance. No hardware writes were made for this update.

## Authoritative contract and delivery

- [Repository design review](settings-design-review.md) and its Lific mirror,
  [LIBCU-DOC-8](https://lific.mjc.lol/LIBCU/pages/30), contain the proposed contract.
- [LIBCU-664](https://lific.mjc.lol/LIBCU/issues/LIBCU-664) is the settings epic;
  LIBCU-662 coordinates the broader audit and LIBCU-382 the library write scope.
- [LIBCU-PLAN-16](https://lific.mjc.lol/LIBCU/plans/68) no longer says the
  implementation is complete. Its settings sequence now starts with offline
  harness repair. Current software and physical failures remain open.
- Approved visual comps remain [PNG 16](https://lific.mjc.lol/api/attachments/16)
  and [SVG 17](https://lific.mjc.lol/api/attachments/17). The revised
  [interaction contract 20](https://lific.mjc.lol/api/attachments/20) supersedes
  attachment 19's lifecycle wording without changing the visual direction.

| Work | Owner | PLAN-16 step |
| --- | --- | --- |
| Offline harness safety, exactly-once execution and honest outcomes | [LIBCU-836](https://lific.mjc.lol/LIBCU/issues/LIBCU-836) | 1326 |
| Typed semantic/dialect definitions and exact conversions across devices | LIBCU-664; alarm workflow LIBCU-349 | 1144, 1145 |
| Operation identity, actual host receipts, deadlines and cancellation | [LIBCU-477](https://lific.mjc.lol/LIBCU/issues/LIBCU-477) | 1327 |
| Backpressure, explicit overflow and receive readiness | LIBCU-641, LIBCU-640; shared with PLAN-14 | 1327 |
| Cross-protocol-safe probing and retiring incompatible queued work | LIBCU-505; coordination LIBCU-663, LIBCU-460 | 1016, 1327 |
| Complete settings observations, distinct domains and freshness | [LIBCU-476](https://lific.mjc.lol/LIBCU/issues/LIBCU-476) | 1328 |
| Generic clients, clean final UI and exact display-unit interaction | LIBCU-793 and SET tickets under LIBCU-664 | 1146, 1329 |
| Individual physical effect, readback and restoration cases | [LIBCU-390](https://lific.mjc.lol/LIBCU/issues/LIBCU-390), LIBCU-346 | 1329 |
| Reconcile separate software, UI and hardware acceptance evidence | LIBCU-664 | 1147 |

The current PR checkpoint adds exact speed conversion rejection in the NOSFET
and Falcon encoders, a subscription-channel gate for selected notification
decoders, protocol provenance on normalized read observations, and retirement
of incompatible Rust-side detection probes after stronger protocol evidence.
These are containment and binding invariants, not completion of the typed
control-definition refactor. Native queued writes, operation receipts and
acknowledgment semantics remain open under LIBCU-477/LIBCU-505/LIBCU-476.

LIBCU-836, LIBCU-477, LIBCU-641 and LIBCU-505 explicitly block LIBCU-390;
LIBCU-836 also blocks LIBCU-346. These are actual prerequisites, not a claim that
every issue in the parent audit must close before any bounded repair can land.

## Records reconciled

61 existing issue descriptions were updated, and LIBCU-836 was created:

- Core settings and evidence: LIBCU-342, 343, 346, 347, 349, 352, 382, 383,
  384, 385, 386, 390, 391, 392, 393, 394, 453, 536, 564, 623, 625, 629,
  662, 664.
- Shared lifecycle, detection and native integration: LIBCU-348, 460, 476,
  477, 505, 632, 640, 641, 653, 655, 663.
- SET findings: LIBCU-753 through 764 and LIBCU-771 through 776.
- Presentation/current-consumer records: LIBCU-387, 388, 389, 518, 793,
  807, 815, 827.

Historical done issues retain their bounded completion, with current failures
assigned to open repair owners. Comments remain historical; corrections are in
the current descriptions. Cancelled consolidation tickets remain cancelled.
LIBCU-794 and LIBCU-796 already direct work to LIBCU-793 and needed no change.
The historical feature-gate/vocabulary/refusal scopes of LIBCU-31, 350, 351 and
429 were reviewed and not reopened: the new gaps belong to the owners above.

Six plans were reconciled: active PLAN-16 (execution), PLAN-14 (shared transport)
and PLAN-15 (native ownership), plus historical PLAN-2, PLAN-3 and PLAN-7.
Archived/done plans retain their status and explicitly defer current settings
acceptance to PLAN-16. Unrelated ride, camera, capture and music sequences were
not rewritten.

All seven existing project pages were inspected. DOC-1 and DOC-3 now mark their
old priority stacks as historical and point to current safety requirements;
DOC-5 coordinates shared VESC/Refloat transport; DOC-6 preserves the Rust/native
boundary; DOC-7 separates release/permissions checks from settings acceptance.
DOC-2 (BMS screenshots) and DOC-4 (Live Activity evidence) were left unchanged.
DOC-8 is the new current design mirror, not another implementation framework.

The repository coverage, source audit, EUC World research, wire contract, safety
matrix/UX, simulator, mobile FFI and Tune comp specifications were reconciled.
The VESC/Refloat boundary now references the same all-device contract without
enabling writable support. PR #106's description explains current test coverage
and open repairs instead of presenting generic green checks as completion.
The feature-audit paths retained in historical tracker records are not present
in this checkout; their current settings requirements are recorded in this
review and PLAN-16 rather than fabricating or replacing those historical files.

## Evidence boundaries retained

- The early descriptor inventory did not establish absent readback or negotiated
  bounds. Existing captures contain settings pages.
- The reported 34.8 speed-alarm value has no established unit; 34.8 mph is about
  56 km/h. The actual display-to-request path needs testing.
- Voltage correction -15..15 at precision 1 is -1.5..1.5 percent.
- The painful sound stopped after power cycling. Its cause and complete setting
  restoration are unknown.
- Positive and negative user reports remain in the
  [physical evidence matrix](aero-settings-coverage.md#physical-evidence-and-open-acceptance).
  Neither simulator agreement nor host submission establishes physical effect.
- No validation dashboard, engineering badges or permanent provenance text is
  added to the approved production UI.

Tracker descriptions, plans, pages and the new attachment were reread after
writes. Repository checks for this documentation-only update cover whitespace
and local Markdown links; they are not software or hardware acceptance tests.
