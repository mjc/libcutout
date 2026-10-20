{ pkgs }:

pkgs.swift-format.overrideAttrs (
  finalAttrs: _: {
    version = "604.0.0";

    src = pkgs.fetchFromGitHub {
      owner = "swiftlang";
      repo = "swift-format";
      tag = "604.0.0";
      hash = "sha256-jpVuasgNbuQfnqg8Pir4o/Wm+pyPaGV3l9AhSlYokCE=";
    };

    swiftpmDeps = pkgs.fetchSwiftPMDeps {
      inherit (finalAttrs) pname version src;
      hash = "sha256-2PgwYCiUKNZ8rKyky1dxbGHd4mEmS3OTIt1ssfbpmfE=";
      postPatch = ''
        ln -s ${./Package.resolved} Package.resolved
      '';
    };
  }
)
