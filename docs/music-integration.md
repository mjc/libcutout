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
off by default. Existing explicit ride/PEVCAP export paths must honor their
metadata policy and redaction rules; local files can contain opted-in metadata.
Do not describe database deletion as deleting separate exported copies unless
the implementation actually does so. No audio belongs in the music event format.

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
