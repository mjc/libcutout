# Cutout feature audit

**83 individual issue documents**, based on source revision `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` and diagnostics retrieved directly from the phone on 2026-09-13. Four reviewers divided the app into feature areas and independently cross-checked high-impact findings.

All issues remain **open**. This is an audit and repair backlog: no app code or wheel settings were changed. Historical crashes remain relevant even where current source contains a plausible mitigation. Closing a crash requires evidence from the affected workflow on the current phone build.

[Live repair and separate-PR tracking](repair-status.md) records current-main rechecks and implementation progress.

## Start here

1. [Tab accent rendering crash](crashes/CRH-001-tab-accent-executor-crash.md): nine phone reports, including both latest crashes.
2. [Database bootstrap watchdog](crashes/CRH-004-database-map-watchdogs.md) and [active-route restoration watchdog](crashes/CRH-010-active-route-launch-projection.md): historical stacks match synchronous paths still present in this checkout.
3. [PWM tilt-back wording](settings/SET-001-pwt-headroom-label.md): the editor shows **20% unused margin**, while its encoder correctly sends **80% duty** on this setting’s basis. Labeling must make that distinction clear; do not blindly invert the encoder.
4. [BMS page collisions](settings/SET-013-veteran-bms-page-collisions.md), [cell spread](settings/SET-014-bms-spread-last-page.md) and [temperature maxima](settings/SET-015-bms-hottest-page-overwrite.md).
5. [Capture overwrite](rides/RID-013-capture-filename-collision.md), [manual reboot recovery](rides/RID-006-manual-recovery-clock.md) and [cross-vehicle attribution](rides/RID-025-cross-vehicle-telemetry.md).
6. Work through connection, individual Tune controls, Map/History, Lighting and Music using the inventories below. Each issue includes its own acceptance checks.

## What the phone establishes

The connected phone is an **iPhone 15 Pro Max, iOS 27.0 beta (24A5430a)**. Current capture metadata identifies a **NOSFET Aero**; the saved accessory profile is **MELK-OC21**. These are retrieved app records, not a new physical-protocol validation.

[Phone evidence](phone-evidence.md) catalogs **51 reports** with filenames, binary UUIDs and hashes: **27 terminations** and **24 nonfatal resource reports**. They are grouped into ten investigation records, not counted as 51 distinct bugs. The phone ride database is about **1.06 GiB**. The installed app says **1.0 (1)**, which does not establish its Git revision; every device acceptance run must identify its actual binary.

Raw diagnostics and two small captures remain outside Git at `/tmp/cutout-feature-audit-phone/`. They were copied read-only; no phone files were deleted or changed. That temporary directory can expire; the sanitized inventory is durable in this audit.

## How to read the findings

Severity orders remediation; evidence status describes what is known. **P1** covers crashes, data loss and misleading high-consequence controls; **P2** covers substantial functional or usability failures; **P3** covers lower-impact interaction issues. Severity does not turn a hypothesis into a proven failure.

- **Phone-confirmed:** the retrieved report establishes a termination or resource event. Current-build recurrence and exact triggering action may still need testing.
- **Source-confirmed defect:** production control flow or data transformation establishes the problem; this is not automatically a phone reproduction.
- **UX inconsistency:** concrete UI behavior conflicts with its data meaning or a stated expectation; choosing the remedy can require a product decision.
- **Hypothesis:** a credible path needs device, layout or SDK evidence before promotion to confirmed defect.

Each document covers a distinct problem/remedy, not every occurrence of shared code. Hundreds of control combinations do not justify hundreds of invented bugs. Coverage gaps and reviewed areas without findings remain listed so they cannot be mistaken for verified working features.

## Coverage and comparison

- [Wheel settings and BMS inventory](settings/coverage.md)
- [Connection and Lighting inventory](connection-lighting/coverage.md)
- [Rides, maps, history and capture inventory](rides/coverage.md)
- [Shared app and music inventory](app/coverage.md)
- [DarknessBot, EUC World and WheelLog comparison baseline](comparison-baseline.md)

