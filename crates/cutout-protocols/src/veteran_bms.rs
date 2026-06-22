use arrayvec::ArrayVec;
use cutout_core::{
    BatteryInfo, BatteryPageKind, BatteryPageMetadata, BatteryPagePayload, VerificationStatus,
};
use thiserror::Error;

use crate::VeteranFrame;

/// Cell-voltage count observed for typed Veteran/NOSFET BMS pages.
pub const VETERAN_BMS_CELL_VALUES_PER_PAGE: u8 = 15;

/// Temperature sensor count documented for Veteran/NOSFET BMS temperature pages.
pub const VETERAN_BMS_TEMPERATURE_VALUES_PER_PAGE: usize = 6;

/// Absolute offset of the first cell voltage value in complete Veteran BMS frames.
pub const VETERAN_BMS_CELL_VALUES_OFFSET: usize = 53;

/// Absolute offset of the first temperature value in complete Veteran BMS frames.
pub const VETERAN_BMS_TEMPERATURE_VALUES_OFFSET: usize = 47;

/// Absolute offset of the first pack-current value in complete Veteran BMS frames.
pub const VETERAN_BMS_PACK_CURRENT_VALUES_OFFSET: usize = 69;

/// Classifies a Veteran/NOSFET BMS page selector from hardware-backed Aero captures.
#[must_use]
pub const fn classify_veteran_bms_selector(selector: u8) -> BatteryPageKind {
    match selector {
        0 | 4 => BatteryPageKind::Metadata,
        1 | 2 | 5 | 6 => BatteryPageKind::CellVoltage,
        3 | 7 => BatteryPageKind::Temperature,
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

    /// Absolute frame offset corresponding to the first byte in [`Self::body`].
    pub const BODY_OFFSET: usize = Self::SELECTOR_OFFSET + 1;
}

/// Typed Veteran/NOSFET BMS cell-voltage values parsed from a cell page body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VeteranBmsCellPage {
    /// BMS page selector.
    pub selector: u8,

    /// Cell voltage values in millivolts.
    pub cell_mv: ArrayVec<u16, { VETERAN_BMS_CELL_VALUES_PER_PAGE as usize }>,
}

impl VeteranBmsCellPage {
    /// Decodes the documented 15 cell-voltage values from a BMS cell page body.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranBmsPageError::InvalidCellCount`] when `selector` is not
    /// a cell page or [`VeteranBmsPageError::PageBodyTooShort`] when the body
    /// cannot contain the documented absolute cell offset and 15 values.
    pub fn from_body(selector: u8, body: &[u8]) -> Result<Self, VeteranBmsPageError> {
        if !matches!(
            classify_veteran_bms_selector(selector),
            BatteryPageKind::CellVoltage
        ) {
            return Err(VeteranBmsPageError::InvalidCellCount {
                selector,
                observed: 0,
                expected: VETERAN_BMS_CELL_VALUES_PER_PAGE,
            });
        }

        let offset = body_offset(VETERAN_BMS_CELL_VALUES_OFFSET);
        let end = offset + usize::from(VETERAN_BMS_CELL_VALUES_PER_PAGE) * 2;
        let values = body
            .get(offset..end)
            .ok_or(VeteranBmsPageError::PageBodyTooShort {
                selector,
                expected: end,
                observed: body.len(),
            })?;
        let mut cell_mv = ArrayVec::new();
        for bytes in values.chunks_exact(2) {
            if let Some(value) = read_be_u16_pair(bytes) {
                cell_mv.push(value);
            }
        }

        Ok(Self { selector, cell_mv })
    }
}

/// Typed Veteran/NOSFET BMS temperature values parsed from a temperature page body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VeteranBmsTemperaturePage {
    /// BMS page selector.
    pub selector: u8,

    /// Temperature values in millicelsius.
    pub temperatures_mc: ArrayVec<i32, VETERAN_BMS_TEMPERATURE_VALUES_PER_PAGE>,
}

