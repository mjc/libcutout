use crate::{BatteryInfo, VerificationStatus};

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
    pub selector: u8,

    /// Current interpretation of the page.
    pub kind: BatteryPageKind,

    /// Verification state for this interpretation.
    pub verification: VerificationStatus,
}

impl BatteryPageMetadata {
    /// Creates a page metadata record.
    #[must_use]
    pub const fn new(
        selector: u8,
        kind: BatteryPageKind,
        verification: VerificationStatus,
    ) -> Self {
        Self {
            selector,
            kind,
            verification,
        }
    }

    /// Creates metadata for an interpreted metadata page.
    #[must_use]
    pub const fn metadata(selector: u8, verification: VerificationStatus) -> Self {
        Self::new(selector, BatteryPageKind::Metadata, verification)
    }

    /// Creates metadata for a typed cell-voltage page.
    #[must_use]
    pub const fn cell_voltage(selector: u8, verification: VerificationStatus) -> Self {
        Self::new(selector, BatteryPageKind::CellVoltage, verification)
    }

    /// Creates metadata for a typed temperature/status page.
    #[must_use]
    pub const fn temperature(selector: u8, verification: VerificationStatus) -> Self {
        Self::new(selector, BatteryPageKind::Temperature, verification)
    }

    /// Creates metadata for a raw or reserved page.
    #[must_use]
    pub const fn raw(selector: u8, verification: VerificationStatus) -> Self {
        Self::new(selector, BatteryPageKind::Raw, verification)
    }
}

/// Page-specific payload for a typed battery cell-voltage page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryCellVoltagePage {
    /// Page metadata for this payload.
    pub page: BatteryPageMetadata,

    /// Generic battery measurements decoded from this page.
    pub battery: BatteryInfo,
}

impl BatteryCellVoltagePage {
    /// Creates a typed cell-voltage payload.
    #[must_use]
    pub const fn new(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self { page, battery }
    }
}

/// Page-specific payload for a typed battery/BMS temperature or status page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryTemperaturePage {
    /// Page metadata for this payload.
    pub page: BatteryPageMetadata,

    /// Generic battery measurements decoded from this page.
    pub battery: BatteryInfo,
}

impl BatteryTemperaturePage {
    /// Creates a typed temperature/status payload.
    #[must_use]
    pub const fn new(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self { page, battery }
    }
}

/// Page-specific payload for a raw or reserved battery page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryRawPage {
    /// Page metadata for this payload.
    pub page: BatteryPageMetadata,

    /// Generic battery measurements decoded from this page.
    pub battery: BatteryInfo,
}

impl BatteryRawPage {
    /// Creates a raw battery payload.
    #[must_use]
    pub const fn new(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self { page, battery }
    }
}

