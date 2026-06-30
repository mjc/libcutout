use cutout_core::{
    Capabilities, CommandKind, CompleteModelAuthoring, FamilyKey, GattFingerprint, GattRoles,
    ManufacturerKey, ModelAuthoring, ModelCatalog, ModelCatalogEntry, ModelKey, ModelRegistryEntry,
    ParserKey, ProtocolFamily, SessionKey, VerificationStatus,
};
use cutout_protocols::{BEGODE_DATA_CHANNEL, BEGODE_SERVICE_CHANNEL, MODEL_CATALOG};

const FAKE_PARSER_KEY: ParserKey = ParserKey::new("example-fake-parser");
const FAKE_SESSION_KEY: SessionKey = SessionKey::new("example-fake-read-only");

const FAKE_GATT: [GattFingerprint; 1] = [GattFingerprint {
    service: BEGODE_SERVICE_CHANNEL,
    characteristic: BEGODE_DATA_CHANNEL,
    roles: GattRoles::empty()
        .with_write_without_response()
        .with_notify(),
    verification: VerificationStatus::Inferred,
}];

const FAKE_AUTHORING: CompleteModelAuthoring = ModelAuthoring::new()
    .manufacturer(ManufacturerKey::new("ExampleCo"))
    .model(ModelKey::new("Typed Fixture Wheel"))
    .family(FamilyKey::new(ProtocolFamily::BegodeGotway))
    .advertised_name_hints(&["TypedFixture", "TypedFixture BLE"])
    .gatt(&FAKE_GATT)
    .capabilities(Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestTelemetry,
    ]))
    .verification(VerificationStatus::Inferred)
    .active_runtime(FAKE_PARSER_KEY, FAKE_SESSION_KEY);

static FAKE_MODEL: ModelRegistryEntry = FAKE_AUTHORING.registry_entry();
const FAKE_CATALOG_ENTRY: ModelCatalogEntry = FAKE_AUTHORING.catalog_entry(&FAKE_MODEL);

fn main() {
    let catalog = ModelCatalog::new(&[MODEL_CATALOG[0], MODEL_CATALOG[1], FAKE_CATALOG_ENTRY]);
    let entry = catalog
        .find_session(FAKE_SESSION_KEY)
        .expect("example fixture model should be discoverable");

    assert_eq!(entry.registry.model, "Typed Fixture Wheel");
    assert_eq!(entry.registration.parser, Some(FAKE_PARSER_KEY));
    assert_eq!(entry.registry.gatt[0].service, BEGODE_SERVICE_CHANNEL);
    assert_eq!(entry.registry.gatt[0].characteristic, BEGODE_DATA_CHANNEL);
    assert!(
        entry
            .registry
            .capabilities
            .supports_command_kind(CommandKind::RequestTelemetry)
    );
    println!(
        "catalog entry ready: {} / {}",
        entry.registry.manufacturer, entry.registry.model
    );
}
