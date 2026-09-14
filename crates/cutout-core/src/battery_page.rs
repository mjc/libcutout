use arrayvec::ArrayVec;

use crate::{
    BatteryInfo, BmsPackCurrents, BmsTemperatureValuesPerPage, Measured, ProtocolSelector,
    ProtocolTag, Temperature, VerificationStatus, Voltage,
};

/// Complete BMS page cycles retained for stabilizing cell-voltage observations.
pub(crate) const BMS_OBSERVATION_HISTORY_CYCLES: usize = 7;

/// One raw sample retained behind a stabilized BMS voltage observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsVoltageSample {
    /// Raw voltage reported by the BMS.
    pub voltage: Voltage,
    /// Host monotonic receipt time attached before the readback entered retained state.
    pub observed_at: crate::MonotonicTimestamp,
}

/// Current stabilized value and recent raw history for one protocol-assigned observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BmsVoltageObservation {
    /// Stable zero-based identity across every reported pack.
    pub index: crate::BmsObservationIndex,
    /// Protocol-assigned zero-based pack/BMS identity, when known.
    pub pack_index: Option<crate::BmsPackIndex>,
    /// Zero-based position within that pack, when known.
    pub pack_observation_index: Option<crate::BmsCellIndex>,
    /// Lower median of the retained raw samples, used for live summaries.
    pub voltage: Voltage,
    /// Most recent raw sample.
    pub latest_voltage: Voltage,
    /// Oldest-to-newest bounded raw history.
    pub samples: Vec<BmsVoltageSample>,
}

/// Numerical summary of identified voltage observations, independent of physical topology.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BmsObservationSummary {
    /// Number of distinct voltage observations (including reported zero values).
    pub observed_count: u32,
    /// Zero-based identity of the lowest voltage; ties choose the smallest identity.
    pub lowest_index: Option<crate::BmsObservationIndex>,
    /// Zero-based identity of the highest voltage; ties choose the smallest identity.
    pub highest_index: Option<crate::BmsObservationIndex>,
    /// Highest minus lowest voltage, saturated to the voltage-delta representation.
    pub voltage_spread: Option<crate::VoltageDelta>,
    /// Stabilized readings and bounded raw history used to produce the summary.
    pub observations: Vec<BmsVoltageObservation>,
}

/// Temperature readings retained from the latest report for every BMS temperature source.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BmsTemperatureSummary {
    /// Flattened readings in stable source-page and sensor order.
    pub readings: Vec<Temperature>,
    /// Highest reading from the current retained pages.
    pub highest_temperature: Option<Temperature>,
}

impl BmsTemperatureSummary {
    /// Summarizes the latest retained temperature source for each page identity.
    #[must_use]
    pub fn from_readbacks(readbacks: &[crate::BatteryReadback]) -> Self {
        let mut pages: Vec<_> = readbacks
            .iter()
            .filter_map(|readback| match readback.page() {
                Some(BatteryPagePayload::Temperature(page)) => Some((
                    page.page,
                    page.temperatures[..usize::from(page.temperature_count.get())].to_vec(),
                )),
                // Some protocols report their sole BMS temperature in a metadata page rather
                // than a typed temperature page. Retain that scalar instead of letting an empty
                // page summary erase it at the shared projection boundary.
                Some(BatteryPagePayload::Raw(page)) => page
                    .battery
                    .temperature
                    .map(|temperature| (page.page, vec![temperature.value])),
                Some(BatteryPagePayload::CellVoltage(_)) | None => None,
            })
            .collect();
        pages.sort_unstable_by_key(|(page, _)| {
            (page.tag.map(ProtocolTag::get), page.selector.get())
        });
        let readings: Vec<_> = pages
            .into_iter()
            .flat_map(|(_, readings)| readings)
            .collect();
        Self {
            highest_temperature: readings
                .iter()
                .copied()
                .max_by_key(|temperature| temperature.as_millicelsius()),
            readings,
        }
    }
}

