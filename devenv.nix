{
  pkgs,
  lib,
  config,
  ...
}:

let
  cutoutSwiftFormat = import ./nix/swift-format { inherit pkgs; };
  cutoutCargoSwift = import ./nix/cargo-swift { inherit pkgs; };
  swiftToolchainCheck = ''
    expected_developer_dir="''${CUTOUT_DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
    if [[ "''${DEVELOPER_DIR:-}" != "$expected_developer_dir" || -n "''${SDKROOT:-}" ]]; then
      echo "Devenv must select $expected_developer_dir and clear inherited SDKROOT before running Swift" >&2
      exit 1
    fi
    # A version check alone misses mismatched SDKs. Exercise SDK module loading
    # with the same bare Swift compiler used by direct shell commands.
    printf 'import Foundation\n' | swiftc -typecheck -
  '';
  swiftTask = command: {
    exec = ''
      source "$DEVENV_ROOT/scripts/swift-package-common.sh"
      cutout_use_xcode_developer_dir
      ${command}
    '';
    after = [ "check:xcode-ios" ];
  };
  nightlyRust = pkgs.rust-bin.nightly.latest.default;
  androidMinApi = "33";
  androidApi = "37";
  androidMinorApi = "2";
  androidPlatform = "${androidApi}.${androidMinorApi}";
  androidSystemImageType = "google_apis_ps16k";
  androidBuildTools = "37.0.0";
  androidPackages = (pkgs.androidenv.override { licenseAccepted = true; }).composeAndroidPackages {
    repoJson = ./nix/android-repository.json;
    cmdLineToolsVersion = "23.0";
    platformToolsVersion = "37.0.1";
    emulatorVersion = "37.2.12";
    includeNDK = true;
    ndkVersion = "30.0.16248370";
    platformVersions = [ androidPlatform ];
    buildToolsVersions = [ androidBuildTools ];
    toolsVersion = null;
    includeEmulator = pkgs.stdenv.hostPlatform.system == "x86_64-linux";
    includeSystemImages = pkgs.stdenv.hostPlatform.system == "x86_64-linux";
    systemImageTypes = [ androidSystemImageType ];
    abiVersions = [ "x86_64" ];
    cmakeVersions = [ ];
  };
  androidNdk = androidPackages.ndk-bundle;
  androidNdkRoot = "${androidNdk}/libexec/android-sdk/ndk-bundle";
  androidNdkHost = if pkgs.stdenv.isDarwin then "darwin-x86_64" else "linux-x86_64";
  androidArm64Clang = "${androidNdkRoot}/toolchains/llvm/prebuilt/${androidNdkHost}/bin/aarch64-linux-android${androidMinApi}-clang";
  androidLlvmAr = "${androidNdkRoot}/toolchains/llvm/prebuilt/${androidNdkHost}/bin/llvm-ar";
  cutoutCargoFuzz = pkgs.writeShellScriptBin "cutout-cargo-fuzz" ''
    export PATH="${nightlyRust}/bin:${pkgs.cargo-fuzz}/bin:$PATH"
    exec cargo fuzz "$@"
  '';
