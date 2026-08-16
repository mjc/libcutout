use arrayvec::{ArrayString, ArrayVec};
use cutout_core::{
    Angle, BatteryCurrent, DutyCycle, FootpadContactState, FootpadTelemetry, Measured,
    MonotonicTimestamp, PhaseCurrent, RideOperatingMode, RideOperatingState, RideStopReason,
    RideWarning, Speed, TelemetryDelta, Temperature, Voltage,
};
use thiserror::Error;

use crate::VESC_MAX_FRAME_LEN;

/// VESC command id for custom package app data.
pub const VESC_COMM_CUSTOM_APP_DATA: u8 = 36;

/// Refloat package interface id used inside VESC custom app data.
pub const REFLOAT_PACKAGE_INTERFACE_ID: u8 = 101;

/// Refloat INFO command id.
pub const REFLOAT_COMMAND_INFO: u8 = 0;

/// Refloat realtime data command id.
pub const REFLOAT_COMMAND_REALTIME_DATA: u8 = 31;

/// Refloat realtime data id discovery command id.
pub const REFLOAT_COMMAND_REALTIME_DATA_IDS: u8 = 32;

/// Maximum Refloat field ids kept by the read-only adapter.
pub const REFLOAT_MAX_REALTIME_FIELDS: usize = 32;

/// Maximum bytes retained for a Refloat realtime field id.
pub const REFLOAT_MAX_FIELD_ID_LEN: usize = 48;

const FRAME_END: u8 = 3;
const FRAME_START_SHORT: u8 = 2;
const FRAME_START_LONG: u8 = 3;
const REFLOAT_MAX_FRAME_LEN: usize = 512;
const INFO_STRING_LEN: usize = 20;
const INFO_V2_BODY_LEN: usize = 58;
const REFLOAT_BEEP_DUTY: u8 = 6;

/// Refloat read-only package request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefloatReadOnlyRequest {
    /// Request package information with the current full response shape.
    Info,

    /// Request dynamic realtime field ids.
    RealtimeDataIds,

    /// Request one realtime data sample.
    RealtimeData,
}

/// Result of feeding bytes into a Refloat stream decoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefloatStreamResult {
    /// The decoder accepted bytes but is still waiting for a complete reply.
    Buffered,

    /// The decoder completed this many read-only replies.
    Replies(usize),
}

/// Borrowed Refloat package reply delivered while decoding a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RefloatReply<'a> {
    /// Package information.
    Info(&'a RefloatInfo),

    /// Dynamic realtime field ids.
    RealtimeFieldIds(&'a RefloatRealtimeFieldIds),

    /// Dynamic realtime data.
    RealtimeData(&'a RefloatRealtimeData),
}

/// Refloat INFO v2 package information.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefloatInfo {
    /// Actual INFO response version.
    pub info_version: u8,

    /// Echoed INFO flags.
    pub flags: u8,

    /// Package name.
    pub package_name: ArrayString<INFO_STRING_LEN>,

    /// Package major version.
    pub package_major: u8,

    /// Package minor version.
    pub package_minor: u8,

    /// Package patch version.
    pub package_patch: u8,

    /// Package version suffix.
    pub package_version_suffix: ArrayString<INFO_STRING_LEN>,

    /// First four bytes of the source git hash.
    pub git_hash: u32,

    /// System tick rate in Hz.
    pub tick_rate_hz: u32,

    /// Package capability bitmask.
    pub capabilities: u32,

    /// Extra INFO flags.
    pub extra_flags: u8,
}

/// Dynamic Refloat realtime field ids.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefloatRealtimeFieldIds {
    /// Fields always present in realtime data.
    pub always: ArrayVec<ArrayString<REFLOAT_MAX_FIELD_ID_LEN>, REFLOAT_MAX_REALTIME_FIELDS>,

    /// Fields present only when the board is running.
    pub runtime: ArrayVec<ArrayString<REFLOAT_MAX_FIELD_ID_LEN>, REFLOAT_MAX_REALTIME_FIELDS>,
}

/// One named Refloat realtime value.
#[derive(Clone, Debug, PartialEq)]
pub struct RefloatRealtimeValue {
    /// Dynamic field id.
    pub id: ArrayString<REFLOAT_MAX_FIELD_ID_LEN>,

    /// Decoded float16 value.
    pub value: f32,
}

/// Refloat realtime data sample.
#[derive(Clone, Debug, PartialEq)]
pub struct RefloatRealtimeData {
    /// Refloat payload mask.
    pub mask: u8,

    /// Extra state flags.
    pub extra_flags: u8,

    /// Refloat system time in ticks.
    pub time_ticks: u32,

    /// Package state nibble.
    pub package_state: u8,

    /// Package mode nibble.
    pub package_mode: u8,

    /// Footpad state.
    pub footpad_state: u8,

    /// Charging flag.
    pub charging: bool,

    /// Active Refloat fatal error.
    pub fatal_error: Option<RefloatFatalError>,

    /// Darkride flag.
    pub darkride: bool,

    /// Wheelslip flag.
    pub wheelslip: bool,

    /// Stop condition nibble.
    pub stop_condition: u8,

    /// Setpoint adjustment type nibble.
    pub sat: u8,

    /// Active Refloat beep reason.
    pub beep_reason: u8,

    /// Always-present dynamic values.
    pub values: ArrayVec<RefloatRealtimeValue, REFLOAT_MAX_REALTIME_FIELDS>,

    /// Runtime dynamic values, present when mask bit 0 is set.
    pub runtime_values: ArrayVec<RefloatRealtimeValue, REFLOAT_MAX_REALTIME_FIELDS>,

    /// Charging current, present when mask bit 1 is set.
    pub charging_current: Option<f32>,

    /// Charging voltage, present when mask bit 1 is set.
    pub charging_voltage: Option<f32>,

    /// Low 32 bits of the active alert mask, present when mask bit 2 is set.
    pub active_alert_mask_low: Option<u32>,

    /// High 32 bits of the active alert mask, present when mask bit 2 is set.
    pub active_alert_mask_high: Option<u32>,

    /// VESC firmware fault code, present when mask bit 2 is set.
    pub firmware_fault_code: Option<u8>,
}

/// Fatal condition reported by Refloat realtime state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefloatFatalError {
    /// Refloat currently uses the fatal bit for firmware faults.
    FirmwareFault,
}

