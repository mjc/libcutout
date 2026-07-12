use arrayvec::{ArrayString, ArrayVec};
use cutout_core::{
    BatteryCurrent, Distance, Duration, PeakCurrent, PhaseCurrent, Power, RawFloatFieldValue,
    RotationalSpeed, Speed, TachometerReading, Temperature, Voltage,
};
use cutout_core::{BatteryLevel, SeriesCount, VescControllerId};
use thiserror::Error;

use crate::{BatteryVoltageProfile, RefloatReadOnlyRequest, encode_refloat_request};

/// Maximum VESC UART frame length supported by the read-only adapter.
pub const VESC_MAX_FRAME_LEN: usize = 1024;

/// Maximum firmware hash string length carried by the private VESC adapter.
pub const VESC_MAX_HASH_LEN: usize = 47;

/// Maximum replies returned from one VESC stream feed.
pub const VESC_MAX_STREAM_REPLIES: usize = 4;

const VESC_FRAME_START_SHORT: u8 = 2;
const VESC_FRAME_START_LONG: u8 = 3;
const VESC_FRAME_END: u8 = 3;
const VESC_COMM_GET_MCCONF: u8 = 14;
const VESC_COMM_GET_MCCONF_TEMP: u8 = 91;
const VESC_MCCONF_SIGNATURE: u32 = 1_470_992_211;
const VESC_MCCONF_SETUP_OFFSET: usize = 452;
const VESC_MCCONF_TEMP_SETUP_OFFSET: usize = 40;

/// Parser-owned result for one VESC UART stream feed.
#[allow(
    clippy::large_enum_variant,
    reason = "reply batches stay inline to keep parser hot paths allocation-free"
)]
#[derive(Clone, Debug, PartialEq)]
pub enum VescReadOnlyStreamResult {
    /// The decoder accepted bytes but is still waiting for a complete reply.
    Buffered,

    /// The decoder completed one or more bounded read-only replies.
    Replies(ArrayVec<VescReadOnlyReply, VESC_MAX_STREAM_REPLIES>),
}

#[cfg(test)]
impl VescReadOnlyStreamResult {
    #[must_use]
    fn into_replies(self) -> ArrayVec<VescReadOnlyReply, VESC_MAX_STREAM_REPLIES> {
        match self {
            Self::Buffered => ArrayVec::new(),
            Self::Replies(replies) => replies,
        }
    }
}

/// Owned VESC selective values mask.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescValuesMask(u32);

bitflags::bitflags! {
    impl VescValuesMask: u32 {
        /// Average input current.
        const AVG_CURRENT_INPUT = 1 << 3;
        /// Electrical RPM.
        const RPM = 1 << 7;
        /// Input voltage.
        const VOLTAGE_IN = 1 << 8;
        /// Consumed watt-hours.
        const WATT_HOURS = 1 << 11;
        /// Relative tachometer.
        const TACHOMETER = 1 << 13;
        /// Fault code.
        const FAULT_CODE = 1 << 15;
        /// Controller identifier.
        const CONTROLLER_ID = 1 << 17;
    }
}

/// Owned VESC statistics mask.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescStatsMask(u16);

bitflags::bitflags! {
    impl VescStatsMask: u16 {
        /// Average speed.
        const SPEED_AVG = 1 << 0;
        /// Maximum speed.
        const SPEED_MAX = 1 << 1;
        /// Average power.
        const POWER_AVG = 1 << 2;
        /// Maximum power.
        const POWER_MAX = 1 << 3;
        /// Average current.
        const CURRENT_AVG = 1 << 4;
        /// Maximum current.
        const CURRENT_MAX = 1 << 5;
        /// Maximum motor temperature.
        const TEMP_MOTOR_MAX = 1 << 9;
        /// Statistics accumulation time.
        const COUNT_TIME = 1 << 10;
    }
}

/// Read-only VESC request allowed through the private codec boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VescReadOnlyRequest {
    /// Request firmware version metadata.
    FirmwareVersion,

    /// Request firmware build information.
    FirmwareInfo,

    /// Request ordinary values telemetry.
    Values,

    /// Request selected values telemetry.
    ValuesSelective(VescValuesMask),

    /// Request selected statistics.
    Stats(VescStatsMask),

    /// Request motor config facts used by VESC itself for pack and speed geometry.
    MotorConfig,

    /// Request the small motor setup config subset used for speed geometry.
    MotorSetupConfig,

    /// Request read-only data from the Refloat custom app package.
    Refloat(RefloatReadOnlyRequest),

    /// Forward a nested read-only request to a CAN controller.
    ForwardCan {
        /// Target VESC controller id on CAN.
        controller_id: VescControllerId,

        /// Nested read-only request.
        request: VescCanReadOnlyRequest,
    },
}

/// Nested read-only VESC request allowed inside CAN forwarding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VescCanReadOnlyRequest {
    /// Request firmware version metadata.
    FirmwareVersion,

    /// Request firmware build information.
    FirmwareInfo,

    /// Request ordinary values telemetry.
    Values,

    /// Request selected values telemetry.
    ValuesSelective(VescValuesMask),

    /// Request selected statistics.
    Stats(VescStatsMask),
}

/// Owned VESC read-only reply.
#[derive(Clone, Debug, PartialEq)]
pub enum VescReadOnlyReply {
    /// Firmware build information.
    FirmwareInfo {
        /// Major firmware version.
        major: u8,

        /// Minor firmware version.
        minor: u8,

        /// Test version number.
        test_version_number: u8,

        /// Firmware commit hash.
        commit_hash: ArrayString<VESC_MAX_HASH_LEN>,

        /// User firmware commit hash.
        user_commit_hash: ArrayString<VESC_MAX_HASH_LEN>,
    },

    /// Runtime values telemetry.
    Values(VescValuesTelemetry),

    /// Runtime statistics telemetry.
    Stats(VescStatsTelemetry),

    /// Motor config facts used for display calculations.
    MotorConfig(VescMotorConfig),

    /// Motor setup config facts used for speed calculations.
    MotorSetupConfig(VescSpeedGeometry),
}

/// Owned VESC values telemetry subset used by the generic read-only session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescValuesTelemetry {
    /// MOSFET/controller temperature.
    pub controller_temperature: Temperature,

    /// Motor temperature.
    pub motor_temperature: Temperature,

    /// Average motor current.
    pub motor_current: PhaseCurrent,

    /// Electrical RPM.
    pub rpm: RotationalSpeed,

    /// Input voltage.
    pub voltage: Voltage,

    /// Input current.
    pub input_current: BatteryCurrent,

    /// Relative tachometer.
    pub tachometer: TachometerReading,

    /// Absolute tachometer count.
    pub tachometer_absolute: TachometerReading,

    /// Controller identifier.
    pub controller_id: VescControllerId,

    /// Current VESC fault code.
    pub fault_code: VescFaultCode,

    /// Controller status byte.
    pub status: u8,

    /// Every floating-point field returned by COMM_GET_VALUES, preserving exact bits.
    pub raw_float_fields: [Option<RawFloatFieldValue>; 32],
}

/// Motor pole-pair count used to convert VESC eRPM to mechanical RPM.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MotorPolePairs(u8);

