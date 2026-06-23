use std::collections::BTreeSet;

use btleplug::api::{CharPropFlags, Characteristic};
use cutout_core::{GattChannel, GattRoles};
use uuid::Uuid;

/// Marker trait for known BTLE UUIDs with static protocol meaning.
pub trait KnownGattUuid {
    /// The concrete Bluetooth UUID represented by this marker.
    const UUID: Uuid;
}

/// Standard BLE Battery service.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StandardBatteryService;

impl KnownGattUuid for StandardBatteryService {
    const UUID: Uuid = Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb);
}

/// Standard BLE Battery Level characteristic.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StandardBatteryLevelCharacteristic;

impl KnownGattUuid for StandardBatteryLevelCharacteristic {
    const UUID: Uuid = Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb);
}

/// Shared FFE0 service used by supported PEV protocol adapters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SharedFfe0Service;

impl KnownGattUuid for SharedFfe0Service {
    const UUID: Uuid = Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb);
}

/// UUID observed at the BTLE boundary, classified when the value has known meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GattUuid {
    /// Standard BLE Battery service.
    StandardBatteryService(StandardBatteryService),

    /// Standard BLE Battery Level characteristic.
    StandardBatteryLevelCharacteristic(StandardBatteryLevelCharacteristic),

    /// Shared FFE0 service used by supported PEV protocol adapters.
    SharedFfe0Service(SharedFfe0Service),

    /// A UUID without a built-in semantic marker.
    Other(Uuid),
}

const KNOWN_GATT_UUIDS: [(Uuid, GattUuid); 3] = [
    (
        StandardBatteryService::UUID,
        GattUuid::StandardBatteryService(StandardBatteryService),
    ),
    (
        StandardBatteryLevelCharacteristic::UUID,
        GattUuid::StandardBatteryLevelCharacteristic(StandardBatteryLevelCharacteristic),
    ),
    (
        SharedFfe0Service::UUID,
        GattUuid::SharedFfe0Service(SharedFfe0Service),
    ),
];

fn find_known_gatt_uuid<T>(
    predicate: impl Fn(&(Uuid, GattUuid)) -> bool,
    project: impl FnOnce(&(Uuid, GattUuid)) -> T,
) -> Option<T> {
    KNOWN_GATT_UUIDS
        .iter()
        .find(|entry| predicate(entry))
        .map(project)
}

impl GattUuid {
    /// Classifies a raw UUID into known semantic markers where possible.
    #[must_use]
    pub fn classify(uuid: Uuid) -> Self {
        find_known_gatt_uuid(|(known, _)| *known == uuid, |(_, classified)| *classified)
            .unwrap_or(Self::Other(uuid))
    }

    /// Returns the concrete UUID represented by this typed classification.
    #[must_use]
    pub fn as_uuid(self) -> Uuid {
        find_known_gatt_uuid(|(_, classified)| *classified == self, |(uuid, _)| *uuid)
            .unwrap_or_else(|| match self {
                Self::Other(uuid) => uuid,
                known => unreachable!("known GATT UUID missing from table: {known:?}"),
            })
    }
}

pub(crate) const fn characteristic_from_summary(
    uuid: Uuid,
    service_uuid: Uuid,
    properties: CharPropFlags,
) -> Characteristic {
    Characteristic {
        uuid,
        service_uuid,
        properties,
        descriptors: BTreeSet::new(),
    }
}

pub(crate) const fn gatt_channel_from_uuid(uuid: Uuid) -> GattChannel {
    GattChannel::from_uuid(uuid)
}

pub(crate) fn gatt_roles_from_flags(flags: CharPropFlags) -> GattRoles {
    [
        (
            CharPropFlags::READ,
            GattRoles::with_read as fn(GattRoles) -> GattRoles,
        ),
        (CharPropFlags::WRITE, GattRoles::with_write),
        (
            CharPropFlags::WRITE_WITHOUT_RESPONSE,
            GattRoles::with_write_without_response,
        ),
        (CharPropFlags::NOTIFY, GattRoles::with_notify),
        (CharPropFlags::INDICATE, GattRoles::with_indicate),
    ]
    .into_iter()
    .filter_map(|(flag, apply)| flags.contains(flag).then_some(apply))
    .fold(GattRoles::empty(), |roles, apply| apply(roles))
}

#[cfg(test)]
mod tests {
    use super::{
        GattUuid, KnownGattUuid, SharedFfe0Service, StandardBatteryLevelCharacteristic,
        StandardBatteryService,
    };
    use uuid::Uuid;

    #[test]
    fn known_uuid_constants_are_backed_by_zst_markers() {
        assert_eq!(
            <StandardBatteryService as KnownGattUuid>::UUID,
            Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb)
        );
        assert_eq!(
            <StandardBatteryLevelCharacteristic as KnownGattUuid>::UUID,
            Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb)
        );
        assert_eq!(
            <SharedFfe0Service as KnownGattUuid>::UUID,
            Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb)
        );
    }

    #[test]
    fn known_uuid_classification_discards_raw_uuid_payload() {
        assert_eq!(
            GattUuid::classify(<StandardBatteryService as KnownGattUuid>::UUID),
            GattUuid::StandardBatteryService(StandardBatteryService)
        );
        assert_eq!(
            GattUuid::classify(<StandardBatteryLevelCharacteristic as KnownGattUuid>::UUID),
            GattUuid::StandardBatteryLevelCharacteristic(StandardBatteryLevelCharacteristic)
        );
        assert_eq!(
            GattUuid::classify(<SharedFfe0Service as KnownGattUuid>::UUID),
            GattUuid::SharedFfe0Service(SharedFfe0Service)
        );
    }

    #[test]
    fn unknown_uuid_classification_preserves_uuid_payload() {
        let uuid = Uuid::from_u128(0x6e40_0003_b5a3_f393_e0a9_e50e_24dc_ca9e);

        assert_eq!(GattUuid::classify(uuid), GattUuid::Other(uuid));
        assert_eq!(GattUuid::classify(uuid).as_uuid(), uuid);
    }
}
