# Spotify music integration

CutOut uses the official `SpotifyiOS` SDK for authorization and session renewal.
New connections request `user-read-playback-state` and
`user-modify-playback-state` and use Spotify's Web API for playback status and
the existing previous, play, pause, and next controls. This path works when the
local App Remote socket refuses connections. Spotify remains the playback
engine. The shared
Rust music contract owns validation, transition classification, ride-history
privacy, and persistence. SDK objects, tokens, artwork bytes, and callbacks
stay in the iOS adapter.

## Build and registration

1. In the Spotify Developer Dashboard, register bundle identifier
   `lol.cutout.app`, redirect URI
   `cutout-spotify://spotify-login-callback`, and the iOS SDK. These values must
   match exactly.
2. Install Spotify and sign in on the physical iPhone used for validation. A
   Premium account is required for on-demand track playback. In development
   mode, the app owner must have Premium; add any different validation account
   under Users Management before authorizing it.
3. Store `CUTOUT_SPOTIFY_CLIENT_ID` in the Devenv SecretSpec development
   profile's OS keyring. The physical UI test runner and device deployment
   resolve it with the `ios-device-deploy` scope; ad-hoc export uses its own
   signing scope. The shared Xcode build helpers pass it as the
   `SPOTIFY_CLIENT_ID` build setting, which is substituted into `Info.plist`
   at build time and is never committed. Physical builds fail instead of
   silently producing a Spotify-disabled app when it is missing. Local Mac
   builds can also use `CUTOUT_SPOTIFY_CLIENT_ID`, `SPOTIFY_CLIENT_ID`, or
   `~/.config/libcutout/spotify-client-id` directly.
4. The app declares the `spotify` query scheme and the `cutout-spotify` URL
   callback scheme. The callback is forwarded through SwiftUI's `onOpenURL`.

The package pins Spotify's official `ios-sdk` Swift package at `5.0.1` and
links it only for iOS. macOS and builds without a client ID retain the typed
unavailable/handoff state.

The pinned SDK implements PKCE and renews directly through Spotify's token
endpoint when `tokenRefreshURL` is nil. `tokenSwapURL` and `tokenRefreshURL`
are optional backend overrides; CutOut does not need or embed a client secret.
The SDK session, including its refresh token, stays in Keychain.

## Lifecycle and policy

Live music observation and reconnect do not require a ride. Scan screen
Setup opens general settings; Setup → Music owns provider selection, history
preferences, Connect, and explicit Reauthorize Spotify. Opening settings,
selecting a provider, restoring the player, and starting a ride do not launch
authorization. Persisted monitoring restores a foreground connection using
cached credentials only. Only an explicit Connect/Reauthorize action grants
one authorization attempt; returning from Spotify or the background resumes
observation without granting another authorization attempt.

An older App Remote-only grant remains usable until the rider explicitly taps
Connect to grant the playback API permissions once. Passive restoration never
upgrades permissions or opens authorization. Web API monitoring requires an
internet connection, polls every five seconds while foregrounded, honors
`Retry-After`, and silently renews an access token rejected with HTTP 401.
Reading playback never starts music. Explicit transport commands address the
device from the latest playback response and never transfer playback.

For older grants, App Remote disconnects when the app enters the background and reconnects in
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

Web API references: [Playback state](https://developer.spotify.com/documentation/web-api/reference/get-information-about-the-users-current-playback)
and [Refreshing tokens](https://developer.spotify.com/documentation/web-api/tutorials/refreshing-tokens).

For device validation, a Debug build launched with `--validate-spotify-renewal`
performs one silent SDK renewal on startup when a playback API grant is saved.
It does not change the saved expiration or delete credentials. A subsequent
normal launch verifies restoration of the renewed session.
