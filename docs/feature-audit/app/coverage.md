# Shared app, dashboard, music and Live Activity coverage

Source revision: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`. Read-only phone diagnostic retrieval supplements source review. Source interpretation is not a full interaction test; all findings remain open.

| Feature | Review result | Further acceptance needed |
| --- | --- | --- |
| Cold/warm launch | CRH-003/004/005/009/010 | Exact installed artifact with large accumulated history |
| Connected tab rendering and appearance | CRH-001; actual phone traps | Repeat with current signed binary and appearance transitions |
| Main app CPU use | CRH-006; actual process kill | Symbolicate historical samples and measure sustained work |
| Recording storage footprint | CRH-008; actual nonfatal resource reports | Producer attribution and write amplification measurement |
| EUC wheel speed | Shared typed formatter and freshness checked; no new conversion defect found | Per-wheel scale/calibration and reverse motion |
| Phone GPS speed | Distinct GPS tile exists; RID-009 covers Map mismatch | GPS accuracy, reduced permission, stale samples |
| Pack voltage | Typed and separately labeled; no new base-unit defect found | Model-specific voltage correction/calibration |
| Battery charge estimate | Provenance/readback presentation inspected; no independent estimator defect claimed | Compare actual pack and chemistry, observed versus estimated |
| Sag-adjusted energy | APP-001 permanently unavailable | Decide whether to hide or implement a defined model |
| Limp-home range | APP-002 no live producer | Honest unavailable state and a defined estimator if implemented |
| Battery versus motor current/power flow | Shared DTO/readouts distinguish current types and signed flow | Protocol-specific measured/estimated provenance and scaling |
| Thermals | Controller/motor/battery names and °C formatting inspected | Per-sensor identity; SET-015/017 cover BMS issues |
| Phone unit preferences | RideUnits hardcodes mph/°C; no setup preference exists | SET-010 and RID-019 cover inconsistent surfaces; decide coherent app units |
| PWM/headroom display and warnings | Complementary concepts traced; SET-001 control mismatch | Firmware basis and hardware/software measurement provenance |
| Stale hero telemetry | Two-second freshness plus periodic UI reevaluation present | Actual packet loss, background transitions and per-field age |
| Phone audible/haptic alarms | APP-006 capability gap | Define delivery independently of wheel firmware settings |
| VESC duty/headroom/angle/footpad | Typed presentation and warnings inspected; no new base-unit defect claimed | Board profile/geometry and actual Refloat/other firmware; no tuning UI found |
| Dynamic Type/layout | Native adaptive grid, scaled hero, ViewThatFits present | Small phones, large text and landscape; source presence is not UI acceptance |
| VoiceOver warnings and navigation | Announcement paths present; APP-005 visibility asymmetry | Real focus and announcement behavior during rapid changes |
| Localization | Source uses catalogs and locale formatters; no full language audit | Full translation completeness and long/RTL text remain unreviewed |
| Music provider selection/authorization | Explicit setup; CRH-002/003/009 failure families | On-device authorize/cancel/renew/open for both providers |
| Music play/pause/skip/handoff | APP-003 discarded outcomes; APP-004 completion hypothesis | Failed/late callbacks and repeat taps |
| Music compact/expanded controls | APP-007 minimum hit-size hypothesis | Measure actual hit regions and VoiceOver order |
| Music artwork | Bounded cache and generation-aware fetches exist | Decode/render profiling and cancellation with real artwork |
| Hide/restore compact player | Visibility preference and restoration present | History independence and page transitions |
| Listening history and deletion | RID-011/012/015/023; contract read | Opt-in behavior, observation gaps, retained artifacts |
| Live Activity start/update/end/stale state | Coordinator guards/throttling inspected; APP-005 error visibility | OS background behavior, stale deadlines and denied permission |
| Live Activity rendering/CPU | CRH-007 repeated nonfatal CPU reports | Device profiling of lock screen and Dynamic Island |
| Camera recording | No camera route or camera capture UI in this checkout | Separate branch/deployment scope; no camera feature accepted as tested |
| Ride-data replay | Current contract calls it future work; no playback controls found | Do not conflate route inspection with playback or music replay |
| Firmware updates / social features / watch app / third-party HUD | No audited end-to-end phone surface | Competitor feature presence does not imply Cutout has implemented them |

The [comparison baseline](../comparison-baseline.md) records primary published references. Three agents independently reviewed root app/crash findings and BMS/connection/riding traces. Raw phone reports stay outside Git; [phone-evidence.md](../phone-evidence.md) preserves sanitized report identity and hashes.
