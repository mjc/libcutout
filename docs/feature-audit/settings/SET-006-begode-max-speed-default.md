# SET-006: Begode max-speed editor invents 30 km/h

- Severity: P2
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open the Begode max-speed control for a connected supported wheel. It begins at 30 km/h without reading a wheel value; a single plus/minus action immediately submits 31/29.

## Expected behavior

Show that the existing maximum is unknown and that the user is choosing a write-only target, not nudging a reported setting.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1254-1275 initializes selectedSpeed to 30 and sends on each step. Lines 448-456 pass nil setting state and nil confirmation time for this capability. CutoutAppModel.swift:2291-2294 merely submits the typed command.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Require an explicit target for write-only speed configuration and label last submitted value as unconfirmed.

## Acceptance and reproduction checks

Open when the wheel’s actual setting differs from 30; first interaction must not masquerade as relative adjustment from that actual value. Check refusal and re-entry.

## Limits

No decoded max-speed readback is asserted. Immediate submission is real; physical effect remains device-dependent.

