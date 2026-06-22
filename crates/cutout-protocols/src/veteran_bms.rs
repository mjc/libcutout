use cutout_core::{
    BatteryInfo, BatteryPageKind, BatteryPageMetadata, BatteryPagePayload, VerificationStatus,
};
use thiserror::Error;

/// Cell-voltage count observed for typed Veteran/NOSFET BMS pages.
pub const VETERAN_BMS_CELL_VALUES_PER_PAGE: u8 = 16;

/// Classifies a Veteran/NOSFET BMS page selector from hardware-backed Aero captures.
#[must_use]
pub const fn classify_veteran_bms_selector(selector: u8) -> BatteryPageKind {
    match selector {
        0 | 4 => BatteryPageKind::Metadata,
        1 | 2 | 5 | 6 => BatteryPageKind::CellVoltage,
        _ => BatteryPageKind::Raw,
    }
}

/// Error returned when a typed Veteran/NOSFET BMS page violates invariants.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum VeteranBmsPageError {
    /// A cell-voltage page had an unexpected number of cell values.
    #[error("selector {selector} carried {observed} cell values, expected {expected}")]
    InvalidCellCount {
        /// BMS page selector.
        selector: u8,

        /// Observed cell value count.
        observed: u8,

        /// Expected cell value count.
        expected: u8,
    },
}

