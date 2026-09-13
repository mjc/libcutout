# RID-021: A new ride can inherit a panned-away camera with Follow still disabled

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Pan away during ride A, stop/save it, then start ride B elsewhere. The presentation object retains followsLatestPoint=false and the previous region, so new points do not recenter until the user presses Recenter.

## Evidence

RideMapPresentationState at RideMapRouteView.swift:8-15 stores follow/camera across rides. The live child receives the constant routeID "live" at 100. RideMapLiveContentView.swift:100-103 declines updates when follow is false, and startGpsOnlyRide changes recording/model state without resetting presentation.

- `swift/CutoutMobile/Apps/CutoutApp/RideMapRouteView.swift:8`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapRouteView.swift:100`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapLiveContentView.swift:100`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:1246`

## Expected behavior and smallest remedy

Key live camera reset to an actual new ride ID, preserving manual pan only within the same ride. Define a clear default for a new ride.

## Acceptance / reproduction

Pan ride A away, save, start B with a distant first point, and verify the initial B viewport. Returning to the same active ride should preserve the intended camera state.

## Comparison and limitations

Competitor camera persistence unverified.

Source confirms retained state and missing ride identity. Exact navigation lifetime depends on the entry route and needs UI testing.

