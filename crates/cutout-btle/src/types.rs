use std::fmt;

use btleplug::{
    api::{CharPropFlags, Characteristic, Service},
    platform::Peripheral,
};
use cutout_core::{GattFingerprint, VerificationStatus};
use smallvec::SmallVec;
use uuid::Uuid;

use crate::{
    GattUuid, KnownGattUuid,
    gatt::{
        characteristic_from_summary as gatt_characteristic_from_summary, gatt_channel_from_uuid,
        gatt_roles_from_flags,
    },
};

/// Advertised service UUIDs carried inline for the common small advertisement set.
pub type AdvertisedServices = SmallVec<[Uuid; 4]>;

/// Manufacturer data summaries carried inline for the common small advertisement set.
pub type ManufacturerDataSummaries = SmallVec<[ManufacturerDataSummary; 4]>;

/// A peripheral observation gathered from a scan or connection pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeripheralObservation {
    /// Platform-specific peripheral identifier.
    pub identifier: String,

    /// Bluetooth address when the platform exposes one.
    pub address: Option<String>,

    /// Peripheral local name, if one was advertised.
    pub name: Option<String>,

    /// Received signal strength, if the platform exposed it.
    pub rssi: Option<i16>,

    /// Advertised service UUIDs, if the peripheral exposed them.
    pub advertised_services: AdvertisedServices,

    /// Manufacturer data company ids and payload lengths advertised by the peripheral.
    pub manufacturer_data: ManufacturerDataSummaries,
}

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

/// Bluetooth address text after platform placeholder normalization.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct BluetoothAddress<'a>(&'a str);

/// Error returned when the platform address is only the null placeholder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NullBluetoothAddress;

impl<'a> BluetoothAddress<'a> {
    /// Creates a typed address unless the platform reported the null placeholder.
    #[must_use]
    pub fn new(value: &'a str) -> Option<Self> {
        (value != "00:00:00:00:00:00").then_some(Self(value))
    }

    /// Returns the normalized address text.
    #[must_use]
    pub const fn as_str(&self) -> &'a str {
        self.0
    }

    /// Returns the borrowed normalized address text.
    #[must_use]
    pub const fn into_inner(self) -> &'a str {
        self.0
    }
}

impl fmt::Display for BluetoothAddress<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl<'a> TryFrom<&'a str> for BluetoothAddress<'a> {
    type Error = NullBluetoothAddress;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(NullBluetoothAddress)
    }
}

/// Summary of advertised manufacturer data without retaining opaque payload bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManufacturerDataSummary {
    /// Bluetooth SIG company identifier.
    pub company_id: u16,

    /// Payload length in bytes.
    pub len: usize,
}

impl PeripheralObservation {
    /// Returns the typed platform-specific peripheral identifier.
    #[must_use]
    pub fn platform_identifier(&self) -> PeripheralIdentifier<'_> {
        PeripheralIdentifier::new(&self.identifier)
    }

    /// Returns the typed normalized Bluetooth address, when present.
    #[must_use]
    pub fn bluetooth_address(&self) -> Option<BluetoothAddress<'_>> {
        self.address.as_deref().and_then(BluetoothAddress::new)
    }

    /// Returns classified advertised service UUIDs.
    pub fn advertised_service_uuids(&self) -> impl Iterator<Item = GattUuid> + '_ {
        self.advertised_services
            .iter()
            .copied()
            .map(GattUuid::classify)
    }

    /// Returns whether a known service marker was observed in advertisements.
    #[must_use]
    pub fn advertises<T>(&self) -> bool
    where
        T: KnownGattUuid,
    {
        self.advertised_service_uuids()
            .any(|service| service.as_uuid() == T::UUID)
    }

    fn family_hints(&self) -> impl Iterator<Item = &'static str> {
        scan_family_hints(self.name.as_deref(), self.advertised_service_uuids())
    }
}

impl fmt::Display for PeripheralObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_observation_identity(f, self.address.as_deref(), &self.identifier)?;
        write!(f, " name={}", self.name.as_deref().unwrap_or("<none>"))?;
        self.rssi.map_or(Ok(()), |rssi| write!(f, " rssi={rssi}"))?;
        f.write_str(" services=[")?;
        write_delimited(f, self.advertised_services.iter(), ", ", |f, uuid| {
            write!(f, "{uuid}")
        })?;
        f.write_str("] manufacturer_data=[")?;
        write_delimited(f, self.manufacturer_data.iter(), ",", |f, value| {
            write!(f, "{:04x}:{}b", value.company_id, value.len)
        })?;
        f.write_str("] family_hints=[")?;
        write_delimited(f, self.family_hints(), ",", write_str_value)?;
        f.write_str("]")
    }
}

fn write_observation_identity(
    f: &mut fmt::Formatter<'_>,
    address: Option<&str>,
    identifier: &str,
) -> fmt::Result {
    match address {
        Some(address) => f.write_str(address),
        None => write!(f, "id={identifier}"),
    }
}

/// Target used to select a peripheral from scan results.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectionTarget {
    /// Match against the peripheral address, when provided.
    pub address: Option<String>,

    /// Match against the platform-specific peripheral identifier.
    pub identifier: Option<String>,

    /// Match against the peripheral local name, when provided.
    pub name_contains: Option<String>,
}

impl ConnectionTarget {
    /// Returns whether an observation matches this target.
    #[must_use]
    pub fn matches(&self, observation: &PeripheralObservation) -> bool {
        [
            self.address
                .as_ref()
                .is_none_or(|address| observation.address.as_deref() == Some(address.as_str())),
            self.identifier
                .as_ref()
                .is_none_or(|identifier| observation.identifier == *identifier),
            self.name_contains.as_ref().is_none_or(|needle| {
                observation
                    .name
                    .as_deref()
                    .is_some_and(|name| name.contains(needle))
            }),
        ]
        .into_iter()
        .all(core::convert::identity)
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
    pub characteristics: Vec<CharacteristicSummary>,
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
    pub services: Vec<ServiceSummary>,
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
    /// Returns observed GATT fingerprints for PEVCAP and registry evidence.
    #[must_use]
    pub fn gatt_fingerprints(&self) -> Vec<GattFingerprint> {
        self.services
            .iter()
            .flat_map(|service| service.characteristics.iter())
            .map(CharacteristicSummary::gatt_fingerprint)
            .collect()
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

fn write_str_value(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    f.write_str(value)
}

fn scan_family_hints(
    name: Option<&str>,
    advertised_services: impl IntoIterator<Item = GattUuid>,
) -> impl Iterator<Item = &'static str> {
    [
        advertised_services
            .into_iter()
            .any(|service| matches!(service, GattUuid::SharedFfe0Service(_)))
            .then_some("shared-ffe0-ffe1"),
        name.is_some_and(|name| {
            name.contains("Aero") || name.contains("NOSFET") || name.starts_with("NF")
        })
        .then_some("name-nosfet-aero"),
        name.is_some_and(|name| {
            name.contains("Falcon") || name.contains("Begode") || name.contains("Gotway")
        })
        .then_some("name-begode-falcon"),
    ]
    .into_iter()
    .flatten()
}
