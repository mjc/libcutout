use cutout_core::{
    BatteryInfo, BatteryPageKind, BatteryPageMetadata, BatteryPagePayload, VerificationStatus,
};
use thiserror::Error;

use crate::VeteranFrame;

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

/// Borrowed BMS page evidence extracted from a complete Veteran/NOSFET frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranBmsPageEvidence<'frame> {
    /// BMS page selector observed in the frame.
    pub selector: u8,

    /// Current conservative page classification.
    pub kind: BatteryPageKind,

    /// Raw page body after the selector and before the frame CRC trailer.
    pub body: &'frame [u8],
}

impl<'frame> VeteranBmsPageEvidence<'frame> {
    /// Offset of the BMS page selector in complete Veteran smart-BMS frames.
    pub const SELECTOR_OFFSET: usize = 46;

    /// Number of CRC trailer bytes excluded from page evidence bodies.
    pub const CRC_TRAILER_LEN: usize = 4;

    /// Extracts conservative BMS page evidence from a complete frame.
    #[must_use]
    pub fn from_frame(frame: &'frame VeteranFrame) -> Option<Self> {
        let bytes = frame.as_slice();
        let selector = *bytes.get(Self::SELECTOR_OFFSET)?;
        let body_start = Self::SELECTOR_OFFSET + 1;
        let body_end = bytes.len().checked_sub(Self::CRC_TRAILER_LEN)?;
        let body = bytes.get(body_start..body_end)?;

        Some(Self {
            selector,
            kind: classify_veteran_bms_selector(selector),
            body,
        })
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

    fn live_aero_selector_3_frame() -> VeteranFrame {
        VeteranFrame::try_from_slice(&hex_literal::hex!(
            "dc5a5c5f2a09000000170000ab6c001700000bea\
             045c00000226021ca8f607801b1f000080c80000\
             808080808080030689065706a20686067c06f700\
             00000000000000000000000e0e0e0200000000a5\
             11000053f401c50000000000bffffaf33f9782"
        ))
        .expect("fixture frame is valid")
    }

    fn live_aero_selector_8_frame() -> VeteranFrame {
        VeteranFrame::try_from_slice(&hex_literal::hex!(
            "dc5a5c4729f2000000170000ab6c001700000be9\
             045a00000226021ca8f607801b25000080c80000\
             808080808080080000803200364f371e00000100\
             808028062e7964800080801540e23a"
        ))
        .expect("fixture frame is valid")
    }

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
    fn page_evidence_extracts_selector_and_crc_free_body() {
        let frame = live_aero_selector_3_frame();
        let evidence = VeteranBmsPageEvidence::from_frame(&frame)
            .expect("selector 3 fixture has BMS evidence");

        assert_eq!(evidence.selector, 3);
        assert_eq!(evidence.kind, BatteryPageKind::Raw);
        assert_eq!(evidence.body.len(), 48);
        assert_eq!(
            &evidence.body[..12],
            &hex_literal::hex!("0689065706a20686067c06f7")
        );
    }

    #[test]
    fn page_evidence_keeps_reserved_page_8_raw() {
        let frame = live_aero_selector_8_frame();
        let evidence = VeteranBmsPageEvidence::from_frame(&frame)
            .expect("selector 8 fixture has BMS evidence");

        assert_eq!(evidence.selector, 8);
        assert_eq!(evidence.kind, BatteryPageKind::Raw);
        assert_eq!(evidence.body.len(), 24);
        assert_eq!(&evidence.body[..8], &hex_literal::hex!("0000803200364f37"));
    }

    #[test]
    fn page_evidence_rejects_short_non_bms_frames() {
        let frame = VeteranFrame::try_from_slice(&hex_literal::hex!("dc5a5c020102"))
            .expect("short fixture frame is structurally valid");

        assert_eq!(VeteranBmsPageEvidence::from_frame(&frame), None);
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
