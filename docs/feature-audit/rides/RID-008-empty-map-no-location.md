# RID-008: Opening Map before recording cannot center on the phone

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Open Map before any ride or before a valid recorded point. Recenter does nothing even when the phone already has a current location, and the map has no user-location annotation.

## Evidence

RideMapPresentationState starts .automatic at RideMapRouteView.swift:9. RideMapLiveContentView.swift:119 requires displayPoints.last and cameraRegion. RideMapCanvasView.swift:260-351 renders route-derived annotations only. Phone location is separately available in CutoutAppModel.swift:692-694.

- `swift/CutoutMobile/Apps/CutoutApp/RideMapRouteView.swift:9`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapLiveContentView.swift:119`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapCanvasView.swift:260`
- `swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift:692`

## Expected behavior and smallest remedy

Use current authorized phone location for initial map positioning and Recenter when there is no route; distinguish unavailable location with the existing readiness state.

## Acceptance / reproduction

With no ride and a valid phone-location fixture, Map opens locally and Recenter works. With denied permission, show the reason instead of a silent no-op.

## Comparison and limitations

Competitor initial viewport unverified.

Exact MapKit automatic fallback viewport depends on OS/device and must be observed. The missing phone-location input and no-op guard are source-confirmed.

