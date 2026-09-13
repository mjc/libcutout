# SET-008: Begode LED mode is an unexplained number with an invented initial value

- Severity: P2
- Evidence status: UX inconsistency
- Audited revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`
- Verification: source tracing; no phone setting writes or runtime reproduction performed by this audit worker.

## Trigger and actual behavior

Open the LED mode picker. It shows 0 by default and offers unnamed choices 0 through 9 that write immediately.

## Expected behavior

Users should know each verified mode’s effect, and distinguish unknown current state from a draft. Unmapped modes should be clearly identified as raw diagnostics.

## Evidence

swift/CutoutMobile/Apps/CutoutApp/CutoutRouteViews.swift:1302-1323 initializes 0, formats each choice as only its number, and sends on selection. Lines 466-474 supply no lifecycle state for the capability row. Localizable.xcstrings:1051-1058 provides only the generic mode name and stationary note.

## Comparison

External competitor behavior was not independently verified for this record. Use [the shared comparison baseline](../comparison-baseline.md) and the repository's source inventory as leads, not as proof of current competitor behavior. The expected behavior above follows the control's own domain and observed data contract.

## Minimal remedy direction

Name device-verified modes; retain explicit raw labels for unmapped ones and an unknown initial state.

## Acceptance and reproduction checks

Compare every supported mode against its verified physical behavior; opening the picker must not imply mode 0 was observed. Confirm how unsupported firmware handles each number.

## Limits

No mode-to-effect mapping was verified in this audit, so no speculative names are supplied.