impl RefloatRealtimeData {
    /// Converts decoded Refloat realtime values to shared telemetry.
    #[must_use]
    pub fn to_delta(
        &self,
        at_ms: MonotonicTimestamp,
        reports_battery_current: bool,
    ) -> TelemetryDelta {
        let stop_reason = refloat_stop_reason(self.stop_condition);
        TelemetryDelta {
            speed: self.value("motor.speed").map(|metres_per_second| {
                Measured::reported(Speed::from_metres_per_second(metres_per_second))
            }),
            battery_current: reports_battery_current
                .then(|| self.value("motor.batt_current"))
                .flatten()
                .map(|amps| Measured::reported(BatteryCurrent::from_milliamps(milliscale(amps)))),
            voltage: self
                .value("motor.batt_voltage")
                .map(|volts| Measured::reported(Voltage::from_millivolts(milliscale(volts)))),
            motor_current: self
                .value("motor.current")
                .map(|amps| Measured::reported(PhaseCurrent::from_milliamps(milliscale(amps)))),
            controller_temperature: self.value("motor.mosfet_temp").map(|celsius| {
                Measured::reported(Temperature::from_millicelsius(milliscale(celsius)))
            }),
            motor_temperature: self.value("motor.motor_temp").map(|celsius| {
                Measured::reported(Temperature::from_millicelsius(milliscale(celsius)))
            }),
            pwm: self
                .value("motor.duty_cycle")
                .map(|duty| Measured::reported(DutyCycle::from_permille(permille(duty)))),
            pitch: self
                .value("imu.pitch")
                .map(|degrees| Measured::reported(Angle::from_millidegrees(milliscale(degrees)))),
            balance_angle: self
                .value("imu.balance_pitch")
                .map(|degrees| Measured::reported(Angle::from_millidegrees(milliscale(degrees)))),
            roll: self
                .value("imu.roll")
                .map(|degrees| Measured::reported(Angle::from_millidegrees(milliscale(degrees)))),
            operating_state: Some(refloat_operating_state(self.package_state, self.charging)),
            operating_mode: Some(refloat_operating_mode(self.package_mode, self.darkride)),
            ride_warning: Some(if self.fatal_error.is_some() {
                RideWarning::Error
            } else if self.wheelslip {
                RideWarning::Wheelslip
            } else if stop_reason != RideStopReason::None {
                RideWarning::None
            } else {
                refloat_ride_warning(self.sat, self.beep_reason)
            }),
            ride_stop_reason: Some(stop_reason),
            footpad: Some(FootpadTelemetry {
                state: self.footpad_state,
                contact_state: refloat_footpad_contact_state(self.footpad_state),
                adc1_milliunits: self.value("footpad.adc1").map(milliscale),
                adc2_milliunits: self.value("footpad.adc2").map(milliscale),
            }),
            ..TelemetryDelta::empty(at_ms)
        }
    }

    fn value(&self, id: &str) -> Option<f32> {
        self.values
            .iter()
            .chain(self.runtime_values.iter())
            .find(|value| value.id.as_str() == id)
            .map(|value| value.value)
    }
}

const fn refloat_footpad_contact_state(state: u8) -> Option<FootpadContactState> {
    match state {
        0 => Some(FootpadContactState::None),
        1 => Some(FootpadContactState::Left),
        2 => Some(FootpadContactState::Right),
        3 => Some(FootpadContactState::Both),
        _ => None,
    }
}

const fn refloat_operating_mode(package_mode: u8, darkride: bool) -> RideOperatingMode {
    if darkride {
        return RideOperatingMode::Darkride;
    }
    match package_mode {
        0 => RideOperatingMode::Normal,
        1 => RideOperatingMode::Handtest,
        2 => RideOperatingMode::Flywheel,
        _ => RideOperatingMode::Unknown,
    }
}

const fn refloat_ride_warning(sat: u8, beep_reason: u8) -> RideWarning {
    match sat {
        6 => return RideWarning::DutyPushback,
        10 => return RideWarning::HighVoltage,
        11 => return RideWarning::LowVoltage,
        12 => return RideWarning::TemperaturePushback,
        _ => {}
    }
    match beep_reason {
        1 => RideWarning::LowVoltage,
        2 => RideWarning::HighVoltage,
        3 => RideWarning::MosfetTemperature,
        4 => RideWarning::MotorTemperature,
        5 => RideWarning::Current,
        REFLOAT_BEEP_DUTY => RideWarning::DutyPushback,
        7 => RideWarning::Sensors,
        8 => RideWarning::LowBattery,
        10 => RideWarning::Error,
        _ => RideWarning::None,
    }
}

const fn refloat_stop_reason(stop_condition: u8) -> RideStopReason {
    match stop_condition {
        1 => RideStopReason::Pitch,
        2 => RideStopReason::Roll,
        3 => RideStopReason::SwitchHalf,
        4 => RideStopReason::SwitchFull,
        5 => RideStopReason::Reverse,
        6 => RideStopReason::QuickStop,
        _ => RideStopReason::None,
    }
}

const REFLOAT_PACKAGE_STATE_READY: u8 = 2;
const REFLOAT_PACKAGE_STATE_RUNNING: u8 = 3;

const fn refloat_operating_state(package_state: u8, charging: bool) -> RideOperatingState {
    if charging {
        return RideOperatingState::Charging;
    }
    match package_state {
        REFLOAT_PACKAGE_STATE_READY => RideOperatingState::Parked,
        REFLOAT_PACKAGE_STATE_RUNNING => RideOperatingState::Riding,
        _ => RideOperatingState::Unknown,
    }
}

#[allow(clippy::cast_possible_truncation)]
fn milliscale(value: f32) -> i32 {
    (value * 1_000.0).round() as i32
}

#[allow(clippy::cast_possible_truncation)]
fn permille(value: f32) -> i16 {
    let value = (value * 1_000.0).round();
    value.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

/// Refloat codec failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefloatCodecError {
    /// Output frame was too large for the bounded buffer.
    #[error("Refloat frame exceeded bounded output")]
    FrameTooLong,

    /// The VESC frame was incomplete.
    #[error("incomplete Refloat VESC frame")]
    IncompleteFrame,

    /// The VESC frame was malformed.
    #[error("malformed Refloat VESC frame")]
    MalformedFrame,

    /// The VESC frame checksum did not match.
    #[error("bad Refloat VESC frame checksum")]
    BadChecksum,

    /// The VESC frame was not a custom-app packet.
    #[error("unexpected Refloat VESC command")]
    UnexpectedVescCommand,

    /// The custom-app packet did not target Refloat.
    #[error("unexpected Refloat package interface")]
    UnexpectedPackageInterface,

    /// The Refloat command id is not supported by this read-only adapter.
    #[error("unsupported Refloat command")]
    UnsupportedCommand,

    /// The Refloat payload ended before all required fields were present.
    #[error("short Refloat payload")]
    ShortPayload,

    /// The Refloat payload exceeded a bounded collection.
    #[error("Refloat payload exceeded bounded collection")]
    TooManyItems,

    /// The Refloat payload contained invalid UTF-8.
    #[error("invalid Refloat string")]
    InvalidString,

    /// Realtime data arrived before dynamic field ids were discovered.
    #[error("Refloat realtime data arrived before field ids")]
    MissingRealtimeFieldIds,
}

/// Stateful decoder for VESC custom-app Refloat replies.
#[derive(Clone, Debug, Default)]
pub struct RefloatStreamDecoder {
    buffer: ArrayVec<u8, REFLOAT_MAX_FRAME_LEN>,
    field_ids: Option<RefloatRealtimeFieldIds>,
}

