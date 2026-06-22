use arrayvec::{ArrayString, ArrayVec};
use thiserror::Error;

/// Maximum VESC UART frame length supported by the read-only adapter.
pub const VESC_MAX_FRAME_LEN: usize = 64;

/// Maximum firmware hash string length carried by the private VESC adapter.
pub const VESC_MAX_HASH_LEN: usize = 47;

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
        controller_id: u8,

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

    /// Input voltage in millivolts.
    pub voltage_mv: i32,

    /// Input current in milliamps.
    pub input_current_ma: i32,

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
    /// Average speed in milli-units reported by VESC.
    pub speed_avg_milli: i32,

    /// Maximum speed in milli-units reported by VESC.
    pub speed_max_milli: i32,

    /// Average power in milliwatts.
    pub power_avg_mw: i32,

    /// Maximum power in milliwatts.
    pub power_max_mw: i32,

    /// Average current in milliamps.
    pub current_avg_ma: i32,

    /// Maximum current in milliamps.
    pub current_max_ma: i32,

    /// Statistics accumulation time in milliseconds.
    pub count_time_ms: u32,
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
        match reply {
            vesc::CommandReply::FwInfo(info) => Ok(VescReadOnlyReply::FirmwareInfo {
                major: info.major,
                minor: info.minor,
                test_version_number: info.test_version_number,
                commit_hash: bounded_string(info.commit_hash().unwrap_or_default()),
                user_commit_hash: bounded_string(info.user_commit_hash().unwrap_or_default()),
            }),
            vesc::CommandReply::GetValues(values)
            | vesc::CommandReply::GetValuesSelective(values) => {
                Ok(VescReadOnlyReply::Values(values.into()))
            }
            vesc::CommandReply::GetStats(stats) => Ok(VescReadOnlyReply::Stats(stats.into())),
            vesc::CommandReply::FwVersion(_)
            | vesc::CommandReply::GetValuesSetupSelective(_)
            | vesc::CommandReply::ResetStats => Err(VescCodecError::UnsupportedReply),
        }
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
            vesc::encode(vesc::Command::ForwardCan(controller_id, &nested), frame)
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
            voltage_mv: round_f32_to_i32(values.voltage_in * 1_000.0),
            input_current_ma: round_f32_to_i32(values.avg_current_input * 1_000.0),
            tachometer: values.tachometer,
            controller_id: values.controller_id,
            fault_code: values.fault_code.into(),
        }
    }
}

impl From<vesc::Stats> for VescStatsTelemetry {
    fn from(stats: vesc::Stats) -> Self {
        Self {
            speed_avg_milli: round_f32_to_i32(stats.speed_avg * 1_000.0),
            speed_max_milli: round_f32_to_i32(stats.speed_max * 1_000.0),
            power_avg_mw: round_f32_to_i32(stats.power_avg * 1_000.0),
            power_max_mw: round_f32_to_i32(stats.power_max * 1_000.0),
            current_avg_ma: round_f32_to_i32(stats.current_avg * 1_000.0),
            current_max_ma: round_f32_to_i32(stats.current_max * 1_000.0),
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
                controller_id: 7,
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
        assert_eq!(telemetry.voltage_mv, 37_500);
        assert_eq!(telemetry.input_current_ma, 40);
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

        assert_eq!(stats.speed_avg_milli, 1_000);
        assert_eq!(stats.speed_max_milli, 2_000);
        assert_eq!(stats.power_avg_mw, 3_000);
        assert_eq!(stats.power_max_mw, 4_000);
        assert_eq!(stats.current_avg_ma, 5_000);
        assert_eq!(stats.current_max_ma, 6_000);
        assert_eq!(stats.count_time_ms, 11_000);
    }
}
