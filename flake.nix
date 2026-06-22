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
          stableRust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain stableRust;
          src = craneLib.cleanCargoSource ./.;
          commonArgs = {
            inherit src;
            pname = "libcutout";
            version = "0.1.0";
            strictDeps = true;
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
          stableRust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain stableRust;
          src = craneLib.cleanCargoSource ./.;
          commonArgs = {
            inherit src;
            pname = "libcutout";
            version = "0.1.0";
            strictDeps = true;
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
          stableRust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          nightlyRust = pkgs.rust-bin.nightly.latest.default;
          cutoutCargoFuzz = pkgs.writeShellScriptBin "cutout-cargo-fuzz" ''
            export PATH="${nightlyRust}/bin:${pkgs.cargo-fuzz}/bin:$PATH"
            exec cargo fuzz "$@"
          '';
        in
        {
          default = pkgs.mkShell {
            packages = [
              stableRust
              cutoutCargoFuzz
              pkgs.cargo-deny
              pkgs.cargo-fuzz
              pkgs.cargo-mutants
              pkgs.jna
              pkgs.kotlin
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