impl VeteranBmsTemperaturePage {
    /// Decodes the documented six BMS temperatures from selectors 3 and 7.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranBmsPageError::InvalidTemperaturePage`] for selectors
    /// other than 3/7, or [`VeteranBmsPageError::PageBodyTooShort`] when the
    /// body cannot contain the documented values.
    pub fn from_body(selector: u8, body: &[u8]) -> Result<Self, VeteranBmsPageError> {
        if !matches!(selector, 3 | 7) {
            return Err(VeteranBmsPageError::InvalidTemperaturePage { selector });
        }

        let offset = body_offset(VETERAN_BMS_TEMPERATURE_VALUES_OFFSET);
        let end = offset + VETERAN_BMS_TEMPERATURE_VALUES_PER_PAGE * 2;
        let values = body
            .get(offset..end)
            .ok_or(VeteranBmsPageError::PageBodyTooShort {
                selector,
                expected: end,
                observed: body.len(),
            })?;
        let mut temperatures_mc = ArrayVec::new();
        for bytes in values.chunks_exact(2) {
            if let Some(value) = read_be_i16_pair(bytes) {
                temperatures_mc.push(i32::from(value) * 10);
            }
        }

        Ok(Self {
            selector,
            temperatures_mc,
        })
    }
}

/// Veteran/NOSFET BMS metadata values parsed from a metadata page body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranBmsMetadataPage {
    /// BMS page selector.
    pub selector: u8,

    /// First documented pack current value in milliamps.
    pub current_0_ma: i32,

    /// Second documented pack current value in milliamps.
    pub current_1_ma: i32,
}

impl VeteranBmsMetadataPage {
    /// Decodes the documented page 0/4 pack-current fields.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranBmsPageError::InvalidMetadataPage`] for selectors other
    /// than 0/4, or [`VeteranBmsPageError::PageBodyTooShort`] when the body
    /// cannot contain both current values.
    pub fn from_body(selector: u8, body: &[u8]) -> Result<Self, VeteranBmsPageError> {
        if !matches!(selector, 0 | 4) {
            return Err(VeteranBmsPageError::InvalidMetadataPage { selector });
        }

        let offset = body_offset(VETERAN_BMS_PACK_CURRENT_VALUES_OFFSET);
        let end = offset + 4;
        let values = body
            .get(offset..end)
            .ok_or(VeteranBmsPageError::PageBodyTooShort {
                selector,
                expected: end,
                observed: body.len(),
            })?;

        Ok(Self {
            selector,
            current_0_ma: read_be_i16_at(values, 0).map_or(0, |value| i32::from(value) * 10),
            current_1_ma: read_be_i16_at(values, 2).map_or(0, |value| i32::from(value) * 10),
        })
    }
}

const fn body_offset(absolute_offset: usize) -> usize {
    absolute_offset - VeteranBmsPageEvidence::BODY_OFFSET
}

fn read_be_i16_at(bytes: &[u8], offset: usize) -> Option<i16> {
    read_be_i16_pair(bytes.get(offset..offset + 2)?)
}

fn read_be_i16_pair(bytes: &[u8]) -> Option<i16> {
    read_be_u16_pair(bytes).map(|value| i16::from_be_bytes(value.to_be_bytes()))
}

