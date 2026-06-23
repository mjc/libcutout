use std::fmt;

use btleplug::{
    api::{CharPropFlags, Characteristic, Service},
    platform::Peripheral,
};
use cutout_core::{GattFingerprint, VerificationStatus};
use smallvec::SmallVec;
use uuid::Uuid;

use crate::{
    GattUuid, PeripheralObservation,
    gatt::{
        characteristic_from_summary as gatt_characteristic_from_summary, gatt_channel_from_uuid,
        gatt_roles_from_flags,
    },
};

/// Service summaries carried inline for the common single-GATT-service devices.
pub type ServiceSummaries = SmallVec<[ServiceSummary; 4]>;

/// Characteristic summaries carried inline for the common small service shape.
pub type CharacteristicSummaries = SmallVec<[CharacteristicSummary; 8]>;

/// Platform-specific peripheral identifier reported by the BLE backend.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct PeripheralIdentifier<'a>(&'a str);

impl<'a> PeripheralIdentifier<'a> {
    /// Creates a typed peripheral identifier from backend text.
    #[must_use]
    pub const fn new(value: &'a str) -> Self {
        Self(value)
    }

    /// Returns the backend identifier as text.
    #[must_use]
    pub const fn as_str(&self) -> &'a str {
        self.0
    }

    /// Returns the borrowed backend identifier.
    #[must_use]
    pub const fn into_inner(self) -> &'a str {
        self.0
    }
}

impl fmt::Display for PeripheralIdentifier<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Summary of a discovered GATT characteristic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacteristicSummary {
    /// Characteristic UUID.
    pub uuid: Uuid,

    /// Owning service UUID.
    pub service_uuid: Uuid,

    /// GATT characteristic properties.
    pub properties: CharPropFlags,
}

impl CharacteristicSummary {
    /// Returns the UUID classification for this characteristic.
    #[must_use]
    pub fn gatt_uuid(&self) -> GattUuid {
        GattUuid::classify(self.uuid)
    }

    /// Returns the UUID classification for the owning service.
    #[must_use]
    pub fn service_gatt_uuid(&self) -> GattUuid {
        GattUuid::classify(self.service_uuid)
    }

    /// Returns whether this characteristic can accept a write.
    #[must_use]
    pub fn can_write(&self) -> bool {
        self.properties
            .intersects(CharPropFlags::WRITE | CharPropFlags::WRITE_WITHOUT_RESPONSE)
    }

    /// Returns whether this characteristic can be read.
    #[must_use]
    pub fn can_read(&self) -> bool {
        self.properties.contains(CharPropFlags::READ)
    }

    /// Returns whether this characteristic can notify or indicate.
    #[must_use]
    pub fn can_notify(&self) -> bool {
        self.properties
            .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
    }

    pub(crate) fn gatt_fingerprint(&self) -> GattFingerprint {
        GattFingerprint {
            service: gatt_channel_from_uuid(self.service_uuid),
            characteristic: gatt_channel_from_uuid(self.uuid),
            roles: gatt_roles_from_flags(self.properties),
            verification: VerificationStatus::HardwareVerified,
        }
    }

    pub(crate) const fn from_characteristic(characteristic: &Characteristic) -> Self {
        Self {
            uuid: characteristic.uuid,
            service_uuid: characteristic.service_uuid,
            properties: characteristic.properties,
        }
    }
}

/// Service-level summary of a discovered peripheral.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceSummary {
    /// Service UUID.
    pub uuid: Uuid,

    /// Whether this is a primary service.
    pub primary: bool,

    /// Discovered characteristics for the service.
    pub characteristics: CharacteristicSummaries,
}

impl ServiceSummary {
    /// Returns the UUID classification for this service.
    #[must_use]
    pub fn gatt_uuid(&self) -> GattUuid {
        GattUuid::classify(self.uuid)
    }

    pub(crate) fn from_service(service: &Service) -> Self {
        Self {
            uuid: service.uuid,
            primary: service.primary,
            characteristics: service
                .characteristics
                .iter()
                .map(CharacteristicSummary::from_characteristic)
                .collect(),
        }
    }
}

/// Summary of a successful connection/discovery pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionSummary {
    /// Selected peripheral observation.
    pub observation: PeripheralObservation,

    /// Discovered GATT services and characteristics.
    pub services: ServiceSummaries,
}

