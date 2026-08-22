use cutout_core::{
    Capabilities, CommandKind, CompleteModelAuthoring, FamilyKey, GattFingerprint, GattRoles,
    ManufacturerKey, ModelAuthoring, ModelCatalog, ModelCatalogEntry, ModelKey, ModelRegistryEntry,
    ParserKey, ProtocolFamily, SessionKey, VerificationStatus,
};
use cutout_protocols::{
    BEGODE_DATA_CHANNEL, BEGODE_SERVICE_CHANNEL, BegodeFalconModel, BenignControlSession,
    MODEL_CATALOG, RegisteredModelDefinition, RegisteredReadOnlySession,
};

const FAKE_PARSER_KEY: ParserKey = ParserKey::new("example-structured-parser");
const FAKE_SESSION_KEY: SessionKey = SessionKey::new("example-structured-read-only");

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
    .model(ModelKey::new("Structured Fixture Wheel"))
    .family(FamilyKey::new(ProtocolFamily::BegodeGotway))
    .advertised_name_hints(&["StructuredFixture", "StructuredFixture BLE"])
    .gatt(&FAKE_GATT)
    .capabilities(Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestTelemetry,
    ]))
    .verification(VerificationStatus::Inferred)
    .active_runtime(FAKE_PARSER_KEY, FAKE_SESSION_KEY);

static FAKE_MODEL: ModelRegistryEntry = FAKE_AUTHORING.registry_entry();

fn fake_session() -> RegisteredReadOnlySession {
    RegisteredReadOnlySession::BegodeFalcon(
        BenignControlSession::<BegodeFalconModel, true>::default(),
    )
}

const FAKE_DEFINITION: RegisteredModelDefinition = RegisteredModelDefinition::new(
    &FAKE_MODEL,
    FAKE_PARSER_KEY,
    FAKE_SESSION_KEY,
    BEGODE_DATA_CHANNEL,
    fake_session,
);

const FAKE_STRUCTURED_CATALOG_ENTRY: ModelCatalogEntry = FAKE_DEFINITION.catalog_entry();

fn main() {
    let catalog = ModelCatalog::new(&[
        MODEL_CATALOG[0],
        MODEL_CATALOG[1],
        FAKE_STRUCTURED_CATALOG_ENTRY,
    ]);
    let entry = catalog
        .find_session(FAKE_SESSION_KEY)
        .expect("structured fixture model should be discoverable");
    let registration = FAKE_DEFINITION.session_registration();

    assert_eq!(entry.registry.model, "Structured Fixture Wheel");
    assert_eq!(entry.registration.parser, Some(FAKE_PARSER_KEY));
    assert_eq!(registration.key, FAKE_SESSION_KEY);
    assert_eq!(registration.model.model, "Structured Fixture Wheel");
    assert!(matches!(
        registration.construct(),
        RegisteredReadOnlySession::BegodeFalcon(_)
    ));
    println!(
        "structured model definition ready: {} / {}",
        entry.registry.manufacturer, entry.registry.model
    );
}
