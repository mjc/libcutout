# Settings, wheel controls and BMS coverage

Audit revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`. This folder contains 22 individual findings. Severity and evidence status are separate: a source-confirmed calculation defect is not a reproduced phone crash.

## Scope and evidence

The audit traced Tune SwiftUI controls through CutoutAppModel, CutoutSessionCore, mobile FFI setting validation/state, Rust setting domains, request encoders, Veteran/Begode page decoding and BMS aggregation. Applicable AGENTS.md, Ponytail and SwiftUI Expert guidance were read. MCPLS was unavailable; source search used rg. The ordinary Nix environment initially failed upstream; subsequent repository reads used the root-provided cached project shell via nix develop. No product source was edited, no wheel writes were sent, and this worker did not execute builds, tests or phone reproduction.

The current Aero ordinary release capability set marks most setting writes unverified and disables them; validation mode allows the unverified set. Therefore the Aero editor findings apply to that mode and any build exposing those controls. It is not accurate to claim all are available in ordinary release mode. The user's phone Aero model evidence and crash logs are tracked by the root audit separately.

## Individual feature inventory

| Feature | Findings or status | Inspected behavior and limits |
| --- | --- | --- |
| PWT / PWM tilt-back margin and Off | SET-001, SET-019, SET-022 | 0–70 margin, complement-to-duty encoder and wire 200 Off inspected. No reversed encoder established. |
| Tilt-back speed | SET-004, SET-009, SET-010, SET-022 | 10–200 km/h editor, Rust speed domain and stationary shared submission traced. |
| Alarm speed | SET-004, SET-009, SET-010, SET-022 | Same integer speed domain, separate field/command. |
| Numeric MD pedal hardness | SET-004, SET-009, SET-022 | 0–100 numeric domain distinct from hard/medium/soft presets. |
| Vertical angle ANG | SET-004, SET-009, SET-022 | −8.0 to +8.0 degrees, tenths wire representation aligned; physical sign/readback not validated. |
| Display backlight | SET-004, SET-005, SET-009 | 0–100 UI/core range aligned; page-readback typed. |
| Wheel display units | SET-005, SET-009, SET-010 | Unknown option exists and Send disabled while nil; wheel setting is distinct from the phone’s currently fixed mph presentation. No phone unit preference exists. |
| Aero beeper volume/key tone | SET-004, SET-005, SET-009 | 0–100 UI/core range aligned; not duplicated as separate key-tone control. |
| Dynamic assist/acceleration assist | SET-004, SET-005, SET-009 | 0–100 field; physical effect and manufacturer naming remain device comparison work. |
| Pedal-dip compensation | SET-004, SET-005, SET-009 | 0–100 field; existing inventory records conflicting acceleration-limit naming, no extra command invented. |
| Lateral cutoff | SET-004, SET-005, SET-009 | 35–75 degrees aligned with Rust bound; physical cutoff tests not performed. |
| Voltage correction | SET-004, SET-005, SET-009 | −15…15 tenths of percent matches −1.5…+1.5%; no sign/inversion evidence. |
| Maximum charge raw setting | SET-002, SET-003, SET-004, SET-005 | Unresolved Aero voltage conversion is already documented; physical range cannot be promoted from raw bounds. |
| High-speed mode | SET-004, SET-005, SET-009 | Typed Boolean; firmware effect not independently verified. |
| Low-battery mode | SET-004, SET-005, SET-009 | Typed Boolean; firmware effect not independently verified. |
| Transportation mode | SET-004, SET-005, SET-009 | Typed Boolean; core says prevents normal motor startup. No wheel writes performed. |
| Gyro calibration | SET-009, SET-012 | Start/wait/complete/stop source lifecycle traced; not a physical acceptance test. |
| Modern riding mode | SET-004, SET-005, SET-009 | Binary command distinct from legacy presets and numeric MD. |
| Brake overpressure alarm | SET-004, SET-005, SET-009 | 90–125% aligned in UI/core. Not interpreted as general motor PWM. |
| Legacy pedal presets | SET-009 | Hard/medium/soft readback and immediate submissions traced; current/readback takes precedence. No separate root cause found. |
| Begode roll angle | SET-009 | Low/medium/high enum, model-specific capability gate inspected; degree meanings unverified. |
| Begode speed-alarm mode | SET-009, SET-011 | Only two documented writable choices; readback can contain four kinds. |
| Begode maximum speed | SET-006, SET-009, SET-010 | 0–99 integer km/h domain; lacks confirmed readback in visible control. |
| Begode beeper volume | SET-007, SET-009 | 1–9 domain; lacks current readback/lifecycle in visible control. |
| Begode LED mode | SET-008, SET-009 | 0–9 raw mode domain; physical effect mapping not verified. |
| Trip reset | No separate finding | Explicit destructive confirmation; pending disabling and typed stationary/busy/expired feedback inspected. Physical reset not tested. |
| Aero manual headlight | SET-021 | Separate SetLights command, capability-gated; unconfirmed status defect recorded. |
| Aero high beam | SET-020 | SetAeroHighBeam separate command; no claim of decoded physical readback. |
| Aero taillight | Coverage gap | Only unsupported capability/status row, no independent UI write control. Do not infer command from headlight. |
| Aero ring RGB/modes, running lights/stealth | Coverage gap | No app control discovered; existing docs say exact commands unknown. MELK accessory is a different surface. |
| Auto-shutdown and charge-mode readback | No separate finding | Read-only availability formatting inspected; no new writable capability assumed. |
| App-generated speed/current/PWM audio alarms | Coverage gap | No dedicated phone alarm preference/editor/output path found in this scoped app search; wheel alarms must not be presented as phone alarms. |
| BMS cell identities and map | SET-013 | Veteran pages collide; Begode offset code inspected as counterexample. |
| BMS lowest cell and spread | SET-014, SET-016 | Page-local extrema and wrong average baseline traced to UI. |
| BMS temperatures and sensor counts | SET-015, SET-017 | Separate pack sensors and multiple page maxima traced. |
| BMS freshness and partial pages | SET-018 | No page timestamps/expiry in aggregate path; device reproduction still needed. |
| BMS fault/balancing/resistance/trend | Coverage gap | Live FFI projection supplies no fault summaries, balancing summaries, per-cell resistance or history. No claims of working diagnostics or health inferred from placeholders. |
| BMS topology | Coverage gap | Projection reports observed-group count with unverified topology and no series count. Physical bank/series/parallel mapping not established. |
| VESC/Refloat firmware tuning | Absent app surface / coverage gap | Only VESC Ride/Debug routes found; no advanced firmware configuration editor or write path to audit. Firmware tuning behavior is not accepted as working. |
| VESC board/battery profile setup | Coverage gap | Public configureVescBoard API exists but no Apps/CutoutApp caller found; setup integration needs end-to-end follow-up with telemetry audit. |

## Finding index

| ID | Severity | Evidence | Finding |
| --- | --- | --- | --- |
| [SET-001](SET-001-pwt-headroom-label.md) | P1 | UX inconsistency | PWT shows headroom under a PWM threshold label |
| [SET-002](SET-002-max-charge-value-format.md) | P2 | source-confirmed defect | Max-charge editor hides its numeric value |
| [SET-003](SET-003-max-charge-unresolved-meaning.md) | P1 | UX inconsistency | Max-charge write offers a number with unresolved physical meaning |
| [SET-004](SET-004-unknown-settings-defaults.md) | P2 | UX inconsistency | Unknown wheel settings look like concrete editable values |
| [SET-005](SET-005-draft-never-reconciles.md) | P2 | source-confirmed defect | Aero toggle and riding-mode drafts outlive rejected writes |
| [SET-006](SET-006-begode-max-speed-default.md) | P2 | UX inconsistency | Begode max-speed editor invents 30 km/h |
| [SET-007](SET-007-begode-volume-default.md) | P2 | UX inconsistency | Begode beeper picker invents volume 5 |
| [SET-008](SET-008-begode-led-mode-numbers.md) | P2 | UX inconsistency | Begode LED mode is an unexplained number with an invented initial value |
| [SET-009](SET-009-settings-refusal-context.md) | P2 | UX inconsistency | Tune hides why a setting write was refused |
| [SET-010](SET-010-tune-speed-units.md) | P2 | UX inconsistency | Tune uses km/h while Ride and readback use mph |
| [SET-011](SET-011-alarm-readback-missing-picker-choice.md) | P2 | source-confirmed defect | Speed-alarm picker cannot represent Off or PWM readback |
| [SET-012](SET-012-gyro-calibration-no-guidance.md) | P1 | UX inconsistency | Gyro calibration lacks a usable physical procedure |
| [SET-013](SET-013-veteran-bms-page-collisions.md) | P1 | source-confirmed defect | Veteran/NOSFET cell pages overwrite one another |
| [SET-014](SET-014-bms-spread-last-page.md) | P1 | source-confirmed defect | BMS pack spread and lowest cell come from one page |
| [SET-015](SET-015-bms-hottest-page-overwrite.md) | P1 | source-confirmed defect | BMS hottest temperature can be replaced by a cooler page |
| [SET-016](SET-016-lowest-cell-wrong-baseline.md) | P2 | source-confirmed defect | Lowest-cell detail labels full spread as distance below average |
| [SET-017](SET-017-bms-sensor-count-wrong-source.md) | P2 | source-confirmed defect | Unknown-topology screen drops reported temperature-sensor counts |
| [SET-018](SET-018-bms-no-freshness.md) | P1 | hypothesis needing device validation | BMS pages can remain current-looking after their updates stop |
| [SET-019](SET-019-swift-pwt-range-drift.md) | P2 | source-confirmed defect | Swift accepts PWT values Rust rejects |
| [SET-020](SET-020-high-beam-unlabeled.md) | P2 | UX inconsistency | Tune does not identify the Aero high-beam buttons |
| [SET-021](SET-021-manual-headlight-status-static.md) | P2 | source-confirmed defect | Headlight status says sent before a request and ignores failure |
| [SET-022](SET-022-first-readback-overwrites-edit.md) | P2 | source-confirmed defect | Delayed first readback can overwrite an Aero edit |

## Checks and hypotheses challenged

- PWT wire conversion is intentionally `100 - headroom`; changing it blindly would introduce a protocol error. Off has its own wire value. The verified defect is the displayed basis, plus a separate Swift 0–100 versus Rust 0–70 domain mismatch.
- Unknown Aero write capability is not release support. Validation UI does expose the controls, and the findings retain that condition.
- Settings guards remain in Rust. Enabled SwiftUI buttons and weak refusal feedback do not prove a moving wheel accepts writes.
- BMS empty-fault merging initially looked suspicious, but live page projection currently emits no decoded faults and known pages are replaced in a cache before aggregation. No independent persistent-fault defect was filed.
- No force-unwrap crash was established in these BMS views. Duplicate-index dictionary construction remains a boundary worth fuzzing, but emitted source groups have unique within-page indices; no speculative crash finding was filed.
- Per-page cell extrema and hottest-temperature errors are independent of the Veteran cell-identity collision. Both deserve their own acceptance scenarios after identity repair.

## Existing tests inspected, not run

- CutoutAppRouteTests checks the three formatted numeric units but omits raw. Its PWT example incorrectly expects 74 to be accepted, matching the drifting Swift bound.
- AeroSettingsSimulatorTests exercises Rust-backed setting submission/readback and explicit Off; it is synthetic command-state evidence, not physical setting proof.
- Rust encoder tests distinguish PWT Off and zero margin and check documented frames. Veteran telemetry tests decode captured MD/PWT readback.
- Mobile FFI tests explicitly check Begode global cell indexing; this does not cover Veteran's multi-page identity.
- BmsSnapshotContractTests covers source values, unavailable fields and synthetic per-group temperatures. It does not establish correct real pack-temperature sensor counting or cross-page extrema.

## Cross-review

The connection audit independently traced SET-013, SET-014 and SET-015 through the production session.rs ingress and confirmed the source findings. CON-001 device-switch callbacks and CON-003 failed-reconnect navigation were reviewed as neighboring concerns: this folder does not duplicate them or assume BMS pages survive a disconnect. SET-018 specifically requires BMS pages to stop while ordinary telemetry and the same connection continue.

## Remaining comparison and device work

Use [the shared comparison baseline](../comparison-baseline.md) for verified external references and limits. Existing [Aero settings coverage](../../aero-settings-coverage.md) and [EUC World reverse-engineering inventory](../../eucworld-aero-settings-re.md) provide useful repository evidence, but their prior external version snapshots were not freshly reverified by this worker.

Open physical acceptance work includes exact firmware-specific modes, transport acknowledgement versus actual actuation, alarm/tilt-back behavior and calibration, units/meaning for charge ceiling, full cell-bank identity, and stale-page behavior. These should be exercised incrementally on captured data or a secured stationary device. A missing app feature, prior failure, unverified protocol effect or untested device path is not marked fixed here.
