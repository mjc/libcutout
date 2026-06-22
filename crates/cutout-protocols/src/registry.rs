use cutout_core::{
    BatterySpec, Capabilities, CommandKind, GattFingerprint, GattRoles, ModelRegistryEntry,
    ProtocolFamily, VerificationStatus,
};

use crate::{BEGODE_DATA_CHANNEL, BEGODE_SERVICE_CHANNEL, BegodePackVoltageProfile};

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
    battery: Some(BatterySpec {
        series_cells: BegodePackVoltageProfile::Falcon84V.series_cells(),
        nominal_capacity_mah: BegodePackVoltageProfile::Falcon84V.nominal_capacity_mah(),
        voltage_range_mv: 60_000..=84_000,
        verification: VerificationStatus::Inferred,
    }),
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

    use crate::{BEGODE_DATA_CHANNEL, BEGODE_FALCON_REGISTRY_ENTRY, BEGODE_SERVICE_CHANNEL};

    #[test]
    fn begode_falcon_registry_entry_records_84v_20s_battery_profile() {
        let battery = BEGODE_FALCON_REGISTRY_ENTRY
            .battery
            .expect("Falcon registry entry should include an initial battery profile");

        assert_eq!(battery.series_cells, 20);
        assert_eq!(battery.voltage_range_mv, 60_000..=84_000);
        assert_eq!(battery.nominal_capacity_mah, Some(3_750));
        assert_eq!(battery.verification, VerificationStatus::Inferred);
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
