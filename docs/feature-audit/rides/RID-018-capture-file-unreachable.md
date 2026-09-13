# RID-018: Saved diagnostic captures have no in-app open, share or delete path

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Finish recording an unknown device. The app announces a saved JSONL filename, but exposes no library or action to retrieve, share, inspect or delete that recording.

## Evidence

CutoutSessionCore.swift:2102 saves to Documents. CutoutAppModel.swift:3208-3210 retains only the filename after completion. DevicePickerViews.swift:69-74 displays status text. The app's only ShareLink is the map summary; Info.plist has no UIFileSharingEnabled or LSSupportsOpeningDocumentsInPlace keys.

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2102`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:3208`
- `swift/CutoutMobile/Apps/CutoutApp/DevicePickerViews.swift:69`
- `swift/CutoutMobile/Apps/CutoutApp/Info.plist:1`

## Expected behavior and smallest remedy

Retain the completed file URL and expose a direct share/open/delete action or a minimal capture list; preserve the existing local-only default.

## Acceptance / reproduction

Complete a capture on a phone and retrieve the actual file through a visible supported action, then remove it without deleting unrelated rides.

## Comparison and limitations

Competitor diagnostic workflow unverified; this follows the app's own 'saved capture' claim.

Developer device-container extraction may work, but is not a rider workflow. Files visibility needs confirmation against the final built Info.plist, which may have generated settings.

