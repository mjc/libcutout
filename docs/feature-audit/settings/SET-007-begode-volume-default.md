# SET-007: Begode beeper picker invents volume 5

- Severity: P2
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open Begode Tune. The beeper picker displays 5 even when no level has been read from the wheel; reopening the view resets its last target.

## Expected behavior

The picker should distinguish unknown device volume from a proposed or last submitted level.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1278-1299 initializes selectedVolume to 5 and only retains locally accepted requests. Lines 457-465 give the capability row nil lifecycle state. The UI neither reads volume nor labels the selection a proposal.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Use an unknown initial selection and retain an explicitly labeled last request only when useful.

## Acceptance and reproduction checks

Open with absent readback, choose a volume, simulate refusal, and reopen. Each state should make clear what is known about the device.

## Limits

The protocol may not offer readable confirmation; do not synthesize one to fix the presentation.

