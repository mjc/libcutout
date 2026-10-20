# Cutout patch to Askama derive 0.16.1

Source: crates.io `askama_derive` 0.16.1, upstream commit
`9ae4d5bdf4352b35e34106e38944c90d7a7ede3e`.
The original manifest, source, templates, README and both upstream licenses
are retained. Registry metadata and the independent lockfile are omitted.
`Cargo.toml.orig` records the unmodified upstream manifest.

The `config` feature uses `toml` 1.1 instead of the unmaintained `basic-toml`
parser (RUSTSEC-2026-0374). `RawConfig::from_toml_str` still deserializes borrowed
configuration fields and maps parse failures through the existing error owner.
`toml` 1.1 supplies the borrowed string visits required by these fields.
Active dependency requirements, including test and benchmark dependencies, are
updated to their current stable releases. `similar` 3 is used by the diff tests.
The template parser and rendering code are unchanged from upstream 0.16.1.

UniFFI 0.32.2 uses this Askama release. Swift and Kotlin bindings continue to use
the shared workspace generator and artifact owner. Remove this patch when an
upstream Askama release selected by UniFFI removes the unmaintained parser.
