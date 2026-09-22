# Begode Falcon and Falcon Pro protocol evidence

## Scope and provenance

Research and standard-Falcon capture date: 2026-09-22. No Falcon Pro was
available. The user confirms that this standard Falcon has no smart BMS.
Source-backed Pro behavior is not hardware-tested behavior.

Static reference: EUC World 2.66.1 / 2020660001, package
`net.lastowski.eucworld`. The APK was inspected, not installed or executed.
The third-party XAPK SHA-256 is
`024f1f88ed0fd447aadec61b34508b70fdb2843ed3b8d3bb49e27c22ae0ab46a`.
Publisher authenticity has not been independently established. APKs and
decompiled implementation remain ignored under `.protocol-references/`;
this document records protocol facts, not copied implementation.

JADX 1.5.6 reported errors in the large Begode decoder. Its structured Java
output contains unreliable reconstructed branches. Field interpretation was
checked against the simplified instruction listing for `uh.v.h`, not solely
against the structured output. Model definitions come from
`res/raw/gotway_euc_list.json`; settings vocabulary and controls come from
`uh.v`, `mi.c`, and `res/xml/preferences_wheel_gotway.xml`.

## Model distinction

| Evidence | Falcon | Falcon Pro |
| --- | --- | --- |
| Exact model banner value | `Falcon` | `Falcon PRO` |
| Firmware matcher in reference | `^(GW\|CF)16210\d{2}$` | `^GW16340\d{2}$` |
| Full / empty voltage in reference | 100.8 / 72 V | 100.8 / 72 V |
| Smart-BMS flags in reference | Neither present | Both banks enabled |
| Physical evidence here | `NAME:Falcon`, `GW1621003` | None |

The app derives 24 series-cell slots per enabled bank from 100.8 V. Do not
activate Pro capabilities from the shared FFE0/FFE1 service, advertisement,
or the presence of BMS-shaped frames. Firmware matching still needs to enter
the shared Rust identity resolver; this table is not a second runtime registry.

## Field interpretation

Offsets are zero-based within a 24-byte `55 aa ... 5a 5a 5a 5a` frame.

| Tag / offset | Reference meaning | Admission or remaining limitation |
| --- | --- | --- |
| `00`, 14–15 | CF firmware: signed tenths of a percent PWM; stock firmware: settings bit word | Preserve the full word. Without qualifying firmware evidence, do not publish PWM from it. |
| `00`, 17 | Beeper volume byte | Raw evidence is retained; actionable levels are 1–9. Zero and other invalid levels must not invent a setting value. |
| `07`, 2–3 | Signed battery current, hundredths of an amp | Preserve valid zero and sign; do not replace invalid/missing evidence with zero. |
| `07`, 5 | Low nibble lateral tilt angle; high nibble field weakening | Source evidence only; generic setting bindings remain follow-up work. |
| `07`, 6–7 | Signed whole degrees Celsius, motor temperature | Distinct from Live A controller-temperature scaling. |
| `07`, 8–9 | Signed whole-percent PWM | Multiply by ten for permille with checked arithmetic; an unrepresentable reading is absent, not wrapped. |
| `01`, 2–3 | PWM limit, whole percent, reference accepts 50–100 | Existing centipercent conversion needs correction. Hardware sample below contains 75. |
| `01`, 6–7 | Pack voltage, tenths of a volt | Frame presence alone does not establish working smart-BMS evidence. |
| `02` / `03`, 2–17 | Eight cell-voltage words per page | Bank and page ownership must be retained; all-zero placeholder pages do not establish healthy zero-volt cells. |

For stock firmware, the reference extracts pedal dip from bits 10–15,
pedal tilt from bits 5–9, safety-margin tiltback from bits 3–4, and racing
mode from bits 0–2. These meanings must remain firmware-qualified.

The reference's maximum-speed disable command is the single byte `22` hex;
positive values use a decimal menu. Existing library menu termination and
timing differ from this reference. No settings command was tested on hardware
in this session; source inspection alone does not prove physical acceptance.

## Standard-Falcon hardware captures

