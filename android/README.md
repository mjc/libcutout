# Android app

Run commands from the repository root through its devenv. The production app is
`lol.cutout`; the instrumentation target is `lol.cutout.test`. ARM64 Android 13+
is required. Generated bindings and native libraries remain ignored.

```sh
devenv tasks run test:kotlin-bindings-smoke
devenv tasks run build:android-app
CUTOUT_ANDROID_DEVICE=ADDRESS:5555 CUTOUT_ANDROID_SERIAL=PHONE_SERIAL devenv tasks run test:android-device
CUTOUT_ANDROID_DEVICE=ADDRESS:5555 CUTOUT_ANDROID_SERIAL=PHONE_SERIAL devenv tasks run run:android-app
```

`run:android-app` builds, verifies the selected physical device, installs without
clearing data and launches the real activity. `test:android-device` uses the same
build/install path, then runs the shared assertions and production adapter/UI
tests. Each invocation records its receipt and logs under `target/test-logs`.
The runner refuses ambiguous or unavailable targets instead of falling back.

Select the current transport with `devenv shell -- adb devices -l`. USB serials
and authorized Wi-Fi endpoints are supported. To restore TCP debugging over USB,
use `adb -s PHONE_SERIAL tcpip 5555`, then `adb connect ADDRESS:5555`, through the
same devenv. Verify the network endpoint's hardware serial before installation.

Keep one native build active. Reuse the existing Cargo/Gradle caches; the shared
runner caps Cargo at one job and Gradle at two workers/1536 MiB. `build:android-app`
is an alias for the existing APK build task, not a separate native pipeline.

The app opens on Devices. Ride becomes available with a Rust connection attempt;
Settings renders the shared ordinary-configuration catalog. The panels follow
the current SwiftUI picker, ride instruments and grouped settings, with native
Android controls and system appearance. They use the production Bluetooth
adapter and Rust-owned projections. Scanning requests Nearby
Devices permission. Background ride survival and live capture/export are separate
work under LIBCU-35/36. Exact-wheel live acceptance is still required for LIBCU-34.
See [the mobile boundary contract](../docs/mobile-ffi.md#android-app-and-physical-tests).

Instrumentation renders the existing public EUC capture through `PevSession` to
exercise the actual Ride view. Picker tests consume Rust discovery candidates;
settings tests inject only the native submission callback, so they cannot send
commands to a wheel. Presentation fixtures are excluded from the production APK.
