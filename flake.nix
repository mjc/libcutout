{
  description = "Cutout Rust development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      crane,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          stableRust = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
            targets = [
              "aarch64-apple-ios"
              "aarch64-apple-ios-sim"
            ];
          };
          craneLib = (crane.mkLib pkgs).overrideToolchain stableRust;
          src = craneLib.cleanCargoSource ./.;
          commonArgs = {
            inherit src;
            pname = "libcutout";
            version = "0.1.0";
            strictDeps = true;
            nativeBuildInputs =
              nixpkgs.lib.optionals pkgs.stdenv.isLinux [
                pkgs.mold
              ]
              ++ nixpkgs.lib.optionals pkgs.stdenv.isDarwin [
                pkgs.llvmPackages.lld
              ];
          };
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        in
        {
          default = self.packages.${system}.cutout-cli;
          cutout-cli = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoExtraArgs = "-p cutout-cli";
            }
          );
        }
      );

      apps = forAllSystems (system: {
        default = self.apps.${system}.cutout-cli;
        cutout-cli = {
          type = "app";
          program = "${self.packages.${system}.cutout-cli}/bin/cutout";
          meta.description = "Run the Cutout CLI";
        };
      });

      checks = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          stableRust = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
            targets = [
              "aarch64-apple-ios"
              "aarch64-apple-ios-sim"
            ];
          };
          craneLib = (crane.mkLib pkgs).overrideToolchain stableRust;
          src = craneLib.cleanCargoSource ./.;
          commonArgs = {
            inherit src;
            pname = "libcutout";
            version = "0.1.0";
            strictDeps = true;
            nativeBuildInputs =
              nixpkgs.lib.optionals pkgs.stdenv.isLinux [
                pkgs.mold
              ]
              ++ nixpkgs.lib.optionals pkgs.stdenv.isDarwin [
                pkgs.llvmPackages.lld
              ];
          };
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        in
        {
          inherit (self.packages.${system}) cutout-cli;
          fmt = craneLib.cargoFmt commonArgs;
          clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--workspace --all-targets --all-features -- -D warnings";
            }
          );
          test = craneLib.cargoTest (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoExtraArgs = "--workspace";
            }
          );
          deny = craneLib.cargoDeny commonArgs;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          stableRust = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
            targets = [
              "aarch64-apple-ios"
              "aarch64-apple-ios-sim"
            ];
          };
          nightlyRust = pkgs.rust-bin.nightly.latest.default;
          cutoutCargoFuzz = pkgs.writeShellScriptBin "cutout-cargo-fuzz" ''
            export PATH="${nightlyRust}/bin:${pkgs.cargo-fuzz}/bin:$PATH"
            exec cargo fuzz "$@"
          '';
          # Keep Xcode's clang away from Nix's incompatible linker and SDK shims.
          appleXcodebuild = pkgs.writeShellScriptBin "xcodebuild" ''
            exec env \
              -u SDKROOT -u LD -u CC -u CXX -u AR -u RANLIB \
              PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
              /usr/bin/xcodebuild "$@"
          '';
        in
        {
          default = (if pkgs.stdenv.isDarwin then pkgs.mkShellNoCC else pkgs.mkShell) {
            packages = [
              stableRust
              cutoutCargoFuzz
              pkgs.cargo-deny
              pkgs.cargo-fuzz
              pkgs.cargo-mutants
              pkgs.cargo-nextest
              pkgs.jna
              pkgs.kotlin
              pkgs.python3Packages.pillow
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.mold
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              appleXcodebuild
              pkgs.llvmPackages.lld
            ]
            ++ [
              pkgs.nixfmt
            ];

            shellHook = ''
              echo "Cutout dev shell"
              echo "  stable: ${stableRust.name}"
              echo "  miri:   tracked separately in cutout-dly"
            '';
          };
        }
      );

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);
    };
}