impl fmt::Display for ConnectionSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "connected {}", self.observation)?;
        self.services.iter().try_for_each(|service| {
            write!(
                f,
                "service {} primary={} characteristics=[",
                service.uuid, service.primary
            )?;
            write_delimited(
                f,
                service.characteristics.iter(),
                ", ",
                |f, characteristic| {
                    write!(
                        f,
                        "{} props={:?}",
                        characteristic.uuid, characteristic.properties
                    )
                },
            )?;
            writeln!(f, "]")
        })
    }
}

impl ConnectionSummary {
    /// Iterates observed GATT fingerprints without allocating an intermediate
    /// flattened collection.
    pub fn iter_gatt_fingerprints(&self) -> impl Clone + Iterator<Item = GattFingerprint> + '_ {
        self.services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .map(CharacteristicSummary::gatt_fingerprint)
    }

    /// Returns observed GATT fingerprints for PEVCAP and registry evidence.
    #[must_use]
    pub fn gatt_fingerprints(&self) -> Vec<GattFingerprint> {
        self.iter_gatt_fingerprints().collect()
    }

    /// Selects the standard BLE Battery Level characteristic when present.
    #[must_use]
    pub fn battery_level_characteristic(&self) -> Option<&CharacteristicSummary> {
        self.services
            .iter()
            .find(|service| matches!(service.gatt_uuid(), GattUuid::StandardBatteryService(_)))
            .and_then(|service| {
                service.characteristics.iter().find(|characteristic| {
                    matches!(
                        characteristic.gatt_uuid(),
                        GattUuid::StandardBatteryLevelCharacteristic(_)
                    ) && characteristic.can_read()
                })
            })
    }

    /// Returns characteristics that can accept writes.
    pub fn write_candidates(&self) -> impl Iterator<Item = &CharacteristicSummary> {
        self.services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .filter(|characteristic| characteristic.can_write())
    }

    /// Returns characteristics that can notify or indicate.
    pub fn notify_candidates(&self) -> impl Iterator<Item = &CharacteristicSummary> {
        self.services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .filter(|characteristic| characteristic.can_notify())
    }

    /// Selects a notify/indicate characteristic by UUID, or the first
    /// notify-capable candidate when no UUID is requested.
    #[must_use]
    pub fn select_notify_characteristic(
        &self,
        requested: Option<Uuid>,
    ) -> Option<&CharacteristicSummary> {
        self.services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .find(|characteristic| {
                characteristic.can_notify()
                    && requested.is_none_or(|uuid| characteristic.uuid == uuid)
            })
    }

    /// Selects session endpoints from the discovered tree.
    #[must_use]
    pub fn select_session_endpoints(&self) -> Option<SessionEndpoints<'_>> {
        let write = self.write_candidates().next()?;
        let notify = self
            .services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .find(|characteristic| {
                characteristic.service_uuid == write.service_uuid && characteristic.can_notify()
            })
            .or_else(|| {
                self.services
                    .iter()
                    .flat_map(|service| service.characteristics.iter())
                    .find(|characteristic| characteristic.can_notify())
            });

        Some(SessionEndpoints { write, notify })
    }
}

/// A connected peripheral paired with its discovered GATT tree.
#[derive(Clone, Debug)]
pub struct ConnectedPeripheral {
    /// Connected peripheral handle that remains live for the bridge.
    pub peripheral: Peripheral,

    /// Discovered services and characteristics for the connected peripheral.
    pub summary: ConnectionSummary,
}

/// Selected endpoints for a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionEndpoints<'a> {
    /// Writable characteristic selected for request writes.
    pub write: &'a CharacteristicSummary,

    /// Notification-capable characteristic, if one was selected.
    pub notify: Option<&'a CharacteristicSummary>,
}

pub(crate) fn characteristic_from_summary(summary: &CharacteristicSummary) -> Characteristic {
    gatt_characteristic_from_summary(summary.uuid, summary.service_uuid, summary.properties)
}

pub(crate) fn write_delimited<T>(
    f: &mut fmt::Formatter<'_>,
    values: impl IntoIterator<Item = T>,
    separator: &str,
    mut write_value: impl FnMut(&mut fmt::Formatter<'_>, T) -> fmt::Result,
) -> fmt::Result {
    values
        .into_iter()
        .try_fold(Delimiter::First, |delimiter, value| {
            delimiter
                .write(f, separator)
                .and_then(|()| write_value(f, value))
                .map(|()| Delimiter::Rest)
        })
        .map(|_| ())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Delimiter {
    First,
    Rest,
}

impl Delimiter {
    fn write(self, f: &mut fmt::Formatter<'_>, separator: &str) -> fmt::Result {
        match self {
            Self::First => Ok(()),
            Self::Rest => f.write_str(separator),
        }
    }
}
