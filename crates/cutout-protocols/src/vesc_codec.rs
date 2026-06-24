use arrayvec::{ArrayString, ArrayVec};
use cutout_core::VescControllerId;
use cutout_core::{BatteryCurrent, Power, Speed, Voltage};
use thiserror::Error;

/// Maximum VESC UART frame length supported by the read-only adapter.
pub const VESC_MAX_FRAME_LEN: usize = 64;

/// Maximum firmware hash string length carried by the private VESC adapter.
pub const VESC_MAX_HASH_LEN: usize = 47;

/// Maximum replies returned from one VESC stream feed.
pub const VESC_MAX_STREAM_REPLIES: usize = 4;

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
}

/// Owned VESC values telemetry subset used by the generic read-only session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescValuesTelemetry {
    /// Raw electrical RPM.
    pub rpm_erpm: i32,

    /// Input voltage.
    pub voltage_mv: Voltage,

    /// Input current.
    pub input_current_ma: BatteryCurrent,

    /// Relative tachometer.
    pub tachometer: i32,

    /// Controller identifier.
    pub controller_id: u8,

    /// Current VESC fault code.
    pub fault_code: VescFaultCode,
}

/// Owned VESC statistics telemetry subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescStatsTelemetry {
    /// Average speed.
    pub speed_avg: Speed,

    /// Maximum speed.
    pub speed_max: Speed,

    /// Average power.
    pub power_avg_mw: Power,

    /// Maximum power.
    pub power_max_mw: Power,

    /// Average current.
    pub current_avg_ma: BatteryCurrent,

    /// Maximum current.
    pub current_max_ma: BatteryCurrent,

    /// Statistics accumulation time in milliseconds.
    pub count_time_ms: u32,
}

/// Verified VESC board geometry used to calculate road speed from eRPM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescBoardProfile {
    /// Motor pole pairs used to convert electrical RPM to mechanical RPM.
    pub motor_pole_pairs: u8,

    /// Mechanical gear reduction denominator.
    pub gear_ratio_denominator: u8,

    /// Wheel circumference in millimeters.
    pub wheel_circumference_mm: u16,
}

impl VescBoardProfile {
    /// Creates a board profile from explicit geometry.
    #[must_use]
    pub const fn new(
        motor_pole_pairs: u8,
        gear_ratio_denominator: u8,
        wheel_circumference_mm: u16,
    ) -> Self {
        Self {
            motor_pole_pairs,
            gear_ratio_denominator,
            wheel_circumference_mm,
        }
    }

    /// Calculates signed road speed in millimeters per second from eRPM.
    #[must_use]
    pub const fn speed_mm_s_from_erpm(self, erpm: i32) -> Option<i32> {
        let denominator = self.motor_pole_pairs as i64 * self.gear_ratio_denominator as i64 * 60;
        if denominator == 0 {
            return None;
        }

        let numerator = erpm as i64 * self.wheel_circumference_mm as i64;
        Some(round_div_i64_to_i32(numerator, denominator))
    }
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
        let (_consumed, reply) =
            vesc::decode(bytes).map_err(|_err| VescCodecError::DecodeFailed)?;
        map_command_reply(reply)
    }
}

/// Bounded streaming VESC decoder with libcutout-owned output types.
#[derive(Debug)]
pub struct VescReadOnlyStreamDecoder {
    inner: vesc::Decoder<VESC_MAX_FRAME_LEN>,
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
            inner: vesc::Decoder::new(),
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
        let consumed = self
            .inner
            .feed(bytes)
            .map_err(|_err| VescCodecError::DecodeFailed)?;
        if consumed != bytes.len() {
            return Err(VescCodecError::DecodeFailed);
        }

        let mut replies = ArrayVec::new();
        for reply in self.inner.by_ref() {
            replies
                .try_push(map_command_reply(reply)?)
                .map_err(|_reply| VescCodecError::DecodeFailed)?;
        }
        Ok(if replies.is_empty() {
            VescReadOnlyStreamResult::Buffered
        } else {
            VescReadOnlyStreamResult::Replies(replies)
        })
    }
}

