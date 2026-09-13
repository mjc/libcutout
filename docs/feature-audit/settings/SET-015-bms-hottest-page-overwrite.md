# SET-015: BMS hottest temperature can be replaced by a cooler page

- Severity: P1
- Evidence status: source-confirmed defect
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Receive temperature pages from more than one BMS area. The aggregate’s highest temperature is simply the last sorted page’s highest value; its sensor list also replaces prior readings.

## Expected behavior

Highest temperature should be the maximum among all current observed sensors, preserving their bank/page identity and freshness.

## Evidence

CutoutMobile.swift:4924-4926 uses `update.highestTemperature ?? highestTemperature` and replaces nonempty temperatureReadings. CutoutSessionCore.swift:1435-1439 sorts then merges pages. veteran_bms.rs:173-217 decodes six temperatures at selectors 3 and 7. Mobile FFI lib.rs:11575-11580 computes a page maximum before this merge.

Production ingress at crates/cutout-protocols/src/session.rs:1456-1477 forwards each temperature page separately. The BmsSnapshot initializer at swift/CutoutMobile/Sources/CutoutMobile/CutoutMobile.swift:4690-4693 does not recompute its maximum.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Merge sensors by stable identity and derive the aggregate maximum; avoid losing hotter earlier banks.

## Acceptance and reproduction checks

Feed selector 3 with a 60 C maximum and selector 7 with 30 C. Pack hottest must remain 60 C, regardless of notification order. Update the hot sensor and ensure the maximum can decrease when justified.

## Limits

No actual overtemperature was observed. The current merge is deterministically incorrect for this input.

