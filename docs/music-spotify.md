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
3. Supply the client ID as the `SPOTIFY_CLIENT_ID` Xcode build setting. It is
   substituted into `Info.plist` at build time and is never committed.
4. The app declares the `spotify` query scheme and the `cutout-spotify` URL
   callback scheme. The callback is forwarded through SwiftUI's `onOpenURL`.

The package pins Spotify's official `ios-sdk` Swift package at `5.0.1` and
links it only for iOS. macOS and builds without a client ID retain the typed
unavailable/handoff state.

## Lifecycle and policy

App Remote is connected while the app is active and disconnected when the app
enters the background. Player-state updates are projected into the same
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