/// Explicit page payload returned by a battery/BMS decoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
            BatteryPageKind::CellVoltage => {
                Self::CellVoltage(BatteryCellVoltagePage::new(page, battery))
            }
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
    pub const fn cell_voltage(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self::CellVoltage(BatteryCellVoltagePage::new(page, battery))
    }

    /// Builds a payload for a typed temperature/status page.
    #[must_use]
    pub const fn temperature(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self::Temperature(BatteryTemperaturePage::new(page, battery))
    }

    /// Builds a payload for a raw or reserved page.
    #[must_use]
    pub const fn raw(page: BatteryPageMetadata, battery: BatteryInfo) -> Self {
        Self::Raw(BatteryRawPage::new(page, battery))
    }

    /// Returns the page metadata for this payload.
    #[must_use]
    pub const fn page(self) -> BatteryPageMetadata {
        match self {
            Self::CellVoltage(page) => page.page,
            Self::Temperature(page) => page.page,
            Self::Raw(page) => page.page,
        }
    }

    /// Returns the decoded battery values for this payload.
    #[must_use]
    pub const fn battery(self) -> BatteryInfo {
        match self {
            Self::CellVoltage(page) => page.battery,
            Self::Temperature(page) => page.battery,
            Self::Raw(page) => page.battery,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_metadata_preserves_selector_kind_and_verification() {
        let page = BatteryPageMetadata::new(
            8,
            BatteryPageKind::Metadata,
            VerificationStatus::SourceVerified,
        );

        assert_eq!(page.selector, 8);
        assert_eq!(page.kind, BatteryPageKind::Metadata);
        assert_eq!(page.verification, VerificationStatus::SourceVerified);
    }

    #[test]
    fn page_metadata_constructors_choose_expected_kinds() {
        assert_eq!(
            BatteryPageMetadata::metadata(1, VerificationStatus::HardwareVerified).kind,
            BatteryPageKind::Metadata
        );
        assert_eq!(
            BatteryPageMetadata::cell_voltage(3, VerificationStatus::HardwareVerified).kind,
            BatteryPageKind::CellVoltage
        );
        assert_eq!(
            BatteryPageMetadata::temperature(7, VerificationStatus::SourceVerified).kind,
            BatteryPageKind::Temperature
        );
        assert_eq!(
            BatteryPageMetadata::raw(8, VerificationStatus::SourceVerified).kind,
            BatteryPageKind::Raw
        );
    }

    #[test]
    fn page_payload_wrappers_preserve_page_and_battery_values() {
        let battery = BatteryInfo {
            voltage_mv: None,
            current_ma: None,
            percent_reported: None,
            percent_estimated: None,
            temperature_mc: None,
            raw_state: None,
        };
        let page = BatteryPageMetadata::cell_voltage(3, VerificationStatus::HardwareVerified);
        let payload = BatteryPagePayload::CellVoltage(BatteryCellVoltagePage::new(page, battery));

        assert_eq!(payload.page(), page);
        assert_eq!(payload.battery(), battery);
    }

    #[test]
    fn temperature_payload_wrapper_preserves_page_and_battery_values() {
        let battery = BatteryInfo {
            voltage_mv: None,
            current_ma: None,
            percent_reported: None,
            percent_estimated: None,
            temperature_mc: Some(crate::Measured::reported(16_730)),
            raw_state: None,
        };
        let page = BatteryPageMetadata::temperature(3, VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::Temperature(BatteryTemperaturePage::new(page, battery));

        assert_eq!(payload.page(), page);
        assert_eq!(payload.battery(), battery);
    }

    #[test]
    fn payload_conversion_chooses_raw_for_reserved_pages() {
        let battery = BatteryInfo::default();
        let page = BatteryPageMetadata::raw(8, VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::from_page(page, battery);

        assert!(matches!(payload, BatteryPagePayload::Raw(_)));
        assert_eq!(payload.page(), page);
    }

    #[test]
    fn payload_conversion_chooses_raw_for_metadata_pages() {
        let battery = BatteryInfo::default();
        let page = BatteryPageMetadata::metadata(8, VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::from_page(page, battery);

        assert!(matches!(payload, BatteryPagePayload::Raw(_)));
        assert_eq!(payload.page(), page);
    }

    #[test]
    fn payload_conversion_chooses_typed_variant_for_cell_pages() {
        let battery = BatteryInfo::default();
        let page = BatteryPageMetadata::cell_voltage(3, VerificationStatus::HardwareVerified);
        let payload = BatteryPagePayload::from_page(page, battery);

        assert!(matches!(payload, BatteryPagePayload::CellVoltage(_)));
        assert_eq!(payload.page(), page);
    }

    #[test]
    fn payload_conversion_chooses_typed_variant_for_temperature_pages() {
        let battery = BatteryInfo::default();
        let page = BatteryPageMetadata::temperature(7, VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::from_page(page, battery);

        assert!(matches!(payload, BatteryPagePayload::Temperature(_)));
        assert_eq!(payload.page(), page);
    }

    #[test]
    fn explicit_temperature_constructor_chooses_temperature_variant() {
        let battery = BatteryInfo {
            temperature_mc: Some(crate::Measured::reported(17_830)),
            ..BatteryInfo::default()
        };
        let page = BatteryPageMetadata::temperature(3, VerificationStatus::SourceVerified);
        let payload = BatteryPagePayload::temperature(page, battery);

        assert!(matches!(payload, BatteryPagePayload::Temperature(_)));
        assert_eq!(payload.battery().temperature_mc, battery.temperature_mc);
    }
}