fn map_command_reply(reply: vesc::CommandReply) -> Result<VescReadOnlyReply, VescCodecError> {
    match reply {
        vesc::CommandReply::FwInfo(info) => Ok(VescReadOnlyReply::FirmwareInfo {
            major: info.major,
            minor: info.minor,
            test_version_number: info.test_version_number,
            commit_hash: bounded_string(info.commit_hash().unwrap_or_default()),
            user_commit_hash: bounded_string(info.user_commit_hash().unwrap_or_default()),
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
            rpm_erpm: round_f32_to_i32(values.rpm),
            voltage_mv: Voltage::from_millivolts(round_f32_to_i32(values.voltage_in * 1_000.0)),
            input_current_ma: BatteryCurrent::from_milliamps(round_f32_to_i32(
                values.avg_current_input * 1_000.0,
            )),
            tachometer: values.tachometer,
            controller_id: values.controller_id,
            fault_code: values.fault_code.into(),
        }
    }
}

impl From<vesc::Stats> for VescStatsTelemetry {
    fn from(stats: vesc::Stats) -> Self {
        Self {
            speed_avg: Speed::from_millimetres_per_second(round_f32_to_i32(
                stats.speed_avg * 1_000.0,
            )),
            speed_max: Speed::from_millimetres_per_second(round_f32_to_i32(
                stats.speed_max * 1_000.0,
            )),
            power_avg_mw: Power::from_milliwatts(i64::from(round_f32_to_i32(
                stats.power_avg * 1_000.0,
            ))),
            power_max_mw: Power::from_milliwatts(i64::from(round_f32_to_i32(
                stats.power_max * 1_000.0,
            ))),
            current_avg_ma: BatteryCurrent::from_milliamps(round_f32_to_i32(
                stats.current_avg * 1_000.0,
            )),
            current_max_ma: BatteryCurrent::from_milliamps(round_f32_to_i32(
                stats.current_max * 1_000.0,
            )),
            count_time_ms: round_f32_to_u32(stats.count_time * 1_000.0),
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn round_f32_to_i32(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn round_f32_to_u32(value: f32) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(0.0, u32::MAX as f32) as u32
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

#[allow(clippy::cast_possible_truncation)]
const fn round_div_i64_to_i32(numerator: i64, denominator: i64) -> i32 {
    let half = denominator / 2;
    let rounded = if numerator >= 0 {
        (numerator + half) / denominator
    } else {
        (numerator - half) / denominator
    };
    if rounded > i32::MAX as i64 {
        i32::MAX
    } else if rounded < i32::MIN as i64 {
        i32::MIN
    } else {
        rounded as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(telemetry.rpm_erpm, 989);
        assert_eq!(telemetry.voltage_mv.as_millivolts(), 37_500);
        assert_eq!(telemetry.input_current_ma.as_milliamps(), 40);
        assert_eq!(telemetry.tachometer, -21_973);
        assert_eq!(telemetry.controller_id, 20);
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
        assert_eq!(stats.power_avg_mw.as_milliwatts(), 3_000);
        assert_eq!(stats.power_max_mw.as_milliwatts(), 4_000);
        assert_eq!(stats.current_avg_ma.as_milliamps(), 5_000);
        assert_eq!(stats.current_max_ma.as_milliamps(), 6_000);
        assert_eq!(stats.count_time_ms, 11_000);
    }

    #[test]
    fn vesc_board_profile_calculates_speed_from_erpm() {
        let profile = VescBoardProfile::new(15, 1, 2_100);

        assert_eq!(profile.speed_mm_s_from_erpm(4_500), Some(10_500));
    }

    #[test]
    fn vesc_board_profile_preserves_reverse_erpm_sign() {
        let profile = VescBoardProfile::new(15, 1, 2_100);

        assert_eq!(profile.speed_mm_s_from_erpm(-4_500), Some(-10_500));
    }

    #[test]
    fn vesc_board_profile_applies_gear_reduction() {
        let direct_drive = VescBoardProfile::new(15, 1, 2_100);
        let geared = VescBoardProfile::new(15, 2, 2_100);

        assert_eq!(direct_drive.speed_mm_s_from_erpm(4_500), Some(10_500));
        assert_eq!(geared.speed_mm_s_from_erpm(4_500), Some(5_250));
    }

    #[test]
    fn vesc_board_profile_refuses_zero_denominators() {
        assert_eq!(
            VescBoardProfile::new(0, 1, 2_100).speed_mm_s_from_erpm(4_500),
            None
        );
        assert_eq!(
            VescBoardProfile::new(15, 0, 2_100).speed_mm_s_from_erpm(4_500),
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
}
