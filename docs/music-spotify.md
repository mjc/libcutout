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

Both native clients use `cutout-music` for bounded Web API metadata parsing,
capability projection, request construction and monotonic rate-limit deadlines.
The native HTTP effect adds its access token to the Rust-authorized request;
Rust never receives or persists that token. Commands target the device in the
latest playback observation and do not transfer playback.

## Android authorization and builds

Android uses [AppAuth-Android 0.11.1](https://github.com/openid/AppAuth-Android/tree/0.11.1)
for [browser-based authorization-code PKCE](https://developer.spotify.com/documentation/web-api/tutorials/code-pkce-flow) and
token renewal. Register the same exact redirect URI,
`cutout-spotify://spotify-login-callback`, in the Spotify app. The AppAuth
redirect receiver forwards its result through a non-exported result Activity to
the app-owned music adapter. Rust owns the explicit authorization transaction
and provider lifetimes. Opening Music settings or restoring the app does not
grant permission to start authorization.

`devenv tasks run build:android-app` loads the existing public
`CUTOUT_SPOTIFY_CLIENT_ID` through SecretSpec's `android-app-build` scope when no
client ID is already supplied in the environment. The scope selects only that
identifier. Gradle also accepts `SPOTIFY_CLIENT_ID` and generates the client ID
in the ignored `BuildConfig` output. No OAuth client secret, access token or
refresh token is a build input. The generic `build:android-tests` task remains
usable without a configured provider; that app exposes Spotify's unavailable
state until a client ID is supplied.

The serialized AppAuth grant is encrypted with a non-exportable Android
Keystore AES-GCM key and stored atomically in the app's no-backup directory.
Credential read, write and deletion failures remain distinct from an absent
grant. Explicit Spotify disconnect retires pending provider work and removes
credentials before Rust deletes Spotify listening history. It preserves rides
and other providers' history.

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
