# RID-007: Follow mode changes zoom with route size and can cut off the route

- Severity: P2
- Evidence status: UX inconsistency
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Leave Follow enabled on an expanding ride. Each point recenters on the latest position using the span of the whole retained route, so local navigation progressively zooms out. Because that route-fit span is moved from the route midpoint to its endpoint, it does not actually fit the full route either.

## Evidence

lib.rs:11013-11015 derives camera bounds from projected route points. RideMapLiveContentView.swift:100-103 recenters on every final sequence; 131-143 calls mapRegion centeredOn the last point. RideMapCanvasView.swift:167-195 keeps the original route span while replacing its center.

- `crates/cutout-mobile-ffi/src/lib.rs:11013`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapLiveContentView.swift:100`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapLiveContentView.swift:118`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapCanvasView.swift:167`

## Expected behavior and smallest remedy

Separate Follow rider from Fit route. Follow should preserve user zoom or a stable navigation scale; Fit should use the Rust route center and extent.

## Acceptance / reproduction

Use a long straight route and a loop: follow remains useful near the rider, fit includes the whole chosen route, and panning exits follow predictably.

## Comparison and limitations

Competitor camera defaults unverified; this is an explicit navigation/fit distinction to decide.

Existing mapRegion test asserts span preservation but does not judge endpoint-centered route visibility. No MapKit device reproduction.