Three separate macOS CLI connections used the observed FFE1 notify endpoint.
Only the read-only `N` and `V` protocol requests were sent. No settings,
calibration, horn, motor, or firmware-update commands were sent.

Private captures are retained in `.tmp/falcon-evidence-2026-09-22/`, ignored
by Git. Do not publish them without a privacy review.

| Capture | Listen window | Inbound chunks | Protocol requests | Result |
| --- | --- | --- | --- | --- |
| `passive.jsonl` | 20 s | 389 | None | Live A, Live B, extra, BMS summary and cell-page frames |
| `identity-firmware.jsonl` | 20 s | 374 | `N`, `V` | `GW1621003`; no model banner observed in this window |
| `model-only.jsonl` | 12 s | 217 | `N` | `NAME:Falcon\r\n` |

The different query outcomes justify testing sequenced identity requests.
They do not yet prove that back-to-back requests caused the missing reply.

SHA-256 digests:

- Passive: `93697e5aa1ceb54a031b886c0d8b1400cc04cba129bd2ea989a7b2127b07e737`
- Identity/firmware: `e6659f9bf1ed26f2089a2f598d50ddfe6c42c4f19fdb44416b377e5b72cb6ea0`
- Model only: `93b07bd55b5a1c9c92e95a608dfc1fc347caecfbb1ec2777035eadc9e6fa282f`

Example complete frames reconstructed from the passive byte stream:

```text
Live A:      55aa19a60000003f0003ffecf92a0088000100185a5a5a5a
Extra:       55aaff860093001c0000000000000000000007185a5a5a5a
BMS summary: 55aa004b000003d90000000013880000000001005a5a5a5a
Cell page:   55aa0000000000000000000000000000000002005a5a5a5a
```

This standard Falcon has no smart BMS. Across the passive capture, all cell
pages were zero-filled placeholders. All four BMS
summary selectors repeatedly carried the same values apart from the selector.
These are not working smart-BMS banks. In particular,
summary zero current and temperature must not overwrite usable main telemetry
just because that frame arrived later. The standard-Falcon session now retains
tags 01–03 as raw evidence only. CLI capture/replay no longer interprets them
as BMS voltage evidence for profile selection. The standalone BMS codecs remain
available for the separate Pro work, where bank ownership and aggregation still
need explicit contracts.

The observed stock Live A word was `0088`, beeper byte `01`, and extra PWM
was zero. Rebuilt code retains that settings word without publishing it as PWM
and emits the raw beeper readback. The old binary incorrectly produced a PWM
reading from the stock settings word.

## Checks and remaining work

Before the BMS admission correction, the rebuilt CLI replayed the first two captures with both one-byte and
arbitrary fragmentation producing identical outputs. The passive capture
produced 323 read-only responses; identity/firmware produced 310. This proves
fragmentation equivalence, not correctness of every normalized field. With the
admission correction, these original unannotated captures require explicit
voltage evidence for CLI replay; their placeholder frames no longer supply it.
The checked-in riding-capture test uses an explicitly configured standard-Falcon
session and verifies that BMS-shaped frames emit no battery measurements.

The mobile discovery projection now uses `Begode Falcon` after a model reply,
with the advertisement name retained in the detail text. Family-only, missing,
malformed and conflicting evidence must not gain that confirmed model label.
Scanning remains passive until the existing user-selected identification flow
connects; this does not authorize probing every nearby FFE0 peripheral.

Outstanding evidence-driven work:

- Feed captured model/firmware evidence into the shared identity and capability
  contracts. CLI capture currently persists the manually selected Falcon route
  as an inferred `resolved_identity`, even without a model reply; selection and
  resolution must remain separate.
- Qualify stock/CF Live A interpretation and standard/Pro BMS admission by
  model and firmware evidence, without adding per-model decoder copies.
- Correct the BMS PWM-limit unit and investigate snapshot distance/current
  ownership. Current replay summaries show inconsistent distance values;
  successful framing does not establish correct distance semantics.
- Bring source-backed remaining settings into the generic Rust facade and
  test their readback contracts before requesting stationary write tests.
- Complete iOS capture/probe-outcome and reconnect validation in LIBCU-480;
  these CLI captures do not close the iOS acceptance requirement.