impl MotorPolePairs {
    /// Creates a motor pole-pair count.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns the motor pole-pair count.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Mechanical gear-reduction denominator.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GearRatioDenominator(u8);

impl GearRatioDenominator {
    /// Creates a gear-reduction denominator.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns the gear-reduction denominator.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Owned VESC statistics telemetry subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescStatsTelemetry {
    /// Average speed.
    pub speed_avg: Speed,

    /// Maximum speed.
    pub speed_max: Speed,

    /// Average power.
    pub power_avg: Power,

    /// Maximum power.
    pub power_max: Power,

    /// Average current.
    pub current_avg: BatteryCurrent,

    /// Peak current.
    pub peak_current: PeakCurrent,

    /// Statistics accumulation time.
    pub count_time: Duration,
}

/// VESC speed geometry from motor setup config.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescSpeedGeometry {
    /// Motor pole pairs used to convert electrical RPM to mechanical RPM.
    pub motor_pole_pairs: MotorPolePairs,

    /// Mechanical gear reduction denominator.
    pub gear_ratio_denominator: GearRatioDenominator,

    /// Wheel circumference.
    pub wheel_circumference: Distance,
}

/// VESC motor config facts needed by read-only display code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescMotorConfig {
    /// Speed geometry from VESC setup config.
    pub speed_geometry: VescSpeedGeometry,

    /// VESC battery type used for voltage-derived pack level.
    pub battery_type: VescBatteryType,

    /// Battery series cell count from VESC setup config.
    pub battery_cells: SeriesCount,
}

/// VESC battery type from motor setup config.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VescBatteryType {
    /// Li-ion 3.0-4.2 V pack type.
    LiIon,

    /// LiFePO4 / lithium iron 2.6-3.6 V pack type.
    LiIron,

    /// Lead-acid 2.1-2.36 V cell model.
    LeadAcid,

    /// A battery type not modeled by libcutout yet.
    Other(u8),
}

/// Verified VESC board facts used to calculate display telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescBoardProfile {
    /// Motor pole pairs used to convert electrical RPM to mechanical RPM.
    pub motor_pole_pairs: MotorPolePairs,

    /// Mechanical gear reduction denominator.
    pub gear_ratio_denominator: GearRatioDenominator,

    /// Wheel circumference.
    pub wheel_circumference: Distance,

    /// Optional voltage curve used to estimate battery level.
    pub battery_profile: Option<VescBatteryProfile>,

    /// True when this profile should calculate speed from eRPM.
    pub calculates_speed: bool,

    /// True when VESC input-current telemetry is a trustworthy battery-current reading.
    pub reports_battery_current: bool,
}

/// VESC battery voltage curve and pack layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescBatteryProfile {
    /// Single-cell voltage curve.
    pub curve: VescBatteryCurve,

    /// Series cell count in the pack.
    pub series_cells: SeriesCount,
}

/// Battery curve used for VESC voltage-derived level estimates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VescBatteryCurve {
    /// Explicit known cell voltage profile.
    CellProfile(&'static BatteryVoltageProfile),

    /// VESC firmware Li-ion curve.
    VescLiIon,

    /// Linear per-cell voltage range.
    Linear {
        /// Empty per-cell voltage in millivolts.
        empty_cell_mv: i32,

        /// Full per-cell voltage in millivolts.
        full_cell_mv: i32,
    },
}

impl VescBoardProfile {
    /// Creates a board profile from explicit geometry.
    #[must_use]
    pub const fn new(
        motor_pole_pairs: MotorPolePairs,
        gear_ratio_denominator: GearRatioDenominator,
        wheel_circumference: Distance,
    ) -> Self {
        Self {
            motor_pole_pairs,
            gear_ratio_denominator,
            wheel_circumference,
            battery_profile: None,
            calculates_speed: true,
            reports_battery_current: false,
        }
    }

    /// Adds a voltage curve used to estimate battery percent from pack voltage.
    #[must_use]
    pub const fn with_battery_profile(
        mut self,
        voltage_profile: &'static BatteryVoltageProfile,
        series_cells: SeriesCount,
    ) -> Self {
        self.battery_profile = Some(VescBatteryProfile {
            curve: VescBatteryCurve::CellProfile(voltage_profile),
            series_cells,
        });
        self
    }

    /// Adds a VESC motor-config battery type and series count.
    #[must_use]
    pub const fn with_vesc_battery_type(
        mut self,
        battery_type: VescBatteryType,
        series_cells: SeriesCount,
    ) -> Self {
        self.battery_profile = match battery_type.curve() {
            Some(curve) => Some(VescBatteryProfile {
                curve,
                series_cells,
            }),
            None => None,
        };
        self
    }

    /// Disables eRPM speed calculation when only non-geometry facts are known.
    #[must_use]
    pub const fn without_calculated_speed(mut self) -> Self {
        self.calculates_speed = false;
        self
    }

    /// Marks VESC input-current telemetry as a displayable battery-current reading.
    #[must_use]
    pub const fn with_reported_battery_current(mut self) -> Self {
        self.reports_battery_current = true;
        self
    }

    /// Creates a profile from VESC speed geometry.
    #[must_use]
    pub const fn from_speed_geometry(geometry: VescSpeedGeometry) -> Self {
        Self::new(
            geometry.motor_pole_pairs,
            geometry.gear_ratio_denominator,
            geometry.wheel_circumference,
        )
    }

    /// Creates a profile from VESC motor config and a known cell voltage curve.
    #[must_use]
    pub const fn from_motor_config(config: VescMotorConfig) -> Self {
        Self::from_speed_geometry(config.speed_geometry)
            .with_vesc_battery_type(config.battery_type, config.battery_cells)
    }

    /// Calculates signed road speed from eRPM.
    #[must_use]
    pub fn speed_from_erpm(self, erpm: RotationalSpeed) -> Option<Speed> {
        if !self.calculates_speed {
            return None;
        }
        erpm.as_speed(
            self.motor_pole_pairs.get(),
            self.gear_ratio_denominator.get(),
            self.wheel_circumference,
        )
    }

    /// Estimates battery level from pack voltage when a voltage curve is known.
    #[must_use]
    pub fn battery_level_from_voltage(self, voltage: Voltage) -> Option<BatteryLevel> {
        self.battery_profile
            .map(|profile| profile.estimate_level_from_pack_voltage(voltage))
    }
}

impl VescBatteryProfile {
    #[must_use]
    fn estimate_level_from_pack_voltage(self, voltage: Voltage) -> BatteryLevel {
        match self.curve {
            VescBatteryCurve::CellProfile(profile) => {
                profile.estimate_level_from_pack_voltage(voltage, self.series_cells)
            }
            VescBatteryCurve::VescLiIon => {
                vesc_liion_level(voltage.as_cell_voltage(self.series_cells))
            }
            VescBatteryCurve::Linear {
                empty_cell_mv,
                full_cell_mv,
            } => linear_cell_voltage_level(
                voltage.as_cell_voltage(self.series_cells),
                empty_cell_mv,
                full_cell_mv,
            ),
        }
    }
}

