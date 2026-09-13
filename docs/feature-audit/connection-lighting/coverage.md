# Connection and Lighting audit coverage

Reviewed source revision `34b6309d7db2ffe2aadbce7bfca839a3e187d95b` on 2026-09-13. This slice traces SwiftUI entry points through Swift adapters, Rust MELK command/reducer/persistence code, and relevant existing tests. It does not establish physical controller behavior or reproduce a phone crash.

MCPLS was unavailable. Repository commands used the cached same-project Nix shell supplied by the root audit after the ordinary flake could not fetch its environment. Ponytail and SwiftUI Expert review guidance was read; no implementation changes, generated FFI edits, commits, or pushes were made. The music scope contract was read; Lighting Music refers only to the accessory controller microphone.

## Issues

| ID | Issue | Severity | Evidence |
| --- | --- | --- | --- |
| [CON-001](CON-001-device-switch-stale-callbacks.md) | Switching devices can let the previous connection mutate the new session | P1 | source-confirmed defect |
| [CON-002](CON-002-wheel-connect-no-deadline.md) | Wheel connection and GATT discovery can remain Connecting indefinitely | P2 | source-confirmed defect |
| [CON-003](CON-003-transient-link-loss-discards-page.md) | A failed wheel reconnect discards the current Tune or Pack page | P2 | UX inconsistency |
| [CON-004](CON-004-standalone-lighting-shell-changes.md) | Home Lighting changes navigation ownership with wheel selection | P2 | UX inconsistency |
| [CON-005](CON-005-melk-discovery-no-deadline.md) | Lighting can become stuck discovering after the BLE connection succeeds | P2 | source-confirmed defect |
| [CON-006](CON-006-evicted-lighting-candidates-remain-visible.md) | Some nearby accessory rows remain selectable after the core has forgotten them | P2 | source-confirmed defect |
| [CON-007](CON-007-schedule-drafts-look-like-state.md) | Saved lighting timers reappear as disabled defaults | P2 | UX inconsistency |
| [CON-008](CON-008-music-no-explicit-start.md) | Lighting Music has a Stop button but no clear way to start its displayed default | P2 | UX inconsistency |
| [CON-009](CON-009-effect-previews-invent-colors.md) | Effect thumbnails show invented colors and geometry | P2 | UX inconsistency |
| [CON-010](CON-010-browsing-effect-group-fakes-selection.md) | Browsing effect groups highlights a pattern that was never sent | P2 | source-confirmed defect |
| [CON-011](CON-011-preset-speed-uses-inverse-raw-byte.md) | Preset summaries use the inverse native speed instead of the speed shown in controls | P3 | UX inconsistency |
| [CON-012](CON-012-restore-confirmed-without-baseline.md) | Lighting says Confirmed while Restore has no usable baseline | P2 | UX inconsistency |
| [CON-013](CON-013-prepair-restore-toggle-is-discarded.md) | Enabling Restore before pairing is silently undone | P2 | source-confirmed defect |
| [CON-014](CON-014-lighting-has-no-disconnect.md) | A remembered lighting accessory cannot be temporarily disconnected | P2 | UX inconsistency |
| [CON-015](CON-015-stop-music-leaves-microphone-command-on.md) | Stop Music never sends the protocol microphone-disable command | P2 | hypothesis needing device validation |
| [CON-016](CON-016-solid-color-off-state-not-reasserted.md) | Changing solid color while off bypasses the off-preserving state plan | P2 | hypothesis needing device validation |
| [CON-017](CON-017-metadata-save-errors-hidden.md) | Lighting alias and vehicle association can fail silently | P2 | source-confirmed defect |
| [CON-018](CON-018-lighting-state-has-no-readback-label.md) | Initial Lighting controls look like observed device state | P2 | UX inconsistency |
| [CON-019](CON-019-microphone-source-unclear.md) | Lighting Music does not explain which microphone it uses | P3 | UX inconsistency |

## Feature inventory

