# Production UI inventory

`ContentView` owns root routing. `PevAppShell` owns the shared connected
header and the single bottom navigation through `safeAreaInset`. Device
picker, capture, EUC Ride, VESC Ride, VESC Debug, and BMS screens are the
production view entry points.

Map, Tune, and Logs are intentionally unavailable tabs. They do not render a
placeholder or fixture screen and have no production route action. Device
picker is owned only by `ContentView`; connected screens are owned only by
`PevScreenContainer`.

The production UI uses Rust/FFI-backed session state for live telemetry,
readbacks, identity, GPS speed, capture status, and unavailable states. The
live catalog contains route metadata only; it has no demo values, preview
providers, fixture snapshots, or placeholder screens.

Layout ownership is tested by `DashboardLayoutContractTests` across iPhone and
iPad viewport geometry, Dynamic Type sizes, safe-area anchoring, and
portrait/landscape matrices. Accessibility identifiers cover the root picker,
connected shell, disconnect, navigation targets, and picker actions.

Swift app/source lines excluding generated FFI:
   before: 13831 (tracked HEAD)
   after:  13093
   delta:   -738

Fake-path scan (production Swift excluding generated FFI): clean except the
Rust DTO's `manualPlaceholder` compatibility case, which is mapped to the
explicit manual picker entry and is not a rendered demo or placeholder screen.

## Deleted or collapsed paths

- Deleted `Apps/CutoutApp/PlaceholderScreenViews.swift` and
  `PevPlaceholderScreenView`.
- Removed unreachable route cases: `eucMap`, `eucTune`, `vescMap`, `vescLogs`,
  and `liveActivity`.
- Removed the catalog-only display models `PevMetric`, `PevDeviceCard`,
  `PevSummaryRow`, and `PevFaultCard`; visible values now come from the live
  session/readback state or an explicit unavailable state.
- Collapsed the old manual-picker compatibility projection into the explicit
  `manualEntry` state. The generated Rust DTO case remains only as an FFI
  compatibility boundary.
- Removed duplicate per-screen navigation and the former fixture/demo catalog
  data. Map, Tune, and Logs remain disabled tab metadata with no substitute
  screen or route action.

## Production ownership map

| Surface | Single production owner |
| --- | --- |
| Root route and device selection | `ContentView` |
| Device picker | `DevicePickerView` |
| Capture | `CaptureRecordingScreen` |
| Shared top/bottom connected chrome | `PevAppShell` in `PevDashboardScaffoldViews.swift` |
| EUC Ride | `PevScreenContainer` → `EucRideScreenView` |
| EUC Pack/BMS and garage | `PevScreenContainer` → `BmsScreenView` / `EucGarageScreenView` |
| VESC/Refloat Ride | `PevScreenContainer` → `VescRideScreenView` |
| VESC/Refloat Debug Ride | `PevScreenContainer` → `VescDebugScreenView` |
| Layout contract | `DashboardLayout` and `DashboardLayoutContractTests` |

## Verification matrix

- Model-level safe-area/layout contracts cover small, standard, and large
  iPhone portrait; iPhone landscape; and iPad portrait/landscape, plus default,
  XL, and accessibility Dynamic Type invariance.
- The signed Xcode app build and picker UI tests were run on iPhone 17 Pro and
  iPad Pro 11-inch (M5), iOS 27. The focused picker frame tests passed
  independently on the iPad Pro after the current test-bundle rebuild; prior
  direct full runs passed 3 tests with 1 explicit connected-hardware skip and
  0 failures.
- Connected Ride/BMS/Debug route geometry remains a physical-device gate: the
  test activates only when a real EUC or VESC is attached and otherwise skips
  explicitly. No simulator fixture or fake session was added to claim that
  evidence.