impl VescBatteryType {
    #[must_use]
    const fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::LiIon,
            1 => Self::LiIron,
            2 => Self::LeadAcid,
            other => Self::Other(other),
        }
    }

    #[must_use]
    const fn curve(self) -> Option<VescBatteryCurve> {
        match self {
            Self::LiIon => Some(VescBatteryCurve::VescLiIon),
            Self::LiIron => Some(VescBatteryCurve::Linear {
                empty_cell_mv: 2_600,
                full_cell_mv: 3_600,
            }),
            Self::LeadAcid => Some(VescBatteryCurve::Linear {
                empty_cell_mv: 2_100,
                full_cell_mv: 2_360,
            }),
            Self::Other(_) => None,
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn vesc_liion_level(cell_voltage: cutout_core::CellVoltage) -> BatteryLevel {
    let norm = ((cell_voltage.as_microvolts() as f32 / 1_000.0) - 3_200.0) / 1_000.0;
    let norm = norm.clamp(0.0, 1.0);
    let v2 = norm * norm;
    let v3 = v2 * norm;
    let v4 = v3 * norm;
    let v5 = v4 * norm;
    let capacity =
        -2.979_767 * v5 + 5.487_810 * v4 - 3.501_286 * v3 + 1.675_683 * v2 + 0.317_147 * norm;
    BatteryLevel::from_percent_i32((capacity.clamp(0.0, 1.0) * 100.0).round() as i32)
}

fn linear_cell_voltage_level(
    cell_voltage: cutout_core::CellVoltage,
    empty_cell_mv: i32,
    full_cell_mv: i32,
) -> BatteryLevel {
    let cell_mv = cell_voltage.as_microvolts() / 1_000;
    if full_cell_mv <= empty_cell_mv {
        return BatteryLevel::from_percent(0);
    }
    BatteryLevel::from_percent_i32(
        (cell_mv - empty_cell_mv).clamp(0, full_cell_mv - empty_cell_mv) * 100
            / (full_cell_mv - empty_cell_mv),
    )
}

/// VESC fault code subset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VescFaultCode {
    /// No fault.
    #[default]
    None,

    /// Absolute over-current fault.
    AbsOverCurrent,

    /// A fault not yet modeled by libcutout.
    Other(u8),
}

/// Error returned by the private VESC codec adapter.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum VescCodecError {
    /// Encoded frame exceeded the bounded output buffer.
    #[error("encoded VESC frame exceeded bounded output")]
    EncodedFrameTooLong,

    /// VESC dependency failed to encode the request.
    #[error("VESC request encoding failed")]
    EncodeFailed,

    /// VESC dependency failed to decode the reply.
    #[error("VESC reply decoding failed")]
    DecodeFailed,

    /// Reply was for a command not exposed by the read-only adapter.
    #[error("unsupported VESC reply")]
    UnsupportedReply,
}

/// Private VESC codec adapter with libcutout-owned public types.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VescReadOnlyCodec;

impl VescReadOnlyCodec {
    /// Encodes a read-only VESC request into the provided bounded output.
    ///
    /// # Errors
    ///
    /// Returns [`VescCodecError`] if encoding fails or the encoded frame does
    /// not fit in `output`.
    pub fn encode_request(
        request: VescReadOnlyRequest,
        output: &mut ArrayVec<u8, VESC_MAX_FRAME_LEN>,
    ) -> Result<(), VescCodecError> {
        let mut frame = [0; VESC_MAX_FRAME_LEN];
        let len = encode_request_frame(request, &mut frame)?;
        let encoded = frame
            .get(..len)
            .ok_or(VescCodecError::EncodedFrameTooLong)?;
        output
            .try_extend_from_slice(encoded)
            .map_err(|_err| VescCodecError::EncodedFrameTooLong)
    }

    /// Decodes a read-only VESC reply.
    ///
    /// # Errors
    ///
    /// Returns [`VescCodecError`] if the frame is invalid or the decoded reply
    /// is outside the read-only adapter surface.
    pub fn decode_reply(bytes: &[u8]) -> Result<VescReadOnlyReply, VescCodecError> {
        if let Some(reply) = decode_private_reply(bytes)? {
            return Ok(reply);
        }
        let (_consumed, reply) =
            vesc::decode(bytes).map_err(|_err| VescCodecError::DecodeFailed)?;
        map_command_reply(reply)
    }
}

/// Bounded streaming VESC decoder with libcutout-owned output types.
#[derive(Clone, Debug)]
pub struct VescReadOnlyStreamDecoder {
    buffer: [u8; VESC_MAX_FRAME_LEN],
    read_position: usize,
    write_position: usize,
}

impl Default for VescReadOnlyStreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl VescReadOnlyStreamDecoder {
    /// Creates an empty VESC stream decoder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: [0; VESC_MAX_FRAME_LEN],
            read_position: 0,
            write_position: 0,
        }
    }

    /// Feeds bytes into the stream decoder and returns a typed parser result.
    ///
    /// # Errors
    ///
    /// Returns [`VescCodecError`] if the private decoder rejects the input,
    /// produces an unsupported reply, or yields more replies than the bounded
    /// result can hold.
    pub fn feed_result(
        &mut self,
        bytes: &[u8],
    ) -> Result<VescReadOnlyStreamResult, VescCodecError> {
        self.compact();
        if bytes.len() > self.buffer.len().saturating_sub(self.write_position) {
            return Err(VescCodecError::DecodeFailed);
        }
        self.buffer[self.write_position..self.write_position + bytes.len()].copy_from_slice(bytes);
        self.write_position += bytes.len();

        let mut replies = ArrayVec::new();
        while self.read_position < self.write_position {
            let frame = &self.buffer[self.read_position..self.write_position];
            match frame_len(frame)? {
                Some(len) => {
                    replies
                        .try_push(decode_frame(&frame[..len])?)
                        .map_err(|_reply| VescCodecError::DecodeFailed)?;
                    self.read_position += len;
                }
                None => break,
            }
        }
        Ok(if replies.is_empty() {
            VescReadOnlyStreamResult::Buffered
        } else {
            VescReadOnlyStreamResult::Replies(replies)
        })
    }

    fn compact(&mut self) {
        if self.read_position == 0 {
            return;
        }
        self.buffer
            .copy_within(self.read_position..self.write_position, 0);
        self.write_position -= self.read_position;
        self.read_position = 0;
    }
}

fn map_command_reply(reply: vesc::CommandReply) -> Result<VescReadOnlyReply, VescCodecError> {
    match reply {
        vesc::CommandReply::FwInfo(info) => Ok(VescReadOnlyReply::FirmwareInfo {
            major: info.major,
            minor: info.minor,
            test_version_number: info.test_version_number,
            commit_hash: bounded_string(info.commit_hash().unwrap_or("")),
            user_commit_hash: bounded_string(info.user_commit_hash().unwrap_or("")),
        }),
        vesc::CommandReply::GetValues(values) | vesc::CommandReply::GetValuesSelective(values) => {
            Ok(VescReadOnlyReply::Values(values.into()))
        }
        vesc::CommandReply::GetStats(stats) => Ok(VescReadOnlyReply::Stats(stats.into())),
        vesc::CommandReply::FwVersion(_)
        | vesc::CommandReply::GetValuesSetupSelective(_)
        | vesc::CommandReply::ResetStats => Err(VescCodecError::UnsupportedReply),
    }
}

