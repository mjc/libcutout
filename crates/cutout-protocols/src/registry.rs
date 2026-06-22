use cutout_core::{
    Capabilities, CommandKind, GattFingerprint, GattRoles, ModelRegistryEntry, ProtocolFamily,
    VerificationStatus,
};

use crate::{BEGODE_DATA_CHANNEL, BEGODE_SERVICE_CHANNEL};

const BEGODE_FALCON_GATT: [GattFingerprint; 1] = [GattFingerprint {
    service: BEGODE_SERVICE_CHANNEL,
    characteristic: BEGODE_DATA_CHANNEL,
    roles: GattRoles::empty()
        .with_write_without_response()
        .with_notify(),
    verification: VerificationStatus::SourceVerified,
}];

/// Source-backed initial registry entry for the Begode Falcon.
pub const BEGODE_FALCON_REGISTRY_ENTRY: ModelRegistryEntry = ModelRegistryEntry {
    manufacturer: "Begode",
    model: "Falcon",
    protocol_family: ProtocolFamily::BegodeGotway,
    advertised_name_hints: &["Falcon", "Begode", "Gotway"],
    wire_model_id: None,
    battery: None,
    bms: None,
    gatt: &BEGODE_FALCON_GATT,
    capabilities: Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestFirmwareInfo,
        CommandKind::RequestTelemetry,
        CommandKind::RequestBatteryInfo,
    ]),
    verification: VerificationStatus::Inferred,
};

#[cfg(test)]
mod tests {
    use cutout_core::{CommandKind, ProtocolFamily, VerificationStatus};

    use crate::{
        BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_SERVICE_CHANNEL,
        BegodePackVoltageProfile,
    };

    #[test]
    fn begode_falcon_registry_entry_does_not_infer_battery_variant_from_name() {
        assert_eq!(BEGODE_FALCON_REGISTRY_ENTRY.battery, None);
    }

    #[test]
    fn begode_falcon_84v_profile_remains_available_for_confirmed_variant() {
        let profile = BegodePackVoltageProfile::Begode84VFullCharge;

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
}
