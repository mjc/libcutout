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
      systemContext =
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          toolchain = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
            targets = [
              "aarch64-apple-ios"
              "aarch64-apple-ios-sim"
              "x86_64-apple-ios"
              "x86_64-apple-darwin"
            ];
          };
          devRust =
            if pkgs.stdenv.isDarwin then
              toolchain.overrideAttrs {
                depsHostHostPropagated = [ ];
                propagatedBuildInputs = [ ];
                depsTargetTargetPropagated = [ ];
              }
            else
              toolchain;
          craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
          commonArgs = {
            src = craneLib.cleanCargoSource ./.;
            pname = "libcutout";
            version = "0.1.0";
            strictDeps = true;
          };
        in
        {
          inherit
            pkgs
            devRust
            craneLib
            commonArgs
            ;
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          inherit (systemContext system) craneLib commonArgs cargoArtifacts;
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
          inherit (systemContext system) craneLib commonArgs cargoArtifacts;
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
          inherit (systemContext system) pkgs devRust;
          nightlyRust = pkgs.rust-bin.nightly.latest.default;
          cutoutCargoFuzz = pkgs.writeShellScriptBin "cutout-cargo-fuzz" ''
            export PATH="${nightlyRust}/bin:${pkgs.cargo-fuzz}/bin:$PATH"
            exec cargo fuzz "$@"
          '';
        in
        {
          default = (if pkgs.stdenv.isDarwin then pkgs.mkShellNoCC else pkgs.mkShell) {
            packages = [
              devRust
              cutoutCargoFuzz
              pkgs.cargo-deny
              pkgs.cargo-fuzz
              pkgs.cargo-mutants
              pkgs.cargo-nextest
              pkgs.jna
              pkgs.kotlin
              pkgs.python3Packages.pillow
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.cargo-swift
            ]
            ++ [
              pkgs.nixfmt
            ];

            shellHook = ''
              ${pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
                export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
                unset CC CXX LD AR RANLIB SDKROOT
                unset NIX_CC NIX_CFLAGS_COMPILE NIX_CXXSTDLIB_COMPILE NIX_LDFLAGS
                unset CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER
                unset CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER
                export RUSTC_WRAPPER=""
                export RUSTC_WORKSPACE_WRAPPER=""
              ''}
              echo "Cutout dev shell"
              echo "  stable: ${devRust.name}"
              echo "  miri:   tracked separately in cutout-dly"
            '';
          };
        }
      );

      formatter = forAllSystems (system: (systemContext system).pkgs.nixfmt);
    };
}
