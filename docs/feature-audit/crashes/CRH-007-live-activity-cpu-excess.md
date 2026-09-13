# CRH-007: Live Activity repeatedly consumes excessive CPU

- Severity: P2
- Evidence status: Phone-confirmed repeated nonfatal resource reports
- Status: OPEN — no current phone acceptance test
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Observed behavior

21 extension CPU reports span September 6–13. The newest records about 16 CPU seconds over 17 seconds (92% average), above the reported budget. Each inspected report says Action taken: none.

## Expected behavior

Lock-screen telemetry should update within budget without sustained CPU saturation.

## Evidence and current source

`swift/CutoutMobile/Apps/CutoutLiveActivityExtension/CutoutLiveActivityExtension.swift:15-119` renders lock-screen/Dynamic Island projections. `CutoutAppModel.swift:3158-3184` deduplicates and throttles updates but bypasses the normal throttle during reconnect. These are candidate measurement boundaries; the reports do not prove the throttle or any single view causes the excess.

Reports (hashes and binary UUIDs in [phone evidence](../phone-evidence.md)):

- `CutoutLiveActivityExtension.cpu_resource-2026-09-06-160716.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-06-161450.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-06-162112.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-06-162733.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-06-163656.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-06-183141.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-06-185746.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-06-190313.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-06-202143.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-08-190216.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-08-190722.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-08-194428.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-08-195248.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-12-120036.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-12-122526.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-12-123121.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-12-124006.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-12-124803.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-12-191819.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-12-193844.ips`
- `CutoutLiveActivityExtension.cpu_resource-2026-09-13-110618.ips`

## Minimal remedy direction

Profile the extension with a matching binary; compare unchanged, changing and reconnecting telemetry, collapse duplicate projection/render work only where measurements support it.

## Acceptance checks

Test locked phone and all Dynamic Island presentations through live, stale and reconnect states; retain extension CPU samples and ensure warning/freshness updates still arrive.

## Comparison and limits

Keeping the app responsive is a baseline expectation, independent of competitor design. These 21 reports are not 21 crashes and do not state that the extension was killed. Repeated historical and latest evidence warrants an open performance issue.