impl RefloatStreamDecoder {
    /// Creates an empty Refloat stream decoder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buffer: ArrayVec::new_const(),
            field_ids: None,
        }
    }

    /// Returns the discovered realtime field ids, if present.
    #[must_use]
    pub const fn field_ids(&self) -> Option<&RefloatRealtimeFieldIds> {
        self.field_ids.as_ref()
    }

    /// Feeds BLE UART bytes and returns decoded Refloat replies.
    ///
    /// # Errors
    ///
    /// Returns [`RefloatCodecError`] if framing, checksums, or bounded parsing
    /// fail.
    pub fn feed_result(
        &mut self,
        bytes: &[u8],
        mut on_reply: impl FnMut(RefloatReply<'_>),
    ) -> Result<RefloatStreamResult, RefloatCodecError> {
        for byte in bytes {
            self.buffer
                .try_push(*byte)
                .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
        }

        let mut reply_count = 0;
        while let Some(frame) = self.take_next_frame()? {
            self.decode_frame(&frame, &mut on_reply)?;
            reply_count += 1;
        }

        Ok(if reply_count == 0 {
            RefloatStreamResult::Buffered
        } else {
            RefloatStreamResult::Replies(reply_count)
        })
    }

    fn take_next_frame(
        &mut self,
    ) -> Result<Option<ArrayVec<u8, REFLOAT_MAX_FRAME_LEN>>, RefloatCodecError> {
        while self
            .buffer
            .first()
            .is_some_and(|byte| !matches!(*byte, FRAME_START_SHORT | FRAME_START_LONG))
        {
            self.buffer.remove(0);
        }

        let Some(start) = self.buffer.first().copied() else {
            return Ok(None);
        };
        let (payload_len, header_len, frame_len_extra) = match start {
            FRAME_START_SHORT => {
                let Some(payload_len) = self.buffer.get(1).copied().map(usize::from) else {
                    return Ok(None);
                };
                (payload_len, 2, 5)
            }
            FRAME_START_LONG => {
                let Some(msb) = self.buffer.get(1).copied() else {
                    return Ok(None);
                };
                let Some(lsb) = self.buffer.get(2).copied() else {
                    return Ok(None);
                };
                (usize::from(u16::from_be_bytes([msb, lsb])), 3, 6)
            }
            _ => unreachable!("non-frame starts are discarded above"),
        };
        let frame_len = payload_len
            .checked_add(frame_len_extra)
            .ok_or(RefloatCodecError::FrameTooLong)?;
        if payload_len == 0 || header_len >= frame_len || frame_len > REFLOAT_MAX_FRAME_LEN {
            return Err(RefloatCodecError::FrameTooLong);
        }
        if self.buffer.len() < frame_len {
            return Ok(None);
        }
        if self.buffer.get(frame_len - 1).copied() != Some(FRAME_END) {
            self.buffer.remove(0);
            return Err(RefloatCodecError::MalformedFrame);
        }

        let mut frame = ArrayVec::new();
        for byte in self.buffer.drain(0..frame_len) {
            frame
                .try_push(byte)
                .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
        }
        Ok(Some(frame))
    }

    fn decode_frame(
        &mut self,
        frame: &[u8],
        on_reply: &mut impl FnMut(RefloatReply<'_>),
    ) -> Result<(), RefloatCodecError> {
        let start = frame
            .first()
            .copied()
            .ok_or(RefloatCodecError::IncompleteFrame)?;
        let (payload_len, payload_start): (usize, usize) = match start {
            FRAME_START_SHORT => {
                let payload_len = frame
                    .get(1)
                    .copied()
                    .map(usize::from)
                    .ok_or(RefloatCodecError::IncompleteFrame)?;
                (payload_len, 2)
            }
            FRAME_START_LONG => {
                let msb = frame
                    .get(1)
                    .copied()
                    .ok_or(RefloatCodecError::IncompleteFrame)?;
                let lsb = frame
                    .get(2)
                    .copied()
                    .ok_or(RefloatCodecError::IncompleteFrame)?;
                (usize::from(u16::from_be_bytes([msb, lsb])), 3)
            }
            _ => return Err(RefloatCodecError::MalformedFrame),
        };
        let payload_end = payload_start
            .checked_add(payload_len)
            .ok_or(RefloatCodecError::FrameTooLong)?;
        let payload = frame
            .get(payload_start..payload_end)
            .ok_or(RefloatCodecError::IncompleteFrame)?;
        let checksum = read_u16_at(frame, payload_end)?;
        if crc16_xmodem(payload) != checksum {
            return Err(RefloatCodecError::BadChecksum);
        }

        let mut cursor = Cursor::new(payload);
        if cursor.read_u8()? != VESC_COMM_CUSTOM_APP_DATA {
            return Err(RefloatCodecError::UnexpectedVescCommand);
        }
        if cursor.read_u8()? != REFLOAT_PACKAGE_INTERFACE_ID {
            return Err(RefloatCodecError::UnexpectedPackageInterface);
        }
        match cursor.read_u8()? {
            REFLOAT_COMMAND_INFO => {
                let info = parse_info(cursor.remaining())?;
                on_reply(RefloatReply::Info(&info));
            }
            REFLOAT_COMMAND_REALTIME_DATA_IDS => {
                let ids = parse_realtime_ids(cursor.remaining())?;
                on_reply(RefloatReply::RealtimeFieldIds(&ids));
                self.field_ids = Some(ids);
            }
            REFLOAT_COMMAND_REALTIME_DATA => {
                let ids = self
                    .field_ids
                    .as_ref()
                    .ok_or(RefloatCodecError::MissingRealtimeFieldIds)?;
                let data = parse_realtime_data(cursor.remaining(), ids)?;
                on_reply(RefloatReply::RealtimeData(&data));
            }
            _ => return Err(RefloatCodecError::UnsupportedCommand),
        }
        Ok(())
    }
}

/// Encodes a Refloat read-only request as a complete VESC UART frame.
///
/// # Errors
///
/// Returns [`RefloatCodecError::FrameTooLong`] if the bounded output cannot hold
/// the request.
pub fn encode_refloat_request(
    request: RefloatReadOnlyRequest,
    output: &mut ArrayVec<u8, VESC_MAX_FRAME_LEN>,
) -> Result<(), RefloatCodecError> {
    let mut app_data = ArrayVec::<u8, 8>::new();
    app_data
        .try_push(REFLOAT_PACKAGE_INTERFACE_ID)
        .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
    match request {
        RefloatReadOnlyRequest::Info => {
            app_data
                .try_push(REFLOAT_COMMAND_INFO)
                .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
            app_data
                .try_push(2)
                .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
            app_data
                .try_push(0)
                .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
        }
        RefloatReadOnlyRequest::RealtimeDataIds => {
            app_data
                .try_push(REFLOAT_COMMAND_REALTIME_DATA_IDS)
                .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
        }
        RefloatReadOnlyRequest::RealtimeData => {
            app_data
                .try_push(REFLOAT_COMMAND_REALTIME_DATA)
                .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
        }
    }
    encode_custom_app_frame(&app_data, output)
}

pub(crate) fn encode_custom_app_frame(
    app_data: &[u8],
    output: &mut ArrayVec<u8, VESC_MAX_FRAME_LEN>,
) -> Result<(), RefloatCodecError> {
    let payload_len = app_data
        .len()
        .checked_add(1)
        .ok_or(RefloatCodecError::FrameTooLong)?;
    let payload_len_u8 =
        u8::try_from(payload_len).map_err(|_len| RefloatCodecError::FrameTooLong)?;
    let total_len = payload_len
        .checked_add(5)
        .ok_or(RefloatCodecError::FrameTooLong)?;
    if total_len > output.capacity() {
        return Err(RefloatCodecError::FrameTooLong);
    }

    let mut payload = ArrayVec::<u8, VESC_MAX_FRAME_LEN>::new();
    payload
        .try_push(VESC_COMM_CUSTOM_APP_DATA)
        .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
    payload
        .try_extend_from_slice(app_data)
        .map_err(|_err| RefloatCodecError::FrameTooLong)?;
    let crc = crc16_xmodem(&payload);

    output.clear();
    output
        .try_push(FRAME_START_SHORT)
        .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
    output
        .try_push(payload_len_u8)
        .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
    output
        .try_extend_from_slice(&payload)
        .map_err(|_err| RefloatCodecError::FrameTooLong)?;
    output
        .try_extend_from_slice(&crc.to_be_bytes())
        .map_err(|_err| RefloatCodecError::FrameTooLong)?;
    output
        .try_push(FRAME_END)
        .map_err(|_byte| RefloatCodecError::FrameTooLong)?;
    Ok(())
}

fn parse_info(bytes: &[u8]) -> Result<RefloatInfo, RefloatCodecError> {
    if bytes.len() < INFO_V2_BODY_LEN {
        return Err(RefloatCodecError::ShortPayload);
    }
    let mut cursor = Cursor::new(bytes);
    let info_version = cursor.read_u8()?;
    if info_version != 2 {
        return Err(RefloatCodecError::UnsupportedCommand);
    }
    let flags = cursor.read_u8()?;
    let package_name = read_fixed_string(cursor.read_bytes(INFO_STRING_LEN)?)?;
    let package_major = cursor.read_u8()?;
    let package_minor = cursor.read_u8()?;
    let package_patch = cursor.read_u8()?;
    let package_version_suffix = read_fixed_string(cursor.read_bytes(INFO_STRING_LEN)?)?;
    let git_hash = cursor.read_u32()?;
    let tick_rate_hz = cursor.read_u32()?;
    let capabilities = cursor.read_u32()?;
    let extra_flags = cursor.read_u8()?;

    Ok(RefloatInfo {
        info_version,
        flags,
        package_name,
        package_major,
        package_minor,
        package_patch,
        package_version_suffix,
        git_hash,
        tick_rate_hz,
        capabilities,
        extra_flags,
    })
}

fn parse_realtime_ids(bytes: &[u8]) -> Result<RefloatRealtimeFieldIds, RefloatCodecError> {
    let mut cursor = Cursor::new(bytes);
    let always = read_id_list(&mut cursor)?;
    let runtime = read_id_list(&mut cursor)?;
    Ok(RefloatRealtimeFieldIds { always, runtime })
}

fn parse_realtime_data(
    bytes: &[u8],
    ids: &RefloatRealtimeFieldIds,
) -> Result<RefloatRealtimeData, RefloatCodecError> {
    let mut cursor = Cursor::new(bytes);
    let mask = cursor.read_u8()?;
    let extra_flags = cursor.read_u8()?;
    let time_ticks = cursor.read_u32()?;
    let state_and_mode = cursor.read_u8()?;
    let flags_and_footpad = cursor.read_u8()?;
    let stop_cond_and_sat = cursor.read_u8()?;
    let beep_reason = cursor.read_u8()?;
    let values = read_values(&mut cursor, &ids.always)?;
    let runtime_values = if mask & 0x1 == 0x1 {
        read_values(&mut cursor, &ids.runtime)?
    } else {
        ArrayVec::new()
    };
    let (charging_current, charging_voltage) = if mask & 0x2 == 0x2 {
        (
            Some(read_float16(&mut cursor)?),
            Some(read_float16(&mut cursor)?),
        )
    } else {
        (None, None)
    };
    let (active_alert_mask_low, active_alert_mask_high, firmware_fault_code) = if mask & 0x4 == 0x4
    {
        (
            Some(cursor.read_u32()?),
            Some(cursor.read_u32()?),
            Some(cursor.read_u8()?),
        )
    } else {
        (None, None, None)
    };

    Ok(RefloatRealtimeData {
        mask,
        extra_flags,
        time_ticks,
        package_state: state_and_mode & 0x0f,
        package_mode: (state_and_mode >> 4) & 0x0f,
        footpad_state: flags_and_footpad >> 6,
        charging: flags_and_footpad & 0x20 == 0x20,
        fatal_error: (flags_and_footpad & 0x10 == 0x10).then_some(RefloatFatalError::FirmwareFault),
        darkride: flags_and_footpad & 0x02 == 0x02,
        wheelslip: flags_and_footpad & 0x01 == 0x01,
        stop_condition: stop_cond_and_sat & 0x0f,
        sat: stop_cond_and_sat >> 4,
        beep_reason,
        values,
        runtime_values,
        charging_current,
        charging_voltage,
        active_alert_mask_low,
        active_alert_mask_high,
        firmware_fault_code,
    })
}

fn read_id_list(
    cursor: &mut Cursor<'_>,
) -> Result<
    ArrayVec<ArrayString<REFLOAT_MAX_FIELD_ID_LEN>, REFLOAT_MAX_REALTIME_FIELDS>,
    RefloatCodecError,
> {
    let count = cursor.read_u8()?;
    let mut output = ArrayVec::new();
    for _ in 0..count {
        output
            .try_push(cursor.read_string()?)
            .map_err(|_id| RefloatCodecError::TooManyItems)?;
    }
    Ok(output)
}

fn read_values(
    cursor: &mut Cursor<'_>,
    ids: &ArrayVec<ArrayString<REFLOAT_MAX_FIELD_ID_LEN>, REFLOAT_MAX_REALTIME_FIELDS>,
) -> Result<ArrayVec<RefloatRealtimeValue, REFLOAT_MAX_REALTIME_FIELDS>, RefloatCodecError> {
    let mut output = ArrayVec::new();
    for id in ids {
        output
            .try_push(RefloatRealtimeValue {
                id: *id,
                value: read_float16(cursor)?,
            })
            .map_err(|_value| RefloatCodecError::TooManyItems)?;
    }
    Ok(output)
}

fn read_float16(cursor: &mut Cursor<'_>) -> Result<f32, RefloatCodecError> {
    Ok(float16_to_f32(cursor.read_u16()?))
}

fn read_fixed_string(bytes: &[u8]) -> Result<ArrayString<INFO_STRING_LEN>, RefloatCodecError> {
    let len = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let string = core::str::from_utf8(bytes.get(..len).ok_or(RefloatCodecError::ShortPayload)?)
        .map_err(|_err| RefloatCodecError::InvalidString)?;
    let mut output = ArrayString::new();
    output
        .try_push_str(string)
        .map_err(|_err| RefloatCodecError::InvalidString)?;
    Ok(output)
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Result<u16, RefloatCodecError> {
    let pair = bytes
        .get(offset..offset + 2)
        .ok_or(RefloatCodecError::IncompleteFrame)?;
    let array = <[u8; 2]>::try_from(pair).map_err(|_err| RefloatCodecError::IncompleteFrame)?;
    Ok(u16::from_be_bytes(array))
}

fn crc16_xmodem(bytes: &[u8]) -> u16 {
    let mut crc = 0_u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

fn float16_to_f32(bits: u16) -> f32 {
    let sign = (u32::from(bits & 0x8000)) << 16;
    let exponent = u32::from((bits >> 10) & 0x1f);
    let fraction = u32::from(bits & 0x03ff);
    let output = match exponent {
        0 if fraction == 0 => sign,
        0 => {
            let mut frac = fraction;
            let mut exp = -14_i32;
            while frac & 0x0400 == 0 {
                frac <<= 1;
                exp -= 1;
            }
            frac &= 0x03ff;
            sign | (u32::try_from(exp + 127).unwrap_or(0) << 23) | (frac << 13)
        }
        0x1f => sign | 0x7f80_0000 | (fraction << 13),
        _ => sign | ((exponent + 112) << 23) | (fraction << 13),
    };
    f32::from_bits(output)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> &'a [u8] {
        self.bytes.get(self.offset..).unwrap_or_default()
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], RefloatCodecError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(RefloatCodecError::ShortPayload)?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(RefloatCodecError::ShortPayload)?;
        self.offset = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, RefloatCodecError> {
        let byte = self
            .bytes
            .get(self.offset)
            .copied()
            .ok_or(RefloatCodecError::ShortPayload)?;
        self.offset += 1;
        Ok(byte)
    }

    fn read_u16(&mut self) -> Result<u16, RefloatCodecError> {
        let bytes = self.read_bytes(2)?;
        let array = <[u8; 2]>::try_from(bytes).map_err(|_err| RefloatCodecError::ShortPayload)?;
        Ok(u16::from_be_bytes(array))
    }

    fn read_u32(&mut self) -> Result<u32, RefloatCodecError> {
        let bytes = self.read_bytes(4)?;
        let array = <[u8; 4]>::try_from(bytes).map_err(|_err| RefloatCodecError::ShortPayload)?;
        Ok(u32::from_be_bytes(array))
    }

    fn read_string(&mut self) -> Result<ArrayString<REFLOAT_MAX_FIELD_ID_LEN>, RefloatCodecError> {
        let len = usize::from(self.read_u8()?);
        let bytes = self.read_bytes(len)?;
        let string =
            core::str::from_utf8(bytes).map_err(|_err| RefloatCodecError::InvalidString)?;
        let mut output = ArrayString::new();
        output
            .try_push_str(string)
            .map_err(|_err| RefloatCodecError::InvalidString)?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    enum CapturedReply {
        Info(RefloatInfo),
        RealtimeFieldIds(Box<RefloatRealtimeFieldIds>),
        RealtimeData(Box<RefloatRealtimeData>),
    }

    #[test]
    fn encodes_refloat_info_request_as_vesc_custom_app_data() {
        let mut frame = ArrayVec::new();

        encode_refloat_request(RefloatReadOnlyRequest::Info, &mut frame).expect("request encodes");

        assert_eq!(frame.as_slice(), &[2, 5, 36, 101, 0, 2, 0, 2, 71, 3]);
    }

    #[test]
    fn decodes_refloat_info_v2_frame() {
        let frame = custom_app_frame(&info_payload());
        let mut decoder = RefloatStreamDecoder::new();

        let (_, replies) = feed_captured(&mut decoder, &frame).expect("decode succeeds");
        assert_eq!(
            replies.as_slice(),
            &[CapturedReply::Info(RefloatInfo {
                info_version: 2,
                flags: 0,
                package_name: fixed_string("Refloat"),
                package_major: 1,
                package_minor: 2,
                package_patch: 3,
                package_version_suffix: fixed_string("dev"),
                git_hash: 0x1234_5678,
                tick_rate_hz: 10_000,
                capabilities: 0x8000_0001,
                extra_flags: 0,
            })]
        );
    }

    #[test]
    fn discovers_realtime_ids_without_static_layout() {
        let frame = custom_app_frame(&ids_payload());
        let mut decoder = RefloatStreamDecoder::new();

        let (_, replies) = feed_captured(&mut decoder, &frame).expect("decode succeeds");
        assert!(matches!(
            replies.as_slice(),
            [CapturedReply::RealtimeFieldIds(_)]
        ));
        let ids = decoder.field_ids().expect("ids retained");
        assert_eq!(
            ids.always.as_slice(),
            &[field_id("motor.speed"), field_id("imu.roll")]
        );
        assert_eq!(ids.runtime.as_slice(), &[field_id("setpoint")]);
    }

    #[test]
    fn decodes_live_realtime_ids_long_frame() {
        let mut decoder = RefloatStreamDecoder::new();
        let mut result = RefloatStreamResult::Buffered;
        let mut replies = Vec::new();
        for chunk in live_realtime_ids_chunks() {
            (result, replies) = feed_captured(&mut decoder, chunk).expect("live ids chunk decodes");
        }

        assert_eq!(result, RefloatStreamResult::Replies(1));
        assert!(matches!(
            replies.as_slice(),
            [CapturedReply::RealtimeFieldIds(_)]
        ));
        let ids = decoder.field_ids().expect("ids retained");
        assert_eq!(ids.always.len(), 16);
        assert_eq!(ids.runtime.len(), 10);
        assert_eq!(
            ids.always.first().map(ArrayString::as_str),
            Some("motor.speed")
        );
        assert_eq!(
            ids.always.last().map(ArrayString::as_str),
            Some("remote.input")
        );
        assert_eq!(
            ids.runtime.first().map(ArrayString::as_str),
            Some("setpoint")
        );
        assert_eq!(
            ids.runtime.last().map(ArrayString::as_str),
            Some("booster.current")
        );
    }

    #[test]
    fn live_refloat_replay_chunk_modes_are_equivalent() {
        let captured = decode_live_refloat_captured_chunks();
        let whole = decode_live_refloat_whole_frames();
        let one_byte = decode_live_refloat_one_byte_chunks();

        assert_eq!(captured, whole);
        assert_eq!(captured, one_byte);
        assert!(matches!(
            captured.as_slice(),
            [
                CapturedReply::RealtimeFieldIds(_),
                CapturedReply::RealtimeData(_)
            ]
        ));
        let CapturedReply::RealtimeData(data) = &captured[1] else {
            panic!("expected realtime data");
        };
        assert_eq!(data.time_ticks, 3_111_813);
        assert_eq!(data.package_state, 2);
        assert_eq!(data.values.len(), 16);
        assert_eq!(
            data.values.get(6).map(|value| value.id.as_str()),
            Some("motor.batt_voltage")
        );
    }

    #[test]
    fn decodes_realtime_data_using_discovered_ids() {
        let mut decoder = RefloatStreamDecoder::new();
        decoder
            .feed_result(&custom_app_frame(&ids_payload()), |_| {})
            .expect("ids decode");

        let (_, replies) = feed_captured(&mut decoder, &custom_app_frame(&realtime_payload()))
            .expect("data decode");

        let [CapturedReply::RealtimeData(data)] = replies.as_slice() else {
            panic!("expected realtime data");
        };
        assert_eq!(data.mask, 0x4);
        assert_eq!(data.time_ticks, 42);
        assert_eq!(data.package_state, 3);
        assert_eq!(data.package_mode, 1);
        assert_eq!(data.footpad_state, 3);
        assert_eq!(data.fatal_error, Some(RefloatFatalError::FirmwareFault));
        assert_eq!(data.stop_condition, 6);
        assert_eq!(data.sat, 10);
        assert_eq!(data.beep_reason, 8);
        assert_eq!(data.values.len(), 2);
        assert_eq!(
            data.values.first().map(|value| value.id.as_str()),
            Some("motor.speed")
        );
        assert_eq!(data.values.first().map(|value| value.value), Some(1.0));
        assert_eq!(
            data.values.get(1).map(|value| value.id.as_str()),
            Some("imu.roll")
        );
        assert_eq!(data.values.get(1).map(|value| value.value), Some(-2.0));
        assert!(data.runtime_values.is_empty());
        assert_eq!(data.active_alert_mask_low, Some(0x0000_0004));
        assert_eq!(data.active_alert_mask_high, Some(0));
        assert_eq!(data.firmware_fault_code, Some(0));
    }

    #[test]
    fn decodes_full_nibbles_for_realtime_package_state_and_mode() {
        let mut decoder = RefloatStreamDecoder::new();
        decoder
            .feed_result(&custom_app_frame(&ids_payload()), |_| {})
            .expect("ids decode");
        let mut realtime = realtime_payload();
        realtime[8] = 0xBA;

        let (_, replies) =
            feed_captured(&mut decoder, &custom_app_frame(&realtime)).expect("data decode");

        let [CapturedReply::RealtimeData(data)] = replies.as_slice() else {
            panic!("expected realtime data");
        };
        assert_eq!(data.package_state, 10);
        assert_eq!(data.package_mode, 11);
    }

    #[test]
    fn realtime_data_reports_refloat_speed_without_board_profile() {
        let mut decoder = RefloatStreamDecoder::new();
        decoder
            .feed_result(&custom_app_frame(&ids_payload()), |_| {})
            .expect("ids decode");
        let (_, replies) = feed_captured(&mut decoder, &custom_app_frame(&realtime_payload()))
            .expect("data decode");
        let [CapturedReply::RealtimeData(data)] = replies.as_slice() else {
            panic!("expected realtime data");
        };

        assert_eq!(
            data.to_delta(MonotonicTimestamp::from_milliseconds(42), true)
                .speed,
            Some(Measured::reported(Speed::from_millimetres_per_second(
                1_000
            )))
        );
    }

    #[test]
    fn realtime_data_maps_named_refloat_fields_to_shared_telemetry() {
        let mut decoder = RefloatStreamDecoder::new();
        decoder
            .feed_result(&custom_app_frame(&telemetry_ids_payload()), |_| {})
            .expect("ids decode");
        let (_, replies) =
            feed_captured(&mut decoder, &custom_app_frame(&telemetry_values_payload()))
                .expect("data decode");
        let [CapturedReply::RealtimeData(data)] = replies.as_slice() else {
            panic!("expected realtime data");
        };

        let without_battery_current =
            data.to_delta(MonotonicTimestamp::from_milliseconds(42), false);
        assert_eq!(without_battery_current.battery_current, None);

        let delta = data.to_delta(MonotonicTimestamp::from_milliseconds(42), true);

        assert_eq!(
            delta
                .battery_current
                .map(|value| value.value.as_milliamps()),
            Some(-4_000)
        );
        assert_eq!(
            delta.voltage.map(|value| value.value.as_millivolts()),
            Some(75_500)
        );
        assert_eq!(
            delta.motor_current.map(|value| value.value.as_milliamps()),
            Some(3_000)
        );
        assert_eq!(
            delta
                .controller_temperature
                .map(|value| value.value.as_millicelsius()),
            Some(32_000)
        );
        assert_eq!(
            delta
                .motor_temperature
                .map(|value| value.value.as_millicelsius()),
            Some(48_000)
        );
        assert_eq!(delta.pwm.map(|value| value.value.as_permille()), Some(250));
        assert_eq!(
            delta.pitch.map(|value| value.value.as_millidegrees()),
            Some(4_000)
        );
        assert_eq!(
            delta.roll.map(|value| value.value.as_millidegrees()),
            Some(-2_000)
        );
        assert_eq!(
            delta.footpad,
            Some(FootpadTelemetry {
                state: 3,
                contact_state: Some(FootpadContactState::Both),
                adc1_milliunits: Some(1_250),
                adc2_milliunits: Some(875),
            })
        );
        assert_eq!(delta.operating_state, Some(RideOperatingState::Riding));
    }

    #[test]
    fn refloat_footpad_contact_state_decodes_documented_states() {
        assert_eq!(
            refloat_footpad_contact_state(0),
            Some(FootpadContactState::None)
        );
        assert_eq!(
            refloat_footpad_contact_state(1),
            Some(FootpadContactState::Left)
        );
        assert_eq!(
            refloat_footpad_contact_state(2),
            Some(FootpadContactState::Right)
        );
        assert_eq!(
            refloat_footpad_contact_state(3),
            Some(FootpadContactState::Both)
        );
        assert_eq!(refloat_footpad_contact_state(4), None);
    }

    #[test]
    fn realtime_data_maps_refloat_state_and_documented_warnings() {
        let mut data = realtime_data_fixture();

        let delta = data.to_delta(MonotonicTimestamp::from_milliseconds(42), false);

        assert_eq!(delta.operating_state, Some(RideOperatingState::Parked));
        assert_eq!(delta.ride_warning, Some(RideWarning::None));
        assert_eq!(delta.ride_stop_reason, Some(RideStopReason::None));

        data.beep_reason = REFLOAT_BEEP_DUTY;
        for (stop_condition, reason) in [
            (1, RideStopReason::Pitch),
            (2, RideStopReason::Roll),
            (3, RideStopReason::SwitchHalf),
            (4, RideStopReason::SwitchFull),
            (5, RideStopReason::Reverse),
            (6, RideStopReason::QuickStop),
            (u8::MAX, RideStopReason::None),
        ] {
            data.stop_condition = stop_condition;
            let delta = data.to_delta(MonotonicTimestamp::from_milliseconds(42), false);
            assert_eq!(
                delta.ride_stop_reason,
                Some(reason),
                "stop {stop_condition}"
            );
            if reason != RideStopReason::None {
                assert_eq!(delta.ride_warning, Some(RideWarning::None));
            }
        }

        data.package_state = 3;
        data.stop_condition = 0;
        for (beep_reason, warning) in [
            (1, RideWarning::LowVoltage),
            (2, RideWarning::HighVoltage),
            (3, RideWarning::MosfetTemperature),
            (4, RideWarning::MotorTemperature),
            (5, RideWarning::Current),
            (REFLOAT_BEEP_DUTY, RideWarning::DutyPushback),
            (7, RideWarning::Sensors),
            (8, RideWarning::LowBattery),
            (9, RideWarning::None),
            (10, RideWarning::Error),
            (u8::MAX, RideWarning::None),
        ] {
            data.beep_reason = beep_reason;
            let delta = data.to_delta(MonotonicTimestamp::from_milliseconds(43), false);
            assert_eq!(delta.operating_state, Some(RideOperatingState::Riding));
            assert_eq!(
                delta.ride_warning,
                Some(warning),
                "beep reason {beep_reason}"
            );
        }

        data.beep_reason = 0;
        data.fatal_error = Some(RefloatFatalError::FirmwareFault);
        let delta = data.to_delta(MonotonicTimestamp::from_milliseconds(44), false);
        assert_eq!(delta.ride_warning, Some(RideWarning::Error));

        data.fatal_error = None;
        data.wheelslip = true;
        let delta = data.to_delta(MonotonicTimestamp::from_milliseconds(45), false);
        assert_eq!(delta.ride_warning, Some(RideWarning::Wheelslip));

        data.wheelslip = false;
        for (sat, warning) in [
            (6, RideWarning::DutyPushback),
            (10, RideWarning::HighVoltage),
            (11, RideWarning::LowVoltage),
            (12, RideWarning::TemperaturePushback),
        ] {
            data.sat = sat;
            let delta = data.to_delta(MonotonicTimestamp::from_milliseconds(45), false);
            assert_eq!(delta.ride_warning, Some(warning), "SAT {sat}");
        }
    }

    fn realtime_data_fixture() -> RefloatRealtimeData {
        RefloatRealtimeData {
            mask: 0,
            extra_flags: 0,
            time_ticks: 0,
            package_state: 2,
            package_mode: 0,
            footpad_state: 0,
            charging: false,
            fatal_error: None,
            darkride: false,
            wheelslip: false,
            stop_condition: 0,
            sat: 0,
            beep_reason: 0,
            values: ArrayVec::new(),
            runtime_values: ArrayVec::new(),
            charging_current: None,
            charging_voltage: None,
            active_alert_mask_low: None,
            active_alert_mask_high: None,
            firmware_fault_code: None,
        }
    }

    #[test]
    fn realtime_data_maps_refloat_operating_modes() {
        let mut data = realtime_data_fixture();
        assert_eq!(
            data.to_delta(MonotonicTimestamp::from_milliseconds(45), false)
                .operating_mode,
            Some(RideOperatingMode::Normal)
        );
        data.darkride = true;
        assert_eq!(
            data.to_delta(MonotonicTimestamp::from_milliseconds(45), false)
                .operating_mode,
            Some(RideOperatingMode::Darkride)
        );
        data.darkride = false;
        for (package_mode, mode) in [
            (1, RideOperatingMode::Handtest),
            (2, RideOperatingMode::Flywheel),
            (u8::MAX, RideOperatingMode::Unknown),
        ] {
            data.package_mode = package_mode;
            assert_eq!(
                data.to_delta(MonotonicTimestamp::from_milliseconds(45), false)
                    .operating_mode,
                Some(mode)
            );
        }
    }

    #[test]
    fn realtime_data_requires_discovered_ids() {
        let mut decoder = RefloatStreamDecoder::new();

        assert_eq!(
            decoder.feed_result(&custom_app_frame(&realtime_payload()), |_| {}),
            Err(RefloatCodecError::MissingRealtimeFieldIds)
        );
    }

    fn custom_app_frame(app_data: &[u8]) -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut frame = ArrayVec::new();
        encode_custom_app_frame(app_data, &mut frame).expect("frame encodes");
        frame
    }

    fn info_payload() -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut payload = ArrayVec::new();
        payload
            .try_extend_from_slice(&[101, 0, 2, 0])
            .expect("header fits");
        append_fixed(&mut payload, "Refloat");
        payload
            .try_extend_from_slice(&[1, 2, 3])
            .expect("version fits");
        append_fixed(&mut payload, "dev");
        payload
            .try_extend_from_slice(&0x1234_5678_u32.to_be_bytes())
            .expect("git hash fits");
        payload
            .try_extend_from_slice(&10_000_u32.to_be_bytes())
            .expect("tick rate fits");
        payload
            .try_extend_from_slice(&0x8000_0001_u32.to_be_bytes())
            .expect("capabilities fit");
        payload.try_push(0).expect("extra flags fit");
        payload
    }

    fn ids_payload() -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut payload = ArrayVec::new();
        payload
            .try_extend_from_slice(&[101, 32, 2])
            .expect("header fits");
        append_string(&mut payload, "motor.speed");
        append_string(&mut payload, "imu.roll");
        payload.try_push(1).expect("runtime count fits");
        append_string(&mut payload, "setpoint");
        payload
    }

    fn realtime_payload() -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut payload = ArrayVec::new();
        payload
            .try_extend_from_slice(&[101, 31, 0x4, 0])
            .expect("header fits");
        payload
            .try_extend_from_slice(&42_u32.to_be_bytes())
            .expect("time fits");
        payload
            .try_extend_from_slice(&[0x13, 0xd1, 0xa6, 8])
            .expect("state fits");
        payload
            .try_extend_from_slice(&0x3c00_u16.to_be_bytes())
            .expect("value fits");
        payload
            .try_extend_from_slice(&0xc000_u16.to_be_bytes())
            .expect("value fits");
        payload
            .try_extend_from_slice(&0x0000_0004_u32.to_be_bytes())
            .expect("alerts fit");
        payload
            .try_extend_from_slice(&0_u32.to_be_bytes())
            .expect("alerts fit");
        payload.try_push(0).expect("fault fits");
        payload
    }

    fn telemetry_ids_payload() -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut payload = ArrayVec::new();
        payload
            .try_extend_from_slice(&[101, 32, 11])
            .expect("header fits");
        for id in [
            "motor.speed",
            "motor.current",
            "motor.batt_current",
            "motor.batt_voltage",
            "motor.mosfet_temp",
            "motor.motor_temp",
            "motor.duty_cycle",
            "imu.pitch",
            "imu.roll",
            "footpad.adc1",
            "footpad.adc2",
        ] {
            append_string(&mut payload, id);
        }
        payload.try_push(0).expect("runtime count fits");
        payload
    }

    fn telemetry_values_payload() -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut payload = ArrayVec::new();
        payload
            .try_extend_from_slice(&[101, 31, 0x4, 0])
            .expect("header fits");
        payload
            .try_extend_from_slice(&42_u32.to_be_bytes())
            .expect("time fits");
        payload
            .try_extend_from_slice(&[0x13, 0xc1, 0xa6, 8])
            .expect("state fits");
        for half in [
            0x3c00_u16, // 1.0 m/s
            0x4200_u16, // 3.0 A motor
            0xc400_u16, // -4.0 A battery
            0x54b8_u16, // 75.5 V battery
            0x5000_u16, // 32 C controller
            0x5200_u16, // 48 C motor
            0x3400_u16, // 0.25 duty
            0x4400_u16, // 4 degrees pitch
            0xc000_u16, // -2 degrees roll
            0x3d00_u16, // 1.25 adc1
            0x3b00_u16, // 0.875 adc2
        ] {
            payload
                .try_extend_from_slice(&half.to_be_bytes())
                .expect("value fits");
        }
        payload
            .try_extend_from_slice(&0_u32.to_be_bytes())
            .expect("alerts fit");
        payload
            .try_extend_from_slice(&0_u32.to_be_bytes())
            .expect("alerts fit");
        payload.try_push(0).expect("fault fits");
        payload
    }

    fn append_fixed(payload: &mut ArrayVec<u8, VESC_MAX_FRAME_LEN>, string: &str) {
        let mut bytes = [0_u8; INFO_STRING_LEN];
        for (slot, byte) in bytes.iter_mut().zip(string.as_bytes()) {
            *slot = *byte;
        }
        payload.try_extend_from_slice(&bytes).expect("string fits");
    }

    fn append_string(payload: &mut ArrayVec<u8, VESC_MAX_FRAME_LEN>, string: &str) {
        payload
            .try_push(u8::try_from(string.len()).expect("fixture string length fits"))
            .expect("string length fits");
        payload
            .try_extend_from_slice(string.as_bytes())
            .expect("string fits");
    }

    fn fixed_string(value: &str) -> ArrayString<INFO_STRING_LEN> {
        let mut output = ArrayString::new();
        output.try_push_str(value).expect("fixture string fits");
        output
    }

    fn field_id(value: &str) -> ArrayString<REFLOAT_MAX_FIELD_ID_LEN> {
        let mut output = ArrayString::new();
        output.try_push_str(value).expect("fixture string fits");
        output
    }

    const LIVE_IDS_CHUNK_0: [u8; 3] = hex_literal::hex!("030196");
    const LIVE_IDS_CHUNK_1: [u8; 20] =
        hex_literal::hex!("246520100b6d6f746f722e73706565640a6d6f74");
    const LIVE_IDS_CHUNK_2: [u8; 20] =
        hex_literal::hex!("6f722e6572706d0d6d6f746f722e63757272656e");
    const LIVE_IDS_CHUNK_3: [u8; 20] =
        hex_literal::hex!("74116d6f746f722e6469725f63757272656e7412");
    const LIVE_IDS_CHUNK_4: [u8; 20] =
        hex_literal::hex!("6d6f746f722e66696c745f63757272656e74106d");
    const LIVE_IDS_CHUNK_5: [u8; 20] =
        hex_literal::hex!("6f746f722e647574795f6379636c65126d6f746f");
    const LIVE_IDS_CHUNK_6: [u8; 20] =
        hex_literal::hex!("722e626174745f766f6c74616765126d6f746f72");
    const LIVE_IDS_CHUNK_7: [u8; 20] =
        hex_literal::hex!("2e626174745f63757272656e74116d6f746f722e");
    const LIVE_IDS_CHUNK_8: [u8; 20] =
        hex_literal::hex!("6d6f736665745f74656d70106d6f746f722e6d6f");
    const LIVE_IDS_CHUNK_9: [u8; 20] =
        hex_literal::hex!("746f725f74656d7009696d752e70697463681169");
    const LIVE_IDS_CHUNK_10: [u8; 20] =
        hex_literal::hex!("6d752e62616c616e63655f706974636808696d75");
    const LIVE_IDS_CHUNK_11: [u8; 20] =
        hex_literal::hex!("2e726f6c6c0c666f6f747061642e616463310c66");
    const LIVE_IDS_CHUNK_12: [u8; 20] =
        hex_literal::hex!("6f6f747061642e616463320c72656d6f74652e69");
    const LIVE_IDS_CHUNK_13: [u8; 20] =
        hex_literal::hex!("6e7075740a08736574706f696e740c6174722e73");
    const LIVE_IDS_CHUNK_14: [u8; 20] =
        hex_literal::hex!("6574706f696e74136272616b655f74696c742e73");
    const LIVE_IDS_CHUNK_15: [u8; 20] =
        hex_literal::hex!("6574706f696e7414746f727175655f74696c742e");
    const LIVE_IDS_CHUNK_16: [u8; 20] =
        hex_literal::hex!("736574706f696e74127475726e5f74696c742e73");
    const LIVE_IDS_CHUNK_17: [u8; 20] =
        hex_literal::hex!("6574706f696e740f72656d6f74652e736574706f");
    const LIVE_IDS_CHUNK_18: [u8; 20] =
        hex_literal::hex!("696e740f62616c616e63655f63757272656e740e");
    const LIVE_IDS_CHUNK_19: [u8; 20] =
        hex_literal::hex!("6174722e616363656c5f646966660f6174722e73");
    const LIVE_IDS_CHUNK_20: [u8; 20] =
        hex_literal::hex!("706565645f626f6f73740f626f6f737465722e63");
    const LIVE_IDS_CHUNK_21: [u8; 6] = hex_literal::hex!("757272656e74");
    const LIVE_IDS_CHUNK_22: [u8; 3] = hex_literal::hex!("7b8503");

    fn live_realtime_ids_chunks() -> [&'static [u8]; 23] {
        [
            &LIVE_IDS_CHUNK_0,
            &LIVE_IDS_CHUNK_1,
            &LIVE_IDS_CHUNK_2,
            &LIVE_IDS_CHUNK_3,
            &LIVE_IDS_CHUNK_4,
            &LIVE_IDS_CHUNK_5,
            &LIVE_IDS_CHUNK_6,
            &LIVE_IDS_CHUNK_7,
            &LIVE_IDS_CHUNK_8,
            &LIVE_IDS_CHUNK_9,
            &LIVE_IDS_CHUNK_10,
            &LIVE_IDS_CHUNK_11,
            &LIVE_IDS_CHUNK_12,
            &LIVE_IDS_CHUNK_13,
            &LIVE_IDS_CHUNK_14,
            &LIVE_IDS_CHUNK_15,
            &LIVE_IDS_CHUNK_16,
            &LIVE_IDS_CHUNK_17,
            &LIVE_IDS_CHUNK_18,
            &LIVE_IDS_CHUNK_19,
            &LIVE_IDS_CHUNK_20,
            &LIVE_IDS_CHUNK_21,
            &LIVE_IDS_CHUNK_22,
        ]
    }

    const LIVE_DATA_CHUNK_0: [u8; 2] = hex_literal::hex!("0236");
    const LIVE_DATA_CHUNK_1: [u8; 20] =
        hex_literal::hex!("24651f0406002f7b850200000080008000000080");
    const LIVE_DATA_CHUNK_2: [u8; 20] =
        hex_literal::hex!("0000000bc553bc00004e834ddc4aea4ae73cdf1a");
    const LIVE_DATA_CHUNK_3: [u8; 14] = hex_literal::hex!("9a1a9a0000000000000000000000");
    const LIVE_DATA_CHUNK_4: [u8; 3] = hex_literal::hex!("9aca03");

    fn live_realtime_data_chunks() -> [&'static [u8]; 5] {
        [
            &LIVE_DATA_CHUNK_0,
            &LIVE_DATA_CHUNK_1,
            &LIVE_DATA_CHUNK_2,
            &LIVE_DATA_CHUNK_3,
            &LIVE_DATA_CHUNK_4,
        ]
    }

    fn decode_live_refloat_captured_chunks() -> Vec<CapturedReply> {
        let mut decoder = RefloatStreamDecoder::new();
        let mut output = Vec::new();
        for chunk in live_realtime_ids_chunks()
            .into_iter()
            .chain(live_realtime_data_chunks())
        {
            collect_replies(&mut decoder, chunk, &mut output);
        }
        output
    }

    fn decode_live_refloat_whole_frames() -> Vec<CapturedReply> {
        let ids = concat_chunks(&live_realtime_ids_chunks());
        let data = concat_chunks(&live_realtime_data_chunks());
        let mut decoder = RefloatStreamDecoder::new();
        let mut output = Vec::new();
        collect_replies(&mut decoder, ids.as_slice(), &mut output);
        collect_replies(&mut decoder, data.as_slice(), &mut output);
        output
    }

    fn decode_live_refloat_one_byte_chunks() -> Vec<CapturedReply> {
        let ids = concat_chunks(&live_realtime_ids_chunks());
        let data = concat_chunks(&live_realtime_data_chunks());
        let mut decoder = RefloatStreamDecoder::new();
        let mut output = Vec::new();
        for byte in ids.iter().chain(data.iter()) {
            collect_replies(&mut decoder, core::slice::from_ref(byte), &mut output);
        }
        output
    }

    fn collect_replies(
        decoder: &mut RefloatStreamDecoder,
        chunk: &[u8],
        output: &mut Vec<CapturedReply>,
    ) {
        let (_, replies) = feed_captured(decoder, chunk).expect("live replay decodes");
        output.extend(replies);
    }

    fn feed_captured(
        decoder: &mut RefloatStreamDecoder,
        bytes: &[u8],
    ) -> Result<(RefloatStreamResult, Vec<CapturedReply>), RefloatCodecError> {
        let mut replies = Vec::new();
        let result = decoder.feed_result(bytes, |reply| {
            replies.push(match reply {
                RefloatReply::Info(info) => CapturedReply::Info(info.clone()),
                RefloatReply::RealtimeFieldIds(ids) => {
                    CapturedReply::RealtimeFieldIds(Box::new(ids.clone()))
                }
                RefloatReply::RealtimeData(data) => {
                    CapturedReply::RealtimeData(Box::new(data.clone()))
                }
            });
        })?;
        Ok((result, replies))
    }

    fn concat_chunks<const N: usize>(chunks: &[&[u8]; N]) -> ArrayVec<u8, REFLOAT_MAX_FRAME_LEN> {
        let mut output = ArrayVec::new();
        for chunk in chunks {
            output
                .try_extend_from_slice(chunk)
                .expect("live frame fits");
        }
        output
    }
}
