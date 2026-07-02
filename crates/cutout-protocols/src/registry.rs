#![allow(unreachable_pub)]

use cutout_core::{
    GattChannel, ModelCatalogEntry, ModelRegistryEntry, ModelRuntimeRegistration, ParserKey,
    ProtocolSession, SessionInput, SessionKey, SessionOutput,
};

use crate::{
    BegodeFalconModel, BegodeNotificationDecoder, BegodePackVoltageProfile, NosfetAeroModel,
    ReadOnlySession,
};

mod begode_falcon;
mod nosfet_aero;

/// Parser registration key for Veteran/LeaperKim/NOSFET notifications.
pub const VETERAN_PARSER_KEY: ParserKey = ParserKey::new("veteran");

/// Parser registration key for Begode/Gotway notifications.
pub const BEGODE_PARSER_KEY: ParserKey = ParserKey::new("begode");

/// Session registration key for the NOSFET Aero read-only session.
pub const NOSFET_AERO_SESSION_KEY: SessionKey = SessionKey::new("nosfet-aero-read-only");

/// Session registration key for the Begode Falcon read-only session.
pub const BEGODE_FALCON_SESSION_KEY: SessionKey = SessionKey::new("begode-falcon-read-only");

#[allow(unused_imports)]
pub use begode_falcon::BEGODE_FALCON_REGISTRY_ENTRY;
#[allow(unused_imports)]
pub use nosfet_aero::NOSFET_AERO_REGISTRY_ENTRY;

/// Structured source data for a statically registered model.
#[derive(Clone, Copy, Debug)]
pub struct RegisteredModelDefinition {
    /// Registry metadata shared by catalog and runtime registrations.
    pub registry: &'static ModelRegistryEntry,

    /// Parser registration key for the model.
    pub parser_key: ParserKey,

    /// Session registration key for the model.
    pub session_key: SessionKey,

    /// Notification data channel expected by the constructed session.
    pub data_channel: GattChannel,

    pub(crate) construct: fn() -> RegisteredReadOnlySession,
}

impl RegisteredModelDefinition {
    /// Builds a structured model definition.
    #[must_use]
    pub const fn new(
        registry: &'static ModelRegistryEntry,
        parser_key: ParserKey,
        session_key: SessionKey,
        data_channel: GattChannel,
        construct: fn() -> RegisteredReadOnlySession,
    ) -> Self {
        Self {
            registry,
            parser_key,
            session_key,
            data_channel,
            construct,
        }
    }

    /// Builds the catalog entry for this model definition.
    #[must_use]
    pub const fn catalog_entry(self) -> ModelCatalogEntry {
        ModelCatalogEntry::new(
            self.registry,
            ModelRuntimeRegistration::active(self.parser_key, self.session_key),
        )
    }

    /// Builds the session registration for this model definition.
    #[must_use]
    pub const fn session_registration(self) -> SessionRegistration {
        SessionRegistration {
            key: self.session_key,
            model: self.registry,
            data_channel: self.data_channel,
            construct: self.construct,
        }
    }
}

/// Session constructor registered for a model catalog entry.
#[derive(Clone, Copy, Debug)]
pub struct SessionRegistration {
    /// Stable registration key referenced by the model catalog.
    pub key: SessionKey,

    /// Registry entry this constructor supports.
    pub model: &'static ModelRegistryEntry,

    /// Notification data channel expected by the constructed session.
    pub data_channel: GattChannel,

    construct: fn() -> RegisteredReadOnlySession,
}

impl SessionRegistration {
    /// Constructs the registered read-only session.
    #[must_use]
    pub fn construct(self) -> RegisteredReadOnlySession {
        (self.construct)()
    }
}

include!(concat!(env!("OUT_DIR"), "/registry_models.rs"));

/// Allocation-free read-only session sum type for statically registered models.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum RegisteredReadOnlySession {
    /// NOSFET Aero read-only protocol session.
    NosfetAero(ReadOnlySession<NosfetAeroModel, false>),

    /// Begode Falcon read-only protocol session.
    BegodeFalcon(ReadOnlySession<BegodeFalconModel, true>),
}

impl ProtocolSession for RegisteredReadOnlySession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match self {
            Self::NosfetAero(session) => session.handle(input, output),
            Self::BegodeFalcon(session) => session.handle(input, output),
        }
    }
}

pub(super) fn nosfet_aero_read_only_session() -> RegisteredReadOnlySession {
    RegisteredReadOnlySession::NosfetAero(ReadOnlySession::<NosfetAeroModel, false>::default())
}

pub(super) fn begode_falcon_read_only_session() -> RegisteredReadOnlySession {
    RegisteredReadOnlySession::BegodeFalcon(ReadOnlySession::<BegodeFalconModel, true>::default())
}

