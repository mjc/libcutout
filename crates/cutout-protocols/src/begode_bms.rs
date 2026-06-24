use arrayvec::ArrayVec;
use cutout_core::{
    BatteryCurrent, BatteryInfo, BatteryPageMetadata, BatteryPagePayload, Measured,
    MonotonicMillis, ProtocolSelector, ProtocolTag, ReadOnlyResponse, TelemetryDelta, Temperature,
    ValueQuality, ValueSource, VerificationStatus, Voltage,
};
use thiserror::Error;

use crate::{
    BegodeFrame,
    parser::{ByteCursor, ByteOffset},
};

/// Cell-voltage count carried by one Begode/Gotway BMS cell page.
pub const BEGODE_BMS_CELL_VALUES_PER_PAGE: usize = 8;

const BEGODE_BMS_CELL_VALUES_PER_PAGE_U16: u16 = 8;

/// Begode/Gotway BMS summary decoded from frame tag `0x01`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BegodeBmsSummary {
    /// Raw BMS sub-index from frame offset 19.
    pub sub_index: ProtocolSelector,

    /// Zero-based BMS pack index inferred from sub-index.
    pub bms_index: u8,

    /// Zero-based half-pack index inferred from sub-index bit 0.
    pub half_index: u8,

    /// PWM limit in centi-percent.
    pub pwm_limit_centi_percent: u16,

    /// BMS-authoritative pack voltage in millivolts.
    pub pack_voltage_mv: i32,

    /// BMS-reported pack current in milliamps.
    pub current_ma: i32,

    /// First temperature for the selected half in millicelsius.
    pub temperature_0_mc: i32,

    /// Second temperature for the selected half in millicelsius.
    pub temperature_1_mc: i32,

    /// Half-pack voltage in millivolts.
    pub half_pack_voltage_mv: i32,
}

impl BegodeBmsSummary {
    /// Decodes a source-backed Begode BMS summary frame.
    ///
    /// # Errors
    ///
    /// Returns [`BegodeBmsPageError::UnexpectedFrameTag`] when the frame tag is
    /// not `0x01`.
    pub fn decode(frame: &BegodeFrame) -> Result<Self, BegodeBmsPageError> {
        require_tag(frame, 0x01)?;
        let cursor = ByteCursor::new(frame.as_slice());
        let sub_index = frame.sub_index();
        let sub_index_value = sub_index.get();
        Ok(Self {
            sub_index,
            bms_index: u8::from(sub_index_value >= 2),
            half_index: sub_index_value & 1,
            pwm_limit_centi_percent: be_u16(cursor, ByteOffset::new(2)),
            pack_voltage_mv: i32::from(be_u16(cursor, ByteOffset::new(6))) * 100,
            current_ma: i32::from(be_i16(cursor, ByteOffset::new(8))) * 100,
            temperature_0_mc: i32::from(be_i16(cursor, ByteOffset::new(10))) * 1_000,
            temperature_1_mc: i32::from(be_i16(cursor, ByteOffset::new(12))) * 1_000,
            half_pack_voltage_mv: i32::from(be_i16(cursor, ByteOffset::new(14))) * 100,
        })
    }

    /// Converts the authoritative BMS voltage/current into a telemetry delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicMillis) -> TelemetryDelta {
        TelemetryDelta {
            voltage_mv: Some(source_reported(Voltage::from_millivolts(
                self.pack_voltage_mv,
            ))),
            battery_current_ma: Some(source_reported(BatteryCurrent::from_milliamps(
                self.current_ma,
            ))),
            battery_temperature_mc: Some(source_reported(Temperature::from_millicelsius(
                self.temperature_0_mc,
            ))),
            ..TelemetryDelta::empty(at_ms)
        }
    }

    /// Converts the BMS summary into a generic battery response.
    #[must_use]
    pub fn to_battery_response(self) -> ReadOnlyResponse {
        ReadOnlyResponse::Battery(BatteryPagePayload::raw(
            BatteryPageMetadata::metadata(self.sub_index, VerificationStatus::SourceVerified),
            BatteryInfo {
                voltage_mv: Some(source_reported(Voltage::from_millivolts(
                    self.pack_voltage_mv,
                ))),
                current_ma: Some(source_reported(BatteryCurrent::from_milliamps(
                    self.current_ma,
                ))),
                temperature_mc: Some(source_reported(Temperature::from_millicelsius(
                    self.temperature_0_mc,
                ))),
                ..BatteryInfo::default()
            },
        ))
    }
}