Competitor references are primary published documentation/source, not a hands-on comparison of every current app build. Device/firmware-specific settings require their own semantic evidence. Aero lighting and MELK accessory lighting are separate systems. Music history is metadata only; ride-data replay must never imply replaying songs or recording audio.

Important scope limits: many Aero writes are exposed only in settings validation mode. VESC/Refloat tuning, built-in Aero ring/tail controls, camera and ride-data playback do not have complete end-to-end app surfaces in this checkout. Language completeness, physical RF behavior, actual lighting output and every device/firmware combination are not accepted as tested. These limitations are retained explicitly in coverage rather than silently omitted or converted into speculative defects.

## Checks performed

Source/caller/encoder/reducer tracing, existing-test inspection, independent cross-review of BMS, connection, capture/recovery and crash findings, and read-only phone report correlation. The cached same-project Nix development environment was used after the default cache failed; MCPLS was unavailable.

Existing `cargo test -p cutout-ride-maps --lib --offline`: **35 passed**, covering reducer transitions, sample admission, segments/distance, bounded projection and camera bounds. Those tests do **not** prove that the reported Swift integration defects or phone crashes are fixed. No new phone crash-reproduction or app deployment was performed. Markdown links, document IDs, source locations and report hashes were checked.

## Issue index

Priority counts: **19 P1**, **60 P2**, **4 P3**.

### Phone crashes and resource failures (10)

| ID | Priority | Issue | Evidence |
| --- | --- | --- | --- |
| [CRH-001](crashes/CRH-001-tab-accent-executor-crash.md) | P1 | Connected tabs crash while resolving their accent color | Phone-confirmed crash; matching current source path |
| [CRH-002](crashes/CRH-002-spotify-authorization-executor-crash.md) | P1 | Spotify authorization has crashed on an SDK callback queue | Phone-confirmed historical crash; current mitigation unverified on phone |
| [CRH-003](crashes/CRH-003-apple-music-launch-watchdog.md) | P1 | App launch can hang while eagerly initializing Apple Music | Phone-confirmed watchdog; matching eager initialization in source |
| [CRH-004](crashes/CRH-004-database-map-watchdogs.md) | P1 | Database integrity checking blocks app launch and has caused watchdog kills | phone-confirmed historical watchdogs; matching current synchronous path |
| [CRH-005](crashes/CRH-005-other-scene-watchdogs.md) | P1 | Other launch and scene watchdogs still need causal traces | Phone-confirmed historical watchdogs; root causes unresolved |
| [CRH-006](crashes/CRH-006-app-cpu-limit-kill.md) | P1 | The phone killed Cutout for exceeding its CPU budget | Phone-confirmed resource termination; hot-path ownership unresolved |
| [CRH-007](crashes/CRH-007-live-activity-cpu-excess.md) | P2 | Live Activity repeatedly consumes excessive CPU | Phone-confirmed repeated nonfatal resource reports |
| [CRH-008](crashes/CRH-008-recording-disk-write-volume.md) | P2 | Cutout produces unusually large disk-write resource reports | Phone-confirmed resource reports; producer attribution pending |
| [CRH-009](crashes/CRH-009-spotify-framework-launch-failure.md) | P1 | Some installed builds could not launch because Spotify was missing | Phone-confirmed historical launch failure; current artifact regression check open |
| [CRH-010](crashes/CRH-010-active-route-launch-projection.md) | P1 | Restoring an active ride synchronously projects its route during launch | phone-confirmed historical watchdog; matching current synchronous path |

### Wheel settings and BMS (22)

