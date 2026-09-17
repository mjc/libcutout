# iOS TestFlight readiness

This is the source audit for the `lol.cutout.app` iPhone build. Recheck the
signed archive and App Store Connect before inviting testers; source inspection
alone cannot establish what a third-party SDK includes or what Apple accepts.

## Permission and background-use justification

| Access | Trigger and purpose | Project declaration |
| --- | --- | --- |
| Bluetooth | Connect to the rider's vehicle and read live telemetry; `bluetooth-central` supports the active connection when the app is backgrounded. | `NSBluetoothAlwaysUsageDescription` and `UIBackgroundModes` |
| Location, While Using | Record a ride route, speed, altitude, and direction. A ride begun in the foreground continues receiving location while backgrounded or locked. The app does not request Always access. | `NSLocationWhenInUseUsageDescription`, `UIBackgroundModes` (`location`), and `allowsBackgroundLocationUpdates` during an active ride |
| Apple Music | Show and control playback when the rider explicitly connects it; optional on-device ride listening history records song metadata, not audio. | `NSAppleMusicUsageDescription` |
| Spotify | User-initiated handoff to the Spotify app and App Remote; no iOS protected-resource prompt. | Spotify URL scheme, query scheme, and dashboard bundle ID/redirect URI |
| Live Activity | Show current ride data on the Lock Screen. It is not itself a location authorization. | `NSSupportsLiveActivities` and extension target |

No camera, microphone, Photos, Health, motion, notification, or tracking
authorization call or usage-description key was found in the app source at this
audit. Revisit this if features or SDKs change. The background modes are
`bluetooth-central` and `location`; neither is a substitute for an iOS
permission prompt.

The app privacy manifest declares these required-reason API uses:

| API category | Reason | Source use |
| --- | --- | --- |
| UserDefaults | `CA92.1` | App-owned preferences and local state |
| System boot time | `35F9.1` | Monotonic elapsed time for ride and music events |
| File timestamps/metadata | `C617.1` | Size of a capture file in the app container |

These reasons describe this app's code. The Spotify iOS SDK 5.0.1 in the
simulator build includes its own privacy manifest, declaring no tracking,
collected data, or required-reason APIs. Check the generated Xcode privacy
report and observed network behavior before answering App Privacy questions.
Data stored only on the device is different from data sent to the developer or
a third party; exports and explicit sharing need their own review.

An unsigned arm64 simulator build and `cargo cutout ios verify-app` passed on
2026-09-17 against current `main`. The verifier confirmed both bundle IDs,
purpose strings, background modes, the absence of Always location keys, and
privacy manifests in the app and Live Activity extension. A signed archive and
physical-device background ride remain unverified.

## Remaining gates

### Before the first internal TestFlight build

- [ ] Confirm the Apple Developer account can register the app and Live
  Activity extension IDs, then create the App Store Connect record for
  `lol.cutout.app` before upload.
- [ ] Build and sign a Release archive with the repository's generated Swift
  FFI and Spotify client ID. Inspect the final app and extension identifiers,
  `Info.plist`, entitlements, and embedded privacy manifests. Generate Xcode's
  privacy report and resolve any upload warnings from Apple or included SDKs.
- [ ] Run a physical-device ride with Bluetooth and While Using location:
  lock the screen, background and reopen the app, confirm route samples and
  Live Activity updates, end the ride, and confirm location updates stop.
  Repeat after denying location and Bluetooth to check the fallback UI.
- [ ] Answer App Store Connect's export-compliance questions for the actual
  archive. Do not predeclare an encryption exemption without inspecting the
  shipped dependencies and Apple's questionnaire.

### Before external TestFlight review or App Store release

- [ ] Publish a working privacy policy URL and verify that it matches the
  shipped app, Spotify integration, optional local music history, exports,
  and any SDK data transfer. Complete App Privacy questions from the final
  archive's privacy report and observed network behavior.
- [ ] Add TestFlight beta description, feedback email, review contact, and
  specific reviewer instructions. Explain how to reach a demo ride or provide
  compatible test hardware/account access where required. State that Bluetooth
  and background location run only for an active ride and that Spotify and
  Apple Music are optional.
- [ ] Complete the App Store product metadata and age rating when preparing
  public release; verify current screenshots, support URL, and distribution
  settings in App Store Connect.

Draft reviewer note, after supplying a workable hardware/demo path:

> CutOut connects to a compatible personal electric vehicle by Bluetooth and
> records a ride route on the phone. Start a ride while the app is open. During
> that active ride, Bluetooth telemetry and location updates continue with the
> screen locked so the route and Live Activity remain current. Ending the ride
> stops location updates. The app requests While Using location; it does not
> request Always location. Spotify and Apple Music controls are optional and
> can be skipped. [Add exact compatible hardware or demo instructions here.]

Apple references: [protected-resource purpose strings](https://developer.apple.com/documentation/bundleresources/protected-resources),
[When In Use background location](https://developer.apple.com/documentation/corelocation/requesting-authorization-to-use-location-services),
[required-reason APIs](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api),
[App Privacy](https://developer.apple.com/help/app-store-connect/manage-app-information/manage-app-privacy),
[TestFlight test information](https://developer.apple.com/help/app-store-connect/test-a-beta-version/provide-test-information),
and [export compliance](https://developer.apple.com/help/app-store-connect/manage-app-information/determine-and-upload-app-encryption-documentation).
