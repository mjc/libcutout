use btleplug::{
    api::{Peripheral as _, PeripheralProperties},
    platform::Peripheral,
};

use crate::{
    BluetoothAddress, ManufacturerDataSummaries, ManufacturerDataSummary, PeripheralObservation,
};

impl PeripheralObservation {
    pub(crate) fn from_peripheral(
        peripheral: &Peripheral,
        properties: PeripheralProperties,
    ) -> Self {
        Self {
            identifier: peripheral.id().to_string(),
            address: normalize_address(properties.address.to_string()),
            name: properties.local_name,
            rssi: properties.rssi,
            advertised_services: properties.services.into_iter().collect(),
            manufacturer_data: manufacturer_data_summary(properties.manufacturer_data),
        }
    }
}

fn manufacturer_data_summary(
    manufacturer_data: std::collections::HashMap<u16, Vec<u8>>,
) -> ManufacturerDataSummaries {
    let mut summary = manufacturer_data
        .into_iter()
        .map(|(company_id, bytes)| ManufacturerDataSummary {
            company_id,
            len: bytes.len(),
        })
        .collect::<ManufacturerDataSummaries>();
    summary.sort_unstable_by_key(|value| value.company_id);
    summary
}

fn normalize_address(address: String) -> Option<String> {
    BluetoothAddress::new(&address).is_some().then_some(address)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::manufacturer_data_summary;
    use crate::ManufacturerDataSummary;

    #[test]
    fn manufacturer_data_summary_sorts_without_retaining_payloads() {
        let summary = manufacturer_data_summary(HashMap::from([
            (0x004c, vec![1, 2, 3, 4]),
            (0x000f, vec![5, 6]),
        ]));

        assert_eq!(
            summary.as_slice(),
            [
                ManufacturerDataSummary {
                    company_id: 0x000f,
                    len: 2
                },
                ManufacturerDataSummary {
                    company_id: 0x004c,
                    len: 4
                },
            ]
        );
    }
}
