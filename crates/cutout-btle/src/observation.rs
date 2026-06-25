use std::fmt;

use btleplug::{
    api::{Peripheral as _, PeripheralProperties},
    platform::Peripheral,
};
use smallvec::SmallVec;
use uuid::Uuid;

use cutout_core::SignalStrength;

use crate::{
    BluetoothAddress, GattUuid, KnownGattUuid, ManufacturerDataSize, PeripheralIdentifier,
    target::normalize_address, types::write_delimited,
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
    pub rssi: Option<SignalStrength>,

    /// Advertised service UUIDs, if the peripheral exposed them.
    pub advertised_services: AdvertisedServices,

    /// Manufacturer data company ids and payload lengths advertised by the peripheral.
    pub manufacturer_data: ManufacturerDataSummaries,
}

/// Summary of advertised manufacturer data without retaining opaque payload bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManufacturerDataSummary {
    /// Bluetooth SIG company identifier.
    pub company_id: u16,

    /// Payload length in bytes.
    pub len: ManufacturerDataSize,
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

    pub(crate) fn from_peripheral(
        peripheral: &Peripheral,
        properties: PeripheralProperties,
    ) -> Self {
        Self {
            identifier: peripheral.id().to_string(),
            address: normalize_address(properties.address.to_string()),
            name: properties.local_name,
            rssi: properties.rssi.map(SignalStrength::from_dbm),
            advertised_services: properties.services.into_iter().collect(),
            manufacturer_data: sorted_manufacturer_data_summaries(
                properties
                    .manufacturer_data
                    .into_iter()
                    .map(ManufacturerDataSummary::from_backend_payload),
            ),
        }
    }

    pub(crate) fn without_properties(peripheral: &Peripheral) -> Self {
        Self {
            identifier: peripheral.id().to_string(),
            address: None,
            name: None,
            rssi: None,
            advertised_services: AdvertisedServices::new(),
            manufacturer_data: ManufacturerDataSummaries::new(),
        }
    }
}

impl ManufacturerDataSummary {
    fn from_backend_payload<B>((company_id, bytes): (u16, B)) -> Self
    where
        B: AsRef<[u8]>,
    {
        Self {
            company_id,
            len: ManufacturerDataSize::from_bytes(bytes.as_ref().len()),
        }
    }
}

impl fmt::Display for PeripheralObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_observation_identity(f, self.address.as_deref(), &self.identifier)?;
        write!(f, " name={}", self.name.as_deref().unwrap_or("<none>"))?;
        self.rssi
            .map_or(Ok(()), |rssi| write!(f, " rssi={}", rssi.as_dbm()))?;
        f.write_str(" services=[")?;
        write_delimited(f, self.advertised_services.iter(), ", ", |f, uuid| {
            write!(f, "{uuid}")
        })?;
        f.write_str("] manufacturer_data=[")?;
        write_delimited(f, self.manufacturer_data.iter(), ",", |f, value| {
            write!(f, "{:04x}:{}b", value.company_id, value.len)
        })?;
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

fn sorted_manufacturer_data_summaries(
    manufacturer_data: impl IntoIterator<Item = ManufacturerDataSummary>,
) -> ManufacturerDataSummaries {
    let mut summary = manufacturer_data
        .into_iter()
        .collect::<ManufacturerDataSummaries>();
    summary.sort_unstable_by_key(|value| value.company_id);
    summary
}

#[cfg(test)]
mod tests {
    use super::sorted_manufacturer_data_summaries;
    use crate::{ManufacturerDataSize, ManufacturerDataSummary};

    #[test]
    fn manufacturer_data_summary_sorts_already_summarized_backend_payloads() {
        let summary = sorted_manufacturer_data_summaries([
            ManufacturerDataSummary {
                company_id: 0x004c,
                len: ManufacturerDataSize::from_bytes(4),
            },
            ManufacturerDataSummary {
                company_id: 0x000f,
                len: ManufacturerDataSize::from_bytes(2),
            },
        ]);

        assert_eq!(
            summary.as_slice(),
            [
                ManufacturerDataSummary {
                    company_id: 0x000f,
                    len: ManufacturerDataSize::from_bytes(2)
                },
                ManufacturerDataSummary {
                    company_id: 0x004c,
                    len: ManufacturerDataSize::from_bytes(4)
                },
            ]
        );
    }
}
