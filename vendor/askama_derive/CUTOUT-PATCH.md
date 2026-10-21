# Askama derive 0.14.0 parser patch

Source: crates.io `askama_derive` 0.14.0, upstream commit
`95867ac8cedc80b69f4047dea9c1272a5bf947b3`.
The archive SHA-256 is
`129397200fe83088e8a68407a8e2b1f826cf0086b21ccdb866a722c8bcd3a94f`.
Both upstream licenses and the unmodified `Cargo.toml.orig` are retained.
Registry bookkeeping and the independent lockfile are omitted.

The `config` feature uses maintained `toml` 1.1 instead of `basic-toml`.
Configuration still borrows nested strings from its input and returns parse
errors through the existing error owner. Tests cover those contracts alongside
the retained upstream template and configuration tests. Template generation is
unchanged, and the workspace continues to use UniFFI 0.31.2.

Remove this patch when the selected Askama release uses a maintained parser.
