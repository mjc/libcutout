use cutout_core::{
    GattChannel, ModelCatalogEntry, ModelRegistryEntry, ModelRuntimeRegistration, ParserKey,
    ProtocolSession, SessionInput, SessionKey, SessionOutput,
};

use crate::{
    BEGODE_DATA_CHANNEL, BegodeFalconModel, BegodeNotificationDecoder, BegodePackVoltageProfile,
    NosfetAeroModel, ReadOnlySession, RegisteredModelSpec, VETERAN_DATA_CHANNEL,
};

/// Parser registration key for Veteran/LeaperKim/NOSFET notifications.
pub const VETERAN_PARSER_KEY: ParserKey = ParserKey::new("veteran");

/// Parser registration key for Begode/Gotway notifications.
pub const BEGODE_PARSER_KEY: ParserKey = ParserKey::new("begode");

/// Session registration key for the NOSFET Aero read-only session.
pub const NOSFET_AERO_SESSION_KEY: SessionKey = SessionKey::new("nosfet-aero-read-only");

/// Session registration key for the Begode Falcon read-only session.
pub const BEGODE_FALCON_SESSION_KEY: SessionKey = SessionKey::new("begode-falcon-read-only");

/// Hardware-backed registry entry for the NOSFET Aero.
pub const NOSFET_AERO_REGISTRY_ENTRY: ModelRegistryEntry =
    <NosfetAeroModel as RegisteredModelSpec>::REGISTRY_ENTRY;

/// Source-backed initial registry entry for the Begode Falcon.
pub const BEGODE_FALCON_REGISTRY_ENTRY: ModelRegistryEntry =
    <BegodeFalconModel as RegisteredModelSpec>::REGISTRY_ENTRY;

/// Compile-time model registry entries known to this crate.
pub const MODEL_REGISTRY: [&ModelRegistryEntry; 2] =
    [&NOSFET_AERO_REGISTRY_ENTRY, &BEGODE_FALCON_REGISTRY_ENTRY];

/// Compile-time model catalog entries known to this crate.
pub const MODEL_CATALOG: [ModelCatalogEntry; 2] = [
    ModelCatalogEntry {
        registry: &NOSFET_AERO_REGISTRY_ENTRY,
        registration: ModelRuntimeRegistration {
            parser: Some(VETERAN_PARSER_KEY),
            session: Some(NOSFET_AERO_SESSION_KEY),
        },
    },
    ModelCatalogEntry {
        registry: &BEGODE_FALCON_REGISTRY_ENTRY,
        registration: ModelRuntimeRegistration {
            parser: Some(BEGODE_PARSER_KEY),
            session: Some(BEGODE_FALCON_SESSION_KEY),
        },
    },
];

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

fn nosfet_aero_read_only_session() -> RegisteredReadOnlySession {
    RegisteredReadOnlySession::NosfetAero(ReadOnlySession::<NosfetAeroModel, false>::default())
}

fn begode_falcon_read_only_session() -> RegisteredReadOnlySession {
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

/// Read-only session registrations available from this protocol crate.
pub const SESSION_REGISTRATIONS: [SessionRegistration; 2] = [
    SessionRegistration {
        key: NOSFET_AERO_SESSION_KEY,
        model: &NOSFET_AERO_REGISTRY_ENTRY,
        data_channel: VETERAN_DATA_CHANNEL,
        construct: nosfet_aero_read_only_session,
    },
    SessionRegistration {
        key: BEGODE_FALCON_SESSION_KEY,
        model: &BEGODE_FALCON_REGISTRY_ENTRY,
        data_channel: BEGODE_DATA_CHANNEL,
        construct: begode_falcon_read_only_session,
    },
];

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
        CommandKind, ManufacturerKey, ModelCatalog, ModelKey, ProtocolFamily, SessionKey,
        VerificationStatus,
    };

    use crate::{
        BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_FALCON_SESSION_KEY,
        BEGODE_SERVICE_CHANNEL, BegodeFalconModel, BegodePackVoltageProfile, MODEL_CATALOG,
        MODEL_REGISTRY, NOSFET_AERO_REGISTRY_ENTRY, NOSFET_AERO_SESSION_KEY, NosfetAeroModel,
        RegisteredModelSpec, RegisteredReadOnlySession, VETERAN_DATA_CHANNEL,
        begode_falcon_target_voltage_profile, find_session_registration,
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
    fn nosfet_aero_registry_entry_uses_veteran_family_gatt_fingerprint() {
        assert_eq!(
            NOSFET_AERO_REGISTRY_ENTRY,
            <NosfetAeroModel as RegisteredModelSpec>::REGISTRY_ENTRY
        );
        let [fingerprint] = NOSFET_AERO_REGISTRY_ENTRY.gatt else {
            panic!("Aero should have exactly one hardware-backed GATT fingerprint");
        };

        assert_eq!(fingerprint.service, VETERAN_DATA_CHANNEL);
        assert_eq!(fingerprint.characteristic, VETERAN_DATA_CHANNEL);
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
    fn begode_falcon_84v_profile_remains_available_for_confirmed_variant() {
        let profile = begode_falcon_target_voltage_profile();

        assert_eq!(profile, BegodePackVoltageProfile::Begode84VFullCharge);
        assert_eq!(profile.series_cells(), 20);
        assert_eq!(profile.voltage_range_mv(), 60_000..=84_000);
        assert_eq!(profile.nominal_capacity_mah(), None);
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