fn encode_request_frame(
    request: VescReadOnlyRequest,
    frame: &mut [u8; VESC_MAX_FRAME_LEN],
) -> Result<usize, VescCodecError> {
    match request {
        VescReadOnlyRequest::FirmwareVersion => vesc::encode(vesc::Command::FwVersion, frame),
        VescReadOnlyRequest::FirmwareInfo => vesc::encode(vesc::Command::FwInfo, frame),
        VescReadOnlyRequest::Values => vesc::encode(vesc::Command::GetValues, frame),
        VescReadOnlyRequest::ValuesSelective(mask) => vesc::encode(
            vesc::Command::GetValuesSelective(vesc_values_mask(mask)),
            frame,
        ),
        VescReadOnlyRequest::Stats(mask) => {
            vesc::encode(vesc::Command::GetStats(vesc_stats_mask(mask)), frame)
        }
        VescReadOnlyRequest::MotorConfig => return encode_raw_command(VESC_COMM_GET_MCCONF, frame),
        VescReadOnlyRequest::MotorSetupConfig => {
            return encode_raw_command(VESC_COMM_GET_MCCONF_TEMP, frame);
        }
        VescReadOnlyRequest::Refloat(request) => {
            let mut output = ArrayVec::new();
            encode_refloat_request(request, &mut output)
                .map_err(|_err| VescCodecError::EncodeFailed)?;
            let len = output.len();
            frame
                .get_mut(..len)
                .ok_or(VescCodecError::EncodedFrameTooLong)?
                .copy_from_slice(output.as_slice());
            return Ok(len);
        }
        VescReadOnlyRequest::ForwardCan {
            controller_id,
            request,
        } => {
            let nested = command_for_can_request(request);
            vesc::encode(
                vesc::Command::ForwardCan(controller_id.get(), &nested),
                frame,
            )
        }
    }
    .map_err(|_err| VescCodecError::EncodeFailed)
}

fn encode_raw_command(
    command_id: u8,
    frame: &mut [u8; VESC_MAX_FRAME_LEN],
) -> Result<usize, VescCodecError> {
    frame[0] = VESC_FRAME_START_SHORT;
    frame[1] = 1;
    frame[2] = command_id;
    let checksum = vesc_crc16(&frame[2..3]);
    frame[3..5].copy_from_slice(&checksum.to_be_bytes());
    frame[5] = VESC_FRAME_END;
    Ok(6)
}

fn decode_frame(frame: &[u8]) -> Result<VescReadOnlyReply, VescCodecError> {
    if let Some(reply) = decode_private_reply(frame)? {
        return Ok(reply);
    }
    let (_consumed, reply) = vesc::decode(frame).map_err(|_err| VescCodecError::DecodeFailed)?;
    map_command_reply(reply)
}

fn decode_private_reply(frame: &[u8]) -> Result<Option<VescReadOnlyReply>, VescCodecError> {
    let Some((payload_start, payload_len, _frame_len)) = frame_parts(frame)? else {
        return Ok(None);
    };
    let payload = frame
        .get(payload_start..payload_start + payload_len)
        .ok_or(VescCodecError::DecodeFailed)?;
    let Some((&command_id, body)) = payload.split_first() else {
        return Err(VescCodecError::DecodeFailed);
    };
    match command_id {
        VESC_COMM_GET_MCCONF => {
            decode_motor_config(body).map(|config| Some(VescReadOnlyReply::MotorConfig(config)))
        }
        VESC_COMM_GET_MCCONF_TEMP => decode_speed_geometry(body)
            .map(|geometry| Some(VescReadOnlyReply::MotorSetupConfig(geometry))),
        _ => Ok(None),
    }
}

fn frame_len(bytes: &[u8]) -> Result<Option<usize>, VescCodecError> {
    frame_parts(bytes).map(|parts| parts.map(|(_payload_start, _payload_len, len)| len))
}

fn frame_parts(bytes: &[u8]) -> Result<Option<(usize, usize, usize)>, VescCodecError> {
    let Some(&start) = bytes.first() else {
        return Ok(None);
    };
    let (payload_start, payload_len) = match start {
        VESC_FRAME_START_SHORT => {
            let Some(&len) = bytes.get(1) else {
                return Ok(None);
            };
            (2, usize::from(len))
        }
        VESC_FRAME_START_LONG => {
            let Some(len_bytes) = bytes.get(1..3) else {
                return Ok(None);
            };
            let len = u16::from_be_bytes([len_bytes[0], len_bytes[1]]);
            (3, usize::from(len))
        }
        _ => return Err(VescCodecError::DecodeFailed),
    };
    let checksum_start = payload_start + payload_len;
    let total_len = checksum_start + 3;
    if bytes.len() < total_len {
        return Ok(None);
    }
    if bytes[checksum_start + 2] != VESC_FRAME_END {
        return Err(VescCodecError::DecodeFailed);
    }
    let expected = u16::from_be_bytes([bytes[checksum_start], bytes[checksum_start + 1]]);
    let actual = vesc_crc16(&bytes[payload_start..checksum_start]);
    if expected != actual {
        return Err(VescCodecError::DecodeFailed);
    }
    Ok(Some((payload_start, payload_len, total_len)))
}

fn decode_motor_config(body: &[u8]) -> Result<VescMotorConfig, VescCodecError> {
    let signature = body
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or(VescCodecError::DecodeFailed)?;
    if signature != VESC_MCCONF_SIGNATURE {
        return Err(VescCodecError::DecodeFailed);
    }
    let setup = body
        .get(VESC_MCCONF_SETUP_OFFSET..)
        .ok_or(VescCodecError::DecodeFailed)?;
    let (speed_geometry, next) = decode_setup_fields(setup)?;
    let battery_type = *setup.get(next).ok_or(VescCodecError::DecodeFailed)?;
    let battery_cells = *setup.get(next + 1).ok_or(VescCodecError::DecodeFailed)?;
    Ok(VescMotorConfig {
        speed_geometry,
        battery_type: VescBatteryType::from_raw(battery_type),
        battery_cells: SeriesCount::new(battery_cells),
    })
}

fn decode_speed_geometry(body: &[u8]) -> Result<VescSpeedGeometry, VescCodecError> {
    let setup = body
        .get(VESC_MCCONF_TEMP_SETUP_OFFSET..)
        .ok_or(VescCodecError::DecodeFailed)?;
    decode_setup_fields(setup).map(|(geometry, _next)| geometry)
}

fn decode_setup_fields(body: &[u8]) -> Result<(VescSpeedGeometry, usize), VescCodecError> {
    let motor_poles = *body.first().ok_or(VescCodecError::DecodeFailed)?;
    let gear_ratio = read_vesc_f32_auto(body, 1)?;
    let wheel_diameter_metres = read_vesc_f32_auto(body, 5)?;
    let motor_pole_pairs = motor_poles / 2;
    if motor_pole_pairs == 0 || gear_ratio <= 0.0 || wheel_diameter_metres <= 0.0 {
        return Err(VescCodecError::DecodeFailed);
    }
    Ok((
        VescSpeedGeometry {
            motor_pole_pairs: MotorPolePairs::new(motor_pole_pairs),
            gear_ratio_denominator: GearRatioDenominator::new(gear_ratio.round() as u8),
            wheel_circumference: Distance::from_metres_f32(
                wheel_diameter_metres * core::f32::consts::PI,
            ),
        },
        9,
    ))
}

