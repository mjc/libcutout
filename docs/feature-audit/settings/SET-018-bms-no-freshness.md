# SET-018: BMS pages can remain current-looking after their updates stop

- Severity: P1
- Evidence status: hypothesis needing device validation
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Keep ordinary telemetry flowing while BMS notifications stop. Previously cached cell/temperature pages remain available for the life of the connection without an age indicator.

## Expected behavior

Pack safety data needs an age/stale state independent of the faster ride telemetry stream.

## Evidence

CutoutSessionCore.swift:539 caches [BmsPageKey:BmsSnapshot]; lines 1416-1439 replace/aggregate on arrival but never timestamp/expire pages. CutoutMobile.swift:4623-4653 BmsSnapshot contains no observation time. BmsScreenViews.swift:21-79 renders pack content without freshness input; snapshots clear on disconnect via CutoutSessionCore.swift:1478-1484.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Track per-page receipt time in the existing session owner and expose stale/partial status in pack presentation.

## Acceptance and reproduction checks

Replay a normal stream, then stop only BMS pages while keeping speed notifications live. After a documented timeout, show last-updated age and stale state; resume and recover.

## Limits

Static inspection establishes absent page freshness in this path. The exact phone-visible persistence and suitable timeout require runtime/firmware validation.