/// Begode/Gotway BMS cell voltages decoded from frame tags `0x02` or `0x03`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BegodeBmsCellPage {
    /// Frame tag, either `0x02` or `0x03`.
    pub tag: ProtocolTag,

    /// Zero-based BMS pack index from the frame tag.
    pub bms_index: u8,

    /// Page index from frame offset 19.
    pub page_index: ProtocolSelector,

    /// First cell index represented by this page.
    pub first_cell_index: u16,

    /// Eight cell voltages in millivolts.
    pub cell_mv: ArrayVec<u16, BEGODE_BMS_CELL_VALUES_PER_PAGE>,
}

impl BegodeBmsCellPage {
    /// Decodes a source-backed Begode BMS cell page frame.
    ///
    /// # Errors
    ///
    /// Returns [`BegodeBmsPageError::UnexpectedFrameTag`] when the frame tag is
    /// not `0x02` or `0x03`.
    pub fn decode(frame: &BegodeFrame) -> Result<Self, BegodeBmsPageError> {
        let tag = frame.tag();
        if !matches!(tag.get(), 0x02 | 0x03) {
            return Err(BegodeBmsPageError::UnexpectedFrameTag {
                expected: 0x02,
                actual: tag_byte(tag),
            });
        }

        let cursor = ByteCursor::new(frame.as_slice());
        let page_index = frame.sub_index();
        let mut cell_mv = ArrayVec::new();
        for offset in (2..18).step_by(2).map(ByteOffset::new) {
            cell_mv.push(be_u16(cursor, offset));
        }

        Ok(Self {
            tag,
            bms_index: u8::try_from(tag.get().saturating_sub(0x02)).unwrap_or_default(),
            page_index,
            first_cell_index: u16::from(page_index.get()) * BEGODE_BMS_CELL_VALUES_PER_PAGE_U16,
            cell_mv,
        })
    }

    /// Converts the cell page into a generic battery page response.
    #[must_use]
    pub fn to_battery_response(&self) -> ReadOnlyResponse {
        ReadOnlyResponse::Battery(BatteryPagePayload::cell_voltage(
            BatteryPageMetadata::cell_voltage(self.page_index, VerificationStatus::SourceVerified),
            BatteryInfo::default(),
        ))
    }
}

/// Begode BMS decode failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BegodeBmsPageError {
    /// Frame tag did not match the typed decoder.
    #[error("unexpected Begode BMS frame tag: expected {expected:#04x}, got {actual:#04x}")]
    UnexpectedFrameTag {
        /// Expected tag for the typed decoder.
        expected: u8,

        /// Actual frame tag.
        actual: u8,
    },
}

fn require_tag(frame: &BegodeFrame, expected: u8) -> Result<(), BegodeBmsPageError> {
    let actual = frame.tag();
    if actual.get() == u16::from(expected) {
        Ok(())
    } else {
        Err(BegodeBmsPageError::UnexpectedFrameTag {
            expected,
            actual: tag_byte(actual),
        })
    }
}

fn tag_byte(tag: ProtocolTag) -> u8 {
    u8::try_from(tag.get()).unwrap_or_default()
}

fn be_u16(cursor: ByteCursor<'_>, offset: ByteOffset) -> u16 {
    cursor.be_u16(offset).unwrap_or_default()
}

fn be_i16(cursor: ByteCursor<'_>, offset: ByteOffset) -> i16 {
    cursor.be_i16(offset).unwrap_or_default()
}

const fn source_reported<T>(value: T) -> Measured<T> {
    Measured {
        value,
        source: ValueSource::Reported,
        quality: ValueQuality::Known,
        verification: VerificationStatus::SourceVerified,
    }
}

#[cfg(test)]
mod tests {
    use cutout_core::{
        BatteryPageKind, Measured, ReadOnlyResponse, TelemetryDelta, ValueQuality, ValueSource,
        VerificationStatus,
    };

    use super::*;
    use crate::{BegodeFrame, BegodeLiveATelemetry, BegodePackVoltageProfile};

    const SUMMARY: [u8; 24] = hex_literal::hex!("55aa271000000320ff9c0019001a0190000001035a5a5a5a");
    const CELL_PAGE: [u8; 24] =
        hex_literal::hex!("55aa0fa00fa10fa20fa30fa40fa50fa60fa702025a5a5a5a");
    const LIVE_A: [u8; 24] = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");

    #[test]
    fn bms_summary_decodes_source_backed_fields() {
        let frame = BegodeFrame::try_from_slice(&SUMMARY).expect("summary frame is valid");
        let summary = BegodeBmsSummary::decode(&frame).expect("summary decodes");

        assert_eq!(
            summary,
            BegodeBmsSummary {
                sub_index: ProtocolSelector::new(3),
                bms_index: 1,
                half_index: 1,
                pwm_limit_centi_percent: 10_000,
                pack_voltage_mv: 80_000,
                current_ma: -10_000,
                temperature_0_mc: 25_000,
                temperature_1_mc: 26_000,
                half_pack_voltage_mv: 40_000,
            }
        );
    }

