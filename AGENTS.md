# libcutout agent guide

- Reuse the shared Swift FFI ensure boundary; do not add consumer-specific generation or linker workarounds.
- Keep generated Swift FFI output ignored and never commit generated bindings, headers, module maps, fingerprints, or static libraries.
- Read [the music scope contract](docs/music-integration.md) before changing music, listening history, or ride-replay code.