/// Constructs a registered Begode Falcon read-only session with explicit pack-voltage evidence.
#[must_use]
pub fn begode_falcon_read_only_session_with_voltage_profile(
    profile: BegodePackVoltageProfile,
) -> RegisteredReadOnlySession {
    RegisteredReadOnlySession::BegodeFalcon(
        ReadOnlySession::<BegodeFalconModel, true>::with_decoder(
            BegodeNotificationDecoder::with_pack_voltage_profile(profile),
        ),
    )
}

/// Finds a session registration by typed key without allocating.
#[must_use]
pub fn find_session_registration(key: SessionKey) -> Option<&'static SessionRegistration> {
    SESSION_REGISTRATIONS
        .iter()
        .find(|registration| registration.key == key)
}

#[cfg(test)]
mod tests {
    use cutout_core::{
        CommandKind, CompleteModelAuthoring, GattFingerprint, GattRoles, ManufacturerKey,
        ModelAuthoring, ModelCatalog, ModelCatalogEntry, ModelKey, ModelRegistryEntry,
        ModelRuntimeRegistration, ParserKey, ProtocolFamily, RegistryValidationError, SessionKey,
        VerificationStatus, Voltage,
    };

    use crate::{
        BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_FALCON_SESSION_KEY,
        BEGODE_SERVICE_CHANNEL, BegodeFalconModel, BegodePackVoltageProfile, MODEL_CATALOG,
        MODEL_REGISTRY, NOSFET_AERO_REGISTRY_ENTRY, NOSFET_AERO_SESSION_KEY, NosfetAeroModel,
        RegisteredModelSpec, RegisteredReadOnlySession, VETERAN_DATA_CHANNEL, VETERAN_PARSER_KEY,
        VETERAN_SERVICE_CHANNEL, begode_falcon_target_voltage_profile, find_session_registration,
    };

    #[test]
    fn catalog_represents_aero_and_falcon_without_btle_model_code() {
        cutout_core::validate_registry_entries(&MODEL_REGISTRY)
            .expect("protocol registry entries are structurally valid");
        cutout_core::validate_model_catalog(&MODEL_CATALOG)
            .expect("protocol catalog entries have runtime registration tokens");

        let catalog = ModelCatalog::new(&MODEL_CATALOG);

        assert_eq!(
            catalog
                .find_model(ManufacturerKey::new("NOSFET"), ModelKey::new("NOSFET Aero"))
                .map(|entry| entry.registry.protocol_family),
            Some(ProtocolFamily::VeteranLeaperkimNosfet)
        );
        assert_eq!(
            catalog
                .find_model(ManufacturerKey::new("Begode"), ModelKey::new("Falcon"))
                .map(|entry| entry.registry.protocol_family),
            Some(ProtocolFamily::BegodeGotway)
        );
    }

    #[test]
    fn session_registration_constructs_model_sessions_by_typed_key() {
        let aero = find_session_registration(NOSFET_AERO_SESSION_KEY)
            .expect("Aero session registration exists");
        let falcon = find_session_registration(BEGODE_FALCON_SESSION_KEY)
            .expect("Falcon session registration exists");

        assert_eq!(aero.model.model, "NOSFET Aero");
        assert_eq!(aero.data_channel, VETERAN_DATA_CHANNEL);
        assert!(matches!(
            aero.construct(),
            RegisteredReadOnlySession::NosfetAero(_)
        ));
        assert_eq!(falcon.model.model, "Falcon");
        assert_eq!(falcon.data_channel, BEGODE_DATA_CHANNEL);
        assert!(matches!(
            falcon.construct(),
            RegisteredReadOnlySession::BegodeFalcon(_)
        ));
        assert!(find_session_registration(SessionKey::new("missing")).is_none());
    }

    #[test]
    fn aero_registry_uses_ffe0_service_and_ffe1_characteristic() {
        let [fingerprint] = NOSFET_AERO_REGISTRY_ENTRY.gatt else {
            panic!("Aero registry should expose one GATT fingerprint");
        };

        assert_eq!(fingerprint.service, VETERAN_SERVICE_CHANNEL);
        assert_eq!(fingerprint.characteristic, VETERAN_DATA_CHANNEL);
        assert_ne!(fingerprint.service, fingerprint.characteristic);
        cutout_core::validate_registry_entries(&[&NOSFET_AERO_REGISTRY_ENTRY])
            .expect("Aero registry GATT is structurally valid");
    }