    #[test]
    fn bms_summary_maps_authoritative_voltage_to_delta() {
        let frame = BegodeFrame::try_from_slice(&SUMMARY).expect("summary frame is valid");
        let summary = BegodeBmsSummary::decode(&frame).expect("summary decodes");

        assert_eq!(
            summary.to_delta(77),
            TelemetryDelta {
                voltage_mv: Some(source_reported(Voltage::from_millivolts(80_000))),
                battery_current_ma: Some(source_reported(BatteryCurrent::from_milliamps(-10_000))),
                battery_temperature_mc: Some(source_reported(Temperature::from_millicelsius(
                    25_000,
                ))),
                ..TelemetryDelta::empty(77)
            }
        );
    }

    #[test]
    fn bms_voltage_remains_distinct_from_live_a_scaled_voltage() {
        let live_frame = BegodeFrame::try_from_slice(&LIVE_A).expect("live frame is valid");
        let bms_frame = BegodeFrame::try_from_slice(&SUMMARY).expect("summary frame is valid");

        let live_delta = BegodeLiveATelemetry::decode(
            &live_frame,
            BegodePackVoltageProfile::Begode84VFullCharge,
        )
        .expect("live A decodes")
        .to_delta(1);
        let bms_delta = BegodeBmsSummary::decode(&bms_frame)
            .expect("summary decodes")
            .to_delta(2);

        assert_eq!(
            live_delta.voltage_mv,
            Some(source_reported(Voltage::from_millivolts(75_063)))
        );
        assert_eq!(
            bms_delta.voltage_mv,
            Some(source_reported(Voltage::from_millivolts(80_000)))
        );
    }

    #[test]
    fn bms_summary_maps_to_battery_response() {
        let frame = BegodeFrame::try_from_slice(&SUMMARY).expect("summary frame is valid");
        let summary = BegodeBmsSummary::decode(&frame).expect("summary decodes");

        let ReadOnlyResponse::Battery(payload) = summary.to_battery_response() else {
            panic!("expected battery response");
        };

        assert_eq!(payload.page().selector, ProtocolSelector::new(3));
        assert_eq!(payload.page().kind, BatteryPageKind::Metadata);
        assert_eq!(
            payload.page().verification,
            VerificationStatus::SourceVerified
        );
        assert_eq!(
            payload.battery().voltage_mv,
            Some(source_reported(Voltage::from_millivolts(80_000)))
        );
        assert_eq!(
            payload.battery().current_ma,
            Some(source_reported(BatteryCurrent::from_milliamps(-10_000)))
        );
    }

    #[test]
    fn bms_cell_page_decodes_eight_cell_voltages() {
        let frame = BegodeFrame::try_from_slice(&CELL_PAGE).expect("cell frame is valid");
        let page = BegodeBmsCellPage::decode(&frame).expect("cell page decodes");

        assert_eq!(page.tag, ProtocolTag::new(0x02));
        assert_eq!(page.bms_index, 0);
        assert_eq!(page.page_index, ProtocolSelector::new(2));
        assert_eq!(page.first_cell_index, 16);
        assert_eq!(
            page.cell_mv.as_slice(),
            &[4_000, 4_001, 4_002, 4_003, 4_004, 4_005, 4_006, 4_007]
        );
    }

    #[test]
    fn bms_cell_page_maps_to_cell_voltage_response() {
        let frame = BegodeFrame::try_from_slice(&CELL_PAGE).expect("cell frame is valid");
        let page = BegodeBmsCellPage::decode(&frame).expect("cell page decodes");

        let ReadOnlyResponse::Battery(payload) = page.to_battery_response() else {
            panic!("expected battery response");
        };

        assert_eq!(payload.page().selector, ProtocolSelector::new(2));
        assert_eq!(payload.page().kind, BatteryPageKind::CellVoltage);
        assert_eq!(
            payload.page().verification,
            VerificationStatus::SourceVerified
        );
    }

    #[test]
    fn bms_decoders_reject_wrong_tags() {
        let live_frame = BegodeFrame::try_from_slice(&LIVE_A).expect("live frame is valid");

        assert_eq!(
            BegodeBmsSummary::decode(&live_frame),
            Err(BegodeBmsPageError::UnexpectedFrameTag {
                expected: 0x01,
                actual: 0x00,
            })
        );
        assert_eq!(
            BegodeBmsCellPage::decode(&live_frame),
            Err(BegodeBmsPageError::UnexpectedFrameTag {
                expected: 0x02,
                actual: 0x00,
            })
        );
    }

    const fn source_reported<T>(value: T) -> Measured<T> {
        Measured {
            value,
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::SourceVerified,
        }
    }
}