| ID | Priority | Issue | Evidence |
| --- | --- | --- | --- |
| [SET-001](settings/SET-001-pwt-headroom-label.md) | P1 | PWT shows headroom under a PWM threshold label | UX inconsistency |
| [SET-002](settings/SET-002-max-charge-value-format.md) | P2 | Max-charge editor hides its numeric value | source-confirmed defect |
| [SET-003](settings/SET-003-max-charge-unresolved-meaning.md) | P1 | Max-charge write offers a number with unresolved physical meaning | UX inconsistency |
| [SET-004](settings/SET-004-unknown-settings-defaults.md) | P2 | Unknown wheel settings look like concrete editable values | UX inconsistency |
| [SET-005](settings/SET-005-draft-never-reconciles.md) | P2 | Aero toggle and riding-mode drafts outlive rejected writes | source-confirmed defect |
| [SET-006](settings/SET-006-begode-max-speed-default.md) | P2 | Begode max-speed editor invents 30 km/h | UX inconsistency |
| [SET-007](settings/SET-007-begode-volume-default.md) | P2 | Begode beeper picker invents volume 5 | UX inconsistency |
| [SET-008](settings/SET-008-begode-led-mode-numbers.md) | P2 | Begode LED mode is an unexplained number with an invented initial value | UX inconsistency |
| [SET-009](settings/SET-009-settings-refusal-context.md) | P2 | Tune hides why a setting write was refused | UX inconsistency |
| [SET-010](settings/SET-010-tune-speed-units.md) | P2 | Tune uses km/h while Ride and readback use mph | UX inconsistency |
| [SET-011](settings/SET-011-alarm-readback-missing-picker-choice.md) | P2 | Speed-alarm picker cannot represent Off or PWM readback | source-confirmed defect |
| [SET-012](settings/SET-012-gyro-calibration-no-guidance.md) | P1 | Gyro calibration lacks a usable physical procedure | UX inconsistency |
| [SET-013](settings/SET-013-veteran-bms-page-collisions.md) | P1 | Veteran/NOSFET cell pages overwrite one another | source-confirmed defect |
| [SET-014](settings/SET-014-bms-spread-last-page.md) | P1 | BMS pack spread and lowest cell come from one page | source-confirmed defect |
| [SET-015](settings/SET-015-bms-hottest-page-overwrite.md) | P1 | BMS hottest temperature can be replaced by a cooler page | source-confirmed defect |
| [SET-016](settings/SET-016-lowest-cell-wrong-baseline.md) | P2 | Lowest-cell detail labels full spread as distance below average | source-confirmed defect |
| [SET-017](settings/SET-017-bms-sensor-count-wrong-source.md) | P2 | Unknown-topology screen drops reported temperature-sensor counts | source-confirmed defect |
| [SET-018](settings/SET-018-bms-no-freshness.md) | P1 | BMS pages can remain current-looking after their updates stop | hypothesis needing device validation |
| [SET-019](settings/SET-019-swift-pwt-range-drift.md) | P2 | Swift accepts PWT values Rust rejects | source-confirmed defect |
| [SET-020](settings/SET-020-high-beam-unlabeled.md) | P2 | Tune does not identify the Aero high-beam buttons | UX inconsistency |
| [SET-021](settings/SET-021-manual-headlight-status-static.md) | P2 | Headlight status says sent before a request and ignores failure | source-confirmed defect |
| [SET-022](settings/SET-022-first-readback-overwrites-edit.md) | P2 | Delayed first readback can overwrite an Aero edit | source-confirmed defect |

### Connection, navigation and accessory lighting (19)

