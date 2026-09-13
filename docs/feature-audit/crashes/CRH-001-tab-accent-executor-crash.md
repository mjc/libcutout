# CRH-001: Connected tabs crash while resolving their accent color

- Severity: P1
- Evidence status: Phone-confirmed crash; matching current source path
- Status: OPEN — no current phone acceptance test
- Audited source: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`

## Observed behavior

The phone recorded nine SIGTRAP / EXC_BREAKPOINT incidents from September 6–13, including both September 13 crashes at 11:04:57 and 11:05:09. The triggered stack goes from `_dispatch_assert_queue_fail` through Swift executor isolation checks to `closure #2 in ContentView.tabAccent.getter`, then UIKit dynamic color resolution and asynchronous SwiftUI rendering.

## Expected behavior

Opening and rendering any connected tab must succeed regardless of the renderer queue, appearance or animation.

## Evidence and current source

`swift/CutoutMobile/Apps/CutoutApp/ContentView.swift:320-334` creates dynamic UIColor closures inside the View getter; `.tint(tabAccent)` applies them at line 169. The exact getter is named in all nine symbolicated reports. `PevDashboardComponents.swift:12-32` contains other dynamic providers worth checking as the same family, not separate proven crashes.

Reports (hashes and binary UUIDs in [phone evidence](../phone-evidence.md)):

- `CutoutApp-2026-09-06-200613.ips`
- `CutoutApp-2026-09-06-200834.ips`
- `CutoutApp-2026-09-06-200857.ips`
- `CutoutApp-2026-09-06-201005.ips`
- `CutoutApp-2026-09-09-153801.ips`
- `CutoutApp-2026-09-12-112108.ips`
- `CutoutApp-2026-09-12-112754.ips`
- `CutoutApp-2026-09-13-110457.ips`
- `CutoutApp-2026-09-13-110509.ips`

## Minimal remedy direction

Use an existing asset/system color or a provider whose isolation is valid for UIKit color resolution. Do not silence runtime concurrency checking. Inspect all provider closures for the same cause.

## Acceptance checks

Exercise EUC and VESC tab navigation, light/dark appearance and background/foreground on the physical phone; retain the tested binary UUID. Verify no queue assertion and preserve both color variants.

## Comparison and limits

Keeping the app responsive is a baseline expectation, independent of competitor design. The precise preceding tap is not recorded. Two latest reports share app binary UUID 0a80b415-90ec-3df2-9daa-f766e13e5571; current-source equivalence still requires build correlation.
