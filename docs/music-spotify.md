# Spotify music integration

CutOut integrates Spotify through the official `SpotifyiOS` App Remote SDK. The
Spotify app remains the playback engine; CutOut receives player-state metadata
and sends the supported previous, play, pause, and next commands. The shared
Rust music contract owns validation, transition classification, ride-history
privacy, and persistence. SDK objects, tokens, artwork bytes, and callbacks
stay in the iOS adapter.

## Build and registration

1. Register the CutOut bundle identifier and redirect URI
   `cutout-spotify://spotify-login-callback` in the Spotify Developer
   Dashboard. Enable the iOS SDK/App Remote product.
2. Install Spotify and sign in on the physical iPhone used for validation. A
   Premium account is required for on-demand track playback.
3. Supply the client ID through `CUTOUT_SPOTIFY_CLIENT_ID`, `SPOTIFY_CLIENT_ID`,
   or `~/.config/libcutout/spotify-client-id`. The shared Xcode build helpers
   pass it as the `SPOTIFY_CLIENT_ID` build setting, which is substituted into
   `Info.plist` at build time and is never committed. Installable device and
   archive builds fail instead of silently producing a Spotify-disabled app
   when it is missing.
4. The app declares the `spotify` query scheme and the `cutout-spotify` URL
   callback scheme. The callback is forwarded through SwiftUI's `onOpenURL`.

The package pins Spotify's official `ios-sdk` Swift package at `5.0.1` and
links it only for iOS. macOS and builds without a client ID retain the typed
unavailable/handoff state.

## Lifecycle and policy

Live music observation and reconnect do not require a ride. Scan screen
Setup opens general settings; Setup → Music owns provider selection, history
preferences, Connect, and explicit Reauthorize Spotify. Opening settings,
selecting a provider, restoring the player, and starting a ride do not launch
authorization. Persisted monitoring restores a foreground connection using
cached credentials only. Only an explicit Connect/Reauthorize action grants
one authorization attempt; returning from Spotify or the background resumes
observation without granting another authorization attempt.

App Remote disconnects when the app enters the background and reconnects in
the foreground, including on Map with no active ride. Generic transport or
wakeup failures retain credentials and use bounded Rust-owned retries. A lost
player-state callback cannot permanently block subsequent requests. Player-state
updates are projected into the same
bounded observation path used by Apple Music. Missing Spotify, cancelled or
failed authorization, logout, token expiry, disconnect, and unavailable
account states disable transport controls and keep the provider handoff
available.

Do not request or retain Spotify audio, PCM, Audio Analysis, beats, bars,
sections, segments, loudness, timbre, lyrics, or other analysis data. Spotify's
platform terms also prohibit synchronizing Spotify content with visual media;
this integration therefore never drives RGB or visualization effects.

References: [Spotify iOS SDK](https://developer.spotify.com/documentation/ios),
[Getting Started](https://developer.spotify.com/documentation/ios/getting-started),
[Application Lifecycle](https://developer.spotify.com/documentation/ios/concepts/application-lifecycle),
and [Making Remote Calls](https://developer.spotify.com/documentation/ios/tutorials/making-remote-calls).