| ID | Priority | Issue | Evidence |
| --- | --- | --- | --- |
| [CON-001](connection-lighting/CON-001-device-switch-stale-callbacks.md) | P1 | Switching devices can let the previous connection mutate the new session | source-confirmed defect |
| [CON-002](connection-lighting/CON-002-wheel-connect-no-deadline.md) | P2 | Wheel connection and GATT discovery can remain Connecting indefinitely | source-confirmed defect |
| [CON-003](connection-lighting/CON-003-transient-link-loss-discards-page.md) | P2 | A failed wheel reconnect discards the current Tune or Pack page | UX inconsistency |
| [CON-004](connection-lighting/CON-004-standalone-lighting-shell-changes.md) | P2 | Home Lighting changes navigation ownership with wheel selection | UX inconsistency |
| [CON-005](connection-lighting/CON-005-melk-discovery-no-deadline.md) | P2 | Lighting can become stuck discovering after the BLE connection succeeds | source-confirmed defect |
| [CON-006](connection-lighting/CON-006-evicted-lighting-candidates-remain-visible.md) | P2 | Some nearby accessory rows remain selectable after the core has forgotten them | source-confirmed defect |
| [CON-007](connection-lighting/CON-007-schedule-drafts-look-like-state.md) | P2 | Saved lighting timers reappear as disabled defaults | UX inconsistency |
| [CON-008](connection-lighting/CON-008-music-no-explicit-start.md) | P2 | Lighting Music has a Stop button but no clear way to start its displayed default | UX inconsistency |
| [CON-009](connection-lighting/CON-009-effect-previews-invent-colors.md) | P2 | Effect thumbnails show invented colors and geometry | UX inconsistency |
| [CON-010](connection-lighting/CON-010-browsing-effect-group-fakes-selection.md) | P2 | Browsing effect groups highlights a pattern that was never sent | source-confirmed defect |
| [CON-011](connection-lighting/CON-011-preset-speed-uses-inverse-raw-byte.md) | P3 | Preset summaries use the inverse native speed instead of the speed shown in controls | UX inconsistency |
| [CON-012](connection-lighting/CON-012-restore-confirmed-without-baseline.md) | P2 | Lighting says Confirmed while Restore has no usable baseline | UX inconsistency |
| [CON-013](connection-lighting/CON-013-prepair-restore-toggle-is-discarded.md) | P2 | Enabling Restore before pairing is silently undone | source-confirmed defect |
| [CON-014](connection-lighting/CON-014-lighting-has-no-disconnect.md) | P2 | A remembered lighting accessory cannot be temporarily disconnected | UX inconsistency |
| [CON-015](connection-lighting/CON-015-stop-music-leaves-microphone-command-on.md) | P2 | Stop Music never sends the protocol microphone-disable command | hypothesis needing device validation |
| [CON-016](connection-lighting/CON-016-solid-color-off-state-not-reasserted.md) | P2 | Changing solid color while off bypasses the off-preserving state plan | hypothesis needing device validation |
| [CON-017](connection-lighting/CON-017-metadata-save-errors-hidden.md) | P2 | Lighting alias and vehicle association can fail silently | source-confirmed defect |
| [CON-018](connection-lighting/CON-018-lighting-state-has-no-readback-label.md) | P2 | Initial Lighting controls look like observed device state | UX inconsistency |
| [CON-019](connection-lighting/CON-019-microphone-source-unclear.md) | P3 | Lighting Music does not explain which microphone it uses | UX inconsistency |

### Rides, maps, history and captures (25)