impl BmsObservationSummary {
    /// Summarizes retained readbacks in oldest-to-newest replacement order.
    ///
    /// Untimestamped decoder-local and non-cell pages have no retained voltage observations.
    /// Later timestamped values extend the raw history for the same identity. An unassigned page
    /// uses page-local indices, as in its DTO. This does not imply the readings were measured
    /// simultaneously or cover a full pack.
    #[must_use]
    pub fn from_readbacks(readbacks: &[crate::BatteryReadback]) -> Self {
        let mut observations = std::collections::BTreeMap::new();
        for readback in readbacks {
            let Some(observed_at) = readback.observed_at() else {
                continue;
            };
            let Some(BatteryPagePayload::CellVoltage(page)) = readback.page() else {
                continue;
            };
            let first = readback
                .first_observation_index()
                .map_or(0, crate::BmsObservationIndex::get);
            for (slot, voltage) in page.cell_voltages.iter().enumerate() {
                if let Some(index) = u16::try_from(slot)
                    .ok()
                    .and_then(|slot| first.checked_add(slot))
                {
                    let pack_observation_index = u16::try_from(slot).ok().and_then(|slot| {
                        readback
                            .first_pack_observation_index()
                            .and_then(|first| first.get().checked_add(slot))
                            .map(crate::BmsCellIndex::new)
                    });
                    let sample = BmsVoltageSample {
                        voltage: *voltage,
                        observed_at,
                    };
                    observations
                        .entry(index)
                        .and_modify(|observation: &mut BmsVoltageObservation| {
                            observation.pack_index = readback.observation_pack_index();
                            observation.pack_observation_index = pack_observation_index;
                            observation.latest_voltage = *voltage;
                            observation.samples.push(sample);
                        })
                        .or_insert_with(|| BmsVoltageObservation {
                            index: crate::BmsObservationIndex::new(index),
                            pack_index: readback.observation_pack_index(),
                            pack_observation_index,
                            voltage: *voltage,
                            latest_voltage: *voltage,
                            samples: vec![sample],
                        });
                }
            }
        }
        for observation in observations.values_mut() {
            observation.voltage = stabilized_voltage(&observation.samples);
        }
        let lowest = observations
            .iter()
            .min_by_key(|(index, observation)| (observation.voltage.as_millivolts(), **index));
        let highest = observations.iter().min_by_key(|(index, observation)| {
            (
                std::cmp::Reverse(observation.voltage.as_millivolts()),
                **index,
            )
        });
        Self {
            // At most 65,536 distinct u16 observation identities can participate.
            observed_count: u32::try_from(observations.len()).unwrap_or(u32::MAX),
            lowest_index: lowest.map(|(index, _)| crate::BmsObservationIndex::new(*index)),
            highest_index: highest.map(|(index, _)| crate::BmsObservationIndex::new(*index)),
            voltage_spread: lowest.zip(highest).map(|((_, low), (_, high))| {
                crate::VoltageDelta::from_millivolts(
                    high.voltage
                        .as_millivolts()
                        .saturating_sub(low.voltage.as_millivolts()),
                )
            }),
            observations: observations.into_values().collect(),
        }
    }
}

fn stabilized_voltage(samples: &[BmsVoltageSample]) -> Voltage {
    let seed = samples
        .first()
        .expect("an observation is created with one sample")
        .voltage;
    let mut values: Vec<_> = samples.iter().map(|sample| sample.voltage).collect();
    values.resize(BMS_OBSERVATION_HISTORY_CYCLES, seed);
    values.sort_unstable_by_key(|value| value.as_millivolts());
    values[values.len().saturating_sub(1) / 2]
}

/// Maximum number of cell or cell-group voltage values carried by one typed BMS page.
pub const BATTERY_CELL_VOLTAGE_VALUES_MAX: usize = 15;

/// Fixed number of temperature values carried by typed BMS temperature pages.
pub const BATTERY_TEMPERATURE_VALUES_PER_PAGE: usize = 6;

/// Bounded cell or cell-group voltage values decoded from one BMS page.
pub type BatteryCellVoltages = ArrayVec<Voltage, BATTERY_CELL_VOLTAGE_VALUES_MAX>;

/// Battery/BMS page classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatteryPageKind {
    /// Metadata-only page, not a direct measurement page.
    Metadata,

    /// Typed cell-voltage page.
    CellVoltage,

    /// Typed temperature/status page.
    Temperature,

    /// Reserved or not-yet-typed page.
    Raw,
}

/// Provenance and interpretation of a battery/BMS page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryPageMetadata {
    /// BMS page selector.
    pub selector: ProtocolSelector,

    /// Protocol tag/opcode that produced this page, when the family exposes one.
    pub tag: Option<ProtocolTag>,

    /// Current interpretation of the page.
    pub kind: BatteryPageKind,

    /// Verification state for this interpretation.
    pub verification: VerificationStatus,
}

impl BatteryPageMetadata {
    /// Creates a page metadata record.
    #[must_use]
    pub const fn new(
        selector: ProtocolSelector,
        kind: BatteryPageKind,
        verification: VerificationStatus,
    ) -> Self {
        Self {
            selector,
            tag: None,
            kind,
            verification,
        }
    }

