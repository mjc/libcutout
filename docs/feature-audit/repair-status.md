# Repair status

Goal: address all 83 audit findings in separate pull requests. Each fix starts from freshly fetched `origin/main`; initial repair baseline is `796a0b17fc516de816722bb65ca6db9d31145da9` (2026-09-13). Historical audit source references remain attached to the original audited revision.

A PR is not an accepted phone fix merely because it compiles. Keep implementation, review/test evidence, and physical-device acceptance separate. Do not close old crash cases on age or source appearance. No merges are requested by this goal.

Current-main delta: `796a0b17f` removes the integrity scan from normal database startup and exposes it as explicit worker maintenance. CRH-004 needs reassessment against this change; synchronous migration/recovery and phone launch acceptance remain to inspect. CRH-010 is a separate active-route restoration path.

| Finding | Work state | PR / commit | Remaining acceptance |
| --- | --- | --- | --- |
| [APP-001](app/APP-001-sag-energy-placeholder.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [APP-002](app/APP-002-limp-home-range-no-producer.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [APP-003](app/APP-003-music-command-failures-discarded.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [APP-004](app/APP-004-spotify-command-unbounded-wait.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [APP-005](app/APP-005-live-activity-errors-voiceover-only.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [APP-006](app/APP-006-visual-warnings-versus-app-alarms.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [APP-007](app/APP-007-music-controls-small-hit-targets.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-001](connection-lighting/CON-001-device-switch-stale-callbacks.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-002](connection-lighting/CON-002-wheel-connect-no-deadline.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-003](connection-lighting/CON-003-transient-link-loss-discards-page.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-004](connection-lighting/CON-004-standalone-lighting-shell-changes.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-005](connection-lighting/CON-005-melk-discovery-no-deadline.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-006](connection-lighting/CON-006-evicted-lighting-candidates-remain-visible.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-007](connection-lighting/CON-007-schedule-drafts-look-like-state.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-008](connection-lighting/CON-008-music-no-explicit-start.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-009](connection-lighting/CON-009-effect-previews-invent-colors.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-010](connection-lighting/CON-010-browsing-effect-group-fakes-selection.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-011](connection-lighting/CON-011-preset-speed-uses-inverse-raw-byte.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-012](connection-lighting/CON-012-restore-confirmed-without-baseline.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-013](connection-lighting/CON-013-prepair-restore-toggle-is-discarded.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-014](connection-lighting/CON-014-lighting-has-no-disconnect.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-015](connection-lighting/CON-015-stop-music-leaves-microphone-command-on.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-016](connection-lighting/CON-016-solid-color-off-state-not-reasserted.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-017](connection-lighting/CON-017-metadata-save-errors-hidden.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-018](connection-lighting/CON-018-lighting-state-has-no-readback-label.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CON-019](connection-lighting/CON-019-microphone-source-unclear.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CRH-001](crashes/CRH-001-tab-accent-executor-crash.md) | Implementing from current main | — | Finding-specific checks and device evidence where required | 
| [CRH-002](crashes/CRH-002-spotify-authorization-executor-crash.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CRH-003](crashes/CRH-003-apple-music-launch-watchdog.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CRH-004](crashes/CRH-004-database-map-watchdogs.md) | Current-main mitigation; reassess | — | Finding-specific checks and device evidence where required | 
| [CRH-005](crashes/CRH-005-other-scene-watchdogs.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CRH-006](crashes/CRH-006-app-cpu-limit-kill.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CRH-007](crashes/CRH-007-live-activity-cpu-excess.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CRH-008](crashes/CRH-008-recording-disk-write-volume.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CRH-009](crashes/CRH-009-spotify-framework-launch-failure.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [CRH-010](crashes/CRH-010-active-route-launch-projection.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-001](rides/RID-001-gps-permission-status.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-002](rides/RID-002-gps-keeps-running.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-003](rides/RID-003-interrupted-stop-invalid.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-004](rides/RID-004-reconnect-overrides-stop.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-005](rides/RID-005-disconnect-recording-continues.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-006](rides/RID-006-manual-recovery-clock.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-007](rides/RID-007-follow-camera-scale.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-008](rides/RID-008-empty-map-no-location.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-009](rides/RID-009-gps-only-speed-unavailable.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-010](rides/RID-010-telemetry-status-reset.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-011](rides/RID-011-automatic-ride-music-default.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-012](rides/RID-012-background-listening-history.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-013](rides/RID-013-capture-filename-collision.md) | Implementing from current main | — | Finding-specific checks and device evidence where required | 
| [RID-014](rides/RID-014-stale-capture-finish.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-015](rides/RID-015-discard-retains-data.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-016](rides/RID-016-saved-ride-delete-unavailable.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-017](rides/RID-017-share-ride-is-summary.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-018](rides/RID-018-capture-file-unreachable.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-019](rides/RID-019-map-units-disagree.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-020](rides/RID-020-lifecycle-main-thread-blocking.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-021](rides/RID-021-new-ride-keeps-old-camera.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-022](rides/RID-022-elapsed-excludes-pause.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-023](rides/RID-023-ride-backup-excluded.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-024](rides/RID-024-storage-failure-no-retry.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [RID-025](rides/RID-025-cross-vehicle-telemetry.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-001](settings/SET-001-pwt-headroom-label.md) | Implementing from current main | — | Finding-specific checks and device evidence where required | 
| [SET-002](settings/SET-002-max-charge-value-format.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-003](settings/SET-003-max-charge-unresolved-meaning.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-004](settings/SET-004-unknown-settings-defaults.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-005](settings/SET-005-draft-never-reconciles.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-006](settings/SET-006-begode-max-speed-default.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-007](settings/SET-007-begode-volume-default.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-008](settings/SET-008-begode-led-mode-numbers.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-009](settings/SET-009-settings-refusal-context.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-010](settings/SET-010-tune-speed-units.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-011](settings/SET-011-alarm-readback-missing-picker-choice.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-012](settings/SET-012-gyro-calibration-no-guidance.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-013](settings/SET-013-veteran-bms-page-collisions.md) | Implementing from current main | — | Finding-specific checks and device evidence where required | 
| [SET-014](settings/SET-014-bms-spread-last-page.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-015](settings/SET-015-bms-hottest-page-overwrite.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-016](settings/SET-016-lowest-cell-wrong-baseline.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-017](settings/SET-017-bms-sensor-count-wrong-source.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-018](settings/SET-018-bms-no-freshness.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-019](settings/SET-019-swift-pwt-range-drift.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-020](settings/SET-020-high-beam-unlabeled.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-021](settings/SET-021-manual-headlight-status-static.md) | Not started | — | Finding-specific checks and device evidence where required | 
| [SET-022](settings/SET-022-first-readback-overwrites-edit.md) | Not started | — | Finding-specific checks and device evidence where required | 
