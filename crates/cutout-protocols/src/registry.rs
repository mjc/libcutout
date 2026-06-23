use cutout_core::{
    ModelCatalogEntry, ModelRegistryEntry, ModelRuntimeFactories, ParserFactory, SessionFactory,
};

use crate::{BegodeFalconModel, NosfetAeroModel, RegisteredModelSpec};

fn veteran_parser_factory_marker() {}
fn begode_parser_factory_marker() {}
fn veteran_session_factory_marker() {}
fn begode_session_factory_marker() {}

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
        factories: ModelRuntimeFactories {
            parser: Some(ParserFactory::new(veteran_parser_factory_marker)),
            session: Some(SessionFactory::new(veteran_session_factory_marker)),
        },
    },
    ModelCatalogEntry {
        registry: &BEGODE_FALCON_REGISTRY_ENTRY,
        factories: ModelRuntimeFactories {
            parser: Some(ParserFactory::new(begode_parser_factory_marker)),
            session: Some(SessionFactory::new(begode_session_factory_marker)),
        },
    },
];

#[cfg(test)]
mod tests {
    use cutout_core::{
        CommandKind, ManufacturerKey, ModelCatalog, ModelKey, ProtocolFamily, VerificationStatus,
    };

    use crate::{
        BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_SERVICE_CHANNEL,
        BegodeFalconModel, BegodePackVoltageProfile, MODEL_CATALOG, MODEL_REGISTRY,
        NOSFET_AERO_REGISTRY_ENTRY, NosfetAeroModel, RegisteredModelSpec, VETERAN_DATA_CHANNEL,
        begode_falcon_target_voltage_profile,
    };

    #[test]
    fn catalog_represents_aero_and_falcon_without_btle_model_code() {
        cutout_core::validate_registry_entries(&MODEL_REGISTRY)
            .expect("protocol registry entries are structurally valid");
        cutout_core::validate_model_catalog(&MODEL_CATALOG)
            .expect("protocol catalog entries have runtime factory markers");

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