fn read_be_u16_pair(bytes: &[u8]) -> Option<u16> {
    let high = *bytes.first()?;
    let low = *bytes.get(1)?;
    Some(u16::from_be_bytes([high, low]))
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

    /// The BMS page body was shorter than the documented value offsets.
    #[error("selector {selector} body had {observed} bytes, expected at least {expected}")]
    PageBodyTooShort {
        /// BMS page selector.
        selector: u8,

        /// Required body length.
        expected: usize,

        /// Observed body length.
        observed: usize,
    },

    /// A non-temperature selector was decoded as a temperature page.
    #[error("selector {selector} is not a BMS temperature page")]
    InvalidTemperaturePage {
        /// BMS page selector.
        selector: u8,
    },

    /// A non-metadata selector was decoded as a metadata page.
    #[error("selector {selector} is not a BMS metadata page")]
    InvalidMetadataPage {
        /// BMS page selector.
        selector: u8,
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
    use crate::VeteranFrameReassembler;
    use proptest::prelude::*;

    const PAGE_8_BODY: [u8; 24] =
        hex_literal::hex!("0000803200364f371e00000100808028062e796480008080");

    fn fixture_bytes(input: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        for token in input
            .lines()
            .filter_map(hex_line)
            .flat_map(str::split_whitespace)
        {
            for chunk in token.as_bytes().chunks_exact(2) {
                let high = hex_nibble(chunk[0]).expect("fixture hex high nibble is valid");
                let low = hex_nibble(chunk[1]).expect("fixture hex low nibble is valid");
                bytes.push((high << 4) | low);
            }
        }
        bytes
    }

    fn hex_line(line: &str) -> Option<&str> {
        line.split_once('#')
            .map_or(Some(line), |(hex, _)| Some(hex))
    }

    const fn hex_nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    fn reassemble_fixture(input: &str) -> Vec<VeteranFrame> {
        let mut reassembler = VeteranFrameReassembler::default();
        fixture_bytes(input)
            .into_iter()
            .filter_map(|byte| reassembler.feed_byte(byte).expect("fixture CRC is valid"))
            .collect()
    }

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
        assert_eq!(classify_veteran_bms_selector(8), BatteryPageKind::Raw);
        assert_eq!(classify_veteran_bms_selector(9), BatteryPageKind::Raw);
    }

    #[test]
    fn hardware_verified_temperature_selectors_are_typed() {
        assert_eq!(
            classify_veteran_bms_selector(3),
            BatteryPageKind::Temperature
        );
        assert_eq!(
            classify_veteran_bms_selector(7),
            BatteryPageKind::Temperature
        );
    }

    #[test]
    fn page_evidence_extracts_selector_and_crc_free_body() {
        let frame = live_aero_selector_3_frame();
        let evidence = VeteranBmsPageEvidence::from_frame(&frame)
            .expect("selector 3 fixture has BMS evidence");

        assert_eq!(evidence.selector, 3);
        assert_eq!(evidence.kind, BatteryPageKind::Temperature);
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
        assert_eq!(evidence.body, PAGE_8_BODY);
    }

    #[test]
    fn moved_no_balancing_capture_keeps_page_8_body_stable() {
        let frames = reassemble_fixture(include_str!(
            "../fixtures/nosfet-aero/nf2557-2026-06-22-moved-no-balancing-page8.hex"
        ));
        let page_8: Vec<_> = frames
            .iter()
            .map(VeteranBmsPageEvidence::from_frame)
            .map(|evidence| evidence.expect("fixture contains only BMS page frames"))
            .collect();

        assert_eq!(page_8.len(), 14);
        assert!(page_8.iter().all(|evidence| evidence.selector == 8));
        assert!(
            page_8
                .iter()
                .all(|evidence| evidence.kind == BatteryPageKind::Raw)
        );
        assert!(page_8.iter().all(|evidence| evidence.body == PAGE_8_BODY));
    }

    #[test]
    fn page_evidence_rejects_short_non_bms_frames() {
        let frame = VeteranFrame::try_from_slice(&hex_literal::hex!("dc5a5c020102"))
            .expect("short fixture frame is structurally valid");

        assert_eq!(VeteranBmsPageEvidence::from_frame(&frame), None);
    }

    #[test]
    fn documented_cell_pages_start_at_absolute_offset_53() {
        let mut body = [0_u8; 36];
        for (index, slot) in body[body_offset(VETERAN_BMS_CELL_VALUES_OFFSET)..]
            .chunks_exact_mut(2)
            .take(usize::from(VETERAN_BMS_CELL_VALUES_PER_PAGE))
            .enumerate()
        {
            slot.copy_from_slice(&(3700 + u16::try_from(index).unwrap()).to_be_bytes());
        }

        let page = VeteranBmsCellPage::from_body(1, &body).expect("documented body decodes");

        assert_eq!(page.selector, 1);
        assert_eq!(page.cell_mv.len(), 15);
        assert_eq!(page.cell_mv[0], 3700);
        assert_eq!(page.cell_mv[14], 3714);
    }

    #[test]
    fn documented_cell_page_decoder_rejects_fourteen_and_sixteen_value_bodies() {
        let fourteen_value_body = [0_u8; body_offset(VETERAN_BMS_CELL_VALUES_OFFSET) + 14 * 2];
        let sixteen_value_body = [0_u8; body_offset(VETERAN_BMS_CELL_VALUES_OFFSET) + 16 * 2];

        assert_eq!(
            VeteranBmsCellPage::from_body(2, &fourteen_value_body),
            Err(VeteranBmsPageError::PageBodyTooShort {
                selector: 2,
                expected: body_offset(VETERAN_BMS_CELL_VALUES_OFFSET)
                    + usize::from(VETERAN_BMS_CELL_VALUES_PER_PAGE) * 2,
                observed: fourteen_value_body.len(),
            })
        );
        assert_eq!(
            VeteranBmsCellPage::from_body(2, &sixteen_value_body)
                .expect("extra trailing data after 15 documented values is ignored")
                .cell_mv
                .len(),
            15
        );
    }

    #[test]
    fn documented_temperature_pages_start_at_absolute_offset_47() {
        let body = hex_literal::hex!("0689065706a20686067c06f7");

        let page = VeteranBmsTemperaturePage::from_body(3, &body)
            .expect("documented temperature body decodes");

        assert_eq!(page.selector, 3);
        assert_eq!(
            page.temperatures_mc.as_slice(),
            &[16730, 16230, 16980, 16700, 16600, 17830]
        );
    }

    #[test]
    fn documented_metadata_pages_decode_pack_currents_at_absolute_offsets_69_and_71() {
        let mut body = [0_u8; body_offset(VETERAN_BMS_PACK_CURRENT_VALUES_OFFSET) + 4];
        let offset = body_offset(VETERAN_BMS_PACK_CURRENT_VALUES_OFFSET);
        body[offset..offset + 2].copy_from_slice(&(-123_i16).to_be_bytes());
        body[offset + 2..offset + 4].copy_from_slice(&(45_i16).to_be_bytes());

        let page = VeteranBmsMetadataPage::from_body(0, &body)
            .expect("documented metadata current body decodes");

        assert_eq!(page.selector, 0);
        assert_eq!(page.current_0_ma, -1230);
        assert_eq!(page.current_1_ma, 450);
    }

    #[test]
    fn documented_aero_page_8_stays_reserved_without_typed_decoder() {
        assert_eq!(classify_veteran_bms_selector(8), BatteryPageKind::Raw);
        assert_eq!(
            VeteranBmsCellPage::from_body(8, &PAGE_8_BODY),
            Err(VeteranBmsPageError::InvalidCellCount {
                selector: 8,
                observed: 0,
                expected: VETERAN_BMS_CELL_VALUES_PER_PAGE,
            })
        );
        assert_eq!(
            VeteranBmsTemperaturePage::from_body(8, &PAGE_8_BODY),
            Err(VeteranBmsPageError::InvalidTemperaturePage { selector: 8 })
        );
        assert_eq!(
            VeteranBmsMetadataPage::from_body(8, &PAGE_8_BODY),
            Err(VeteranBmsPageError::InvalidMetadataPage { selector: 8 })
        );
    }

    #[test]
    fn typed_cell_pages_require_fifteen_cell_values() {
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
    fn raw_metadata_and_temperature_pages_do_not_claim_cell_voltage_typing() {
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
        let temperature = decode_veteran_bms_page(
            3,
            0,
            BatteryInfo::default(),
            VerificationStatus::HardwareVerified,
        )
        .expect("temperature pages should preserve evidence without cell typing");

        assert!(matches!(raw, BatteryPagePayload::Raw(_)));
        assert!(matches!(metadata, BatteryPagePayload::Raw(_)));
        assert!(matches!(temperature, BatteryPagePayload::Temperature(_)));
        assert_eq!(raw.page().kind, BatteryPageKind::Raw);
        assert_eq!(metadata.page().kind, BatteryPageKind::Metadata);
        assert_eq!(temperature.page().kind, BatteryPageKind::Temperature);
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
        fn raw_metadata_and_temperature_selectors_preserve_evidence_for_any_count(selector in prop_oneof![Just(0u8), Just(3), Just(4), Just(7), Just(8)], count in 0u8..32) {
            let decoded = decode_veteran_bms_page(
                selector,
                count,
                BatteryInfo::default(),
                VerificationStatus::HardwareVerified,
            )
            .expect("non-cell selectors should preserve evidence without cell-count invariants");

            prop_assert_eq!(decoded.page().selector, selector);
            prop_assert_eq!(decoded.page().kind, classify_veteran_bms_selector(selector));
            prop_assert_eq!(decoded.page().verification, VerificationStatus::HardwareVerified);
            if matches!(selector, 3 | 7) {
                prop_assert!(matches!(decoded, BatteryPagePayload::Temperature(_)));
            } else {
                prop_assert!(matches!(decoded, BatteryPagePayload::Raw(_)));
            }
        }
    }
}
