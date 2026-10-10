# SwiftUI audit repair evidence, 2026-10-07

Continuation of [LIBCU-DOC-12](https://lific.mjc.lol/LIBCU/pages/129) and
[LIBCU-PLAN-24](https://lific.mjc.lol/LIBCU/plans/117). This file retains current
implementation and verification evidence. The current working-tree inventory is
[LIBCU-DOC-16](https://lific.mjc.lol/LIBCU/pages/158), mirrored in
[the path inventory](working-tree-inventory-2026-10-09.md).
[The completion report](working-tree-completion-2026-10-09.md) records combined
source and gate status. Lific is available. Historical results below remain tied
to their exact checkout and run; they do not establish acceptance of the current
combined source.

## Constraints

- Ride has no scrolling. Speed, safety headroom, battery/pack metrics, connection
  state, and navigation remain visible as telemetry changes.
- Recording uses a dot in the Ride header. Connection/retry state belongs in its
  status pill. Missing telemetry must not insert a warning panel or move rows.
- The music player works without a ride and across primary navigation. Playback
  state belongs to the app-retained music model. Provider commands remain explicit
  user actions. See [the music contract](music-integration.md).
- SwiftUI accessibility must retain typed labels, values, and units even when
  the compact visual presentation omits a secondary metric.
- Verification uses native ARM64 package tests and the ARM64 iOS simulator through
  the repository's shared FFI ensure boundary. Physical-device deployment is a
  separate action.

## Changes and regression coverage

| Issue | Failure | Change | Required evidence |
| --- | --- | --- | --- |
| LIBCU-873 | A reload that returned early for unavailable storage left an older initial/page query admissible. Its late result could overwrite the storage error or rows. | Cancel and clear both query tasks and advance query generation before checking storage availability. | Gated initial-page and cursor-page regressions release obsolete work after a storage failure and verify retained error/rows/selection, then retry. |
| LIBCU-874 | Returning to History refreshed its first page and replaced a ride selected from a later page. | History entry explicitly reloads the current selection. The existing selected-ride lookup reinserts it when absent from page one. Filter changes still use ordinary reload. | Select a second-page ride, call the same reload entry point used by the view, and verify selected ID, route, detail, and filter survive. |
| LIBCU-875 | The Live Activity hid temperature and beep chips at accessibility sizes without retaining their speech. Temperature is also absent from its grid. | Footer accessibility representation includes typed headroom, beep, and temperature values independently of visual chip visibility. | Simulator accessibility hierarchy at the largest text size, including the compact fallback: each safety metric once, critical headroom first. |
| LIBCU-876 | Brightness had a percentage but no purpose. Repeated Tune Apply/Off controls and procedure steps lacked setting context. | Label Brightness; name the setting for Apply/Off; include draft value and current action step. | Package assertions cover prepare/busy/start/restart/invoke action speech; simulator verifies slider and actual form controls. |
| LIBCU-868 | Fixed grid slots surrounded metric cards whose backgrounds remained at intrinsic content height. | Ride tiles accept the proposed height before drawing their background. Units remain on one line. | Normal and accessibility-size screenshots plus metric frames in portrait/landscape. |
| LIBCU-869 | Screen-wide text caps and fixed PWM/charge/range slots prevented Ride from honoring requested accessibility sizes. Actual Large-to-AX5 tests reproduced unchanged PWM height. | Use scalable essential readings on the fixed Ride surface at accessibility sizes; put secondary values in a 44pt Readings sheet with typed speech. Preserve the requested category without a screen-wide clamp. | Retained actual portrait/landscape, music, category/frame, type-growth and available secondary-reading cases pass; exact receipts and the separate actual contrast/Reduce Motion case appear below. |
| LIBCU-803 / LIBCU-872 | The compact player was attached only to connected Ride views and disappeared on picker, Map, and More. | Attach it once to shared primary navigation using the native tab accessory. Its single-row controls fit the 48-point system slot, with 44-point targets. Full metadata remains in the expansion button value and unrestricted detail sheet; a DEBUG monitor fixture drives the production model, command admission, and snapshot ingestion. | Playing, paused, and recovery UI tests traverse primary routes, check player/tab/control frames, open/close details, and exercise play/pause. |
| Startup routing follow-up | A fixture already connected before ContentView mounted stayed on Devices because the connection observer handled only subsequent changes. | Apply the existing connection navigation intent initially as well. It opens Ride only from the picker and preserves an explicitly selected Map or More route. | The nominal Live Activity fixture must open Ride and reach its lock-screen assertions. |
| LIBCU-803 | Activation during asynchronous database bootstrap was discarded while the model was nil. Startup later used a captured inactive environment phase and suspended music indefinitely. | Record the current scene phase in shared SwiftUI state, including the initial phase, and read it after bootstrap. | All three simulator fixtures previously resolved enabled/visible but logged `start active=false` and never started their monitor. Fresh-launch player tests must pass after the handoff repair. |
| Follow-up runtime finding | Core Location global service checks ran on the main actor during telemetry publication and demand updates. Xcode reported potential UI unresponsiveness. | The location adapter refreshes a cached service result off-main at startup and authorization changes. Pending remains checking; disabled services remain distinct from denied app permission. Demand application does not publish recursively. Permission loss stops updates, while an unchanged authorized refresh preserves acquisition. | Gated worker-query, pending demand, stop/recovery, no recursive publication, and no restart regressions in `PhoneLocationAdapterTests`. |
| LIBCU-802 | Pending/rejected GPS samples inserted an orange diagnostic row; the next accepted sample removed it. Initial History detail also showed unavailable content before its projection arrived. | Remove sample decisions from the primary Map presentation API; retain persistent interruption/omission/error surfaces. Render initial detail loading in the eventual map slot without drawing another ride's projection. | Projection/loading/error policy regressions and rendered notice geometry at normal/accessibility sizes. Storage error publication still reaches the live Map error surface. |

Follow-up for LIBCU-802: a direct History Detail destination whose requested ride
was absent from the loaded page could still render unavailable content before its
`.task` began the requested-ID query. The detail view now tracks which selection
task ID has started. It renders loading during that initial gap, then preserves
the missing-ride result after lookup completion and keeps real errors visible.
`testHistoryDetailShowsLoadingBeforeTheInitialSelectionTaskStarts` covers both
sides of that transition.

Ride accounts for the bottom safe-area inset and the native accessory’s measured global frame when proposing its non-scrolling layout height. The floating accessory did not appear in the GeometryProxy bottom inset (34 points on the test device), so safe-area accounting alone still covered the bottom cards. The shared player modifier publishes its outer occupied frame through an environment value and clears it when hidden. Ride ends at the earlier boundary. System tab and music accessory heights are not hardcoded. Edge-to-edge page colors are backgrounds rather than layout siblings.

The compact music title intentionally ellipsizes long metadata. Its clipping audit exception is restricted to `music.now-playing-title`; all other nodes remain audited. The test verifies the complete title and artist in the expansion control and detail sheet. The recovery fixture advertises unavailable transport commands, matching the recovery state being exercised.

The selection repair refreshes the first page and preserves the selected ride.
It does not cache every previously loaded page. The music fixture supplies metadata
and provider results only; it does not contact a music service or play audio.

## Current source and acceptance plan, 2026-10-09

The complete working-tree inventory records every baseline and added path;
shared native files have more than one semantic owner. Its final manifest owns
the path count, which changes as related fixes add source or proof files. All
accumulated paths are related project work. Source
review used sem and the requested frontend-design, SwiftUI expert, UI patterns
and view refactor guides. No phone, USB discovery, Android or computer-use
automation belongs to this gate.

| Scope | Current implementation | Actual proof and remaining gate |
| --- | --- | --- |
| 803 compact music | Empty startup/provider preference produces no compact player. Missing artwork and artist produce no placeholder. Retained meaningful items preserve recovery access and Rust-admitted command capabilities; explicit More access remains available without a snapshot. | Behavioral RED in `swift-package-20261009T175509-99322.log` reproduced empty-player/artwork/artist/history geometry. `swift-package-20261009T181717-16285.log` reproduced empty Spotify recovery capability/inset. Playing/paused route integration, empty-player reopen, settings restoration and cold relaunch passed the retained actual Simulator cases. Provider hardware/network acceptance remains separate. |
| 803 listening-history presentation | Empty, disabled, deleted, missing and redacted sections are omitted. Actual events, persistence errors and meaningful delete actions remain visible. | Rendered geometry regressions were observed in the 17:55 RED. Music ordering, privacy, exact SQL policy and capture receipts also have separate Rust/native tests; those semantics do not move into SwiftUI. |
| 868/869 fixed Ride | At accessibility sizes, speed and status use semantic fonts; primary battery/voltage and scalable headroom stay visible. Secondary EUC/VESC readings, GPS, charging/range and footpad values remain available through the accessible Readings sheet. Standard geometry stays fixed and Ride has no ScrollView. | Actual ARM64 Simulator RED `run.nUGjTK/Result.xcresult`: EUC PWM height was 20.3333pt at both Large and AX5; VESC was 37.3333pt at both. Both growth tests failed despite verified UIKit/SwiftUI category changes. Retained actual normal/AX5/landscape cases pass, including available EUC/VESC secondary values, target sizes, fixed Ride geometry and both type-growth cases. Separate contrast/Reduce Motion acceptance passes in `run.SjiTRL`. |
| 872 navigation | One native TabView owns Ride/Map/Tune/More. Camera, lighting/RGB and pack remain under More with typed nested navigation. Disconnect appears only on connected Ride. | Tune alarm Back and Lighting text-field focus/typing/submission passed in the same four-test RED run. Those source hypotheses did not justify production fixes. Retained affected reruns pass Tune Back, Lighting focus/typing/submission, typed Camera Back, primary-tab roles and destinations. The actual preference/navigation case also passes in `run.SjiTRL`. |
| 802/873/874 Map/history | Storage checks invalidate obsolete history work. Reentry refreshes the first page while retaining an out-of-page selected UUID. Clear occupies a stable filter strip. Empty/loading/error presentation preserves the route viewport. | Gated model regressions cover obsolete work, later-page selection and deleted selection. The actual 51-route pagination/reentry/filter/reset/relaunch scenario passes in `run.mPXrPS` in 102.164 seconds, with exact UUID/projection/generation fences and unchanged filter/viewport geometry. |
| 875/876 secondary speech and controls | Live Activity accessibility retains headroom, beep and temperature values once each; Tune controls name their setting, value and action, and omit false readback rows. | Connected Lighting passes the enabled native Brightness gesture and exact Rust-write oracle in `run.PPcgYT`. Current AX1–AX4 unchanged-branch pairs and the latest AX5 pair in `run.movDXx` have original-image, full-warning Vision OCR, native geometry, exact-once/order and lifecycle proof for all 20 state/surface combinations. The preceding AX5 strict shared-body failure remains retained below. All43 finalized UIKit runtime cases also pass. Physical Live Activity acceptance remains separate. |
| 891/892 native bridge | Rust owns admission, identity, lifecycle and durable receipts. Swift owns bounded native execution and replaceable presentation delivery. Source-clock capture precedes backpressure; async Rust queries preserve generation and weak-owner fences. | Real SQLite/64-owned/65th-callback and Main/lifetime regressions are covered by the central native suite. The latest AX5-only wordmark source passes 1,084 Swift cases with zero failures in `swift-package-20261010T013055-89416.log` and its complete UI pair. The final supported ARM64 app/extension build also passes, with retained current binaries in `ios-app-final-binary-receipt.json`. All43 finalized UIKit runtime cases pass: locked25, music11 and capture7, with zero failures/skips. The earlier interrupted collector-hung invocation remains diagnostic only. |

The music diagnostic repair keeps timeline readback ownership separate from
successful explicit deletion. Ordinary history adoption/readback retains the
exact incomplete-capture or terminal-history receipt; a late receipt after
successful deletion cannot restore deleted presentation metadata. The ordered
Rust lease remains held through the awaited native capture receipt and history
readback, and the model is acquired weakly only after those awaits. The held
capture/readback regression and combined native gate pass in the retained
1,084-case Swift receipt above. Physical provider behavior remains separate.

### Saved-history pagination fixture

The opt-in fixture is DEBUG and Simulator-only. It uses canonical Rust APIs to
create `historyPageLimit + 1` saved routes, currently 51, with three accepted
points each. It uses coherent recent wall and monotonic clocks, validates Saved
state, and retains only the oldest/newest UUID receipts for deterministic
relaunch reuse. Existing rides are neither deleted nor transitioned. Two retained
receipts do not mean a two-ride database or first-page-only coverage.

The passing UI case chooses All time, proves that the oldest seed is absent from
page one, activates actual Load More, then opens that exact UUID and waits for its
settled route projection. More-to-Map reentry must complete a newer query with the
same selection/projection and All time filter. Clear returns Last 30 days while
the filter and viewport x/y/width/height stay within two points. Relaunch validates
the same seed receipts. DEBUG readback reports the actual fixture ID, query
generation, loading state, selected UUID and projected UUID on existing visible
containers; it does not fabricate selection or geometry. Saved-ride deletion has
no UI action in this surface, so its invariants remain model-test evidence.

### Pending ARM64 Simulator matrix

The current plan has 25 base selectors plus one separate actual OS preference
selector, 26 total. Every selector is in
`CutoutAppUITests/CutoutAppUITests`. The parent runs the supported repository
runner on ARM64 iPhone 18 Pro with the shared FFI ensure boundary. Cases read back
the actual requested UIKit/SwiftUI category and wait for settled window,
orientation and screenshot aspect before checking frames. Old October 8
landscape screenshots with a portrait canvas, black upper region and clipped
lower content remain excluded from visual acceptance.

Base selectors, 25:

- `testEucPwmReadoutHonorsRequestedAccessibilityTextSize`
- `testVescPwmReadoutHonorsRequestedAccessibilityTextSize`
- `testEucEssentialRideControlsRemainVisibleWithoutScrolling`
- `testVescEssentialRideControlsRemainVisibleWithoutScrolling`
- `testEucEssentialRideControlsRemainVisibleWithoutScrollingAtAccessibilityDynamicType`
- `testVescEssentialRideControlsRemainVisibleWithoutScrollingAtAccessibilityDynamicType`
- `testEucEssentialRideControlsRemainVisibleWithoutScrollingInLandscape`
- `testVescEssentialRideControlsRemainVisibleWithoutScrollingInLandscape`
- `testEucEssentialRideControlsRemainVisibleWithoutScrollingInLandscapeAtExtraExtraExtraLargeType`
- `testVescEssentialRideControlsRemainVisibleWithoutScrollingInLandscapeAtExtraExtraExtraLargeType`
- `testEucEssentialRideControlsRemainVisibleWithoutScrollingInLandscapeAtAccessibilityDynamicType`
- `testVescEssentialRideControlsRemainVisibleWithoutScrollingInLandscapeAtAccessibilityDynamicType`
- `testMusicPlayerPausedAcrossEucRideMapMoreAtAccessibilityDynamicType`
- `testMusicPlayerPlayingAcrossPickerMapMore`
- `testMoreMusicAppleOpensWithoutSnapshotAfterRelaunch`
- `testMoreMusicSpotifyOpensWithoutSnapshotAfterRelaunch`
- `testMusicPlayerCanReopenFromMoreAfterHiding`
- `testPickerSurfaceHomeMapRouteKeepsMapAndLifecycleActionsReachable`
- `testEucMoreKeepsMapOnTabBar`
- `testPickerSurfaceCameraTabReturnsToDevicesWithoutConnecting`
- `testPickerSurfaceSavedHistoryPreservesSelectionAndClearsFiltersWithoutReflow`
- `testEucTuneAlarmSettingsHasAReachableBackAction`
- `testEucLightingPresetNameKeepsKeyboardFocusUntilSubmission`
- `testEucAccessibleReadingsPreserveAvailableSecondaryValuesAtAccessibilityDynamicType`
- `testVescAccessibleReadingsPreserveAvailableSecondaryValuesAtAccessibilityDynamicType`

Separate OS preference selector, 1:

- `testEucPrimaryRoutesRemainUsableWithReduceMotionAndIncreasedContrastAtAccessibilityDynamicType`

The separate invocation enables Increase Contrast through the runner and verifies
and restores it. The test changes the real Settings Reduce Motion switch through
XCTest, restores its prior value in `defer`, and requires matching UIKit/SwiftUI
readbacks before exercising AX5 Ride/readings and primary/nested navigation. If
Settings cannot be reached, record the actual failure as an acceptance boundary;
an injected environment value is not a substitute. No result for these 24 cases
is claimed until the actual combined-source run completes. Real Spotify
authorization/audio, accessory commands, sustained locked riding, physical
MapKit/Lock Screen behavior and deployment remain separate proof.

## Historical verification log, 2026-10-07 through 2026-10-08

- Initial ARM64 package run:
  `target/test-logs/swift-package-20261007T130505-64625.log`.
  Three new History regressions failed because their default state shared a
  persistent database with other tests. A later attempt at separate database
  paths exposed the process-wide single-database contract. The final fixtures
  implement `RideHistoryQuerying` in memory, gate queries explicitly, and return
  nonempty, distinct route projections. They do not change database ownership.
  These intermediate runs are not passing gates.
- ARM64 package gate passed in
  `target/test-logs/swift-package-20261008T042009-425.log`: 559 mobile tests
  (one skipped), 5 widget tests, and 393 app tests; zero failures. This includes
  all three History regressions, all three off-main location-adapter regressions,
  and the Map and Tune presentation assertions. The Map suite includes two new
  native accessory viewport calculations.
- The final viewport source and UI-test query passed the ARM64 package task in
  `target/test-logs/swift-package-20261008T044319-12982.log`: 559 mobile tests
  (one skipped), 5 widget tests, and 393 app tests; zero failures. Five app
  layout tests and 30 Map presentation tests passed. The focused accessibility
  simulator run `run.Meyzf6/Result.xcresult` passed 1/1 on iPhone 18 Pro, iOS
  27.0, arm64. It scrolls each available live Map action above the player and
  verifies the visible frame and hit target. The preceding `run.MmpBhr` failed
  because the test queried a nested ScrollView inside the element that was
  already the ScrollView; changing the query to `app.scrollViews["ride-map.screen"]`
  resolved it. The successful run still emitted Xcode QoS priority-inversion
  runtime warnings; they are not test failures and remain uninvestigated.
- The playing-state music test `run.hv1Tzf/Result.xcresult` passed 1/1 on the
  same arm64 simulator. It covers playback across picker, Map, and More;
  pause/play, next/previous, expanded metadata, and hide/restore. The provider
  remains a deterministic fixture, so this proves presentation and command
  admission, not real-player acceptance.
- The direct-Detail pre-task regression was reproduced red in
  `target/test-logs/swift-package-20261008T045113-16625.log`; the assertion
  failed while all other package suites passed. After tracking selection-task
  startup, the final gate passed in
  `target/test-logs/swift-package-20261008T045246-17496.log`: 559 mobile tests
  (one skipped), 5 widget tests, and 394 app tests; zero failures. Map
  presentation passed 31 tests. The direct-ID timing case is covered by the
  package predicate test, not by a cold-launch simulator scenario.
- Intermediate package run `swift-package-20261007T134513-94684.log` stopped at compilation: the new location fixture used an iOS-only authorization case. It now uses `.authorizedAlways`, covering the same authorized acquisition branch on macOS and iOS.
- Initial normal-size simulator run `run.sjeZhR`: 1 passed, 2 failed.
  EUC Ride passed; music was absent at startup; VESC's motor-current caption was
  flagged for larger-text clipping.
- Largest-text diagnostic run `run.vnZS2h`: 2 passed, 5 failed. EUC portrait and
  landscape passed. All music fixtures reproduced the activation handoff defect.
  VESC portrait/landscape identified the same caption with retained frames,
  hierarchy, and screenshots under `target/diagnostics/swiftui-audit-20261007`.
  The visible caption is now `current`; its accessible label remains `motor current`.
- `run.qI3EPO`: production Lighting controls passed at the largest text size,
  including the named Brightness slider. The Core Location main-thread warning
  was absent after the caching repair. Unrelated QoS warnings remain.
- `run.ihOveo`: nominal VESC Live Activity passed at the largest text size. Its
  accessibility hierarchy contains headroom, beeps, and temperature once each;
  temperature can correctly include the stale suffix after backgrounding.
- `run.BnLRf1`: normal-size EUC and VESC essential Ride controls passed. Music
  remained present on Map, where the strict clipping audit exposed the summary's
  two-line `Speed unavailable` truncation. Retained screenshots and issue/hierarchy
  attachments are in `target/diagnostics/swiftui-audit-20261007/normal-current-attachments`.
  The repair uses the established `--` visual readout with full unavailable speech,
  permits summary text wrapping, removes Map's duplicate player modifier, and
  bounds Live/History/Detail scroll viewports above the measured accessory.

## Historical remaining audit work, 2026-10-08

The following records the October 8 findings and proof. The current matrix above
supersedes its pending-run descriptions.

The strict text audit alone did not detect an unused viewport gap in the passing
normal music screenshots. `run.r7NkNu` passed route coverage, full metadata,
play/pause, next/previous, hide, and restore, but its Map screenshot exposed an
additional geometry regression: the ScrollView ended at y596 while the player
started at y737. Map's proposal had already been reduced by the native tab safe
area; subtracting that inset again removed 141 points. The corrected Map helper
caps only an overlapping player frame. Two package regressions cover the actual
590-point proposal, a proposal extending behind the player, hidden/non-overlapping
players, and a zero-height clamp. The music UI test now asserts that Map's viewport
actually reaches the player, in addition to auditing text.

The paused music regression also rotates Ride with the player shown, hides it,
rotates while hidden, restores through More, and checks the restored layout in
the new orientation. It waits for visible window/metric geometry, asserts 44-point
non-overlapping transport controls and zero Ride scroll views, and retains
screenshots for each geometry state. The focused accessibility-size run passed.

- Paused music placement now has accessibility-size simulator evidence on Ride,
  Map, and More. Playing-state transport, next/previous, expanded metadata, and
  hide/restore passed the normal-size simulator test. Verify recovery layout,
  keyboard behavior, and real-provider/device acceptance.
- Complete simulator acceptance for Map loading/empty/error states (LIBCU-802).
- Complete map-first integration and filter geometry (LIBCU-598).
- Measure identified rendering candidates before claiming a performance fix
  (LIBCU-403).
- Verified accessibility-size and playing-state evidence is synced to Lific
  `LIBCU-DOC-12`, `LIBCU-803`, and `LIBCU-802`; the full audit remains open.

## Continued source and simulator audit — 2026-10-08

- The disconnected Lighting color wheel's accessibility sliders inherit the
  parent control identifier, so child identifiers were not queryable. The
  accessibility tree confirmed the Hue and Saturation sliders were disabled;
  the UI test now queries them by accessible label. `run.pSR5c5` passed
  `testEucLightingRouteUsesProductionControls` on the ARM64 iPhone 18 Pro / iOS
  27.0 simulator. Lighting effect buttons also expose their selected state to
  VoiceOver.
- `DeviceActionRow` now gives its button label an explicit 44-point minimum
  height and rectangular hit shape, so surrounding row padding does not define
  the tappable area.
- Tune source review found a false readback presentation: a missing setting
  value was rendered as a `Wheel` current-value row with an unavailable
  placeholder. Tune now omits that row until a current value exists. The UI
  assertion again requires the row to stay absent while a write-only command
  is pending.
- The EUC accessibility-size Tune fixture has no readback base for display
  brightness, so its stepper must remain disabled and produce no draft or Apply
  action. The test now covers that unavailable state and retains the
  interaction path when a base value exists. A simulator retry reached the
  earlier pairing step but could not connect because Simulator reported
  Bluetooth unavailable; this corrected branch still needs a stable simulator
  run.
- Final Swift package gate after the Tune presentation and UI-test changes
  passed: 559 mobile tests (one existing platform-specific skip), 5 widget
  tests, 394 app tests, zero failures. Log:
  `target/test-logs/swift-package-20261008T051520-27836.log`.
- Ride geometry follow-up: EUC and VESC metric rows now cap at 108pt and
  shrink to available space; the portrait speed hero allocation increased from
  32% to 42%. The no-scroll UI helper asserts metric frames stay at or below
  112pt. Package gate after these edits passed with 559 mobile tests (one
  existing skip), 5 widget tests, 394 app tests, zero failures:
  `target/test-logs/swift-package-20261008T052048-29710.log`.
- ARM64 iPhone 18 Pro / iOS 27.0 UI tests passed for EUC and VESC portrait at
  explicitly pinned standard content size (`run.fnbBuJ`, 2/2), and each
  separately passed with the simulator's prior content-size configuration
  (`run.kDpcvh`, `run.nts98e`; size was not pinned in those runs).
  Standard-size screenshots are retained as
  `euc-ride-compact-large.png` and `vesc-ride-compact-large.png` under
  `target/diagnostics/swiftui-audit-20261008/`.
- Both landscape UI cases passed their AX frame, hit-target and no-scroll
  assertions in `run.khrRNy` (2/2), but manual inspection of the exported
  landscape screenshots shows a large black upper region with the Ride
  composition shifted into the lower portion. Do not treat landscape visual
  acceptance as complete; inspect app window/simulator capture behavior before
  closing LIBCU-868. Player-visible layout and physical acceptance also remain
  open.
- The first combined rerun before correcting the no-readback assertion failed
  both cases: Lighting's identifiers were overridden by its parent and Tune's
  current row falsely claimed a Wheel readback. After correcting those, the
  Lighting case passed. `run.x9a7AI` did not exercise Tune because the fixture
  never appeared after a Bluetooth-unavailable simulator launch.
- These are source/presentation corrections; they do not close broad Music,
  Map, History, keyboard, appearance/Dynamic Type, RTL or physical-device gates.

### Combined UI RED and current repair, 2026-10-09

The first combined ARM64 iPhone 18 Pro/iOS 27.0 UI run executed the original
23 base selectors: eight passed and 15 failed, bundle
`target/xcode-ui-tests/TestResults/run.umpqAY/Result.xcresult`. Exported attachments,
case outcomes, raw orientation dimensions and SHA256 hashes are retained in
`target/working-tree-completion/ui-final-base-review/review-manifest.json`.
This run is not release acceptance.

The failures separated into concrete source and harness problems:

- VESC title text visibly truncates at AX5; its normal/AX clipping audits name
  that title. The header now wraps title text and places title/status on separate
  rows at accessibility sizes, preserving the requested category.
- The combined Readings opener appeared as an accessibility Other with a nested
  Button lacking the identifier. The combined control now retains an explicit
  button trait, and the sheet has one explicit accessibility container and a
  minimum 44pt Done action. Typed button and unique-container assertions remain.
- All six landscape cases passed actual window/device/screen geometry readiness
  and failed the raw UIImage.size check. The harness now checks displayed size
  with UIImage orientation, waits both geometry and displayed screenshot, and
  retains raw metadata and an unmodified native screenshot. The UIKit redraw
  attachment was distorted and is excluded from visual proof. Original recordings
  supply the separate visual evidence; no pass is inferred from raster aspect alone.
- The PWM fixture's preferred category persisted into later tests that omitted
  their own category input. Every test launch now sets the intended category,
  including Large for normal cases. The AX5 Music helper now expects the primary
  reading on the fixed surface and all secondary values in Readings.
- Empty Apple/Spotify Music reached settings; the requested history row was below
  the lazy Form viewport. The harness now reaches that actual row by scrolling
  before asserting availability and hit testing.
- Map root identifiers propagated into scroll descendants. Production now has
  an explicit unique root. Live/History native ScrollViews use the canonical
  `ride-map.live-viewport` / `ride-map.history-viewport` identifiers; redundant
  child IDs were removed after actual native AX inspection. The harness verifies
  root uniqueness and uses the global mode wrapper and exact typed scroll query.

Two additional DEBUG Simulator-only, explicitly enabled typed rendering fixtures
exercise exact EUC battery/pack/power/thermal values plus available Time to full,
limp-home range and fresh GPS speed; VESC exercises exact voltage/current/angle/
controller values plus both-pressed footpad state. The tests require spoken values,
units/details, full scroll frames, a 44pt Done action and restoration of the fixed
Ride surface. The fixture maps existing DTOs and does not claim real GPS, charger
or accessory evidence. The final 25+1 UI matrix and native recheck after header/
accessibility source changes remain pending.
### Second combined UI run and focused target repair, 2026-10-09

`target/xcode-ui-tests/TestResults/run.8UQauf/Result.xcresult` executed the
25 base selectors on the ARM64 iPhone 18 Pro Simulator: 15 passed, 10 failed,
zero skipped. This is a diagnostic run, not final acceptance. Original attachments,
case outcomes, timestamps and SHA256 hashes are retained in
`target/working-tree-completion/ui-final-25-review/review-manifest.json`.

- Six AX5 Ride/secondary cases reached the Readings sheet and failed the actual
  Done target height: the native toolbar button was 74.7 × 36pt despite an outer
  SwiftUI minimum-height frame. The frame and hit shape now belong to the explicit
  Text label, with plain button style. Focused EUC essential AX5 rerun
  `run.t7F59v/Result.xcresult` passed its exact 44pt target, secondary reading
  reachability, dismissal and restored fixed-Ride checks (one test, zero failures).
- EucMore failed a direct `dashboard.nav.ride` query before tapping Map. The
  original recording at 12 seconds shows Ride/Map/Tune/More present. The harness
  now uses the established native-title fallback and requires actual button role,
  unique title, 44 × 44pt targets, exclusions from the tab bar and exact destination
  roots. This does not weaken the navigation oracle.
- Two music cases and saved history failed redundant child ScrollView IDs. The
  actual native AX tree exposed the established viewport IDs. Typed queries now
  use those canonical IDs; all geometry, pagination, selection, filter and receipt
  assertions remain. Saved history did not reach its later-page assertions in
  this run, so LIBCU-874 runtime acceptance remains pending.
- Normal Ride, Large/XXXL landscape, PWM growth, Apple/Spotify cold relaunch,
  More reopen, HomeMap, Camera/Devices, Tune Back and Lighting focus passed in
  this run. These results remain distinct from the pending affected-source rerun.

The original AX5 EUC landscape recording at 15.6 seconds contains the complete
fixed Ride composition, with the requested larger typography and all native tabs,
rotated within its stored portrait raster. Exact source/derived hashes and the
unmodified frame extraction are in `derived-landscape-manifest.json` under the
same review directory. The actual orientation receipt was device 3, window
874 × 402pt, UIImage orientation 2, raw image size 402 × 874, displayed size
874 × 402. The UIImage redraw attachment is distorted and the native PNG can be
cropped; neither is used as landscape visual acceptance. The harness now retains
an unmodified screenshot and the exact orientation/category/frame checks.

Setup Done, the empty music sheet Done and the expanded player Done use the same
explicit-label target sizing as Readings. Existing cold/reopen/playing/paused
helpers now assert actual button role, hittability, 44 × 44pt bounds and full
window containment before dismissal or navigation. These are presentation changes;
Rust music/history/capture semantics are unchanged.

Fourteen base selectors are affected by the repaired source. The focused EUC
essential AX5 case is already green; the remaining 13 selectors and the separate
actual Increase Contrast/Reduce Motion case are pending. Final native package,
UIKit runtime and app-build receipts remain separate. Physical music providers,
GPS/charger/footpad operation and locked riding are not established by the typed
Simulator rendering fixtures.

### Affected UI rerun and remaining targeted checks, 2026-10-09

`target/xcode-ui-tests/TestResults/run.1pGom7/Result.xcresult` executed 13 affected
base selectors: 10 passed, three failed, zero skipped. Original attachment names,
timestamps and SHA256 hashes are in
`target/working-tree-completion/ui-final-remaining-13-review/review-manifest.json`.

Both available-secondary AX5 cases passed their exact spoken values, units/details,
full metric frames, real 44pt Done target, dismissal and restored fixed-Ride
checks. EUC includes available Time to full, range and fresh GPS speed; VESC
includes motor/controller temperatures and both-pressed footpad state. These are
explicit typed rendering fixtures, not physical telemetry acceptance. AX5
EUC/VESC landscape and VESC portrait, Apple/Spotify cold relaunch, More reopen,
playing Music across routes and HomeMap passed. Existing helper assertions also
verify the actual Setup and empty/populated Music Done targets are buttons,
hittable, at least 44 × 44pt and inside the window.

The three failures isolated remaining defects:

- EucMore traversed native tabs, Map, More and Camera, then found the real shell
  Back target at 19.3333pt. Its existing label now owns the 44pt minimum frame
  and rectangular hit shape; the back callback and typed navigation remain.
- Paused AX Music reached landscape Ride but received a logical 44pt Readings
  opener frame as 43.99999999999997pt. All 35 exact 44pt frame assertions use
  one documented 0.000001pt tolerance for CGRect arithmetic. Larger minimums,
  button role, hittability, window/occlusion geometry and data assertions remain
  unchanged. Actual 36pt Done and 19pt Back targets still fail this threshold.
- SavedHistory reached the filter bar, where its parent identifier had propagated
  to all three native buttons. The filter bar now has an explicit accessibility
  container. The same source ownership rule is applied to the detail header;
  Load More has an explicit 44pt label while retaining its callback, localization,
  bordered style and padding. Pagination/detail were unvisited in the failing
  run; these two source corrections still require the exact runtime assertions.

Only EucMore, paused AX Music and the full 51-route SavedHistory test require the
next affected rerun. SavedHistory retains all page-boundary UUID, projection,
new-query generation, filter, relaunch and no-reflow frame oracles. The separate
actual Increase Contrast/Reduce Motion case, final native package, UIKit runtime
and app build are pending distinct receipts. No physical provider, locked-riding,
phone deployment, Android or publication result is implied.

Representative visual artifacts reviewed from the passing secondary cases:

- EUC Time to full: `target/working-tree-completion/ui-final-remaining-13-review/testEucAccessibleReadingsPreserveAvailableSecondaryValuesAtAccessibilityDynamicType/8758E09A-6290-48A9-9FB0-94CBEA8413A4.png`.
- EUC range and GPS: `target/working-tree-completion/ui-final-remaining-13-review/testEucAccessibleReadingsPreserveAvailableSecondaryValuesAtAccessibilityDynamicType/9EED91E4-E3F6-46B2-96E1-81BF5BAC3D35.png`.
- VESC controller/motor temperatures and footpad: `target/working-tree-completion/ui-final-remaining-13-review/testVescAccessibleReadingsPreserveAvailableSecondaryValuesAtAccessibilityDynamicType/3F8C051A-F9F8-473C-AEDC-F25C0759E5F4.png`.

These unmodified portrait screenshots show the full requested readings at AX5.
The complete AX5 landscape visual proof is the original screen recording
`target/working-tree-completion/ui-final-25-review/testEucEssentialRideControlsRemainVisibleWithoutScrollingInLandscapeAtAccessibilityDynamicType/70689137-8270-4BC2-A03B-6BB09CC74037.mp4`
at 15.6 seconds. Its unmodified raster extraction is
`target/working-tree-completion/ui-final-25-review/derived-ax5-landscape-euc-at-15p6s.png`;
exact source and extraction SHA256 hashes are retained in
`target/working-tree-completion/ui-final-25-review/derived-landscape-manifest.json`.
The video stores the complete landscape interface rotated within a portrait
raster. Native orientation/category/AX frames and visual recording are separate
proof layers. The distorted UIKit redraw and cropped native landscape PNG are
excluded from visual acceptance.

### Three-case retry and exact History readiness repair, 2026-10-09

`target/xcode-ui-tests/TestResults/run.1wSaiz/Result.xcresult` executed the three
remaining base cases: EucMore passed in 39.421 seconds, paused AX Music hit the
runner's 120-second allowance, and SavedHistory failed a readiness assertion in
85.794 seconds. The intervening `run.HlMAV6` invocation was compile-only, with
zero tests executed; it provides no UI behavior evidence.

Paused AX Music continued ordinary reading queries, scrolling and rotations
through the runner timeout: thermal checks at 104–108 seconds, rotation at 110
seconds, further Readings checks through 134 and 166 seconds, then another
rotation at 168 seconds. This compound test covers three player routes and six
Ride geometry states, including repeated secondary-reading sheet checks. Its
per-test allowance is now 360 seconds. SavedHistory and the actual Settings /
Increase Contrast / Reduce Motion scenario also explicitly request 360 seconds
for their bounded compound sequences. The runner retains its ordinary 120-second
default, permits a maximum of 360 seconds, and the matrix adds budget only for
these exact three selectors. Per-step readiness waits and all assertions remain.

SavedHistory passed the filter and page traversal stages and displayed a selected
route detail with its map and distance/duration/speed summary. Its readiness wait
still queried `ride-map.screen`, which leaves the native accessibility tree when
`ride-map.detail-screen` is pushed. The corrected wait uses the unique visible
detail-screen container while retaining the exact selected UUID, detail projection
UUID, loading state and query-generation conditions. On Back, the test returns to
the original list-screen query. The detail view now contains its children before
assigning `ride-map.detail`, preserving the header and viewport identifiers rather
than forwarding the parent identifier to both native elements. This failure was a
readiness-identity error, not a timeout or evidence of missing stored geometry.

The paused AX / SavedHistory pair and the separate actual contrast / Reduce Motion
case still require runtime receipts for the current source. Final native package,
UIKit runtime and app-build validation remain separate.

### Bounded compound-case retry and connected Lighting coverage, 2026-10-09

`target/xcode-ui-tests/TestResults/run.9YM525/Result.xcresult` executed the paused
AX Music / SavedHistory pair: one passed, one failed, zero skipped. Paused AX
Music completed the full route, orientation, hide/restore and repeated Readings
scenario in 251.422 seconds under its explicit 360-second allowance.

SavedHistory failed at 80.092 seconds while querying a redundant inner
`ride-map.detail` identity. The actual tree already contained the unique visible
`ride-map.detail-screen`, its fixture readback, the retained `ride-map.detail-header`,
`ride-map.detail-viewport` ScrollView, and rendered route map. The outer route root
owns the native screen identity. The test now uses that canonical root directly
for the unchanged exact UUID, detail-projection, loading and query-generation
checks and for the map/header/Back assertions. No additional rendering or domain
change is involved. The subsequent `run.mPXrPS` completed the full 51-route
pagination/reentry/filter/no-reflow/relaunch path in 102.164 seconds. The same
three-case run had two widget failures; it is not a combined widget pass.

LIBCU-876 had disconnected slider-purpose checks and model request tests, but no
actual enabled native-slider gesture-to-write proof. A new explicit
DEBUG/Simulator-only connected Lighting adapter supplies native connection/GATT
facts to the existing Rust MELK session reducer. Rust verifies readiness and
produces the command write. The candidate UI case adjusts the real Brightness
slider and checks its role, purpose, enabled/hittable state, spoken percentage,
requested value, and the actual Rust-produced nine-byte payload, FFF3 channel
and write-without-response mode. The adapter retains only the latest write receipt
and count; it does not claim physical confirmation. Ordinary Simulator, device
and release startup keep the production accessory session. Its current runtime
receipt is recorded below.

### Enabled Lighting and actual Settings harness findings, 2026-10-09

`target/xcode-ui-tests/TestResults/run.x2XiIJ/Result.xcresult` executed Lighting
brightness and the real Settings contrast/Reduce Motion case: zero passed, two
failed, zero skipped. Lighting failed in 39.606 seconds before its adjustment;
Settings failed in 52.097 seconds before the preference change. Neither establishes
a successful brightness write or actual Reduce Motion acceptance.

Lighting's Brightness slider was present, enabled and named correctly, but remained
below the viewport after twelve scroll attempts. The native hierarchy showed Hue
and Saturation controls across the viewport's horizontal center; Hue had changed
to 75% while the Brightness/header frames stayed fixed. The enabled color wheel
owns a zero-distance drag gesture. The generic scrolling helper's center drags
therefore adjusted color instead of moving the ScrollView. The Lighting test now
scrolls through the existing padded edge at horizontal fraction 0.08, outside
those controls. The helper's ordinary default remains 0.5. All bounded scrolling,
full visibility, hittability, spoken percentage and exact typed-write assertions
remain; production gestures and layout are unchanged.

The Settings test queried `cells["Accessibility"]`, but the actual iOS 27 tree
contains unlabeled Cell wrappers with named native Button children. Those failed
queries scrolled the Settings collection to its bottom. The helper now targets
the root Accessibility Button, restores toward the list top when the target
has not been materialized, and uses bounded scrolling plus actual role, visibility
and hittability checks before tapping. It still changes the real Reduce Motion
Switch, checks its value, verifies both SwiftUI and UIKit readbacks, and restores
the initial setting in `defer`.

`target/xcode-ui-tests/TestResults/run.PPcgYT/Result.xcresult` subsequently executed
two cases: Lighting passed in 35.985 seconds; contrast/Reduce Motion failed in
22.242 seconds; zero were skipped. Lighting reached the enabled native slider
gesture and passed the unchanged spoken-percentage and exact Rust-write checks.
This is Simulator adapter proof, not accessory hardware acceptance.

The Settings retry reached and tapped the actual root Accessibility Button. Its
next query failed because the Accessibility page uses a native Table, while the
helper still requested a CollectionView. The exported native tree shows the
NavigationBar identified as `Accessibility` and an actionable Cell identified as
`MOTION_TITLE`, labeled `Motion`; the nested Motion Button has zero bounds.
The helper now waits for that page, scrolls the Table to the exact Motion Cell,
and checks its cell role, label, full visibility and hittability before tapping.
The real Switch change, live preference readbacks and deferred restoration remain
required. Their current-source runtime receipt is pending.

The exact page trees and original video are retained in
`target/working-tree-completion/ui-contrast-native-settings-review/`, including
`manifest.json`, `E0324D29-8057-4848-90C0-5277443524FE.txt` and
`5FF13EFE-7833-4BA8-906A-F6A404FE6B8A.mp4`.

The next actual Settings retry,
`target/xcode-ui-tests/TestResults/run.mfkFfW/Result.xcresult`, failed in 28.745
seconds at the five-second wait for the Reduce Motion Switch after tapping the
Motion Cell. Its activities show four bounded scroll gestures followed by passing
row visibility and hittability checks. Motion was fully visible at y712 with a
height of 98.3 points; scrolling was not exhausted. Its nested `MOTION_TITLE`
Button had then materialized at x55.7, y727.5, width221 and height63.3. The Cell tap left the
Accessibility page open. The helper now retains the Cell/Table geometry checks,
then verifies the materialized native Button's role, label, full viewport bounds
and hittability before tapping that action. The actual Switch and live preference
readbacks still require a current-source pass.

The native Button retry `run.iO4yv1` failed in 22.176 seconds at the same
post-tap Switch wait. Its synthesized event targeted the native Button center
(166.1667,759.1667), and the original video showed no navigation. The Settings
process had remained pid65721 across several failed setups. Starting a fresh
Settings process then produced `run.Xjc5l6`: it failed in 65.246 seconds with
Settings pid89449, again after the native Motion Button tap. This disproves the
retained-process explanation; neither run is preference-change acceptance.

The next isolated harness candidate gives only the freshly launched Settings
process the standard Large text category through its UIApplication launch
argument domain. It operates the real OS preference rather than changing global
text-size settings. CutOut's requested AX5 category, actual category assertions,
real Reduce Motion Switch and UIKit/SwiftUI preference readbacks remain required.
The candidate's actual interaction receipt is described below. The original fresh-process video is
`target/working-tree-completion/ui-contrast-v5-fresh-settings-review/7F855CE9-4871-4B0B-96F3-BE361847F370.mp4`.

`run.CVXz7u` reached the real Motion page under Settings' process-local Large
category, then failed in 40.445 seconds at the enabled-value waiter. Its named
`REDUCE_MOTION` Switch spans the whole label row, x36–366; the nested native
Switch track spans x305–368. The synthesized event tapped the wrapper center,
(201,160.333), outside the rendered toggle. The original video shows Reduce
Motion remaining Off. The repaired harness now verifies the sole Switch
descendant's role, enabled state, complete window bounds and hittability, and
taps that actual control for both enable and conditional restoration. It keeps
the named outer row's original-value and enabled/restored-value waiters. The
toggle and CutOut's live readbacks still need a current-source pass. Original
video, event and tree evidence are retained in
`target/working-tree-completion/ui-contrast-v6-native-switch-review/`.

### Current History, widget and package proof boundaries, 2026-10-09

The actual saved-History case in `run.mPXrPS` passed the 51-route scenario in
102.164 seconds, including selecting beyond page one and retaining exact UUID,
projection and query-generation checks across reentry, filter reset and relaunch.

The subsequent AX1 widget pair `run.KfKJeK` failed both compound cases. Independent
inspection corrected the ordering attribution: the actual expanded native tree
places Headroom, Beeps and Temp before Speed, while XCTest's descendant query
flattens nodes by depth and returned the shallower Speed first. The oracle now
retains the native owner hierarchy and checks its preorder, preserving exactly
two safety readings and their values. Critical Lock Screen secondary counts and
values passed within that failed compound run; nominal foreground acknowledgement
did not settle. An isolated DEBUG observation invalidation now exposes the actual
Rust/coordinator receipt after the platform await. It does not invent an
acknowledgement. The later first AX1 matrix cell `run.7yua57` still failed with
Core live but Rust idle and no activity; root and the platform lane are tracing
that exact ownership state. No widget matrix pass is claimed.

The final supported Swift package v2 gate passed 1,082 cases: 630 Mobile, 447 App
and five validators, with zero failures. Its exact log is
`target/test-logs/swift-package-20261009T224824-91236.log`; the parsed receipt is
`target/working-tree-completion/swift-completion-final-v2-summary.json`. This
package receipt is separate from the pending actual Settings and widget gates.

### Actual preference acceptance and remaining widget startup race, 2026-10-09

`target/xcode-ui-tests/TestResults/run.SjiTRL/Result.xcresult` passed the actual
contrast/Reduce Motion case in 93.978 seconds: one executed, one passed, zero
skipped. It navigated the real Settings page, changed the verified native toggle,
and checked CutOut's SwiftUI and UIKit Reduce Motion and increased-contrast
readbacks at the requested AX5 category. The route, Camera Back, fixed Ride and
Readings assertions passed, followed by restoring the original OS preference.
The Settings-process Large presentation argument did not replace CutOut's
actual AX5 or live OS preference checks.

The five original retained attachments are exported under
`target/working-tree-completion/ui-contrast-final-review/`, with `manifest.json`
and `review-sha256-manifest.json`. They include the unmodified Ride screenshots,
native hierarchy and settled window/orientation metadata. This completes the
26 distinct base/contrast selectors through retained unaffected passes and
affected reruns: the prior 25 base selectors plus this separate preference case.
It is not a single combined 26-case run. Connected Lighting's separate actual
gesture/write case also passed, as recorded above; physical accessory output
remains separate.

Widget acceptance remains pending. The startup investigation found that a delayed
nil persisted-marker load can finish the Rust/Swift recovery gate without
requesting another Live Activity reconciliation. A static already-live Core can
therefore remain paired with Rust idle and no activity. The root/platform lanes
added two ordering regressions and two continuation calls to `syncLiveActivity`.
Both new cases passed in the post-repair package run, but the full command exited
1: three existing Activity cases failed seven assertions within 449 App cases.
They cover observable start failure/retry, clearing an orphaned activity while
scanning, and preserving identity through transient reconnect. The exact log is
`target/test-logs/swift-package-20261009T230610-3134.log`. The root/platform lanes
are reviewing those effects before another gate. The 1,082-case Swift v2 receipt
above predates this startup repair. The interrupted widget matrix `run.BSAvZf`
is partial and is not counted as a pass. No widget matrix or full post-repair
native gate is claimed for that failed run.

The subsequent supported full Swift run passed after settling the startup
continuation repair: 1,084 cases, comprising 630 Mobile, 449 App and five
validators, with zero failed cases or assertions. Both new marker-ordering cases
passed in 0.338 and 0.350 seconds. They use the actual VESC connected selection
and an open Rust ride before and after the held marker load, preserving one
Activity start, the active ride identity and the durable marker. The three
existing failure/retry, orphan and transient-reconnect tests passed unchanged.
The exact log is `target/test-logs/swift-package-20261009T231120-9410.log`; the
parsed receipt is
`target/working-tree-completion/swift-startup-continuation-final-summary.json`.
The final five-cell AX1–5 widget matrix is running; UIKit/runtime and the app
build follow as separate serialized gates. None is inferred from this package
pass.

The first final AX1 widget cell,
`target/xcode-ui-tests/TestResults/run.ledB9z/Result.xcresult`, executed two
compound cases: expanded passed; Lock Screen failed only the two-minute case
execution cap. The summary contains no failed semantic assertion. Retained
activities show both Critical and Nominal Lock Screen secondary-count/value
checks and native safety-order attachments completing before the cap, which
hit during the final verified-dismissal sequence's bounded native permission
checks. This is incomplete cleanup acceptance, not a full Lock Screen pass.

Only the two compound widget selectors now request an explicit 360-second
allowance for initial dismissal, two real ActivityKit states, native surface
checks and final dismissal. Normal test defaults remain 120 seconds; each
existing per-step readiness/assertion/cleanup timeout is unchanged. The full
five-cell AX1–5 matrix still requires its subsequent actual receipts. The App
source is unchanged by this test allowance repair, so the 1,084-case native
package receipt remains current.

The AX3 cell `run.5evG7v` exposed a real exact-once failure: Expanded Critical
failed in 60.247 seconds, while the full Lock Screen compound case passed in
123.554 seconds. Temp appeared twice inside one expanded widget: the safety
footer's StaticText at (131.3,157.7,55,27.7), and the grid's Other node at
(257.7,274.7,112.7,61.7). Beeps, Headroom and Speed each appeared once. Narrowing
the SpringBoard query would not resolve that duplicate; the oracle was retained.

Moving the hidden flag onto the composite cell's internal node did not resolve
AX3. `run.3OxT2X` again failed Expanded Critical in 56.828 seconds, with Lock
Screen passing in 123.806 seconds. The foreground receipt's activity identity
`C57D9BE6-3993-47CC-B1D4-E6F9E929BEB2` exactly matches the current native owner;
the prior failure belonged to `9DDBF153-793D-4DC5-8971-C72F9D939984`. Installed
and built embedded extension SHA256 both equal
`07431670beb3ee754d80356abec5f7f7c6414d5af833417448e0a3b0a8cac48b`.
The matching build/installation hashes and activity proof are retained in
`target/working-tree-completion/ui-widget-ax3-node-attribution/attribution.json`.
This failure was current rendering, not an older activity or installation.

The next repair removes the failed cell-flag API and gives the Grid one explicit
accessibility representation with seven typed snapshot values: Battery, Voltage,
PWM, Mode, Duration, Distance and Charge. The SafetyFooter owns Temp and Headroom.
Independent source review confirms the visual grid, layout fallbacks, footer and
safety priority stay unchanged, and labels/spoken values come from the existing
snapshot. Exact-once counts and semantic assertions remain unchanged. Runtime
AX3 and the complete AX1–5 matrix are pending for that representation. The last
full Swift pass and app-build receipts before this shared component repair do
not validate its final source.

The explicit Grid representation subsequently passed the actual AX3 rerun
`target/xcode-ui-tests/TestResults/run.pNTyqA/Result.xcresult`: two compound cases,
zero failures and skips. Expanded completed in 116.408 seconds; Lock Screen
completed in 122.194 seconds. Independent review of each original native safety
tree confirmed one Headroom, Beeps, Temp and Speed node in all four
Critical/Nominal surface states. Critical native preorder places Headroom before
Speed; Nominal places Speed before Headroom. The unchanged typed spoken-value
assertions passed. All four foreground receipts report a live phase, active Rust
session, acknowledged ActivityKit activity, complete restoration and no error.
The original 16 selected attachments, timestamps and SHA256 hashes are retained
in `target/working-tree-completion/ui-widget-ax3-representation-review/manifest.json`
and `review-manifest.json`.

This establishes the AX3 speech/order repair. It does not establish full visual
widget acceptance. Review of the unmodified Critical Expanded screenshot
`BD928554-0575-48B7-A998-BE5E3C3810F3.png` shows only the top metric-grid labels,
followed by a large black area in the expanded surface. The Nominal Lock Screen
screenshot `2B9B01FC-CA82-4CA9-95A3-060BB7497213.png` wraps the Stale label as
“Stal” and “e” beside the vehicle name. The native Critical Expanded hierarchy
independently exposes a 341.3-point composition inside a 160-point full-width
native viewport. The visible Lock Screen Stale text measures 70.3 points high
beside a 35.3-point one-line vehicle name. These observations were reported
separately to the root review lane; the semantic test pass does not resolve them.
The Footer's Headroom, Beeps and Temp accessibility nodes are virtual speech
representations. Their frames can extend beyond the visual owner and do not
establish physical text clipping by themselves.

The visual repair selects the existing compact Speed/Footer Island composition
at accessibility categories and reserves one line for Header status. New native
assertions compare the actual composition and physical Speed against the
smallest same-origin, full-width native viewport, and compare visible Stale text
height with the actual one-line vehicle name. They retain all exact speech counts,
values, native order and full-case cleanup checks. Virtual Footer values receive
no physical-bounds assertion. The current visual rerun, other AX categories,
post-component native/runtime gates and app build remain pending receipts.

The current visual-repair source subsequently passed the supported full Swift
package gate: 1,084 cases and zero failures in
`target/test-logs/swift-package-20261010T000441-41526.log`. The supported
`build:ios-app` task also exited zero; its current ARM64 app and embedded
extension were built at 06:05:33 and 06:05:32 UTC on October 10. The retained
binary receipt is
`target/working-tree-completion/ios-app-final-binary-receipt.json`, with app
SHA256 `54074a2b75d27bd9ca3f65ebd75798de601ae8bb31767662c6b132a879643bd0`
and extension SHA256
`20cffe73d2143f095fab75f08b000081d2d42a20f2ae23266d31da1863029ce6`.
These validate the current source/build layers. The new actual visual AX1–5
matrix and UIKit runtime remain pending.

The first current visual AX3 rerun, `run.dzKyLQ`, executed both compound cases:
Lock Screen passed in 128.889 seconds; Expanded failed in 62.885 seconds before
its Nominal state. The failure was the new geometry helper resolving an indexed
native element after ActivityKit changed the collection, producing “No matches
found for Element at index 16”. It was not a failed spoken-value or geometry
comparison, and this run is not accepted as a complete AX3 pass.

The original current Critical Expanded native tree measures a 141.3-point
composition instead of the prior 341.3-point oversized layout. The retained
video's frame at 53 seconds shows the full Speed and Reduce acceleration content
without the cropped grid. Both original Lock Screen state screenshots show
whole-line Stale, Speed and headroom; their visible status and vehicle text pass
the unchanged one-line-height comparison. All 44 original attachments and hashes
are retained under `target/working-tree-completion/ui-widget-ax3-visual-review/`;
`review-manifest.json` identifies the original video and explicitly attributes
the extracted frame. No original screenshot was redrawn.

The helper now parses the already-retained immutable native Element subtree
instead of resolving each indexed element and frame separately. Required owner,
physical Speed and visible header nodes, finite numeric frames, viewport bounds,
speech assertions and tolerances remain unchanged. The retained-tree
characterization in `geometry-parser-characterization.json` rejects the old
341.3/160-point overflow and old two-line status, while accepting the current
Critical Expanded and both Lock Screen layouts. This characterization does not
replace the pending complete current AX3 and AX1–5 runtime reruns.

The coherent current AX3 rerun `run.BWFbkc` subsequently passed both complete
compound cases with zero failures or skips: Expanded in 119.675 seconds and
Lock Screen in 122.913 seconds. Independent review verified one Headroom,
Beeps, Temp and Speed node, correct Critical/Nominal native preorder, typed spoken
values and active acknowledged lifecycle receipts in all four states. Coherent
native geometry measures 141.3-point Expanded compositions and 212.3-point Lock
Screen compositions within their native viewports; visible Stale fits the actual
one-line identity height.

All four original screenshots were reviewed. Critical Reduce acceleration and
Nominal Headroom good are complete on both surfaces, Stale stays on one line,
and the oversized cropped Grid is absent. The 16 original selected attachments,
timestamps, hashes and per-state native proofs are retained in
`target/working-tree-completion/ui-widget-ax3-coherent-final-review/review-manifest.json`.
This accepts the complete current AX3 visual/semantic repair. The other four
AX1–5 category cells and UIKit runtime remain separate pending receipts.

The same frozen source also passed the complete AX1 cell `run.0q9yWT`: Expanded
in 112.819 seconds and Lock Screen in 122.580 seconds, with no failures or skips.
Independent review of all four original state screenshots found complete Speed,
Headroom good / Reduce acceleration and whole-line Stale, with no cropped grid
or phrase overflow. All four native exact-once counts and preorder checks pass;
the unchanged test also completes coherent native geometry and typed speech
checks. The 16 original selected attachments and hashes are retained in
`target/working-tree-completion/ui-widget-ax1-coherent-final-review/review-manifest.json`.
AX1 and AX3 are accepted for the current source. AX2, AX4, AX5 and UIKit runtime
remain pending.

The complete AX2 cell `run.5xiqIT` passed both tests, Expanded in 118.047 seconds
and Lock Screen in 124.453 seconds, with no failed semantic/geometry assertions.
Independent review of the original Critical Lock Screen screenshot
`6601593E-4E4C-42A4-BE4A-C88B9521C423.png` nevertheless found the visible warning
truncated to “Reduce accelerati…”. The other three original state images show
complete values. Full typed speech and owner bounds do not detect an ellipsis
inside the visible label, so AX2 visual acceptance remains open. The 16 original
attachments, current receipts, native count/order checks and hashes are retained
in `target/working-tree-completion/ui-widget-ax2-coherent-final-review/review-manifest.json`.
This finding was reported to the root/platform lanes while the remaining matrix
was still frozen; no production change is inferred from the passing test status.

The warning repair now gives the actual inner FooterChip `Text` its vertical
ideal height at the proposed width, preserving its two-line limit and requested
font size. The earlier outer chip constraint did not prevent the inner text from
ellipsizing. A test-only Vision oracle recognizes the actual native widget
viewport in each Critical state and requires the full visible words “Reduce
acceleration”, including wrapped text. It disables language correction and uses
no custom word hints. Each assertion retains the original native screenshot and
recognized lines; full accessibility labels remain independently checked.

The current inner-Text source passes the supported full Swift package gate:
1,084 cases and zero failures in
`target/test-logs/swift-package-20261010T003451-57223.log`. The supported ARM64
app/extension build also passes. The binaries retained in
`target/working-tree-completion/ios-app-final-binary-receipt.json` were built at
06:37:12 and 06:37:11 UTC, with app SHA256
`938dbaaa54bc97c8c66d17b45bd93f13314d3cad4121d2c5c25be8d9079a004c`
and extension SHA256
`20ccba6a52580e07131f7c6d1044062697771eb0da33dfe3f55752ea014605f8`.
The new AX1–5 compound matrix, OCR runtime and UIKit runtime are pending. Earlier
AX1/AX3 passes establish the preceding source, rather than accepting this final
matrix before it completes.

The first current inner-Text/OCR cell, AX2 `run.1Tm45B`, executed both compound
tests: Expanded passed in 117.510 seconds; Lock Screen failed in 65.097 seconds
on the new visible-warning assertion. Vision recognized the Critical Expanded
warning as “Reduce acceleration” and the Critical Lock warning as “Reduce
accelerati...”. Independent review of the original Lock OCR screenshot confirms
that exact ellipsis. The native Lock viewport is 374 × 203 points; the preceding
speech/order/geometry checks pass, while the visible warning remains incomplete.
This provides a meaningful runtime RED for the new oracle and establishes that
the inner Text height repair alone is insufficient. The failed Lock case did
not execute its Nominal state. The command stopped before the other four
categories and UIKit runtime; those are unexecuted, rather than accepted.

Selected original screenshots, recognized lines, native owner trees, foreground
receipts, the retained failed runner log and SHA256 manifest are in
`target/working-tree-completion/ui-widget-ax2-inner-text-ocr-review/review-manifest.json`.
The warning oracle and all existing exact native assertions remain unchanged.

The next source repair explicitly reserves the existing two-line text budget
when `lineLimit > 1`. It keeps the requested font, typed value, two-line limit
and native speech representation. This also allocates two lines for nominal AX
chips, so current Nominal screenshots and native viewport bounds must be checked
alongside the Critical OCR result. The earlier native/build receipts precede
this reservation change; final source/build and runtime acceptance are pending.

Current AX2 `run.XLJjho` passes both complete cases: Expanded in 113.796 seconds
and Lock Screen in 123.464 seconds, with zero failures or skips. Independent
review of all four original state screenshots confirms full Speed and safety
text, whole-line Stale and no cropped Grid. The formerly truncated Critical Lock
warning now wraps as “Reduce” / “acceleration”; native Vision recognizes both
full words. The Nominal Lock warning remains complete with its reserved space.
Both Lock compositions measure 229.7 points inside their native viewports.

All four states retain exactly one Headroom, Beeps, Temp and Speed node, the
correct Critical/Nominal native preorder, typed values and current acknowledged
active lifecycle receipts. The 20 original attachments, OCR diagnostics, exact
Activity IDs, native proofs and SHA256 hashes are retained in
`target/working-tree-completion/ui-widget-ax2-reserved-lines-final-review/review-manifest.json`.
This accepts current AX2; the other four categories, final native/build gates
and UIKit runtime remain pending.

Current AX1 `run.7VxlIY` also passes both complete cases, Expanded in 113.643
seconds and Lock Screen in 123.257 seconds. All four original state screenshots
show complete warning/headroom and Speed, whole-line Stale and no cropped Grid;
both Critical OCR diagnostics retain full words. Independent native review
verifies exact-once safety values, correct preorder and all four current
acknowledged Activity IDs. Compositions measure 130.7 points Expanded and 217.7
points Lock Screen. The 20 originals and hashes are in
`target/working-tree-completion/ui-widget-ax1-reserved-lines-final-review/review-manifest.json`.
Current AX1/AX2 are accepted; AX3/AX4/AX5 and UIKit runtime remain pending.

The supported `build:ios-app` task now passes for the current reserved-two-line
source. Its retained ARM64 Simulator binaries were built at 06:55:13/06:55:12
UTC, with app SHA256
`1626a41ab63184518246b200d53922499f2b5426df295ceb6be960c09bc9e532`
and extension SHA256
`397a69832fac6f19f60301dc214bcc6b45683f92ba40ad344ec3eeaca43b31f3`.
The current `ios-app-final-binary-receipt.json` supersedes the preceding 06:37
inner-Text build receipt. The full current Swift package subsequently passed
1,084 cases: 630 Mobile, 449 App and 5 Validator, with zero failures, in
`target/test-logs/swift-package-20261010T005052-69062.log`.

Current AX3 `run.JEpBJB` passes Expanded in 114.708 seconds and Lock Screen in
123.566 seconds. All four original state images show full safety text and Speed,
whole-line Stale and no cropped Grid; both Critical OCR results contain the full
warning. Native exact-once/order checks and all four current acknowledged
Activity receipts pass. The actual compositions are 147.7 points Expanded and
244.7 points Lock Screen, within their respective native viewports. The 20
originals and SHA256/native/OCR proofs are in
`target/working-tree-completion/ui-widget-ax3-reserved-lines-final-review/review-manifest.json`.
Current AX1/AX2/AX3 are accepted; AX4/AX5 and UIKit runtime remain pending.

Current AX4 `run.cb1lEb` passes the complete Lock Screen case but fails Critical
Expanded geometry: the `regular.view` owner is 163.7 points high, against native
160-point viewport wrappers (maxY 177.7 exceeds 175 with the existing one-point
alignment tolerance). Physical Speed remains inside the viewport. The native
Footer representation includes virtual Temp beyond it, so the owner-union
failure needs attribution before inferring physical glyph clipping.

The unchanged original Expanded video frame at 42 seconds shows full “Reduce” /
“acceleration” and Speed, with no observed glyph truncation. No primary Expanded
screenshot or OCR was reached because the geometry assertion failed first; its
Nominal state was not executed. Both original Lock states show complete text,
and Critical Lock OCR recognizes the full phrase. AX4 is not accepted as a
complete pass. The command stopped before AX5 and UIKit runtime. Original native
trees/video, the explicitly attributed unmodified video frame, Lock screenshots,
OCR and hashes are retained in
`target/working-tree-completion/ui-widget-ax4-reserved-lines-failed-review/review-manifest.json`.

Native parent attribution resolves that AX4 failure as an oracle selecting the
outer virtual union. The actual lowest shared body of physical Speed and Footer
is `(32.3, 89, 338, 70.7)`, ending at 159.7 inside the native viewport ending at
174. The original cropped-Grid failure remains distinguishable: its shared body
is `(31.5, 81.8, 339, 190)`, ending at 271.8 outside the same 174-point viewport.
The test now parses native parent relationships and bounds this shared body,
plus physical Speed and the existing visible status/identity text. It preserves
the one-point alignment tolerance and all exact speech/order/value/lifecycle
assertions. Critical OCR runs before geometry to retain original pixels on a
future frame failure.

The characterization covers the old cropped Grid, current AX4 and all 12
retained current AX1–3 state trees; the earlier oversized Stale height rejection
is retained as well. Exact frames and source hashes are recorded in
`target/working-tree-completion/ui-widget-native-body-characterization.json`.
Production is unchanged. Current AX4/AX5 and UIKit runtime still require their
complete rerun receipts; this attribution alone does not establish a pass.

Current AX4 `run.ccT4Ht` passes both complete cases, Expanded in 114.125 seconds
and Lock Screen in 123.684 seconds. Independent review of all four original
state screenshots finds full warning/headroom and Speed, whole-line Stale and
no visual crop. Both Critical OCRs and all native exact-once/order/typed-value
and current acknowledged Activity ID checks pass. The actual shared Expanded
body `(32.3, 89, 338, 70.7)` fits the 160-point viewport; both Lock bodies fit
their 261-point native viewport. The 20 originals, hashes and native body/OCR
proofs are retained in
`target/working-tree-completion/ui-widget-ax4-native-body-final-review/review-manifest.json`.
Current AX1–AX4 are accepted; AX5 and UIKit runtime remain pending.

AX5 `run.NAYzmT` passes the complete Lock Screen case in 125.046 seconds but
fails Critical Expanded geometry in 60.510 seconds. Its native shared body is
`(32.3, 96, 338, 82.7)`, ending at 178.7 beyond the native viewport's 174-point
bottom and unchanged one-point alignment tolerance. Physical Speed fits. The
original Critical Expanded and both Lock screenshots show full warning/headroom
and Speed; both Critical OCRs recognize the complete warning. Nominal Expanded
and the subsequent UIKit runtime task were not executed.

The Expanded leading decorative brand measures 42.3 points. The approved repair
uses its existing caption/bold wordmark alone at AX5, recovering leading-region
height without changing fonts or safety text. AX1–AX4 and regular categories
retain their original `ViewThatFits` branches and the outer CutOut spoken
identity. The strict body, status, physical Speed, OCR and exact-once/order
assertions remain unchanged. The source branch has been independently reviewed;
its complete AX5 rerun and current supported Swift/app/runtime receipts remain
pending. Selected originals, native trees, OCR, current Activity receipts and
hashes are retained in
`target/working-tree-completion/ui-widget-ax5-native-body-failed-review/review-manifest.json`.

The AX5-only wordmark repair passes both complete cases in `run.movDXx`:
Expanded in 115.328 seconds and Lock Screen in 122.900 seconds, with zero failures
or skips. All four original state screenshots have been independently reviewed:
full warning/headroom and Speed, whole-line Stale and no cropped Grid. Both
Critical Vision OCR results recognize the full warning. Native shared-body and
physical Speed bounds, exact-once safety values, critical-first/nominal-first
preorder, typed values and all four current acknowledged Activity ID receipts
pass. The actual Expanded viewport is 151.3 points high; Lock Screen is 279
points. The 20 originals, SHA256 hashes, native body proofs, OCR and copied
passing runner log are retained in
`target/working-tree-completion/ui-widget-ax5-wordmark-final-review/review-manifest.json`.

The current widget acceptance consists of these actual AX5 receipts plus the
retained AX1–AX4 complete receipts for their unchanged source branches. Each
category covers Critical and Nominal on both Lock Screen and Expanded Dynamic
Island: 20 state/surface combinations in total. The AX5-only brand change does
not alter the shared safety layout or smaller-category branches. Current full
Swift/app receipts and locked/music/capture UIKit runtime gates remain pending
under the parent command; this UI result does not establish those separate
gates.

The latest supported full Swift gate for the AX5-only wordmark source passes
1,084 cases, 630 Mobile, 449 App and 5 Validator, with zero failures, in
`target/test-logs/swift-package-20261010T013055-89416.log`. This supersedes the
preceding reserved-lines Swift source receipt. The current supported app build
and locked/music/capture UIKit runtime commands remain pending; the doc is
frozen until those exact receipts arrive.

The latest supported ARM64 Simulator app/extension build also passes for the
AX5-only wordmark source. The exact 07:46:55/07:46:54 UTC binary receipt is
`target/working-tree-completion/ios-app-final-binary-receipt.json`, with its
retained binary copies and build log
`target/working-tree-completion/build-ios-app-largest-brand-final.log`.

The first locked-runtime invocation ran its 25 native assertions successfully,
then hung in Xcode's post-test verbose diagnostics collector. Its interrupted,
unfinalized result bundle is retained as diagnostics and is not runtime
acceptance. The locked/music/capture tasks now use the same
`-collect-test-diagnostics never` policy as the existing UI runner; runtime
assertions and warning checks are unchanged. Music/capture continue serially,
followed by the final locked rerun. Their complete receipts remain pending.

Music-monitor and capture-staging now have complete finalized runtime receipts:
11/11 and 7/7 respectively, with zero failures or skips. The original bundles
are retained under
`target/working-tree-completion/final-runtime-result-bundles/{music-monitor,capture-staging}.xcresult`;
the parsed receipts are `ios-music-monitor-final-summary.json` and
`ios-capture-staging-final-summary.json` in the same completion directory.
These establish 18 runtime cases. The enclosing interrupted serial command
exited 130 and does not establish an aggregate pass. The unfinalized first
locked bundle remains diagnostic only; its final 25-case rerun is pending in
`test-ios-locked-ride-collector-final.log`.

The final locked-only rerun exits zero with a complete finalized bundle:
25/25 cases, zero failures or skips. Its original bundle is retained at
`target/working-tree-completion/final-runtime-result-bundles/locked-ride.xcresult`,
with `ios-locked-ride-final-summary.json`, per-bundle hash manifests and
`ios-runtime-final-receipt.json`. Together with the separately finalized music
and capture bundles, this establishes 43 current ARM64 UIKit runtime cases.
It does not change the earlier interrupted serial command's exit status.

The current software acceptance is complete: retained 26-case Ride/Map/music/
navigation/actual-preference coverage, the enabled Brightness/Rust-write case,
all 20 Live Activity state/surface/category combinations, 1,084 native Swift
cases, the supported ARM64 app/extension build and 43 finalized UIKit runtime
cases. The AX1–AX4 widget receipts apply to their unchanged branches; AX5 uses
the latest repaired branch and its own complete original receipts. Physical
wheel/provider/iPhone acceptance, including the locked-ride operating-system
termination boundary under LIBCU-711 and physical Live Activity acceptance
under LIBCU-875, remains separate. No physical-device claim follows from these
Simulator fixtures.