| Individual feature | Coverage | Result or gap |
| --- | --- | --- |
| First launch / automatic scanning | Reviewed | CON-002; scan starts from root model; real permission prompt UX remains device validation. |
| Bluetooth denied/unavailable states | Reviewed | Status rendering exists; navigation impact CON-003. Permission recovery flow needs device review. |
| Remembered wheel selection | Reviewed | Selection/restoration gates inspected; CON-004 shell dependency. |
| Choose supported device / protocol probe | Reviewed | CON-001 late callbacks; CON-002 connect/discovery deadline. |
| Switch devices during connect | Finding | CON-001. |
| Empty GATT inventory | Finding | CON-002. |
| Protocol detection / fallback exhaustion | Reviewed, no new finding | Current fallback/record-only boundary traced; no broad firmware compatibility proof. |
| Unknown device record-only capture route | Reviewed, no new finding | Record-only excludes Ride and live activity; capture internals assigned to other audit area. |
| Reconnect jitter/backoff | Reviewed, no new finding | Existing bounded controller inspected; actual RF loss/recovery untested. |
| Retry exhaustion / Bluetooth loss navigation | Finding | CON-003; ordinary retry state preserves route. |
| Restoration / background BLE lifetime | Partial | WillRestoreState and startup reuse inspected; OS process termination/restoration requires real-device validation. |
| Home Lighting entry / ride Lighting entry | Finding | CON-004. |
| Standalone accessory central lifetime | Finding | CON-014; separate central identity is retained. |
| Accessory first-pair selection | Finding | CON-006; candidate eviction and visible list diverge. |
| Accessory remembered identifier matching | Reviewed, no new finding | UUID validation and exact identifier restore inspected. |
| Accessory GATT/notify/handshake readiness | Finding | CON-005; incomplete discovery has no deadline. |
| Accessory retry count / connect timeout | Reviewed, no new finding | Reducer bounds retry and connection attempts; hardware callback races untested. |
| Accessory BLE write backpressure/coalescing | Reviewed, no new finding | Bounded writes and latest-color coalescing inspected; no throughput measurement. |
| Power toggle / readback semantics | Finding | CON-018; normal write maps to typed power frame. |
| Solid RGB wheel / white / quick colors | Finding | CON-016 off-state output hypothesis; palette and accessible sliders inspected. |
| Brightness range and commit | Reviewed, no new finding | 0–100 typed range and rejected-write rollback inspected; exact physical curve untested. |
| Effect catalog names / groups / all IDs | Reviewed | 0–227 bounded catalog exposed; no claim of exact OC21 visual parity. |
| Effect preview thumbnails | Finding | CON-009 arbitrary illustration is not labeled. |
| Effect group browsing / current selection | Finding | CON-010. |
| Effect speed direction / byte limits | Reviewed | Main slider inverse-byte conversion coherent; CON-011 preset summary differs. |
| Effect speed parity with official app | Runtime validation required | Repository docs retain older report of slower effects; this audit did not revalidate that report. |
| Microphone effect activation | Finding | CON-008. |
| Microphone sensitivity | Finding | CON-008 inactive commit behavior; actual response remains unverified. |
| Microphone source explanation | Finding | CON-019; accessory microphone distinct from phone provider music. |
| Stop microphone mode | Hypothesis | CON-015; explicit microphone-disable semantics require hardware validation. |
| Timer on/off slots / time / weekday mask | Finding | CON-007 drafts reset; clock and slot write ordering inspected. |
| Timer physical firing / DST / one-shot behavior | Runtime validation required | No OC21 timer execution/readback evidence collected. |
| Scene save / replace / delete | Reviewed | Existing typed lifecycle inspected; CON-011 speed summary. Persistence tests read, not executed. |
| Restore preference before pairing | Finding | CON-013. |
| Restore confirmation / initial baseline | Finding | CON-012; conservative complete baseline gate is intentional but undisclosed. |
| Restore identity / version / fingerprint | Reviewed, no new finding | Same-accessory, schema, capabilities fingerprint, and confirmed-state guards inspected. |
| Accessory alias / vehicle association | Finding | CON-017 error propagation. |
| Forget accessory | Reviewed | Removes complete record; CON-014 temporary disconnect gap. |
| Built-in Aero headlight / high beam / ring / tail settings | Assigned elsewhere | Settings audit owns built-in EUC lighting; these are not MELK accessory capabilities. |
| Phone crash reports / actual render crashes | Assigned elsewhere | Root reviewing device diagnostics and shared SwiftUI shell; no crash reproduced in this slice. |

## Evidence limits and next pass

Source-confirmed defects describe demonstrable control flow, state, and error-handling failures; they do not assert an observed iPhone symptom. UX inconsistencies describe concrete presentation/state mismatches requiring product decisions. The two hardware hypotheses require exact OC21 captures and visible output checks. Existing tests were inspected as evidence of the current contract; no test pass is claimed by this slice.

Prioritize rapid A→B wheel switching, absent/empty GATT callbacks, long accessory scans, group browsing during an active effect, fresh-accessory restore setup, and leaving/reopening Schedule. Then test controller microphone Stop and direct RGB edits while Off. Keep built-in Aero light tests and MELK accessory tests separate. Competitor-specific claims are delegated to the central verified comparison notes.