    /// Attaches the source protocol tag/opcode to this page.
    #[must_use]
    pub const fn with_tag(mut self, tag: ProtocolTag) -> Self {
        self.tag = Some(tag);
        self
    }

    /// Creates metadata for an interpreted metadata page.
    #[must_use]
    pub const fn metadata(selector: ProtocolSelector, verification: VerificationStatus) -> Self {
        Self::new(selector, BatteryPageKind::Metadata, verification)
    }

    /// Creates metadata for a typed cell-voltage page.
    #[must_use]
    pub const fn cell_voltage(
        selector: ProtocolSelector,
        verification: VerificationStatus,
    ) -> Self {
        Self::new(selector, BatteryPageKind::CellVoltage, verification)
    }

    /// Creates metadata for a typed temperature/status page.
    #[must_use]
    pub const fn temperature(selector: ProtocolSelector, verification: VerificationStatus) -> Self {
        Self::new(selector, BatteryPageKind::Temperature, verification)
    }

    /// Creates metadata for a raw or reserved page.
    #[must_use]
    pub const fn raw(selector: ProtocolSelector, verification: VerificationStatus) -> Self {
        Self::new(selector, BatteryPageKind::Raw, verification)
    }
}

/// Page-specific payload for a typed battery cell-voltage page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatteryCellVoltagePage {
    /// Page metadata for this payload.
    pub page: BatteryPageMetadata,

    /// Generic battery measurements decoded from this page.
    pub battery: BatteryInfo,

    /// Cell or cell-group voltage values decoded from this page.
    pub cell_voltages: BatteryCellVoltages,
}

impl BatteryCellVoltagePage {
    /// Creates a typed cell-voltage payload.
    #[must_use]
    pub fn new(
        page: BatteryPageMetadata,
        battery: BatteryInfo,
        cell_voltages: BatteryCellVoltages,
    ) -> Self {
        Self {
            page,
            battery,
            cell_voltages,
        }
    }
}

/// Page-specific payload for a typed battery/BMS temperature or status page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryTemperaturePage {
    /// Page metadata for this payload.
    pub page: BatteryPageMetadata,

    /// Generic battery measurements decoded from this page.
    pub battery: BatteryInfo,

    /// Contiguous BMS temperature values.
    pub temperatures: [Temperature; BATTERY_TEMPERATURE_VALUES_PER_PAGE],

    /// Number of populated temperature values.
    pub temperature_count: BmsTemperatureValuesPerPage,
}

impl BatteryTemperaturePage {
    /// Creates a typed temperature/status payload.
    #[must_use]
    pub const fn new(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self {
            page,
            battery,
            temperatures: [Temperature::from_millicelsius(0); BATTERY_TEMPERATURE_VALUES_PER_PAGE],
            temperature_count: BmsTemperatureValuesPerPage::new(0),
        }
    }

    /// Creates a typed temperature/status payload with page-specific values.
    #[must_use]
    pub const fn with_temperatures(
        page: BatteryPageMetadata,
        battery: BatteryInfo,
        temperatures: [Option<Measured<Temperature>>; BATTERY_TEMPERATURE_VALUES_PER_PAGE],
    ) -> Self {
        let mut values = [Temperature::from_millicelsius(0); BATTERY_TEMPERATURE_VALUES_PER_PAGE];
        let mut index = 0;
        let mut count = 0_u8;
        while index < BATTERY_TEMPERATURE_VALUES_PER_PAGE {
            let Some(temperature) = temperatures[index] else {
                break;
            };
            values[index] = temperature.value;
            index += 1;
            count += 1;
        }
        Self {
            page,
            battery,
            temperatures: values,
            temperature_count: BmsTemperatureValuesPerPage::new(count),
        }
    }
}

/// Page-specific payload for a raw or reserved battery page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryRawPage {
    /// Page metadata for this payload.
    pub page: BatteryPageMetadata,

    /// Generic battery measurements decoded from this page.
    pub battery: BatteryInfo,

    /// Page-specific paired BMS pack currents.
    pub bms_pack_currents: Option<BmsPackCurrents>,
}

impl BatteryRawPage {
    /// Creates a raw battery payload.
    #[must_use]
    pub const fn new(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self {
            page,
            battery,
            bms_pack_currents: None,
        }
    }

    /// Adds paired BMS pack-current values to this page.
    #[must_use]
    pub const fn with_bms_pack_currents(mut self, currents: BmsPackCurrents) -> Self {
        self.bms_pack_currents = Some(currents);
        self
    }
}

