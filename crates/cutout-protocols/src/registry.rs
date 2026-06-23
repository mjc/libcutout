use cutout_core::ModelRegistryEntry;

use crate::{BegodeFalconModel, RegisteredModelSpec};

/// Source-backed initial registry entry for the Begode Falcon.
pub const BEGODE_FALCON_REGISTRY_ENTRY: ModelRegistryEntry =
    <BegodeFalconModel as RegisteredModelSpec>::REGISTRY_ENTRY;

/// Compile-time model registry entries known to this crate.
pub const MODEL_REGISTRY: [&ModelRegistryEntry; 1] = [&BEGODE_FALCON_REGISTRY_ENTRY];

#[cfg(test)]
mod tests {
    use cutout_core::{CommandKind, ProtocolFamily, VerificationStatus};

    use crate::{
        BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_SERVICE_CHANNEL,
        BegodeFalconModel, BegodePackVoltageProfile, RegisteredModelSpec,
        begode_falcon_target_voltage_profile,
    };

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
