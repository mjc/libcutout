# Model Onboarding

`cutout-core` and `cutout-protocols` are the synchronous, transport-independent
library layers. Host adapters such as `cutout-btle`, `cutout-cli`, and
`cutout-mobile-ffi` own async runtimes, Bluetooth transports, and serialization
ergonomics.

The intended model path is typed and static:

1. Author model metadata with `cutout_core::ModelAuthoring`.
2. Group the resulting source data in a `RegisteredModelDefinition::new(...)`
   record so the registry, catalog, and session registration all derive from
   one typed source definition.
3. List the registered model definitions in
   `crates/cutout-protocols/registry/models.json` so `build.rs` can generate
   the composed registry arrays deterministically.
4. Emit a `ModelRegistryEntry` and `ModelCatalogEntry` from that authored data.
5. Use a borrowed `ModelCatalog` for allocation-free lookup after setup.
6. Convert untrusted host, PEVCAP, or transport bytes at the boundary before
   they reach model/session logic.

Concrete examples live in `crates/cutout-protocols/examples/`:

- `model_authoring.rs` shows a fixture model added through the typed authoring
  API.
- `registered_model_definition.rs` shows the structured registry/catalog/session
  bridge derived from one typed source record.
- `read_only_session.rs` shows a protocol session driven from typed host inputs
  without `btleplug`.
- `registry/models.json` shows the bundled structured model list that feeds the
  generated registry arrays.

The goal is not a runtime registry or service locator. Each new model should add
structured static data and typed tests, while keeping lookup and hot-path
decisions allocation-free after setup.