/// Explicit page payload returned by a battery/BMS decoder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatteryPagePayload {
    /// Typed cell-voltage page payload.
    CellVoltage(BatteryCellVoltagePage),

    /// Typed temperature/status page payload.
    Temperature(BatteryTemperaturePage),

    /// Raw or reserved page payload.
    Raw(BatteryRawPage),
}

impl BatteryPagePayload {
    /// Builds a payload from page metadata and battery values.
    #[must_use]
    pub const fn from_page(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        match page.kind {
            BatteryPageKind::CellVoltage => Self::Raw(BatteryRawPage::new(
                BatteryPageMetadata::raw(page.selector, page.verification),
                battery,
            )),
            BatteryPageKind::Temperature => {
                Self::Temperature(BatteryTemperaturePage::new(page, battery))
            }
            BatteryPageKind::Metadata | BatteryPageKind::Raw => {
                Self::Raw(BatteryRawPage::new(page, battery))
            }
        }
    }

    /// Builds a payload for a typed cell-voltage page.
    #[must_use]
    pub fn cell_voltage(
        page: BatteryPageMetadata,
        battery: BatteryInfo,
        cell_voltages: BatteryCellVoltages,
    ) -> Self {
        Self::CellVoltage(BatteryCellVoltagePage::new(page, battery, cell_voltages))
    }

    /// Builds a payload for a typed temperature/status page.
    #[must_use]
    pub const fn temperature(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self::Temperature(BatteryTemperaturePage::new(page, battery))
    }

    /// Builds a payload for a typed temperature/status page with fixed values.
    #[must_use]
    pub const fn temperature_values(
        page: BatteryPageMetadata,
        battery: BatteryInfo,
        temperatures: [Option<Measured<Temperature>>; BATTERY_TEMPERATURE_VALUES_PER_PAGE],
    ) -> Self {
        Self::Temperature(BatteryTemperaturePage::with_temperatures(
            page,
            battery,
            temperatures,
        ))
    }

    /// Returns page-specific temperature values when present.
    #[must_use]
    pub fn temperatures(&self) -> [Option<Measured<i32>>; BATTERY_TEMPERATURE_VALUES_PER_PAGE] {
        match self {
            Self::Temperature(page) => {
                let mut temperatures = [None; BATTERY_TEMPERATURE_VALUES_PER_PAGE];
                let mut index = 0;
                while index < page.temperature_count.get() as usize {
                    temperatures[index] = Some(Measured::reported(
                        page.temperatures[index].as_millicelsius(),
                    ));
                    index += 1;
                }
                temperatures
            }
            Self::CellVoltage(_) | Self::Raw(_) => [None; BATTERY_TEMPERATURE_VALUES_PER_PAGE],
        }
    }

    /// Builds a payload for a raw or reserved page.
    #[must_use]
    pub const fn raw(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self::Raw(BatteryRawPage::new(page, battery))
    }

    /// Adds paired BMS pack-current values to a raw/metadata page payload.
    #[must_use]
    pub fn with_bms_pack_currents(self, currents: BmsPackCurrents) -> Self {
        match self {
            Self::Raw(page) => Self::Raw(page.with_bms_pack_currents(currents)),
            Self::CellVoltage(page) => Self::CellVoltage(page),
            Self::Temperature(page) => Self::Temperature(page),
        }
    }

    /// Returns the page metadata for this payload.
    #[must_use]
    pub fn page(&self) -> BatteryPageMetadata {
        match self {
            Self::CellVoltage(page) => page.page,
            Self::Temperature(page) => page.page,
            Self::Raw(page) => page.page,
        }
    }

    /// Returns the decoded battery values for this payload.
    #[must_use]
    pub fn battery(&self) -> BatteryInfo {
        match self {
            Self::CellVoltage(page) => page.battery,
            Self::Temperature(page) => page.battery,
            Self::Raw(page) => page.battery,
        }
    }