fn read_vesc_f32_auto(bytes: &[u8], offset: usize) -> Result<f32, VescCodecError> {
    let word = bytes
        .get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or(VescCodecError::DecodeFailed)?;
    Ok(f32::from_bits(word))
}

fn vesc_crc16(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0u16, |mut crc, byte| {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
        crc
    })
}

fn command_for_can_request(request: VescCanReadOnlyRequest) -> vesc::Command<'static> {
    match request {
        VescCanReadOnlyRequest::FirmwareVersion => vesc::Command::FwVersion,
        VescCanReadOnlyRequest::FirmwareInfo => vesc::Command::FwInfo,
        VescCanReadOnlyRequest::Values => vesc::Command::GetValues,
        VescCanReadOnlyRequest::ValuesSelective(mask) => {
            vesc::Command::GetValuesSelective(vesc_values_mask(mask))
        }
        VescCanReadOnlyRequest::Stats(mask) => vesc::Command::GetStats(vesc_stats_mask(mask)),
    }
}

fn vesc_values_mask(mask: VescValuesMask) -> vesc::ValuesMask {
    let mut converted = vesc::ValuesMask::empty();
    if mask.contains(VescValuesMask::AVG_CURRENT_INPUT) {
        converted |= vesc::ValuesMask::AVG_CURRENT_INPUT;
    }
    if mask.contains(VescValuesMask::RPM) {
        converted |= vesc::ValuesMask::RPM;
    }
    if mask.contains(VescValuesMask::VOLTAGE_IN) {
        converted |= vesc::ValuesMask::VOLTAGE_IN;
    }
    if mask.contains(VescValuesMask::WATT_HOURS) {
        converted |= vesc::ValuesMask::WATT_HOURS;
    }
    if mask.contains(VescValuesMask::TACHOMETER) {
        converted |= vesc::ValuesMask::TACHOMETER;
    }
    if mask.contains(VescValuesMask::FAULT_CODE) {
        converted |= vesc::ValuesMask::FAULT_CODE;
    }
    if mask.contains(VescValuesMask::CONTROLLER_ID) {
        converted |= vesc::ValuesMask::CONTROLLER_ID;
    }
    converted
}

fn vesc_stats_mask(mask: VescStatsMask) -> vesc::StatsMask {
    let mut converted = vesc::StatsMask::empty();
    if mask.contains(VescStatsMask::SPEED_AVG) {
        converted |= vesc::StatsMask::SPEED_AVG;
    }
    if mask.contains(VescStatsMask::SPEED_MAX) {
        converted |= vesc::StatsMask::SPEED_MAX;
    }
    if mask.contains(VescStatsMask::POWER_AVG) {
        converted |= vesc::StatsMask::POWER_AVG;
    }
    if mask.contains(VescStatsMask::POWER_MAX) {
        converted |= vesc::StatsMask::POWER_MAX;
    }
    if mask.contains(VescStatsMask::CURRENT_AVG) {
        converted |= vesc::StatsMask::CURRENT_AVG;
    }
    if mask.contains(VescStatsMask::CURRENT_MAX) {
        converted |= vesc::StatsMask::CURRENT_MAX;
    }
    if mask.contains(VescStatsMask::TEMP_MOTOR_MAX) {
        converted |= vesc::StatsMask::TEMP_MOTOR_MAX;
    }
    if mask.contains(VescStatsMask::COUNT_TIME) {
        converted |= vesc::StatsMask::COUNT_TIME;
    }
    converted
}

fn bounded_string(value: &str) -> ArrayString<VESC_MAX_HASH_LEN> {
    let mut output = ArrayString::new();
    let _ = output.try_push_str(value);
    output
}

impl From<vesc::Values> for VescValuesTelemetry {
    fn from(values: vesc::Values) -> Self {
        Self {
            controller_temperature: temperature_from_celsius_f32(values.temp_mosfet),
            motor_temperature: temperature_from_celsius_f32(values.temp_motor),
            motor_current: PhaseCurrent::from_amps_f32(values.avg_current_motor),
            rpm: RotationalSpeed::from_erpm_f32(values.rpm),
            voltage: Voltage::from_volts_f32(values.voltage_in),
            input_current: BatteryCurrent::from_amps_f32(values.avg_current_input),
            tachometer: TachometerReading::from_counts(values.tachometer),
            tachometer_absolute: TachometerReading::from_counts(values.tachometer_abs),
            controller_id: VescControllerId::new(values.controller_id),
            fault_code: values.fault_code.into(),
            status: values.status,
            raw_float_fields: vesc_values_float_fields(values),
        }
    }
}

fn vesc_values_float_fields(values: vesc::Values) -> [Option<RawFloatFieldValue>; 32] {
    let values = [
        values.temp_mosfet,
        values.temp_motor,
        values.avg_current_motor,
        values.avg_current_input,
        values.avg_current_d,
        values.avg_current_q,
        values.duty_cycle,
        values.rpm,
        values.voltage_in,
        values.amp_hours,
        values.amp_hours_charged,
        values.watt_hours,
        values.watt_hours_charged,
        values.pid_pos,
        values.temp_mosfet1,
        values.temp_mosfet2,
        values.temp_mosfet3,
        values.avg_voltage_d,
        values.avg_voltage_q,
    ];
    let mut fields = [None; 32];
    for (index, value) in values.into_iter().enumerate() {
        let id = 0x8100 + u16::try_from(index).expect("VESC raw field index fits u16");
        fields[index] = Some(RawFloatFieldValue::new(id, value));
    }
    fields
}

#[allow(clippy::cast_possible_truncation)]
fn temperature_from_celsius_f32(value: f32) -> Temperature {
    Temperature::from_millicelsius((value * 1_000.0).round() as i32)
}

impl From<vesc::Stats> for VescStatsTelemetry {
    fn from(stats: vesc::Stats) -> Self {
        Self {
            speed_avg: Speed::from_metres_per_second(stats.speed_avg),
            speed_max: Speed::from_metres_per_second(stats.speed_max),
            power_avg: Power::from_watts_f32(stats.power_avg),
            power_max: Power::from_watts_f32(stats.power_max),
            current_avg: BatteryCurrent::from_amps_f32(stats.current_avg),
            peak_current: PeakCurrent::from_amps_f32(stats.current_max),
            count_time: Duration::from_seconds_f32(stats.count_time),
        }
    }
}