    #[test]
    fn model_authoring_workflow_registers_fake_and_real_models() {
        const FAKE_PARSER_KEY: ParserKey = ParserKey::new("fake-parser");
        const FAKE_SESSION_KEY: SessionKey = SessionKey::new("fake-read-only");
        const FAKE_GATT: [GattFingerprint; 1] = [GattFingerprint {
            service: BEGODE_SERVICE_CHANNEL,
            characteristic: BEGODE_DATA_CHANNEL,
            roles: GattRoles::empty()
                .with_write_without_response()
                .with_notify(),
            verification: VerificationStatus::Inferred,
        }];
        static FAKE_MODEL: ModelRegistryEntry = ModelRegistryEntry {
            manufacturer: ManufacturerKey::new("FakeCo"),
            model: ModelKey::new("Fixture Wheel"),
            protocol_family: ProtocolFamily::BegodeGotway,
            advertised_name_hints: &["FixtureWheel"],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: &FAKE_GATT,
            capabilities: cutout_core::Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestTelemetry,
            ]),
            verification: VerificationStatus::Inferred,
        };
        let catalog = [
            MODEL_CATALOG[0],
            MODEL_CATALOG[1],
            ModelCatalogEntry::new(
                &FAKE_MODEL,
                ModelRuntimeRegistration::active(FAKE_PARSER_KEY, FAKE_SESSION_KEY),
            ),
        ];
        let registry = [
            MODEL_CATALOG[0].registry,
            MODEL_CATALOG[1].registry,
            &FAKE_MODEL,
        ];

        assert_eq!(cutout_core::validate_registry_entries(&registry), Ok(()));
        assert_eq!(cutout_core::validate_model_catalog(&catalog), Ok(()));
        let catalog = ModelCatalog::new(&catalog);
        assert_eq!(
            catalog
                .find_model(
                    ManufacturerKey::new("FakeCo"),
                    ModelKey::new("Fixture Wheel")
                )
                .map(|entry| entry.registration.session),
            Some(Some(FAKE_SESSION_KEY))
        );
        assert_eq!(
            catalog
                .find_parser(FAKE_PARSER_KEY)
                .map(|entry| entry.registry.model),
            Some(ModelKey::new("Fixture Wheel"))
        );
        assert!(matches!(
            catalog.resolve_advertised_name("FixtureWheel BLE"),
            cutout_core::CatalogModelResolution::Matched(entry)
                if entry.registry.model == "Fixture Wheel"
        ));
        assert_eq!(
            catalog
                .find_model(ManufacturerKey::new("NOSFET"), ModelKey::new("NOSFET Aero"))
                .map(|entry| entry.registration.session),
            Some(Some(NOSFET_AERO_SESSION_KEY))
        );
    }

    #[test]
    fn typed_model_authoring_builds_catalog_without_raw_field_literals() {
        const FAKE_PARSER_KEY: ParserKey = ParserKey::new("typed-fake-parser");
        const FAKE_SESSION_KEY: SessionKey = SessionKey::new("typed-fake-read-only");
        const FAKE_GATT: [GattFingerprint; 1] = [GattFingerprint {
            service: BEGODE_SERVICE_CHANNEL,
            characteristic: BEGODE_DATA_CHANNEL,
            roles: GattRoles::empty()
                .with_write_without_response()
                .with_notify(),
            verification: VerificationStatus::Inferred,
        }];
        const FAKE_AUTHORING: CompleteModelAuthoring = ModelAuthoring::new()
            .manufacturer(ManufacturerKey::new("TypedFakeCo"))
            .model(ModelKey::new("Typed Fixture Wheel"))
            .family(cutout_core::FamilyKey::new(ProtocolFamily::BegodeGotway))
            .advertised_name_hints(&["TypedFixture"])
            .gatt(&FAKE_GATT)
            .capabilities(cutout_core::Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestTelemetry,
            ]))
            .verification(VerificationStatus::Inferred)
            .active_runtime(FAKE_PARSER_KEY, FAKE_SESSION_KEY);
        static FAKE_MODEL: ModelRegistryEntry = FAKE_AUTHORING.registry_entry();
        const FAKE_CATALOG_ENTRY: ModelCatalogEntry = FAKE_AUTHORING.catalog_entry(&FAKE_MODEL);
        let catalog = [MODEL_CATALOG[0], MODEL_CATALOG[1], FAKE_CATALOG_ENTRY];

        assert_eq!(cutout_core::validate_model_catalog(&catalog), Ok(()));
        assert_eq!(
            ModelCatalog::new(&catalog)
                .find_session(FAKE_SESSION_KEY)
                .map(|entry| entry.registry.model),
            Some(ModelKey::new("Typed Fixture Wheel"))
        );
    }

    #[test]
    fn model_authoring_validation_errors_name_missing_registration() {
        let missing_parser = ModelCatalogEntry::new(
            &NOSFET_AERO_REGISTRY_ENTRY,
            ModelRuntimeRegistration {
                parser: None,
                session: Some(NOSFET_AERO_SESSION_KEY),
            },
        );
        let missing_session = ModelCatalogEntry::new(
            &NOSFET_AERO_REGISTRY_ENTRY,
            ModelRuntimeRegistration {
                parser: Some(VETERAN_PARSER_KEY),
                session: None,
            },
        );

        assert_eq!(
            cutout_core::validate_model_catalog(&[missing_parser]),
            Err(RegistryValidationError::MissingParserRegistration { index: 0 })
        );
        assert_eq!(
            cutout_core::validate_model_catalog(&[missing_session]),
            Err(RegistryValidationError::MissingSessionRegistration { index: 0 })
        );
    }

    #[test]
    fn nosfet_aero_registry_entry_uses_veteran_family_gatt_fingerprint() {
        assert_eq!(
            NOSFET_AERO_REGISTRY_ENTRY,
            <NosfetAeroModel as RegisteredModelSpec>::REGISTRY_ENTRY
        );
        let [fingerprint] = NOSFET_AERO_REGISTRY_ENTRY.gatt else {
            panic!("Aero should have exactly one hardware-backed GATT fingerprint");
        };

        assert_eq!(fingerprint.service, VETERAN_SERVICE_CHANNEL);
        assert_eq!(fingerprint.characteristic, VETERAN_DATA_CHANNEL);
        assert_ne!(fingerprint.service, fingerprint.characteristic);
        assert!(fingerprint.roles.supports_read());
        assert!(fingerprint.roles.supports_write());
        assert!(fingerprint.roles.supports_write_without_response());
        assert!(fingerprint.roles.supports_notify());
        assert_eq!(
            fingerprint.verification,
            VerificationStatus::HardwareVerified
        );
    }

    #[test]
    fn begode_falcon_registry_entry_does_not_select_battery_from_model_name() {
        assert_eq!(BEGODE_FALCON_REGISTRY_ENTRY.battery, None);
        assert_eq!(
            BEGODE_FALCON_REGISTRY_ENTRY,
            <BegodeFalconModel as RegisteredModelSpec>::REGISTRY_ENTRY
        );
    }

    #[test]
    fn begode_falcon_100v_profile_remains_available_for_confirmed_variant() {
        let profile = begode_falcon_target_voltage_profile();

        assert_eq!(profile, BegodePackVoltageProfile::Begode100VFullCharge);
        assert_eq!(profile.series_cells(), cutout_core::SeriesCount::new(24));
        assert_eq!(
            profile.voltage_range(),
            Voltage::from_millivolts(72_000)..=Voltage::from_millivolts(100_800)
        );
        assert_eq!(profile.nominal_capacity(), None);
    }

    #[test]
    fn begode_falcon_registry_entry_uses_begode_family_gatt_fingerprint() {
        let [fingerprint] = BEGODE_FALCON_REGISTRY_ENTRY.gatt else {
            panic!("Falcon should have exactly one source-backed GATT fingerprint");
        };

        assert_eq!(fingerprint.service, BEGODE_SERVICE_CHANNEL);
        assert_eq!(fingerprint.characteristic, BEGODE_DATA_CHANNEL);
        assert!(fingerprint.roles.supports_write_without_response());
        assert!(fingerprint.roles.supports_notify());
        assert_eq!(fingerprint.verification, VerificationStatus::SourceVerified);
    }

    #[test]
    fn begode_falcon_registry_entry_keeps_model_hints_separate_from_family_evidence() {
        assert_eq!(
            BEGODE_FALCON_REGISTRY_ENTRY.protocol_family,
            ProtocolFamily::BegodeGotway
        );
        assert!(
            BEGODE_FALCON_REGISTRY_ENTRY
                .advertised_name_hints
                .contains(&"Falcon")
        );
        assert_eq!(BEGODE_FALCON_REGISTRY_ENTRY.wire_model_id, None);
        assert_eq!(
            BEGODE_FALCON_REGISTRY_ENTRY.verification,
            VerificationStatus::Inferred
        );
    }

    #[test]
    fn begode_falcon_registry_entry_exposes_read_only_capabilities_only() {
        let capabilities = BEGODE_FALCON_REGISTRY_ENTRY.capabilities;

        assert!(capabilities.supports_command_kind(CommandKind::RequestIdentity));
        assert!(capabilities.supports_command_kind(CommandKind::RequestFirmwareInfo));
        assert!(capabilities.supports_command_kind(CommandKind::RequestTelemetry));
        assert!(capabilities.supports_command_kind(CommandKind::RequestBatteryInfo));
        assert!(!capabilities.supports_command_kind(CommandKind::RequestDiagnostics));
        assert!(!capabilities.supports_command_kind(CommandKind::SetLights));
    }

    #[test]
    fn begode_falcon_registry_entry_passes_core_validation() {
        cutout_core::validate_registry_entries(&[&BEGODE_FALCON_REGISTRY_ENTRY])
            .expect("Falcon registry entry is structurally valid");
    }
}