    /// Returns paired BMS pack-current values when present.
    #[must_use]
    pub fn bms_pack_currents(&self) -> Option<BmsPackCurrents> {
        match self {
            Self::Raw(page) => page.bms_pack_currents,
            Self::CellVoltage(_) | Self::Temperature(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn sel(value: u8) -> ProtocolSelector {
        ProtocolSelector::new(value)
    }

    #[test]
    fn page_metadata_preserves_selector_kind_and_verification() {
        let page = BatteryPageMetadata::new(
            sel(8),
            BatteryPageKind::Metadata,
            VerificationStatus::SourceVerified,
        );

        assert_eq!(page.selector, sel(8));
        assert_eq!(page.kind, BatteryPageKind::Metadata);
        assert_eq!(page.verification, VerificationStatus::SourceVerified);
    }

    #[test]
    fn page_metadata_constructors_choose_expected_kinds() {
        assert_eq!(
            BatteryPageMetadata::metadata(sel(1), VerificationStatus::HardwareVerified).kind,
            BatteryPageKind::Metadata
        );
        assert_eq!(
            BatteryPageMetadata::cell_voltage(sel(3), VerificationStatus::HardwareVerified).kind,
            BatteryPageKind::CellVoltage
        );
        assert_eq!(
            BatteryPageMetadata::temperature(sel(7), VerificationStatus::SourceVerified).kind,
            BatteryPageKind::Temperature
        );
        assert_eq!(
            BatteryPageMetadata::raw(sel(8), VerificationStatus::SourceVerified).kind,
            BatteryPageKind::Raw
        );
    }

    #[test]
    fn page_payload_wrappers_preserve_page_and_battery_values() {
        let battery = BatteryInfo {
            voltage: None,
            current: None,
            level_reported: None,
            level_estimated: None,
            temperature: None,
            raw_state: None,
        };
        let page = BatteryPageMetadata::cell_voltage(sel(3), VerificationStatus::HardwareVerified);
        let cell_voltages = [
            Voltage::from_millivolts(3_701),
            Voltage::from_millivolts(3_699),
        ]
        .into_iter()
        .collect();
        let payload = BatteryPagePayload::cell_voltage(page, battery, cell_voltages);

        assert_eq!(payload.page(), page);
        assert_eq!(payload.battery(), battery);
        if let BatteryPagePayload::CellVoltage(cell_page) = payload {
            assert_eq!(cell_page.cell_voltages.len(), 2);
            assert_eq!(cell_page.cell_voltages[0], Voltage::from_millivolts(3_701));
        }
    }

    #[test]
    fn temperature_payload_wrapper_preserves_page_and_battery_values() {
        let battery = BatteryInfo {
            voltage: None,
            current: None,
            level_reported: None,
            level_estimated: None,
            temperature: Some(Measured::reported(Temperature::from_millicelsius(16_730))),
            raw_state: None,
        };
        let page = BatteryPageMetadata::temperature(sel(3), VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::Temperature(BatteryTemperaturePage::new(page, battery));

        assert_eq!(payload.page(), page);
        assert_eq!(payload.battery(), battery);
        if let BatteryPagePayload::Temperature(temperature_page) = payload {
            assert_eq!(
                temperature_page.temperature_count,
                BmsTemperatureValuesPerPage::new(0)
            );
        }
    }

    #[test]
    fn payload_conversion_chooses_raw_for_reserved_pages() {
        let battery = BatteryInfo::default();
        let page = BatteryPageMetadata::raw(sel(8), VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::from_page(page, battery);

        assert!(matches!(payload, BatteryPagePayload::Raw(_)));
        assert_eq!(payload.page(), page);
    }

    #[test]
    fn payload_conversion_chooses_raw_for_metadata_pages() {
        let battery = BatteryInfo::default();
        let page = BatteryPageMetadata::metadata(sel(8), VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::from_page(page, battery);

        assert!(matches!(payload, BatteryPagePayload::Raw(_)));
        assert_eq!(payload.page(), page);
    }

    #[test]
    fn payload_conversion_does_not_invent_empty_typed_cell_pages() {
        let battery = BatteryInfo::default();
        let page = BatteryPageMetadata::cell_voltage(sel(3), VerificationStatus::HardwareVerified);
        let payload = BatteryPagePayload::from_page(page, battery);

        assert!(matches!(payload, BatteryPagePayload::Raw(_)));
        assert_eq!(payload.page().kind, BatteryPageKind::Raw);
        assert_eq!(payload.page().selector, page.selector);
        assert_eq!(payload.page().verification, page.verification);
    }

    #[test]
    fn payload_conversion_chooses_typed_variant_for_temperature_pages() {
        let battery = BatteryInfo::default();
        let page = BatteryPageMetadata::temperature(sel(7), VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::from_page(page, battery);

        assert!(matches!(payload, BatteryPagePayload::Temperature(_)));
        assert_eq!(payload.page(), page);
    }

    #[test]
    fn explicit_temperature_constructor_chooses_temperature_variant() {
        let battery = BatteryInfo {
            temperature: Some(Measured::reported(Temperature::from_millicelsius(17_830))),
            ..BatteryInfo::default()
        };
        let page = BatteryPageMetadata::temperature(sel(3), VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::temperature(page, battery);

        assert!(matches!(payload, BatteryPagePayload::Temperature(_)));
        assert_eq!(payload.battery().temperature, battery.temperature);
    }
}
