# CRH-005: Other launch and scene watchdogs still need causal traces

- Severity: P1
- Evidence status: Phone-confirmed historical watchdogs; root causes unresolved
- Status: OPEN — no current phone acceptance test
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Observed behavior

Four additional scene watchdog reports remain outside the identified database and Apple Music families. Triggered samples include NSBundle.module initialization or main/system event processing. A sampled frame alone is not enough to assign a root cause.

## Expected behavior

Cold launch and scene transitions must consistently produce a responsive screen.

## Evidence and current source

`swift/CutoutMobile/Apps/CutoutApp/CutoutApp.swift:12-15` initializes shared state; `CutoutAppModel.swift:589-721` wires dependencies/startup. These are investigation locations, not demonstrated faulty lines. Inspect full report termination reasons and all threads before choosing a fix.

Reports (hashes and binary UUIDs in [phone evidence](../phone-evidence.md)):

- `CutoutApp-2026-09-07-181245.ips`
- `CutoutApp-2026-09-07-182940.ips`
- `CutoutApp-2026-09-08-185548.ips`
- `CutoutApp-2026-09-12-181951.ips`

## Minimal remedy direction

Keep these incidents open as a distinct unresolved triage queue. Correlate binary UUIDs with artifacts and add launch/scene timing evidence before splitting into further root-cause documents.

## Acceptance checks

Reproduce cold/warm launches, resume after lock and navigation while the app database is large. Inspect any new watchdog in full; verify resource-bundle access and dependency initialization without assuming the sampled frame is the cause.

## Comparison and limits

Keeping the app responsive is a baseline expectation, independent of competitor design. No claim that every launch watchdog shares the same cause; no current phone acceptance run.
