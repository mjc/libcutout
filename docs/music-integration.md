# Music controls and ride listening history

## Product contract

Music integration lets the rider explicitly control a supported provider and,
separately, opt into a private on-device record of which songs they listened to
during a ride. History retains bounded identifiers and timestamps, plus optional
titles and artists under the user's retention policy. Portable admission,
ordering, privacy, persistence, and deletion remain Rust-owned; Swift owns the
provider SDK and UI boundaries.

Live now-playing and explicit provider controls work without a ride, including
on Map. Ride state gates only recording listening-history metadata, never
provider connection recovery or displaying playback. General Setup is reached
from the scan screen's Setup button; its Music page owns provider preferences
and explicit authorization. Foreground restoration may reuse authorization but
must not launch a new authorization flow.

Future **ride replay** means replaying route and telemetry while displaying which
song was playing at the corresponding ride time. Play, pause, and seek on that
replay must control ride data only. They must not issue provider play, pause,
seek, queue, or track-selection commands. Explicit live music controls are a
separate user action. This contract does not claim ride-replay UI is implemented.

## Audio is excluded

- Do not capture provider audio, system audio, or microphone audio, including
  incidental background music. Do not add audio capture to record a ride.
- Do not store, mux, or export a soundtrack or synchronize music playback with
  route animation, telemetry, or video.
- Do not introduce FFT, audio analysis, provider-derived beats, or music-driven
  RGB. Displaying a historical song name is not an audio-reactive visualizer.
- Any separate camera/import feature must not acquire audio merely to support
  this integration. Handling audio already present in an external artifact is a
  separate contract; listening-history support does not authorize retaining it.

## Local metadata and artifacts

Saving listening history is not an upload or sharing action. Keep it opt-in and
off by default in the ride database. BTLE diagnostic captures have no music
purpose: do not attach provider observations, listening identifiers or music
context to their records, writer startup, or exports. Preserve the raw Bluetooth
evidence independently of music settings. PEVCAP 1.5 contains transport records,
locations and settings observations. Runtime readers reject earlier versions,
standalone music events and embedded music fields, including null fields.

Separate user-exported copies are outside the app's managed capture store. Do not
claim that local cleanup removes those copies. No audio belongs in the music event
format or the BTLE capture path.

The Music setting is a persisted preference for future rides. An explicit change
also updates the active ride when Rust accepts the write. Rust commits the saved
preference with each newly created ride, including automatic wheel-connected
rides. The active ride's effective policy remains separate: restoring,
associating, or resuming a ride must preserve its existing retention state.
Deleted history stays deleted even when the preference enables history for the
next ride. With no active ride, saving the preference must not enable history
writes. Delayed history readbacks must not overwrite a newer policy change or
deletion.

## Spotify disconnection

Music settings provides an explicit Spotify disconnect action. It persists the
monitoring opt-out, retires provider work and clears authorization before removing
Spotify listening events through the Rust database owner. Cleanup preserves rides,
other providers and their history policies; pending presentation readbacks must
not restore disconnected Spotify data. A failed credential or database cleanup
must remain retryable without reconnecting or reporting success.

The existing Bluetooth capture corpus is migrated separately to PEVCAP 1.5,
removing music metadata while preserving its raw Bluetooth evidence and
recording/ride associations. Migrated content-addressed artifacts receive new,
accurately indexed identities. Spotify disconnection now cleans ride listening
history; it has no capture writer, file rewrite or capture cleanup target.

## Review and wording

Use "ride listening history" for stored song metadata and "ride-data replay"
for route/telemetry playback. Do not describe the feature as a soundtrack or
music replay. Permission text must disclose the actual optional metadata storage.

[Apple's App Review Guidelines, section 4.5.2](https://developer.apple.com/app-store/review/guidelines/#apple-music)
separately discuss metadata use and deeper integrations such as timed song
playback. The latter is not this feature. The metadata-use restriction remains
relevant, but the text does not explicitly settle local listening-history use.
A review asserting a violation must explain applicability to this metadata-only
workflow; do not turn an unverified interpretation into a requirement to remove
history or obtain synchronization rights. Local storage alone is not proof of
blanket provider-policy approval either.

## Acceptance for future replay work

- Fake-provider tests must show opening, playing, pausing, seeking, and closing a
  ride replay issue zero music transport commands and preserve current playback.
- Ride recording and replay must not request audio input or add audio tracks.
- Historical metadata follows the saved ride clock and retention state without
  changing live-player state or reconstructing deleted data.

These are requirements for future replay integration, not claims of existing
test coverage. This clarification changes documentation and permission copy;
it does not add playback, capture, or replay behavior.

## Explicit Stop and accepted observations

The map Core binds the first music lifecycle's existing bounded admission queue.
Replacement native models join that same queue before admitting observations.
Each lease retains its original provider owner; classification and retirement
validate that owner, while Stop covers the Core's complete queue. A lifecycle
that already used another queue cannot be rebound and lose its obligations. An
explicit asynchronous Stop captures its admitted prefix before returning the
command. The existing Rust lifecycle worker waits for required history effects;
observations admitted after the cutoff stay pending until the durable Stop receipt. Native code carries the request and the actual history
outcome without maintaining a second queue.

Only the music command's actual SQLite result can settle required history.
The app supplies no BTLE capture target for music observations. Optional history
readback and presentation do not define successful settlement. Music observation
APIs have no capture target or capture-writer receipt.

Release, cancellation, provider retirement, and storage failure cannot
acknowledge successful history recording. Stop reports `MusicObservationIncomplete`
and leaves the ride open when its accepted prefix is incomplete. The failure is
reported once; an explicit Stop retry can close the ride. Pending history retains
its original association and is never rebound to a replacement ride. Synchronous
Stop rejects outstanding work instead of waiting while holding the Core mutex.

The native required-error catch reports failed processing to the original Rust
request before optional readback. Rust marks only that retained nonterminal
obligation failed, wakes the next required owner, and preserves publication
currency. Repeated failure reports and reports after settlement or retirement
are no-ops. Optional readback errors do not change required recording outcomes.
