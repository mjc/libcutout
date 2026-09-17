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
          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
          commonArgs = {
            src = craneLib.cleanCargoSource ./.;
            pname = "libcutout";
            version = "0.1.0";
            strictDeps = true;
            nativeBuildInputs = nixpkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.pkg-config
              pkgs.rustPlatform.bindgenHook
            ];
            buildInputs = nixpkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.dbus ];
          };
        in
        {
          inherit
            pkgs
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
          inherit (systemContext system)
            craneLib
            commonArgs
            cargoArtifacts
            ;
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
          inherit (systemContext system)
            pkgs
            craneLib
            commonArgs
            cargoArtifacts
            ;
        in
        {
          inherit (self.packages.${system}) cutout-cli;
          fmt = craneLib.cargoFmt commonArgs;
          clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--workspace --all-targets --all-features --locked -- -D warnings";
            }
          );
          test = craneLib.cargoTest (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoExtraArgs = "--workspace --locked";
            }
          );
          deny = craneLib.cargoDeny commonArgs;
        }
      );

      formatter = forAllSystems (system: (systemContext system).pkgs.nixfmt);
    };
}