in
{
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
    clangLinker.enable = false;
  };
  overlays = [
    (final: prev: {
      cargo-nextest =
        let
          binaries = {
            aarch64-darwin = {
              url = "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-0.9.148/cargo-nextest-0.9.148-universal-apple-darwin.tar.gz";
              hash = "sha256-bCO3+0yoLFcfyN/turRb+nc9RZGPkzFwyF0hzX9LhSs=";
            };
            x86_64-linux = {
              url = "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-0.9.148/cargo-nextest-0.9.148-x86_64-unknown-linux-gnu.tar.gz";
              hash = "sha256-IjupNnFMyGGpze2tWo62ZN/dadb732zqaqgYT1IykNw=";
            };
            aarch64-linux = {
              url = "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-0.9.148/cargo-nextest-0.9.148-aarch64-unknown-linux-gnu.tar.gz";
              hash = "sha256-gUaeuWPviD/VkOYBied9aro9kbQYWR0zkfUP4+lIDSs=";
            };
          };
          binary = binaries.${prev.stdenv.hostPlatform.system};
        in
        prev.stdenvNoCC.mkDerivation {
          pname = "cargo-nextest";
          version = "0.9.148";
          src = prev.fetchurl binary;
          sourceRoot = ".";
          dontBuild = true;
          nativeBuildInputs = lib.optionals prev.stdenv.isLinux [ prev.autoPatchelfHook ];
          buildInputs = lib.optionals prev.stdenv.isLinux [ prev.stdenv.cc.cc.lib ];
          installPhase = ''
            runHook preInstall
            install -Dm755 cargo-nextest "$out/bin/cargo-nextest"
            runHook postInstall
          '';
          meta = prev.cargo-nextest.meta // {
            platforms = builtins.attrNames binaries;
          };
        };
      # Kotlin and Java tooling share this Gradle/JDK selection.
      gradle =
        (prev.gradle-packages.mkGradle {
          version = "9.8.2";
          hash = "sha256-Ib1dot5JtkKQQHfzjA79x3eOkrPtCIk5LcphuX44lgc=";
          defaultJava = final.temurin-bin-27;
        }).wrapped;
      kotlin = (prev.kotlin.override { jre = final.temurin-bin-27; }).overrideAttrs (_: {
        version = "2.4.21";
        src = prev.fetchurl {
          url = "https://github.com/JetBrains/kotlin/releases/download/v2.4.21/kotlin-compiler-2.4.21.zip";
          hash = "sha256-fMFA522vQWoEJKVVf5mv18qJ69Rj6hEBJh4+AcM+dc0=";
        };
      });
    })
  ];

  languages.java = {
    enable = true;
    jdk.package = pkgs.temurin-bin-27;
    lsp.enable = false;
  };
  languages.kotlin.enable = true;
  languages.nix.enable = true;

  packages = [
    androidPackages.androidsdk
    cutoutCargoFuzz
    pkgs.cargo-deny
    pkgs.cargo-fuzz
    pkgs.cargo-mutants
    pkgs.cargo-nextest
    pkgs.jna
    pkgs.python315
    pkgs.secretspec
  ]
  ++ lib.optionals pkgs.stdenv.isDarwin [
    cutoutCargoSwift
  ]
  ++ lib.optionals pkgs.stdenv.isLinux [
    pkgs.dbus
    pkgs.pkg-config
    # devenv maps packages into nativeBuildInputs, which activates this hook.
    pkgs.rustPlatform.bindgenHook
    pkgs.valgrind
  ];

  env.ANDROID_NDK_ROOT = androidNdkRoot;
  env.ANDROID_HOME = "${androidPackages.androidsdk}/libexec/android-sdk";
  env.ANDROID_SDK_ROOT = config.env.ANDROID_HOME;
  env.CUTOUT_ANDROID_MIN_API = androidMinApi;
  env.CUTOUT_ANDROID_API = androidApi;
  env.CUTOUT_ANDROID_API_MINOR = androidMinorApi;
  env.CUTOUT_ANDROID_SDK_PLATFORM = androidPlatform;
  env.CUTOUT_ANDROID_SYSTEM_IMAGE_TYPE = androidSystemImageType;
  env.CUTOUT_ANDROID_BUILD_TOOLS = androidBuildTools;
  env.CUTOUT_JNA_VERSION = pkgs.jna.version;
  env.CC_aarch64_linux_android = androidArm64Clang;
  env.AR_aarch64_linux_android = androidLlvmAr;
  env.CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = androidArm64Clang;
  env.CC_x86_64_linux_android = "${androidNdkRoot}/toolchains/llvm/prebuilt/${androidNdkHost}/bin/x86_64-linux-android${androidMinApi}-clang";
  env.AR_x86_64_linux_android = androidLlvmAr;
  env.CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER = "${androidNdkRoot}/toolchains/llvm/prebuilt/${androidNdkHost}/bin/x86_64-linux-android${androidMinApi}-clang";
  env.JNA_JAR = "${pkgs.jna}/share/java/jna.jar";
  env.KOTLIN_COROUTINES_JAR = "${pkgs.kotlin}/lib/kotlinx-coroutines-core-jvm.jar";

  treefmt = {
    enable = true;
    # Keep pinned third-party sources comparable with their upstream release.
    config.settings.excludes = [ "vendor/**" ];
    config.programs = {
      nixfmt.enable = true;
      rustfmt = {
        enable = true;
        package = config.languages.rust.toolchainPackage;
      };
      swift-format = {
        enable = true;
        package = cutoutSwiftFormat;
      };
    };
  };

  git-hooks.hooks.treefmt.enable = true;

  # Formatting belongs to the explicit project tasks and commits—not every
  # interactive shell or Codex command.
  tasks."devenv:treefmt:run".before = lib.mkForce [ ];

  tasks."project:lint".exec = ''
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  '';

  tasks."project:test".exec = "bash scripts/run-rust-workspace-tests.sh";

  # One deterministic test entry point for local use and CI. Keep live-device
  # validators and deployment tasks opt-in because they require external state.
  tasks."project:tests" = {
    exec = "true";
    after = [
      "project:test"
      "test:rust-lint-policy"
      "test:kotlin-bindings-smoke"
      "test:dashboard-input"
    ]
    ++ lib.optionals pkgs.stdenv.isDarwin [
      "test:swift-package"
      "test:ios-music-monitor"
      "test:ios-capture-staging"
      "test:shell-regressions"
    ];
  };

  tasks."project:dependency-policy".exec = "cargo deny --locked check";

  tasks."check:android-camera-protocols".exec =
    "cargo check --locked -p cutout-protocols --target aarch64-linux-android";

  tasks."check:android-camera-ffi".exec =
    "cargo check --locked -p cutout-mobile-ffi --target aarch64-linux-android";

  tasks."test:rust-lint-policy".exec = "bash tests/fixtures/macro-policy/run.sh";

  tasks."test:dashboard-input".exec = ''
    cargo build --locked -p cutout-cli
    python3 tests/scripts/dashboard-input.py target/debug/cutout
  '';

  tasks."project:quality-gate" = {
    # Run formatting after the other checks so it cannot edit their inputs.
    exec = "treefmt --ci";
    after = [
      "project:lint"
      "project:tests"
      "project:dependency-policy"
    ]
    ++ lib.optionals pkgs.stdenv.isDarwin [
      "build:ios-ui-tests"
    ];
  };

  tasks."build:swift-ffi-package" = {
    exec = "cargo cutout swift-ffi";
    after = [ "check:xcode-ios" ];
  };

  tasks."check:xcode-ios".exec = ''
    source "$DEVENV_ROOT/scripts/swift-package-common.sh"
    if [[ "$(cutout_host_os)" != Darwin ]]; then
      echo "iOS and Xcode tasks require Darwin/Xcode" >&2
      exit 1
    fi
    ${swiftToolchainCheck}
    cutout_use_xcode_developer_dir
    printf 'Using Xcode developer directory: %s\n' "$DEVELOPER_DIR"
    /usr/bin/xcrun --find xcodebuild
    /usr/bin/xcrun --find devicectl
    /usr/bin/xcrun xcodebuild -version
    /usr/bin/xcrun swift --version
    /usr/bin/xcrun --sdk iphoneos --show-sdk-path >/dev/null
  '';

  tasks."test:swift-package" = swiftTask ''
    bash "$DEVENV_ROOT/scripts/run-swift-package-tests.sh"
  '';

  tasks."validate:aero-live-connection" =
    (swiftTask ''
      echo "libcutout_commit=$(git rev-parse HEAD)"
      validator_args=( "''${CUTOUT_AERO_VALIDATION_TIMEOUT:-45}" )
      if [[ "''${CUTOUT_AERO_SETTINGS_TEST:-0}" == "1" ]]; then
        validator_args+=(--settings)
      fi
      exec cargo cutout swift -- run \
        --package-path "$DEVENV_ROOT/swift/CutoutMobile" \
        CutoutMobileLiveValidator \
        "''${validator_args[@]}"
    '')
    // {
      showOutput = true;
    };

  scripts.cutout-melk-live.exec = ''
    source "$DEVENV_ROOT/scripts/swift-package-common.sh"
    if [[ "$(cutout_host_os)" != Darwin ]]; then
      echo "MELK validation requires Darwin/CoreBluetooth" >&2
      exit 1
    fi
    cutout_use_xcode_developer_dir
    timeout_seconds="''${CUTOUT_MELK_VALIDATION_TIMEOUT:-60}"
    platform_identifier="''${CUTOUT_MELK_PLATFORM_IDENTIFIER:-}"
    if ! [[ "$timeout_seconds" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
      echo "CUTOUT_MELK_VALIDATION_TIMEOUT must be a non-negative number of seconds" >&2
      exit 2
    fi
    echo "libcutout_commit=$(git rev-parse HEAD)"
    args=("$timeout_seconds")
    if [[ -n "$platform_identifier" ]]; then
      args+=("$platform_identifier")
    fi
    exec cargo cutout swift -- run \
      --package-path "$DEVENV_ROOT/swift/CutoutMobile" \
      MelkLightingLiveValidator \
      "''${args[@]}"
  '';

  tasks."test:kotlin-bindings-smoke".exec = ''
    python3 scripts/test-mobile-ffi.py host
  '';
  tasks."test:kotlin-bindings-smoke".after = [ "test:mobile-ffi-runner" ];
  tasks."test:mobile-ffi-runner".exec =
    "python3 -m unittest discover -s tests/mobile-ffi -p 'test_runner.py'";
  tasks."build:android-tests".exec = "python3 scripts/test-mobile-ffi.py build";
  tasks."build:android-app".after = [ "build:android-tests" ];
  tasks."test:android-device".exec = "python3 scripts/test-mobile-ffi.py device";
  tasks."test:android-emulator".exec = "python3 scripts/test-mobile-ffi.py emulator";
  tasks."test:android-ui".exec = "python3 scripts/test-mobile-ffi.py local";
  tasks."run:android-app".exec = "python3 scripts/test-mobile-ffi.py app";
  tasks."build:ios-ui-tests" = {
    exec = "CUTOUT_IOS_TEST_DESTINATION='generic/platform=iOS Simulator' scripts/run-ios-ui-tests.sh --build-only -jobs 2 ARCHS=arm64 ONLY_ACTIVE_ARCH=YES";
    after = [ "check:xcode-ios" ];
  };
  tasks."test:ios-music-monitor" = {
    exec = ''
      destination="''${CUTOUT_IOS_TEST_DESTINATION:-platform=iOS Simulator,name=iPhone 18 Pro,OS=latest}"
      cargo cutout xcodebuild -- \
        -project "$DEVENV_ROOT/swift/CutoutMobile/CutoutApp.xcodeproj" \
        -scheme CutoutAppIOSUnitTests \
        -destination "$destination" \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testActualMonitorTaskUsesPassiveAuthorizationAndCancelsOnBackground \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testSoundCloudRestorationAndSwitchingNeverDispatchAppleMusicOrHistory \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testExplicitConnectAllowsAuthorizationPrompt \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testConnectAllowsAuthorizationPromptForUnauthorizedSnapshot \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testConnectReusesAuthorizationForExistingPlaybackSnapshots \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testColdStartRestoreUsesPassiveAuthorizationAndShowsPlayer \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testRestorePlayerPreservesCurrentFeedbackWhenMonitorIsAlreadyActive \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testExplicitShutdownInvalidatesLateMonitorObservationBeforeAdapterTeardown \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testAppModelDeallocationStopsMusicMonitor \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testAppModelKeepsOneMusicMonitorAcrossStartupAndSceneResume \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testOpeningHistoricalRideDetailDoesNotDispatchMusicTransport \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testSpotifyCallbackAdmissionSurvivesEitherSceneEventOrder \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testStopWaitsForAcceptedHistoryWithoutABluetoothCaptureReceipt \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testProviderResetMakesUnsettledAcceptedObservationFailStopExplicitly \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testOptionalOldMusicReadbackCannotDelayStopOrOverwriteNewerPresentation \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testHistoryDeletionPreventsLateReadbackFromRestoringMetadataAndKeepsLivePlayer \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testStaleRideContextFailsStopBeforeOptionalErrorReadbackReturns \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testSpotifyDisconnectRemovesOnlySpotifyHistoryAndPersistsMonitoringOptOut \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testSpotifyDisconnectReportsFailedDeletionAndCanRetryWithoutReconnect \
        -only-testing:CutoutAppIOSUnitTests/MusicFeatureModelTests/testSpotifyDisconnectRejectsAnObservationWhoseHistoryReadbackFinishesLate \
        -parallel-testing-enabled NO \
        -collect-test-diagnostics never \
        ARCHS=arm64 ONLY_ACTIVE_ARCH=YES \
        test
    '';
    after = [ "check:xcode-ios" ];
  };
  tasks."test:ios-locked-ride" = {
    exec = ''
      destination="''${CUTOUT_IOS_TEST_DESTINATION:-platform=iOS Simulator,name=iPhone 18 Pro,OS=latest}"
      cargo cutout xcodebuild -- \
        -project "$DEVENV_ROOT/swift/CutoutMobile/CutoutApp.xcodeproj" \
        -scheme CutoutAppIOSUnitTests \
        -destination "$destination" \
        -only-testing:CutoutAppIOSUnitTests/PhoneLocationAdapterTests \
        -only-testing:CutoutAppIOSUnitTests/LockedRideMainThreadTests \
        -only-testing:CutoutAppIOSUnitTests/RideSessionMarkerNonblockingTests \
        -parallel-testing-enabled NO \
        -collect-test-diagnostics never \
        ARCHS=arm64 ONLY_ACTIVE_ARCH=YES \
        test
    '';
    after = [ "check:xcode-ios" ];
  };
  tasks."test:ios-capture-staging" = swiftTask ''
    cargo cutout xcodebuild -- \
      -project "$DEVENV_ROOT/swift/CutoutMobile/CutoutApp.xcodeproj" \
      -scheme CutoutMobileIOSRuntimeTests \
      -destination "''${CUTOUT_IOS_TEST_DESTINATION:-platform=iOS Simulator,name=iPhone 18 Pro,OS=latest}" \
      -only-testing:CutoutMobileIOSRuntimeTests \
      -parallel-testing-enabled NO \
      -collect-test-diagnostics never \
        ARCHS=arm64 ONLY_ACTIVE_ARCH=YES test
  '';

  tasks."test:shell-regressions" = {
    # The production FFI fixture temporarily changes Rust source. Run it after
    # native checks so it cannot invalidate their captured source generations.
    after = [
      "test:swift-package"
      "test:ios-music-monitor"
      "test:ios-capture-staging"
      "build:ios-ui-tests"
    ];
    exec = ''
      bash tests/scripts/swift-package-common.sh
      bash tests/scripts/run-ios-ui-tests.sh
      bash tests/scripts/run-swift-package-tests.sh
      bash tests/fixtures/ffi-freshness/run.sh
      bash tests/fixtures/ffi-production/run.sh
      scripts/probe-novatek-camera.sh --self-test
      scripts/probe-novatek-camera-controls.sh --self-test
    '';
  };
  tasks."build:ios-app" = swiftTask ''
    cargo cutout xcodebuild -- \
      -project "$DEVENV_ROOT/swift/CutoutMobile/CutoutApp.xcodeproj" \
      -scheme CutoutApp \
      -destination 'generic/platform=iOS Simulator' \
      -jobs 2 \
      ARCHS=arm64 ONLY_ACTIVE_ARCH=YES build
  '';
  tasks."run:ios-app-on-mac" = {
    exec = ''
      source "$DEVENV_ROOT/scripts/swift-package-common.sh"
      if [[ "$(uname -m)" != arm64 ]]; then
        echo "CutoutApp iOS-on-Mac requires Apple Silicon" >&2
        exit 1
      fi
      destination="''${CUTOUT_IOS_ON_MAC_DESTINATION:-platform=macOS}"
      export CUTOUT_IOS_APP_BUILD_DESTINATION="$destination"
      product="''${CUTOUT_IOS_ON_MAC_PRODUCT:-$(cutout_build_ios_app_bundle)}"
      printf 'ios_app_product=%s\n' "$product"
      printf 'ios_app_destination=%s\n' "$destination"
      exec /usr/bin/open -n "$product"
    '';
    after = [ "check:xcode-ios" ];
  };
  tasks."deploy:ios-device" = {
    exec = ''
      secretspec run --profile development --scope ios-device-deploy --reason "Deploy signed CutoutApp to a connected iPhone" -- cargo cutout ios deploy
    '';
    after = [ "check:xcode-ios" ];
  };
  tasks."export:ios-ad-hoc-ipa" = {
    exec = ''
      secretspec run --profile development --scope ios-ad-hoc-export --reason "Export a signed CutoutApp ad-hoc archive" -- bash -c '
        source "$DEVENV_ROOT/scripts/swift-package-common.sh"
        cutout_export_ios_ad_hoc_ipa
      '
    '';
    after = [ "check:xcode-ios" ];
  };

  tasks."devenv:enterTest".exec = lib.mkIf pkgs.stdenv.isDarwin swiftToolchainCheck;

  enterShell = lib.optionalString pkgs.stdenv.isDarwin ''
    export PATH="${pkgs.python315}/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
    # Nix's SDK setup can supply DEVELOPER_DIR independently of SDKROOT.
    # Keep direct Swift commands on the same Xcode toolchain as the iOS tasks.
    export DEVELOPER_DIR="''${CUTOUT_DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
    # Task environment propagation retains inherited variables when merely unset.
    export SDKROOT=""
    unset CC CXX LD AR RANLIB
    unset NIX_CC NIX_CFLAGS_COMPILE NIX_CXXSTDLIB_COMPILE NIX_LDFLAGS
    unset CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER
    unset CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER
    export RUSTC_WRAPPER=""
    export RUSTC_WORKSPACE_WRAPPER=""
  '';
}