/// Decodes a pre-parsed Veteran/NOSFET BMS page into the generic battery payload shape.
///
/// The caller supplies the already-parsed cell value count because byte-level
/// Veteran frame decoding is a separate parser concern.
///
/// # Errors
///
/// Returns [`VeteranBmsPageError::InvalidCellCount`] when a selector classified
/// as a typed cell-voltage page does not carry the observed fixed count.
pub const fn decode_veteran_bms_page(
    selector: u8,
    observed_cell_values: u8,
    battery: BatteryInfo,
    verification: VerificationStatus,
) -> Result<BatteryPagePayload, VeteranBmsPageError> {
    let kind = classify_veteran_bms_selector(selector);
    if matches!(kind, BatteryPageKind::CellVoltage)
        && observed_cell_values != VETERAN_BMS_CELL_VALUES_PER_PAGE
    {
        return Err(VeteranBmsPageError::InvalidCellCount {
            selector,
            observed: observed_cell_values,
            expected: VETERAN_BMS_CELL_VALUES_PER_PAGE,
        });
    }

    Ok(BatteryPagePayload::from_page(
        BatteryPageMetadata::new(selector, kind, verification),
        battery,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn hardware_observed_selectors_classify_known_cell_pages() {
        for selector in [1, 2, 5, 6] {
            assert_eq!(
                classify_veteran_bms_selector(selector),
                BatteryPageKind::CellVoltage
            );
        }
    }

    #[test]
    fn hardware_observed_selectors_keep_metadata_and_unknown_pages_raw() {
        assert_eq!(classify_veteran_bms_selector(0), BatteryPageKind::Metadata);
        assert_eq!(classify_veteran_bms_selector(4), BatteryPageKind::Metadata);
        assert_eq!(classify_veteran_bms_selector(3), BatteryPageKind::Raw);
        assert_eq!(classify_veteran_bms_selector(7), BatteryPageKind::Raw);
        assert_eq!(classify_veteran_bms_selector(8), BatteryPageKind::Raw);
        assert_eq!(classify_veteran_bms_selector(9), BatteryPageKind::Raw);
    }

    #[test]
    fn typed_cell_pages_require_sixteen_cell_values() {
        let decoded = decode_veteran_bms_page(
            1,
            VETERAN_BMS_CELL_VALUES_PER_PAGE,
            BatteryInfo::default(),
            VerificationStatus::HardwareVerified,
        )
        .expect("hardware-backed cell page count should decode");

        assert!(matches!(decoded, BatteryPagePayload::CellVoltage(_)));
        assert_eq!(decoded.page().selector, 1);
        assert_eq!(
            decoded.page().verification,
            VerificationStatus::HardwareVerified
        );
    }

    #[test]
    fn typed_cell_pages_reject_wrong_cell_counts() {
        assert_eq!(
            decode_veteran_bms_page(
                2,
                VETERAN_BMS_CELL_VALUES_PER_PAGE - 1,
                BatteryInfo::default(),
                VerificationStatus::HardwareVerified,
            ),
            Err(VeteranBmsPageError::InvalidCellCount {
                selector: 2,
                observed: VETERAN_BMS_CELL_VALUES_PER_PAGE - 1,
                expected: VETERAN_BMS_CELL_VALUES_PER_PAGE,
            })
        );
    }

    #[test]
    fn raw_and_metadata_pages_do_not_claim_cell_voltage_typing() {
        let raw = decode_veteran_bms_page(
            8,
            0,
            BatteryInfo::default(),
            VerificationStatus::HardwareVerified,
        )
        .expect("raw pages should preserve evidence without typing");
        let metadata = decode_veteran_bms_page(
            0,
            0,
            BatteryInfo::default(),
            VerificationStatus::HardwareVerified,
        )
        .expect("metadata pages should preserve evidence without cell typing");

        assert!(matches!(raw, BatteryPagePayload::Raw(_)));
        assert!(matches!(metadata, BatteryPagePayload::Raw(_)));
        assert_eq!(raw.page().kind, BatteryPageKind::Raw);
        assert_eq!(metadata.page().kind, BatteryPageKind::Metadata);
    }

    proptest! {
        #[test]
        fn unknown_selectors_never_become_typed_cell_pages(selector in 9u8..) {
            prop_assert_eq!(classify_veteran_bms_selector(selector), BatteryPageKind::Raw);

            let decoded = decode_veteran_bms_page(
                selector,
                VETERAN_BMS_CELL_VALUES_PER_PAGE,
                BatteryInfo::default(),
                VerificationStatus::HardwareVerified,
            )
            .expect("unknown selectors should stay raw instead of failing typed invariants");

            prop_assert_eq!(decoded.page().kind, BatteryPageKind::Raw);
            prop_assert!(matches!(decoded, BatteryPagePayload::Raw(_)));
        }

        #[test]
        fn cell_page_selectors_only_accept_exact_cell_count(selector in prop_oneof![Just(1u8), Just(2), Just(5), Just(6)], count in 0u8..32) {
            let decoded = decode_veteran_bms_page(
                selector,
                count,
                BatteryInfo::default(),
                VerificationStatus::HardwareVerified,
            );

            if count == VETERAN_BMS_CELL_VALUES_PER_PAGE {
                let payload = decoded.expect("exact cell count should type the page");
                prop_assert_eq!(payload.page().kind, BatteryPageKind::CellVoltage);
                prop_assert!(matches!(payload, BatteryPagePayload::CellVoltage(_)));
            } else {
                prop_assert_eq!(
                    decoded,
                    Err(VeteranBmsPageError::InvalidCellCount {
                        selector,
                        observed: count,
                        expected: VETERAN_BMS_CELL_VALUES_PER_PAGE,
                    })
                );
            }
        }

        #[test]
        fn raw_and_metadata_selectors_preserve_evidence_for_any_count(selector in prop_oneof![Just(0u8), Just(3), Just(4), Just(7), Just(8)], count in 0u8..32) {
            let decoded = decode_veteran_bms_page(
                selector,
                count,
                BatteryInfo::default(),
                VerificationStatus::HardwareVerified,
            )
            .expect("non-cell selectors should preserve evidence without cell-count invariants");

            prop_assert!(matches!(decoded, BatteryPagePayload::Raw(_)));
            prop_assert_eq!(decoded.page().selector, selector);
            prop_assert_eq!(decoded.page().kind, classify_veteran_bms_selector(selector));
            prop_assert_eq!(decoded.page().verification, VerificationStatus::HardwareVerified);
        }
    }
}
