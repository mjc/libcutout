# RID-001: Map says recording is available after location permission is denied

- Severity: P2
- Evidence status: source-confirmed defect
- Baseline: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` (2026-09-13)

## Rider trigger and actual behavior

Launch with Location denied or disable Location Services, then start a GPS-only ride. The lifecycle becomes active and its timer runs; the map never publishes the matching permission or services-disabled warning.

## Evidence

CutoutSessionCore.swift:2017-2021 publishes `.ready` solely from ride database initialization. Its CLLocationManager delegate at 3192-3205 ignores denied/restricted and never publishes authorization status. RideMapLiveContentView.swift:153-190 already has warning UI for the missing states.

- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:2017`
- `swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift:3192`
- `swift/CutoutMobile/Apps/CutoutApp/RideMapLiveContentView.swift:153`

## Expected behavior and smallest remedy

Publish actual authorization, services and transient location failure state through the existing availability callback. Keep lifecycle and GPS readiness separate.

## Acceptance / reproduction

With denied, restricted, not-determined and services-disabled fixtures, show a specific reason and recovery action; a valid ride database must not imply working GPS.

## Comparison and limitations

WheelLog's official Android strings expose distinct GPS-permission and logging controls; see ../comparison-baseline.md. Cross-platform wording need not match.

The missing callback is verified in source. No permission flow was exercised on a phone.