impl From<vesc::FaultCode> for VescFaultCode {
    fn from(code: vesc::FaultCode) -> Self {
        match code {
            vesc::FaultCode::None => Self::None,
            vesc::FaultCode::AbsOverCurrent => Self::AbsOverCurrent,
            other => Self::Other(other as u8),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SAMSUNG_50S_PROFILE;

    #[test]
    fn encodes_read_only_requests_without_exposing_vesc_types() {
        let mut output = ArrayVec::<u8, VESC_MAX_FRAME_LEN>::new();

        VescReadOnlyCodec::encode_request(VescReadOnlyRequest::FirmwareVersion, &mut output)
            .expect("firmware version request encodes");
        assert_eq!(output.as_slice(), &[2, 1, 0, 0, 0, 3]);

        output.clear();
        VescReadOnlyCodec::encode_request(VescReadOnlyRequest::Values, &mut output)
            .expect("values request encodes");
        assert_eq!(output.as_slice(), &[2, 1, 4, 64, 132, 3]);

        output.clear();
        VescReadOnlyCodec::encode_request(VescReadOnlyRequest::FirmwareInfo, &mut output)
            .expect("firmware info request encodes");
        assert_eq!(output.as_slice(), &[2, 1, 157, 82, 20, 3]);

        output.clear();
        VescReadOnlyCodec::encode_request(VescReadOnlyRequest::MotorConfig, &mut output)
            .expect("motor config request encodes");
        assert_eq!(output.as_slice(), &[2, 1, 14, 225, 206, 3]);

        output.clear();
        VescReadOnlyCodec::encode_request(VescReadOnlyRequest::MotorSetupConfig, &mut output)
            .expect("motor setup config request encodes");
        assert_eq!(output.as_slice(), &[2, 1, 91, 235, 158, 3]);
    }

    #[test]
    fn encodes_selective_values_and_stats_requests_through_owned_masks() {
        let mut output = ArrayVec::<u8, VESC_MAX_FRAME_LEN>::new();

        VescReadOnlyCodec::encode_request(
            VescReadOnlyRequest::ValuesSelective(
                VescValuesMask::RPM | VescValuesMask::WATT_HOURS | VescValuesMask::CONTROLLER_ID,
            ),
            &mut output,
        )
        .expect("selective values request encodes");
        assert_eq!(output.as_slice(), &[2, 5, 50, 0, 2, 8, 128, 62, 44, 3]);

        output.clear();
        VescReadOnlyCodec::encode_request(
            VescReadOnlyRequest::Stats(
                VescStatsMask::SPEED_AVG
                    | VescStatsMask::TEMP_MOTOR_MAX
                    | VescStatsMask::COUNT_TIME,
            ),
            &mut output,
        )
        .expect("stats request encodes");
        assert_eq!(output.as_slice(), &[2, 3, 128, 6, 1, 129, 221, 3]);
    }

    #[test]
    fn encodes_can_forwarding_only_for_nested_read_only_requests() {
        let mut output = ArrayVec::<u8, VESC_MAX_FRAME_LEN>::new();

        VescReadOnlyCodec::encode_request(
            VescReadOnlyRequest::ForwardCan {
                controller_id: VescControllerId::new(7),
                request: VescCanReadOnlyRequest::Values,
            },
            &mut output,
        )
        .expect("forwarded read-only values request encodes");

        assert_eq!(output.as_slice(), &[2, 3, 34, 7, 4, 49, 181, 3]);
    }

    #[test]
    fn decodes_firmware_info_into_owned_reply() {
        let input = [
            2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104,
            0, 38, 208, 3,
        ];

        let reply = VescReadOnlyCodec::decode_reply(&input).expect("firmware info decodes");

        assert_eq!(
            reply,
            VescReadOnlyReply::FirmwareInfo {
                major: 7,
                minor: 1,
                test_version_number: 2,
                commit_hash: bounded_string("abc123"),
                user_commit_hash: bounded_string("userhash"),
            }
        );
    }

    #[test]
    fn decodes_selective_values_into_owned_telemetry() {
        let input = [
            2, 23, 50, 0, 2, 161, 138, 0, 0, 0, 0, 0, 4, 0, 0, 3, 221, 1, 119, 255, 255, 170, 43,
            0, 20, 45, 58, 3,
        ];

        let reply = VescReadOnlyCodec::decode_reply(&input).expect("selective values decode");
        let VescReadOnlyReply::Values(telemetry) = reply else {
            panic!("expected values reply");
        };

        assert_eq!(telemetry.rpm, RotationalSpeed::from_erpm(989));
        assert_eq!(telemetry.voltage.as_millivolts(), 37_500);
        assert_eq!(telemetry.input_current.as_milliamps(), 40);
        assert_eq!(
            telemetry.tachometer,
            TachometerReading::from_counts(-21_973)
        );
        assert_eq!(telemetry.controller_id, VescControllerId::new(20));
        assert_eq!(telemetry.fault_code, VescFaultCode::None);
    }

    #[test]
    fn decodes_stats_into_owned_telemetry() {
        let input = [
            2, 49, 128, 0, 0, 7, 255, 63, 128, 0, 0, 64, 0, 0, 0, 64, 64, 0, 0, 64, 128, 0, 0, 64,
            160, 0, 0, 64, 192, 0, 0, 64, 224, 0, 0, 65, 0, 0, 0, 65, 16, 0, 0, 65, 32, 0, 0, 65,
            48, 0, 0, 213, 206, 3,
        ];

        let reply = VescReadOnlyCodec::decode_reply(&input).expect("stats decode");
        let VescReadOnlyReply::Stats(stats) = reply else {
            panic!("expected stats reply");
        };

        assert_eq!(stats.speed_avg.as_millimetres_per_second(), 1_000);
        assert_eq!(stats.speed_max.as_millimetres_per_second(), 2_000);
        assert_eq!(stats.power_avg.as_milliwatts(), 3_000);
        assert_eq!(stats.power_max.as_milliwatts(), 4_000);
        assert_eq!(stats.current_avg.as_milliamps(), 5_000);
        assert_eq!(stats.peak_current.as_milliamps(), 6_000);
        assert_eq!(stats.count_time, Duration::from_seconds(11));
    }

    #[test]
    fn decodes_long_motor_config_into_owned_board_facts() {
        let frame = motor_config_frame(30, 1.0, 0.280, 20);

        let reply = VescReadOnlyCodec::decode_reply(&frame).expect("motor config decodes");
        let VescReadOnlyReply::MotorConfig(config) = reply else {
            panic!("expected motor config reply");
        };

        assert_eq!(config.battery_cells, SeriesCount::new(20));
        assert_eq!(config.battery_type, VescBatteryType::LiIon);
        assert_eq!(
            config.speed_geometry.motor_pole_pairs,
            MotorPolePairs::new(15)
        );
        assert_eq!(
            config.speed_geometry.gear_ratio_denominator,
            GearRatioDenominator::new(1)
        );
        assert_eq!(
            config.speed_geometry.wheel_circumference.as_millimetres(),
            880
        );
        assert_eq!(
            VescBoardProfile::from_motor_config(config)
                .battery_level_from_voltage(Voltage::from_millivolts(84_000)),
            Some(BatteryLevel::from_percent(100))
        );
    }

    #[test]
    fn decodes_short_motor_setup_config_into_owned_speed_geometry() {
        let frame = motor_setup_config_frame(30, 1.0, 0.280);

        let reply = VescReadOnlyCodec::decode_reply(&frame).expect("motor setup config decodes");
        let VescReadOnlyReply::MotorSetupConfig(geometry) = reply else {
            panic!("expected motor setup config reply");
        };

        assert_eq!(geometry.motor_pole_pairs, MotorPolePairs::new(15));
        assert_eq!(
            geometry.gear_ratio_denominator,
            GearRatioDenominator::new(1)
        );
        assert_eq!(geometry.wheel_circumference.as_millimetres(), 880);
    }

    #[test]
    fn stream_decoder_decodes_fragmented_long_motor_config_frame() {
        let frame = motor_config_frame(30, 1.0, 0.280, 20);
        let mut decoder = VescReadOnlyStreamDecoder::new();

        assert_eq!(
            decoder.feed_result(&frame[..200]).expect("first fragment"),
            VescReadOnlyStreamResult::Buffered
        );
        let replies = decoder
            .feed_result(&frame[200..])
            .expect("second fragment")
            .into_replies();

        assert!(matches!(
            replies.first(),
            Some(VescReadOnlyReply::MotorConfig(config))
                if config.battery_cells == SeriesCount::new(20)
        ));
    }

    #[test]
    fn vesc_board_profile_calculates_speed_from_erpm() {
        let profile = VescBoardProfile::new(
            MotorPolePairs::new(15),
            GearRatioDenominator::new(1),
            Distance::from_millimetres(2_100),
        );

        assert_eq!(
            profile.speed_from_erpm(RotationalSpeed::from_erpm(4_500)),
            Some(Speed::from_millimetres_per_second(10_500))
        );
    }

    #[test]
    fn vesc_board_profile_estimates_battery_level_from_voltage_curve() {
        let profile = VescBoardProfile::new(
            MotorPolePairs::new(15),
            GearRatioDenominator::new(1),
            Distance::from_millimetres(2_100),
        )
        .with_battery_profile(&SAMSUNG_50S_PROFILE, SeriesCount::new(20));

        assert_eq!(
            profile.battery_level_from_voltage(Voltage::from_millivolts(61_800)),
            Some(SAMSUNG_50S_PROFILE.estimate_level_from_pack_voltage(
                Voltage::from_millivolts(61_800),
                SeriesCount::new(20)
            ))
        );
    }

    #[test]
    fn vesc_board_profile_does_not_report_battery_current_by_default() {
        let profile = VescBoardProfile::new(
            MotorPolePairs::new(15),
            GearRatioDenominator::new(1),
            Distance::from_millimetres(2_100),
        );

        assert!(!profile.reports_battery_current);
    }

    #[test]
    fn vesc_board_profile_can_mark_battery_current_reported() {
        let profile = VescBoardProfile::new(
            MotorPolePairs::new(15),
            GearRatioDenominator::new(1),
            Distance::from_millimetres(2_100),
        )
        .with_reported_battery_current();

        assert!(profile.reports_battery_current);
    }

    #[test]
    fn vesc_board_profile_can_disable_calculated_speed() {
        let profile = VescBoardProfile::new(
            MotorPolePairs::new(15),
            GearRatioDenominator::new(1),
            Distance::from_millimetres(2_100),
        )
        .without_calculated_speed();

        assert_eq!(
            profile.speed_from_erpm(RotationalSpeed::from_erpm(4_500)),
            None
        );
    }

    #[test]
    fn vesc_board_profile_preserves_reverse_erpm_sign() {
        let profile = VescBoardProfile::new(
            MotorPolePairs::new(15),
            GearRatioDenominator::new(1),
            Distance::from_millimetres(2_100),
        );

        assert_eq!(
            profile.speed_from_erpm(RotationalSpeed::from_erpm(-4_500)),
            Some(Speed::from_millimetres_per_second(-10_500))
        );
    }

    #[test]
    fn vesc_board_profile_applies_gear_reduction() {
        let direct_drive = VescBoardProfile::new(
            MotorPolePairs::new(15),
            GearRatioDenominator::new(1),
            Distance::from_millimetres(2_100),
        );
        let geared = VescBoardProfile::new(
            MotorPolePairs::new(15),
            GearRatioDenominator::new(2),
            Distance::from_millimetres(2_100),
        );

        assert_eq!(
            direct_drive.speed_from_erpm(RotationalSpeed::from_erpm(4_500)),
            Some(Speed::from_millimetres_per_second(10_500))
        );
        assert_eq!(
            geared.speed_from_erpm(RotationalSpeed::from_erpm(4_500)),
            Some(Speed::from_millimetres_per_second(5_250))
        );
    }

    #[test]
    fn vesc_board_profile_refuses_zero_denominators() {
        assert_eq!(
            VescBoardProfile::new(
                MotorPolePairs::new(0),
                GearRatioDenominator::new(1),
                Distance::from_millimetres(2_100)
            )
            .speed_from_erpm(RotationalSpeed::from_erpm(4_500)),
            None
        );
        assert_eq!(
            VescBoardProfile::new(
                MotorPolePairs::new(15),
                GearRatioDenominator::new(0),
                Distance::from_millimetres(2_100)
            )
            .speed_from_erpm(RotationalSpeed::from_erpm(4_500)),
            None
        );
    }

    #[test]
    fn stream_decoder_decodes_whole_frame_like_complete_frame_decoder() {
        let frame = selective_values_frame();
        let expected = VescReadOnlyCodec::decode_reply(&frame).expect("fixture decodes");
        let mut decoder = VescReadOnlyStreamDecoder::new();

        let replies = decoder
            .feed_result(&frame)
            .expect("stream feed succeeds")
            .into_replies();

        assert_eq!(replies.as_slice(), &[expected]);
    }

    #[test]
    fn stream_decoder_reports_typed_buffered_result_for_partial_frame() {
        let mut decoder = VescReadOnlyStreamDecoder::new();

        assert_eq!(
            decoder.feed_result(&selective_values_frame()[..3]),
            Ok(VescReadOnlyStreamResult::Buffered)
        );
    }

    #[test]
    fn stream_decoder_reports_typed_replies_for_complete_frame() {
        let frame = selective_values_frame();
        let expected = VescReadOnlyCodec::decode_reply(&frame).expect("fixture decodes");
        let mut decoder = VescReadOnlyStreamDecoder::new();
        let mut expected_replies = ArrayVec::<VescReadOnlyReply, VESC_MAX_STREAM_REPLIES>::new();
        expected_replies
            .try_push(expected)
            .expect("fixture emits one reply");

        assert_eq!(
            decoder.feed_result(&frame),
            Ok(VescReadOnlyStreamResult::Replies(expected_replies))
        );
    }

    #[test]
    fn stream_decoder_decodes_one_byte_at_a_time_like_complete_frame_decoder() {
        let frame = selective_values_frame();
        let expected = VescReadOnlyCodec::decode_reply(&frame).expect("fixture decodes");
        let mut decoder = VescReadOnlyStreamDecoder::new();
        let mut replies = ArrayVec::<VescReadOnlyReply, VESC_MAX_STREAM_REPLIES>::new();

        for byte in frame {
            let feed_result = decoder
                .feed_result(&[byte])
                .expect("single-byte feed succeeds");
            if let VescReadOnlyStreamResult::Replies(feed_replies) = feed_result {
                for reply in feed_replies {
                    replies.try_push(reply).expect("fixture emits one reply");
                }
            }
        }

        assert_eq!(replies.as_slice(), &[expected]);
    }

    #[test]
    fn stream_decoder_decodes_arbitrary_chunks_and_multiple_replies() {
        let values_frame = selective_values_frame();
        let stats_frame = stats_frame();
        let expected_values =
            VescReadOnlyCodec::decode_reply(&values_frame).expect("values decode");
        let expected_stats = VescReadOnlyCodec::decode_reply(&stats_frame).expect("stats decode");
        let mut decoder = VescReadOnlyStreamDecoder::new();
        let mut input = ArrayVec::<u8, 128>::new();
        input
            .try_extend_from_slice(&values_frame)
            .expect("values fit");
        input
            .try_extend_from_slice(&stats_frame)
            .expect("stats fit");
        let mut replies = ArrayVec::<VescReadOnlyReply, VESC_MAX_STREAM_REPLIES>::new();

        for chunk in input.chunks(7) {
            let feed_result = decoder.feed_result(chunk).expect("chunk feed succeeds");
            if let VescReadOnlyStreamResult::Replies(feed_replies) = feed_result {
                for reply in feed_replies {
                    replies
                        .try_push(reply)
                        .expect("fixture emits bounded replies");
                }
            }
        }

        assert_eq!(replies.as_slice(), &[expected_values, expected_stats]);
    }

    #[test]
    fn stream_decoder_decodes_live_full_values_over_ble_uart_chunks() {
        let frame = live_full_values_frame();
        let expected =
            VescReadOnlyCodec::decode_reply(frame.as_slice()).expect("live values decode");
        let mut decoder = VescReadOnlyStreamDecoder::new();
        let mut replies = ArrayVec::<VescReadOnlyReply, VESC_MAX_STREAM_REPLIES>::new();

        for chunk in live_full_values_ble_uart_chunks() {
            let feed_result = decoder.feed_result(chunk).expect("chunk feed succeeds");
            if let VescReadOnlyStreamResult::Replies(feed_replies) = feed_result {
                for reply in feed_replies {
                    replies
                        .try_push(reply)
                        .expect("fixture emits bounded replies");
                }
            }
        }

        assert_eq!(replies.as_slice(), &[expected]);
    }

    fn selective_values_frame() -> [u8; 28] {
        [
            2, 23, 50, 0, 2, 161, 138, 0, 0, 0, 0, 0, 4, 0, 0, 3, 221, 1, 119, 255, 255, 170, 43,
            0, 20, 45, 58, 3,
        ]
    }

    fn stats_frame() -> [u8; 54] {
        [
            2, 49, 128, 0, 0, 7, 255, 63, 128, 0, 0, 64, 0, 0, 0, 64, 64, 0, 0, 64, 128, 0, 0, 64,
            160, 0, 0, 64, 192, 0, 0, 64, 224, 0, 0, 65, 0, 0, 0, 65, 16, 0, 0, 65, 32, 0, 0, 65,
            48, 0, 0, 213, 206, 3,
        ]
    }

    const LIVE_FULL_VALUES_CHUNK_0: [u8; 2] = hex_literal::hex!("024a");
    const LIVE_FULL_VALUES_CHUNK_1: [u8; 20] =
        hex_literal::hex!("04010b00ea000000000000000000000000000000");
    const LIVE_FULL_VALUES_CHUNK_2: [u8; 20] =
        hex_literal::hex!("00000000000000026b0000000000000000000000");
    const LIVE_FULL_VALUES_CHUNK_3: [u8; 20] =
        hex_literal::hex!("0000000000fffffffe00000004000036ee861700");
    const LIVE_FULL_VALUES_CHUNK_4: [u8; 14] = hex_literal::hex!("000000000000000007ffffffec00");
    const LIVE_FULL_VALUES_CHUNK_5: [u8; 3] = hex_literal::hex!("e3be03");

    fn live_full_values_ble_uart_chunks() -> [&'static [u8]; 6] {
        [
            &LIVE_FULL_VALUES_CHUNK_0,
            &LIVE_FULL_VALUES_CHUNK_1,
            &LIVE_FULL_VALUES_CHUNK_2,
            &LIVE_FULL_VALUES_CHUNK_3,
            &LIVE_FULL_VALUES_CHUNK_4,
            &LIVE_FULL_VALUES_CHUNK_5,
        ]
    }

    fn live_full_values_frame() -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut frame = ArrayVec::new();
        for chunk in live_full_values_ble_uart_chunks() {
            frame
                .try_extend_from_slice(chunk)
                .expect("fixture frame fits");
        }
        frame
    }

    fn motor_config_frame(
        motor_poles: u8,
        gear_ratio: f32,
        wheel_diameter_metres: f32,
        battery_cells: u8,
    ) -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut payload = ArrayVec::<u8, VESC_MAX_FRAME_LEN>::new();
        payload.push(VESC_COMM_GET_MCCONF);
        push_bytes(&mut payload, &VESC_MCCONF_SIGNATURE.to_be_bytes());
        pad_to(&mut payload, 1 + VESC_MCCONF_SETUP_OFFSET);
        append_setup_fields(&mut payload, motor_poles, gear_ratio, wheel_diameter_metres);
        payload.push(0);
        payload.push(battery_cells);
        frame_from_payload(&payload)
    }