| ID | Priority | Issue | Evidence |
| --- | --- | --- | --- |
| [RID-001](rides/RID-001-gps-permission-status.md) | P2 | Map says recording is available after location permission is denied | source-confirmed defect |
| [RID-002](rides/RID-002-gps-keeps-running.md) | P2 | High-accuracy background GPS keeps running without a ride | source-confirmed defect |
| [RID-003](rides/RID-003-interrupted-stop-invalid.md) | P2 | Recovered interrupted ride offers a Stop button that cannot work | source-confirmed defect |
| [RID-004](rides/RID-004-reconnect-overrides-stop.md) | P2 | Reconnect starts a new ride before the stopped ride is saved or discarded | source-confirmed defect |
| [RID-005](rides/RID-005-disconnect-recording-continues.md) | P2 | Disconnect ends the ride activity but silently leaves map recording active | UX inconsistency |
| [RID-006](rides/RID-006-manual-recovery-clock.md) | P1 | Manual Resume after a phone reboot does not rebase the ride clock | source-confirmed defect |
| [RID-007](rides/RID-007-follow-camera-scale.md) | P2 | Follow mode changes zoom with route size and can cut off the route | UX inconsistency |
| [RID-008](rides/RID-008-empty-map-no-location.md) | P2 | Opening Map before recording cannot center on the phone | UX inconsistency |
| [RID-009](rides/RID-009-gps-only-speed-unavailable.md) | P2 | GPS-only rides show no live speed despite valid phone GPS speed | source-confirmed defect |
| [RID-010](rides/RID-010-telemetry-status-reset.md) | P2 | Fresh map telemetry status is reset to no telemetry on notifications | source-confirmed defect |
| [RID-011](rides/RID-011-automatic-ride-music-default.md) | P2 | Automatically started rides ignore the saved listening-history preference | source-confirmed defect |
| [RID-012](rides/RID-012-background-listening-history.md) | P2 | Listening history stops observing songs when the ride is backgrounded | UX inconsistency |
| [RID-013](rides/RID-013-capture-filename-collision.md) | P1 | Two capture starts in one second can overwrite the earlier capture | source-confirmed defect |
| [RID-014](rides/RID-014-stale-capture-finish.md) | P2 | An old capture's late finish can replace the new recording status | source-confirmed defect |
| [RID-015](rides/RID-015-discard-retains-data.md) | P2 | Discard hides the ride but keeps its route and listening metadata in storage | UX inconsistency |
| [RID-016](rides/RID-016-saved-ride-delete-unavailable.md) | P2 | Saved rides have no visible deletion action | UX inconsistency |
| [RID-017](rides/RID-017-share-ride-is-summary.md) | P2 | Share Ride sends only summary text, with no route or telemetry artifact | UX inconsistency |
| [RID-018](rides/RID-018-capture-file-unreachable.md) | P2 | Saved diagnostic captures have no in-app open, share or delete path | UX inconsistency |
| [RID-019](rides/RID-019-map-units-disagree.md) | P2 | Map mixes locale-derived distance and history speed with mph live speed | source-confirmed defect |
| [RID-020](rides/RID-020-lifecycle-main-thread-blocking.md) | P2 | Ride lifecycle buttons synchronously wait for the database worker | hypothesis needing device validation |
| [RID-021](rides/RID-021-new-ride-keeps-old-camera.md) | P2 | A new ride can inherit a panned-away camera with Follow still disabled | UX inconsistency |
| [RID-022](rides/RID-022-elapsed-excludes-pause.md) | P3 | The elapsed timer actually reports recording time excluding pauses | UX inconsistency |
| [RID-023](rides/RID-023-ride-backup-excluded.md) | P2 | Ride database is excluded from backup without a usable ride backup/export flow | UX inconsistency |
| [RID-024](rides/RID-024-storage-failure-no-retry.md) | P2 | A transient database-open failure disables recording until the process restarts | source-confirmed defect |
| [RID-025](rides/RID-025-cross-vehicle-telemetry.md) | P1 | Switching vehicles can attribute the new wheel's telemetry to the old ride | source-confirmed defect |

### Shared dashboard, music and app behavior (7)

| ID | Priority | Issue | Evidence |
| --- | --- | --- | --- |
| [APP-001](app/APP-001-sag-energy-placeholder.md) | P2 | Sag-adjusted energy is always an empty safety bar | UX inconsistency |
| [APP-002](app/APP-002-limp-home-range-no-producer.md) | P2 | Limp-home range is advertised but never populated by live telemetry | UX inconsistency |
| [APP-003](app/APP-003-music-command-failures-discarded.md) | P2 | Music buttons discard failed and refused command outcomes | source-confirmed defect |
| [APP-004](app/APP-004-spotify-command-unbounded-wait.md) | P2 | Spotify transport commands have no timeout or cancellation completion | hypothesis needing device validation |
| [APP-005](app/APP-005-live-activity-errors-voiceover-only.md) | P2 | Live Activity failures have no ordinary visible explanation | UX inconsistency |
| [APP-006](app/APP-006-visual-warnings-versus-app-alarms.md) | P2 | Riding warnings do not provide a configurable phone alarm system | UX inconsistency / capability gap |
| [APP-007](app/APP-007-music-controls-small-hit-targets.md) | P3 | Music transport button minimum sizes may produce cramped touch targets | hypothesis needing layout validation |

