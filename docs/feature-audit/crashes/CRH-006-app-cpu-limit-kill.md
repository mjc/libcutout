# CRH-006: The phone killed Cutout for exceeding its CPU budget

- Severity: P1
- Evidence status: Phone-confirmed resource termination; hot-path ownership unresolved
- Status: OPEN — no current phone acceptance test
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Observed behavior

The September 12 14:39:06 cpu_resource_fatal report explicitly records Process killed: 48 seconds CPU time over about 58 seconds, 82% average CPU. This is a real process termination, unlike the nonfatal extension warnings.

## Expected behavior

Normal recording/navigation/background work must remain within the phone CPU budget.

## Evidence and current source

The report contains microstackshots across SwiftUI and application work. Current investigation sites include app state publication in `CutoutAppModel.swift:657-717`, live projection at `:2140-2198` and Live Activity reconciliation at `:3158-3184`. These locations are hypotheses until samples are symbolicated against the report binary UUID.

Reports (hashes and binary UUIDs in [phone evidence](../phone-evidence.md)):

- `CutoutApp.cpu_resource_fatal-2026-09-12-143906.ips`

## Minimal remedy direction

Symbolicate the existing samples, measure the hot path, and reduce repeated work at its producer. Preserve bounded recording and necessary warning updates.

## Acceptance checks

Run sustained realistic telemetry plus maps and Live Activity on the phone; record CPU over the relevant background interval and verify no resource kill. Compare with the same recorded input and database scale.

## Comparison and limits

Keeping the app responsive is a baseline expectation, independent of competitor design. CPU budget termination and UI watchdog termination are separate observed mechanisms; temporal proximity does not prove one common cause.