    fn motor_setup_config_frame(
        motor_poles: u8,
        gear_ratio: f32,
        wheel_diameter_metres: f32,
    ) -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut payload = ArrayVec::<u8, VESC_MAX_FRAME_LEN>::new();
        payload.push(VESC_COMM_GET_MCCONF_TEMP);
        pad_to(&mut payload, 1 + VESC_MCCONF_TEMP_SETUP_OFFSET);
        append_setup_fields(&mut payload, motor_poles, gear_ratio, wheel_diameter_metres);
        frame_from_payload(&payload)
    }

    fn append_setup_fields(
        payload: &mut ArrayVec<u8, VESC_MAX_FRAME_LEN>,
        motor_poles: u8,
        gear_ratio: f32,
        wheel_diameter_metres: f32,
    ) {
        payload.push(motor_poles);
        push_bytes(payload, &gear_ratio.to_bits().to_be_bytes());
        push_bytes(payload, &wheel_diameter_metres.to_bits().to_be_bytes());
    }

    fn frame_from_payload(payload: &[u8]) -> ArrayVec<u8, VESC_MAX_FRAME_LEN> {
        let mut frame = ArrayVec::<u8, VESC_MAX_FRAME_LEN>::new();
        if payload.len() <= u8::MAX.into() {
            frame.push(VESC_FRAME_START_SHORT);
            frame.push(u8::try_from(payload.len()).expect("short payload length"));
        } else {
            frame.push(VESC_FRAME_START_LONG);
            push_bytes(
                &mut frame,
                &u16::try_from(payload.len())
                    .expect("long payload length")
                    .to_be_bytes(),
            );
        }
        push_bytes(&mut frame, payload);
        push_bytes(&mut frame, &vesc_crc16(payload).to_be_bytes());
        frame.push(VESC_FRAME_END);
        frame
    }

    fn pad_to(payload: &mut ArrayVec<u8, VESC_MAX_FRAME_LEN>, len: usize) {
        while payload.len() < len {
            payload.push(0);
        }
    }

    fn push_bytes(output: &mut ArrayVec<u8, VESC_MAX_FRAME_LEN>, bytes: &[u8]) {
        output
            .try_extend_from_slice(bytes)
            .expect("fixture bytes fit");
    }
}
