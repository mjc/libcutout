{
  pkgs,
  lib,
  config,
  ...
}:

let
  swiftTask = command: {
    exec = ''
      source "$DEVENV_ROOT/scripts/swift-package-common.sh"
      cutout_use_xcode_developer_dir
      cutout_ensure_swift_ffi_build_input "$DEVENV_ROOT"
      ${command}
    '';
    after = [ "check:xcode-ios" ];
  };
  nightlyRust = pkgs.rust-bin.nightly.latest.default;
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
  languages.kotlin.enable = true;
  languages.nix.enable = true;

  packages = [
    cutoutCargoFuzz
    pkgs.cargo-deny
    pkgs.cargo-fuzz
    pkgs.cargo-mutants
    pkgs.cargo-nextest
    pkgs.jna
    pkgs.python3Packages.pillow
    pkgs.secretspec
  ]
  ++ lib.optionals pkgs.stdenv.isDarwin [ pkgs.cargo-swift ]
  ++ lib.optionals pkgs.stdenv.isLinux [
    pkgs.dbus
    pkgs.pkg-config
    # devenv maps packages into nativeBuildInputs, which activates this hook.
    pkgs.rustPlatform.bindgenHook
    pkgs.valgrind
  ];

  env.JNA_JAR = "${pkgs.jna}/share/java/jna.jar";

  treefmt = {
    enable = true;
    config.programs = {
      nixfmt.enable = true;
      rustfmt = {
        enable = true;
        package = config.languages.rust.toolchainPackage;
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

  tasks."project:test".exec = ''
    cargo nextest run --workspace --locked
    cargo test --workspace --doc --locked
  '';

  tasks."project:dependency-policy".exec = "cargo deny --locked check";

  tasks."project:quality-gate" = {
    # Run formatting after the other checks so it cannot edit their inputs.
    exec = "treefmt --ci";
    after = [
      "project:lint"
      "project:test"
      "project:dependency-policy"
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
    cutout_use_xcode_developer_dir
    printf 'Using Xcode developer directory: %s\n' "$DEVELOPER_DIR"
    /usr/bin/xcrun --find xcodebuild
    /usr/bin/xcrun --find devicectl
    /usr/bin/xcrun xcodebuild -version
    /usr/bin/xcrun swift --version
    /usr/bin/xcrun --sdk iphoneos --show-sdk-path >/dev/null
  '';

  tasks."test:swift-package" = swiftTask ''
    swift test --package-path "$DEVENV_ROOT/swift/CutoutMobile"
  '';

  tasks."validate:aero-live-connection" =
    (swiftTask ''
      echo "libcutout_commit=$(git rev-parse HEAD)"
      exec swift run \
        --package-path "$DEVENV_ROOT/swift/CutoutMobile" \
        CutoutMobileLiveValidator \
        "''${CUTOUT_AERO_VALIDATION_TIMEOUT:-45}"
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
    cutout_ensure_swift_ffi_build_input "$DEVENV_ROOT"
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
    exec swift run \
      --package-path "$DEVENV_ROOT/swift/CutoutMobile" \
      MelkLightingLiveValidator \
      "''${args[@]}"
  '';

  tasks."test:kotlin-bindings-smoke".exec = ''
    cargo build -p cutout-mobile-ffi
    rm -rf target/uniffi-smoke
    cargo run -p cutout-uniffi-bindgen -- generate \
      --library "target/debug/libcutout_mobile_ffi.$([[ "$(uname -s)" == Darwin ]] && printf dylib || printf so)" \
      --language kotlin \
      --no-format \
      --out-dir target/uniffi-smoke/kotlin
    kotlinc \
      target/uniffi-smoke/kotlin/uniffi/cutout_mobile_ffi/cutout_mobile_ffi.kt \
      tests/mobile-ffi/kotlin-smoke.kt \
      -cp "$JNA_JAR" \
      -include-runtime \
      -d target/uniffi-smoke/kotlin-smoke.jar
    java \
      -Djava.library.path="$DEVENV_ROOT/target/debug" \
      -cp "target/uniffi-smoke/kotlin-smoke.jar:$JNA_JAR" \
      Kotlin_smokeKt
  '';
  tasks."build:ios-ui-tests" = {
    exec = "scripts/run-ios-ui-tests.sh --build-only";
    after = [ "check:xcode-ios" ];
  };
  tasks."run:ios-app-on-mac" = {
    exec = ''
      source "$DEVENV_ROOT/scripts/swift-package-common.sh"
      if [[ "$(uname -m)" != arm64 ]]; then
        echo "CutoutApp iOS-on-Mac requires Apple Silicon" >&2
        exit 1
      fi
      destination="''${CUTOUT_IOS_ON_MAC_DESTINATION:-platform=macOS,id=00008103-001935121A8A001E}"
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

  enterShell = lib.optionalString pkgs.stdenv.isDarwin ''
    export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
    unset CC CXX LD AR RANLIB SDKROOT
    unset NIX_CC NIX_CFLAGS_COMPILE NIX_CXXSTDLIB_COMPILE NIX_LDFLAGS
    unset CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER
    unset CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER
    export RUSTC_WRAPPER=""
    export RUSTC_WORKSPACE_WRAPPER=""
  '';
}
