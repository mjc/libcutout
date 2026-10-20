{ pkgs }:

# Cargo Swift extracts metadata before the workspace generator replaces bindings.
# Keep its UniFFI format aligned and reuse the shared Askama parser fix.
pkgs.cargo-swift.overrideAttrs (
  finalAttrs: old: {
    src = pkgs.applyPatches {
      src = old.src;
      patches = [ ./uniffi-0.32.patch ];
      postPatch = ''
        cp -R ${../../vendor/askama_derive} cutout-askama-derive
      '';
    };

    cargoHash = "sha256-mJqcWp7fZGfNvbEK2s6Xpw30+EpOrHL1C1BHHtmpfuA=";
    cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
      inherit (finalAttrs) pname version src;
      hash = finalAttrs.cargoHash;
    };
  }
)
