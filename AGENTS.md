# libcutout agent guide

## Development environment

- Use `nix develop -c ...` for repository commands.
- Keep generated Swift FFI output in the ignored `target/swift-ffi/CutoutMobileFFI` package. Prepare it with the repository ensure/regeneration scripts; never commit generated bindings, headers, module maps, fingerprints, or static libraries.

## Code navigation and edits

- Use MCPLS for semantic search, symbol inspection, structural edits, and LSP validation when available.
- Reuse the shared Swift FFI ensure boundary instead of adding consumer-specific generation or linker workarounds.
