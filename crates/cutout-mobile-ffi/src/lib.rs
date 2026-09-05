//! Concrete `UniFFI` mobile binding surface for Cutout.

use std::{
    collections::VecDeque,
    convert::TryFrom,
    fmt,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use cutout_core::{
    AccelerationAssistState as CoreAccelerationAssistState, AccelerationAssistStateDto,
    ActivityProjectionState as CoreActivityProjectionState,
    AeroAngleAdjustment as CoreAeroAngleAdjustment, AeroPwmPercent as CoreAeroPwmPercent,
    AeroSpeedSetting as CoreAeroSpeedSetting, AngleReadingDto,
    BatteryCurrent as CoreBatteryCurrent, BatteryCurrentReadingDto, BatteryInfoDto,
    BatteryLevel as CoreBatteryLevel, BatteryLevelBasis, BatteryLevelReadingDto,
    BatteryPageKindDto, BatteryReadbackAvailabilityDto, BatteryReadbackDto,
    BegodeBeeperVolume as CoreBegodeBeeperVolume, BegodeLedModeSetting as CoreBegodeLedModeSetting,
    BegodeMaxSpeed as CoreBegodeMaxSpeed, BluetoothServiceUuid as CoreBluetoothServiceUuid,
    Capacity, ChargeEstimateError, ChargeEstimateInput, ChargeEstimateResetReason,
    ChargeEstimateState, ChargeEstimateUnavailableReason, ChargeFlow, ChargeMode, ChargeModeDto,
    ChargeModeReadingDto, ChargeProfileIdentity, ChargeSessionIdentity, ChargeTimeEstimate,
    CommandKindDto, ControlRefusalReason as CoreControlRefusalReason, ControlRefusalReasonDto,
    CutoutSessionState, DeviceCommandDto, DiscoveryCandidateSnapshot,
    DiscoveryCandidateSupport as CoreDiscoveryCandidateSupport,
    DiscoveryConnectionRoute as CoreDiscoveryConnectionRoute,
    DiscoveryElectricUnicycleModel as CoreDiscoveryElectricUnicycleModel,
    DiscoveryManufacturerDataSummary as CoreDiscoveryManufacturerDataSummary,
    DiscoveryObservation as CoreDiscoveryObservation, DistanceReadingDto, Duration as CoreDuration,
    DutyCycleReadingDto, EffectiveResistance, FaultCode, FaultCodeDto, FaultHistoryAvailability,
    FaultHistoryAvailabilityDto, FaultHistoryEntry, FaultHistoryEntryDto, FaultHistoryReadback,
    FaultHistoryReadbackDto, FootpadContactStateDto, FootpadTelemetryDto, GattChannel,
    GattFingerprint, GattRoles, IgnoredNotificationEvidenceDto, IgnoredNotificationReasonDto,
    LightState as CoreLightState, LightStateDto, Measured, MonotonicMillisDto, MonotonicTimestamp,
    NotificationByteLenDto, NotificationEvidenceDto, NotificationIngestOutcomeDto,
    ParserDiagnosticCountDto, ParserDiagnosticsDto, ParserDroppedBytesDto, ParserErrorDto,
    ParserFrameLenDto, ParserGapEvidenceDto, PayloadBodyLenDto, PedalMode as CorePedalMode,
    PevcapEncoding as CorePevcapEncoding, PevcapHeader, PevcapLocationSample, PevcapPhoneLocation,
    PevcapRecord, PevcapResolvedIdentity, PhaseCurrentReadingDto, PowerReadingDto, ProtocolFamily,
    ProtocolFamilyDto, ProtocolTag, RIDE_SESSION_STALE_AFTER, RawFieldValue, RawFieldValueDto,
    RawTelemetryReadback, RawTelemetryReadbackDto, ReadOnlyOutputPayload,
    ReservedPayloadEvidenceDto, RideOperatingModeDto, RideOperatingStateDto,
    RideSessionAppPresence as CoreRideSessionAppPresence,
    RideSessionDecision as CoreRideSessionDecision, RideSessionEffect as CoreRideSessionEffect,
    RideSessionEndReason as CoreRideSessionEndReason,
    RideSessionIdentity as CoreRideSessionIdentity, RideSessionInput as CoreRideSessionInput,
    RideSessionLifecycle as CoreRideSessionLifecycle, RideSessionMarker as CoreRideSessionMarker,
    RideSessionMarkerError as CoreRideSessionMarkerError, RideSessionPhase as CoreRideSessionPhase,
    RideStopReasonDto, RideWarningDto, RollAngle as CoreRollAngle,
    SETTING_WRITE_CONFIRMATION_TIMEOUT, SemanticEventCountDto, SeriesCount, SessionInputDto,
    SessionOutputDto, SettingState as CoreSettingState,
    SettingValueSource as CoreSettingValueSource, SettingsEntry, SettingsEntryDto,
    SettingsReadback, SettingsReadbackAvailability, SettingsReadbackAvailabilityDto,
    SettingsReadbackDto, Speed as CoreSpeed, SpeedAlarmMode as CoreSpeedAlarmMode, SpeedReadingDto,
    TelemetryFreshness, TelemetrySnapshotDto, TemperatureReadingDto, TransportActionDto,
    TransportWriteLimit, TransportWriteLimitDto, UsablePackCapacity, ValueQuality,
    ValueQuality as CoreValueQuality, ValueQualityDto, ValueSource, ValueSource as CoreValueSource,
    ValueSourceDto, VerificationStatus, VerificationStatusDto, VerifiedValue,
    Voltage as CoreVoltage, VoltageReadingDto, VoltageSagEstimate, VoltageSagEstimator,
    VoltageSagInput, VoltageSagModel, WallClockUnixTimestamp, WriteMode,
};
use cutout_protocols::{
    BEGODE_DATA_CHANNEL, BEGODE_FIELD_LED_AND_LIGHT_MODE, BEGODE_FIELD_SETTINGS_BITS,
    BEGODE_FIELD_TILTBACK_SPEED_KMH, ConcreteAeroBenignControlSession,
    ConcreteFalconBenignControlSession, ConcreteFalconProfileDto, ConcreteSessionErrorDto,
    ConcreteSessionStepResultDto, DeviceDetectionEvent, DeviceDetectionResolution,
    DeviceDetectionSession, DeviceFamily, IdentityBannerEvidence, PendingProbe,
    ProtocolFamilyClassification, ProtocolFamilyState, ProtocolModelIdentityEvidence,
    StagedIdentityInput, StagedIdentityOutcome, VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS,
    VETERAN_FIELD_CHARGE_MODE, VETERAN_FIELD_PEDALS_MODE, VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
    VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, VescBatteryType as CoreVescBatteryType,
    VescBoardProfile as CoreVescBoardProfile, VescReadOnlySession as CoreVescReadOnlySession,
    begode_identification_probes, identify_known_model, new_nosfet_aero_benign_control_session,
    try_new_begode_falcon_benign_control_session,
};
use cutout_ride_maps as ride_maps;
use libcutout_persistence as persistence;
use uuid::Uuid;

uniffi::setup_scaffolding!();

/// Mobile discovery candidate support state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum DiscoveryCandidateSupport {
    /// Candidate has a supported read-only route.
    Supported,

    /// Candidate has a read-only test route from provisional evidence.
    ProvisionalRoute,

    /// Candidate should be identified with a read-only probe before routing.
    ProbeRecommended,

    /// Candidate is relevant enough to record for future support.
    UnknownRecordable,

    /// Candidate category is known, but no route exists yet.
    KnownUnsupported,

    /// Candidate has multiple plausible identities or variants.
    Ambiguous,

    /// Candidate has contradictory identity evidence.
    Conflicting,

    /// Candidate is unrelated Bluetooth noise.
    RejectedNoise,

    /// Manual add / record placeholder until capture flow is available.
    ManualPlaceholder,

    /// Candidate is not supported for launch.
    Unsupported,
}

/// Mobile picker action recommended by Rust-owned discovery projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum DiscoveryCandidateAction {
    /// Use a routeable candidate.
    Use,

    /// Run a read-only identity probe before routing.
    Probe,

    /// Record a capture for unsupported or unknown devices.
    Record,

    /// Ask the user to confirm ambiguous identity evidence.
    Confirm,

    /// Ask the user to review conflicting identity evidence.
    Review,

    /// Placeholder action for future/manual flows.
    Later,
}

/// Mobile picker section recommended by Rust-owned discovery projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum DiscoveryCandidateSection {
    /// Routeable candidates.
    Supported,

    /// Candidates that should be probed before routing.
    ProbeFirst,

    /// Candidates that can only be recorded or reviewed.
    RecordOnly,

    /// Manual add / record placeholder.
    Manual,
}

impl DiscoveryCandidateSupport {
    fn recommended_action(self) -> DiscoveryCandidateAction {
        match self {
            Self::Supported | Self::ProvisionalRoute => DiscoveryCandidateAction::Use,
            Self::ProbeRecommended => DiscoveryCandidateAction::Probe,
            Self::UnknownRecordable
            | Self::KnownUnsupported
            | Self::RejectedNoise
            | Self::Unsupported => DiscoveryCandidateAction::Record,
            Self::Ambiguous => DiscoveryCandidateAction::Confirm,
            Self::Conflicting => DiscoveryCandidateAction::Review,
            Self::ManualPlaceholder => DiscoveryCandidateAction::Later,
        }
    }

    fn picker_section(self) -> DiscoveryCandidateSection {
        match self {
            Self::Supported | Self::ProvisionalRoute => DiscoveryCandidateSection::Supported,
            Self::ProbeRecommended => DiscoveryCandidateSection::ProbeFirst,
            Self::ManualPlaceholder => DiscoveryCandidateSection::Manual,
            Self::UnknownRecordable
            | Self::KnownUnsupported
            | Self::Ambiguous
            | Self::Conflicting
            | Self::RejectedNoise
            | Self::Unsupported => DiscoveryCandidateSection::RecordOnly,
        }
    }
}

/// Mobile EUC read-only session model.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum DiscoveryElectricUnicycleModel {
    /// NOSFET Aero session.
    Aero,

    /// Begode Falcon session.
    Falcon,
}

/// Mobile connection route for supported picker candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum DiscoveryConnectionRoute {
    /// Electric unicycle read-only session route.
    ElectricUnicycle,

    /// VESC/Onewheel read-only route.
    VescOnewheel,
}

/// Begode/Gotway protocol identity probe evidence.
#[derive(Clone, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct MobileBegodeIdentityProbeDto {
    /// Model text returned by the `N` probe when available.
    pub reported_model: Option<String>,

    /// Firmware or model-code text returned by the `V` probe when available.
    pub reported_code_name: Option<String>,

    /// IMU text returned by the `M` probe when available.
    pub reported_imu: Option<String>,

    /// Firmware version text returned by the wheel when available.
    pub reported_firmware_version: Option<String>,

    /// Stable device serial or other persistent identity text when available.
    pub reported_serial: Option<String>,

    /// Nominal voltage hint or pack-class observation, in millivolts.
    pub nominal_voltage_hint_mv: Option<u32>,

    /// Probe that was issued but did not produce a matching response.
    pub missing_probe_response: Option<MobilePendingProbeDto>,

    /// Probe that produced malformed identity evidence.
    pub malformed_probe_response: Option<MobilePendingProbeDto>,
}

/// Mobile discovery candidate for picker UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct DiscoveryCandidate {
    /// Platform-local peripheral identifier; not a Bluetooth MAC address.
    pub platform_identifier: String,

    /// Observed display name for UI use.
    pub display_name: String,

    /// Product category for grouping and copy.
    pub product_category: String,

    /// Human-readable evidence summary.
    pub evidence: String,

    /// Row detail or disabled reason.
    pub detail: String,

    /// Whether this advertisement is relevant to the mobile picker.
    pub is_picker_candidate: bool,

    /// Support state.
    pub support: DiscoveryCandidateSupport,

    /// Picker action recommended by Rust-owned projection.
    pub recommended_action: DiscoveryCandidateAction,

    /// Picker section recommended by Rust-owned projection.
    pub section: DiscoveryCandidateSection,

    /// Supported connection route, when connecting is allowed.
    pub connection_route: Option<DiscoveryConnectionRoute>,

    /// Electric-unicycle session model to construct for the route.
    pub electric_unicycle_model: Option<DiscoveryElectricUnicycleModel>,

    /// Disabled reason, when connecting is not allowed.
    pub disabled_reason: Option<String>,
}

/// Mobile advertised manufacturer data summary.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct DiscoveryManufacturerDataSummary {
    /// Bluetooth company identifier.
    pub company_identifier: u16,

    /// Opaque manufacturer payload length in bytes.
    pub payload_len: u64,
}

/// Normalized 128-bit Bluetooth service UUID supplied by the mobile BLE stack.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct DiscoveryServiceUuid {
    /// UUID bytes in network order.
    pub bytes: Vec<u8>,
}

/// Mobile discovery observation to feed into Rust-owned session state.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct DiscoveryObservation {
    /// Platform-local peripheral identifier; not a Bluetooth MAC address.
    pub platform_identifier: String,

    /// Raw advertised-name bytes from the mobile BLE stack.
    pub advertised_name: Option<Vec<u8>>,

    /// Normalized advertised service UUIDs used as protocol evidence.
    pub advertised_service_uuids: Vec<DiscoveryServiceUuid>,

    /// Manufacturer data summaries without opaque payload bytes.
    pub manufacturer_data: Vec<DiscoveryManufacturerDataSummary>,

    /// Last observed RSSI in dBm.
    pub rssi_dbm: Option<i16>,
}

/// Mobile discovery observation snapshot returned from Rust state.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct DiscoveryObservationSnapshot {
    /// Platform-local peripheral identifier; not a Bluetooth MAC address.
    pub platform_identifier: String,

    /// Raw advertised-name bytes retained by Rust state.
    pub advertised_name: Option<Vec<u8>>,

    /// UTF-8 advertised-name view, when valid.
    pub advertised_name_text: Option<String>,

    /// Normalized advertised service UUIDs used as protocol evidence.
    pub advertised_service_uuids: Vec<DiscoveryServiceUuid>,

    /// Manufacturer data summaries without opaque payload bytes.
    pub manufacturer_data: Vec<DiscoveryManufacturerDataSummary>,

    /// Last observed RSSI in dBm.
    pub rssi_dbm: Option<i16>,
}

/// Mobile discovery state snapshot returned from Rust state.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct DiscoverySnapshot {
    /// Retained discovery observations.
    pub observations: Vec<DiscoveryObservationSnapshot>,

    /// Picker candidates derived from retained Rust discovery evidence.
    pub picker_candidates: Vec<DiscoveryCandidate>,

    /// Platform identifier selected for the current mobile session.
    pub selected_platform_identifier: Option<String>,
}

/// Stable logical ride identity exposed to Apple-platform adapters.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideSessionIdentityDto {
    /// Platform-local device identifier.
    pub platform_identifier: String,
    /// Rust-created UUID for this logical ride.
    pub session_id: String,
}

/// Whether the app UI is currently foregrounded.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideSessionAppPresenceDto {
    /// App UI is foregrounded.
    Foreground,
    /// App UI is backgrounded or suspended.
    Background,
}

/// Terminal reason for a logical ride session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideSessionEndReasonDto {
    /// Rider explicitly disconnected the device.
    UserDisconnect,
    /// Rider explicitly stopped the ride.
    UserStop,
    /// Another ride replaced this one.
    ReplacedByNewSession,
    /// Reconnection attempts were exhausted.
    ReconnectExhausted,
    /// App explicitly reset its session.
    AppReset,
    /// The logical session cannot recover.
    UnrecoverableSessionFailure,
}

/// Current logical ride phase.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideSessionPhaseDto {
    /// No logical ride exists.
    Idle,
    /// Ride exists and its `ActivityKit` projection is starting.
    Starting,
    /// Ride is receiving current transport data.
    Active,
    /// Ride is waiting for transport reconnection.
    Reconnecting,
    /// Ride telemetry exceeded its freshness deadline.
    Stale,
    /// Ride is executing terminal effects.
    Ending {
        reason: MobileRideSessionEndReasonDto,
    },
    /// Ride completed terminal effects.
    Ended {
        reason: MobileRideSessionEndReasonDto,
    },
}

/// Current state of the `ActivityKit` projection.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileActivityProjectionStateDto {
    /// No activity exists.
    Absent,
    /// `ActivityKit` has been asked to start an activity.
    Starting,
    /// `ActivityKit` confirmed an active activity.
    Active { activity_id: String },
    /// The activity remains visible with stale content.
    Stale { activity_id: String },
    /// `ActivityKit` has been asked to end the activity.
    Ending,
    /// `ActivityKit` confirmed that the activity ended.
    Ended,
    /// `ActivityKit` cannot currently project the ride.
    Unavailable,
}

/// Immutable Rust-owned ride-session snapshot.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideSessionSnapshotDto {
    /// Logical ride identity, when a ride exists.
    pub identity: Option<MobileRideSessionIdentityDto>,
    /// Logical ride phase.
    pub phase: MobileRideSessionPhaseDto,
    /// Desired `ActivityKit` projection state.
    pub activity: MobileActivityProjectionStateDto,
    /// Most recent monotonic telemetry timestamp.
    pub last_telemetry_at_ms: Option<u64>,
    /// Rust-owned maximum telemetry age before the activity becomes stale.
    pub stale_after_ms: u64,
    /// Current app UI presence.
    pub app_presence: MobileRideSessionAppPresenceDto,
}

/// Typed Apple-platform event submitted to the Rust reducer.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideSessionInputDto {
    /// Starts a logical ride with a Rust-created UUID.
    Start { platform_identifier: String },
    /// `ActivityKit` confirmed a successful start or adoption.
    ActivityStarted {
        identity: MobileRideSessionIdentityDto,
        activity_id: String,
    },
    /// `ActivityKit` confirmed terminal end.
    ActivityEnded {
        identity: MobileRideSessionIdentityDto,
    },
    /// `ActivityKit` could not execute the requested projection.
    ActivityUnavailable {
        identity: MobileRideSessionIdentityDto,
    },
    /// App entered the background.
    AppBackgrounded,
    /// App returned to the foreground.
    AppForegrounded,
    /// Bluetooth transport disconnected without ending the ride.
    BluetoothDisconnected { at_ms: u64 },
    /// Bluetooth transport reconnected to the same ride.
    BluetoothConnected,
    /// Fresh telemetry was observed.
    TelemetryObserved { at_ms: u64 },
    /// Evaluate telemetry freshness against the Rust-owned deadline.
    FreshnessChecked { now_ms: u64 },
    /// Rider explicitly disconnected the device.
    UserDisconnected,
    /// Rider explicitly stopped the ride.
    UserStopped,
    /// The transport retry policy can no longer continue this logical ride.
    ReconnectExhausted,
    /// The app explicitly reset its logical ride session.
    AppReset,
    /// The logical ride cannot recover from a session failure.
    UnrecoverableSessionFailure,
}

/// One Apple-platform effect requested by the Rust reducer.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideSessionEffectDto {
    /// No platform work is required.
    None,
    /// Start or adopt an `ActivityKit` activity.
    StartActivity {
        identity: MobileRideSessionIdentityDto,
    },
    /// Update an existing `ActivityKit` activity.
    UpdateActivity {
        identity: MobileRideSessionIdentityDto,
    },
    /// Mark an existing `ActivityKit` activity stale.
    MarkActivityStale {
        identity: MobileRideSessionIdentityDto,
    },
    /// End an existing `ActivityKit` activity.
    EndActivity {
        identity: MobileRideSessionIdentityDto,
        reason: MobileRideSessionEndReasonDto,
    },
    /// Flush capture data without ending the ride.
    RequestCaptureFlush {
        identity: MobileRideSessionIdentityDto,
    },
}

/// Result of applying one ride-session input.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideSessionDecisionDto {
    /// Next immutable Rust-owned state.
    pub snapshot: MobileRideSessionSnapshotDto,
    /// At most one requested Apple-platform effect.
    pub effect: MobileRideSessionEffectDto,
}

/// Invalid data presented at the mobile ride-session boundary.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileRideSessionInputError {
    /// A callback did not carry a valid UUID returned by Rust.
    #[error("invalid ride session identifier")]
    InvalidSessionIdentifier,
}

/// Invalid persisted ride-session marker data presented by the mobile platform.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileRideSessionMarkerError {
    /// Bytes do not match the marker schema.
    #[error("invalid ride session marker encoding")]
    InvalidEncoding,
    /// The marker schema version is not supported by this build.
    #[error("unsupported ride session marker version")]
    UnsupportedVersion,
    /// The marker does not carry a valid logical ride identity.
    #[error("invalid ride session marker identity")]
    InvalidIdentity,
}

impl From<CoreRideSessionMarkerError> for MobileRideSessionMarkerError {
    fn from(error: CoreRideSessionMarkerError) -> Self {
        match error {
            CoreRideSessionMarkerError::InvalidEncoding => Self::InvalidEncoding,
            CoreRideSessionMarkerError::UnsupportedVersion => Self::UnsupportedVersion,
            CoreRideSessionMarkerError::InvalidIdentity => Self::InvalidIdentity,
        }
    }
}

impl From<&CoreRideSessionIdentity> for MobileRideSessionIdentityDto {
    fn from(identity: &CoreRideSessionIdentity) -> Self {
        Self {
            platform_identifier: identity.platform_identifier().to_owned(),
            session_id: identity.session_id().to_string(),
        }
    }
}

impl TryFrom<MobileRideSessionIdentityDto> for CoreRideSessionIdentity {
    type Error = MobileRideSessionInputError;

    fn try_from(identity: MobileRideSessionIdentityDto) -> Result<Self, Self::Error> {
        let session_id = Uuid::parse_str(&identity.session_id)
            .map_err(|_| MobileRideSessionInputError::InvalidSessionIdentifier)?;
        Ok(Self::new(identity.platform_identifier, session_id))
    }
}

impl From<CoreRideSessionAppPresence> for MobileRideSessionAppPresenceDto {
    fn from(presence: CoreRideSessionAppPresence) -> Self {
        match presence {
            CoreRideSessionAppPresence::Foreground => Self::Foreground,
            CoreRideSessionAppPresence::Background => Self::Background,
            _ => unreachable!("mobile FFI must map every core app-presence variant"),
        }
    }
}

impl From<CoreRideSessionEndReason> for MobileRideSessionEndReasonDto {
    fn from(reason: CoreRideSessionEndReason) -> Self {
        match reason {
            CoreRideSessionEndReason::UserDisconnect => Self::UserDisconnect,
            CoreRideSessionEndReason::UserStop => Self::UserStop,
            CoreRideSessionEndReason::ReplacedByNewSession => Self::ReplacedByNewSession,
            CoreRideSessionEndReason::ReconnectExhausted => Self::ReconnectExhausted,
            CoreRideSessionEndReason::AppReset => Self::AppReset,
            CoreRideSessionEndReason::UnrecoverableSessionFailure => {
                Self::UnrecoverableSessionFailure
            }
            _ => unreachable!("mobile FFI must map every core ride-end variant"),
        }
    }
}

impl From<&CoreRideSessionPhase> for MobileRideSessionPhaseDto {
    fn from(phase: &CoreRideSessionPhase) -> Self {
        match phase {
            CoreRideSessionPhase::Idle => Self::Idle,
            CoreRideSessionPhase::Starting => Self::Starting,
            CoreRideSessionPhase::Active => Self::Active,
            CoreRideSessionPhase::Reconnecting => Self::Reconnecting,
            CoreRideSessionPhase::Stale => Self::Stale,
            CoreRideSessionPhase::Ending(reason) => Self::Ending {
                reason: (*reason).into(),
            },
            CoreRideSessionPhase::Ended(reason) => Self::Ended {
                reason: (*reason).into(),
            },
            _ => unreachable!("mobile FFI must map every core ride-phase variant"),
        }
    }
}

impl From<&CoreActivityProjectionState> for MobileActivityProjectionStateDto {
    fn from(activity: &CoreActivityProjectionState) -> Self {
        match activity {
            CoreActivityProjectionState::Absent => Self::Absent,
            CoreActivityProjectionState::Starting => Self::Starting,
            CoreActivityProjectionState::Active { activity_id } => Self::Active {
                activity_id: activity_id.clone(),
            },
            CoreActivityProjectionState::Stale { activity_id } => Self::Stale {
                activity_id: activity_id.clone(),
            },
            CoreActivityProjectionState::Ending => Self::Ending,
            CoreActivityProjectionState::Ended => Self::Ended,
            CoreActivityProjectionState::Unavailable => Self::Unavailable,
            _ => unreachable!("mobile FFI must map every core activity variant"),
        }
    }
}

impl From<&CoreRideSessionLifecycle> for MobileRideSessionSnapshotDto {
    fn from(lifecycle: &CoreRideSessionLifecycle) -> Self {
        Self {
            identity: lifecycle.identity().map(MobileRideSessionIdentityDto::from),
            phase: lifecycle.phase().into(),
            activity: lifecycle.activity().into(),
            last_telemetry_at_ms: lifecycle
                .last_telemetry_at()
                .map(MonotonicTimestamp::as_milliseconds),
            stale_after_ms: RIDE_SESSION_STALE_AFTER.as_milliseconds(),
            app_presence: lifecycle.app_presence().into(),
        }
    }
}

impl TryFrom<MobileRideSessionInputDto> for CoreRideSessionInput {
    type Error = MobileRideSessionInputError;

    fn try_from(input: MobileRideSessionInputDto) -> Result<Self, Self::Error> {
        Ok(match input {
            MobileRideSessionInputDto::Start {
                platform_identifier,
            } => Self::Start {
                identity: CoreRideSessionIdentity::new_session(platform_identifier),
            },
            MobileRideSessionInputDto::ActivityStarted {
                identity,
                activity_id,
            } => Self::ActivityStarted {
                identity: identity.try_into()?,
                activity_id,
            },
            MobileRideSessionInputDto::ActivityEnded { identity } => Self::ActivityEnded {
                identity: identity.try_into()?,
            },
            MobileRideSessionInputDto::ActivityUnavailable { identity } => {
                Self::ActivityUnavailable {
                    identity: identity.try_into()?,
                }
            }
            MobileRideSessionInputDto::AppBackgrounded => Self::AppBackgrounded,
            MobileRideSessionInputDto::AppForegrounded => Self::AppForegrounded,
            MobileRideSessionInputDto::BluetoothDisconnected { at_ms } => {
                Self::BluetoothDisconnected {
                    at: MonotonicTimestamp::new(at_ms),
                }
            }
            MobileRideSessionInputDto::BluetoothConnected => Self::BluetoothConnected,
            MobileRideSessionInputDto::TelemetryObserved { at_ms } => Self::TelemetryObserved {
                at: MonotonicTimestamp::new(at_ms),
            },
            MobileRideSessionInputDto::FreshnessChecked { now_ms } => Self::FreshnessChecked {
                now: MonotonicTimestamp::new(now_ms),
            },
            MobileRideSessionInputDto::UserDisconnected => Self::UserDisconnected,
            MobileRideSessionInputDto::UserStopped => Self::UserStopped,
            MobileRideSessionInputDto::ReconnectExhausted => Self::ReconnectExhausted,
            MobileRideSessionInputDto::AppReset => Self::AppReset,
            MobileRideSessionInputDto::UnrecoverableSessionFailure => {
                Self::UnrecoverableSessionFailure
            }
        })
    }
}

impl From<CoreRideSessionEffect> for MobileRideSessionEffectDto {
    fn from(effect: CoreRideSessionEffect) -> Self {
        match effect {
            CoreRideSessionEffect::None => Self::None,
            CoreRideSessionEffect::StartActivity { identity } => Self::StartActivity {
                identity: (&identity).into(),
            },
            CoreRideSessionEffect::UpdateActivity { identity } => Self::UpdateActivity {
                identity: (&identity).into(),
            },
            CoreRideSessionEffect::MarkActivityStale { identity } => Self::MarkActivityStale {
                identity: (&identity).into(),
            },
            CoreRideSessionEffect::EndActivity { identity, reason } => Self::EndActivity {
                identity: (&identity).into(),
                reason: reason.into(),
            },
            CoreRideSessionEffect::RequestCaptureFlush { identity } => Self::RequestCaptureFlush {
                identity: (&identity).into(),
            },
            _ => unreachable!("mobile FFI must map every core ride effect"),
        }
    }
}

impl MobileRideSessionDecisionDto {
    fn from_core(decision: CoreRideSessionDecision) -> (CoreRideSessionLifecycle, Self) {
        let (state, effect) = decision.into_parts();
        let output = Self {
            snapshot: (&state).into(),
            effect: effect.into(),
        };
        (state, output)
    }
}

/// Mobile-facing Rust-owned `CutOut` session state handle.
#[derive(Debug, uniffi::Object)]
pub struct CutoutSessionStateHandle {
    inner: Mutex<MobileSessionState>,
}

#[derive(Debug, Default)]
struct MobileSessionState {
    state: CutoutSessionState,
    detector: DeviceDetectionSession,
}

impl DiscoveryObservation {
    fn into_core(self) -> CoreDiscoveryObservation {
        CoreDiscoveryObservation {
            platform_identifier: self.platform_identifier,
            advertised_name: self.advertised_name,
            advertised_service_uuids: self
                .advertised_service_uuids
                .into_iter()
                .filter_map(|uuid| CoreBluetoothServiceUuid::try_from(uuid.bytes).ok())
                .collect(),
            manufacturer_data: self
                .manufacturer_data
                .into_iter()
                .map(CoreDiscoveryManufacturerDataSummary::from)
                .collect(),
            rssi_dbm: self.rssi_dbm,
        }
    }
}

impl From<DiscoveryManufacturerDataSummary> for CoreDiscoveryManufacturerDataSummary {
    fn from(summary: DiscoveryManufacturerDataSummary) -> Self {
        Self {
            company_identifier: summary.company_identifier,
            payload_len: usize::try_from(summary.payload_len).unwrap_or(usize::MAX),
        }
    }
}

impl From<CoreDiscoveryManufacturerDataSummary> for DiscoveryManufacturerDataSummary {
    fn from(summary: CoreDiscoveryManufacturerDataSummary) -> Self {
        Self {
            company_identifier: summary.company_identifier,
            payload_len: summary.payload_len as u64,
        }
    }
}

impl From<&CoreDiscoveryObservation> for DiscoveryObservationSnapshot {
    fn from(observation: &CoreDiscoveryObservation) -> Self {
        Self {
            platform_identifier: observation.platform_identifier.clone(),
            advertised_name: observation.advertised_name.clone(),
            advertised_name_text: observation.advertised_name_text().map(str::to_owned),
            advertised_service_uuids: observation
                .advertised_service_uuids
                .iter()
                .map(|uuid| DiscoveryServiceUuid {
                    bytes: uuid.as_bytes().to_vec(),
                })
                .collect(),
            manufacturer_data: observation
                .manufacturer_data
                .iter()
                .copied()
                .map(DiscoveryManufacturerDataSummary::from)
                .collect(),
            rssi_dbm: observation.rssi_dbm,
        }
    }
}

impl DiscoverySnapshot {
    fn from_state(state: &CutoutSessionState) -> Self {
        let discovery = state.discovery();
        Self {
            observations: discovery
                .observations
                .iter()
                .map(DiscoveryObservationSnapshot::from)
                .collect(),
            picker_candidates: discovery
                .picker_candidates()
                .into_iter()
                .map(DiscoveryCandidate::from)
                .collect(),
            selected_platform_identifier: discovery.selected_platform_identifier.clone(),
        }
    }
}

impl From<DiscoveryCandidateSnapshot> for DiscoveryCandidate {
    fn from(candidate: DiscoveryCandidateSnapshot) -> Self {
        let support = DiscoveryCandidateSupport::from(candidate.support);
        let electric_unicycle_model = candidate
            .electric_unicycle_model
            .map(DiscoveryElectricUnicycleModel::from);
        Self {
            platform_identifier: candidate.platform_identifier,
            display_name: candidate.display_name,
            product_category: candidate.product_category,
            evidence: candidate.evidence,
            detail: candidate.detail.clone(),
            is_picker_candidate: true,
            support,
            recommended_action: support.recommended_action(),
            section: support.picker_section(),
            connection_route: candidate
                .connection_route
                .map(DiscoveryConnectionRoute::from),
            electric_unicycle_model,
            disabled_reason: match candidate.support {
                CoreDiscoveryCandidateSupport::Supported
                | CoreDiscoveryCandidateSupport::ProvisionalRoute => None,
                CoreDiscoveryCandidateSupport::ProbeRecommended
                | CoreDiscoveryCandidateSupport::UnknownRecordable => {
                    Some(candidate.detail.clone())
                }
                CoreDiscoveryCandidateSupport::KnownUnsupported => {
                    Some("Not yet supported".to_owned())
                }
                CoreDiscoveryCandidateSupport::Ambiguous => {
                    Some("Needs user confirmation".to_owned())
                }
                CoreDiscoveryCandidateSupport::Conflicting => {
                    Some("Conflicting identity evidence".to_owned())
                }
                CoreDiscoveryCandidateSupport::RejectedNoise => Some("Rejected noise".to_owned()),
                CoreDiscoveryCandidateSupport::ManualPlaceholder => {
                    Some("Capture flow later".to_owned())
                }
                CoreDiscoveryCandidateSupport::Unsupported => Some("Not yet supported".to_owned()),
            },
        }
    }
}

impl From<CoreDiscoveryElectricUnicycleModel> for DiscoveryElectricUnicycleModel {
    fn from(model: CoreDiscoveryElectricUnicycleModel) -> Self {
        match model {
            CoreDiscoveryElectricUnicycleModel::Aero => Self::Aero,
            CoreDiscoveryElectricUnicycleModel::Falcon => Self::Falcon,
        }
    }
}

impl From<CoreDiscoveryConnectionRoute> for DiscoveryConnectionRoute {
    fn from(route: CoreDiscoveryConnectionRoute) -> Self {
        match route {
            CoreDiscoveryConnectionRoute::ElectricUnicycle => Self::ElectricUnicycle,
            CoreDiscoveryConnectionRoute::VescOnewheel => Self::VescOnewheel,
        }
    }
}

impl From<CoreDiscoveryCandidateSupport> for DiscoveryCandidateSupport {
    fn from(support: CoreDiscoveryCandidateSupport) -> Self {
        match support {
            CoreDiscoveryCandidateSupport::Supported => Self::Supported,
            CoreDiscoveryCandidateSupport::ProvisionalRoute => Self::ProvisionalRoute,
            CoreDiscoveryCandidateSupport::ProbeRecommended => Self::ProbeRecommended,
            CoreDiscoveryCandidateSupport::UnknownRecordable => Self::UnknownRecordable,
            CoreDiscoveryCandidateSupport::KnownUnsupported => Self::KnownUnsupported,
            CoreDiscoveryCandidateSupport::Ambiguous => Self::Ambiguous,
            CoreDiscoveryCandidateSupport::Conflicting => Self::Conflicting,
            CoreDiscoveryCandidateSupport::RejectedNoise => Self::RejectedNoise,
            CoreDiscoveryCandidateSupport::ManualPlaceholder => Self::ManualPlaceholder,
            CoreDiscoveryCandidateSupport::Unsupported => Self::Unsupported,
        }
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Creates an empty Rust-owned session state handle.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(MobileSessionState::default()),
        })
    }

    /// Observes one mobile discovery advertisement.
    pub fn observe_discovery(&self, observation: DiscoveryObservation) -> DiscoverySnapshot {
        let mut state = self.lock_inner();
        state.state.observe_discovery(observation.into_core());
        DiscoverySnapshot::from_state(&state.state)
    }

    /// Selects a discovered platform identifier for this session.
    pub fn select_discovered_platform(&self, platform_identifier: String) -> DiscoverySnapshot {
        let mut state = self.lock_inner();
        state.state.select_discovered_platform(platform_identifier);
        DiscoverySnapshot::from_state(&state.state)
    }

    /// Returns the current discovery snapshot.
    #[must_use]
    pub fn discovery_snapshot(&self) -> DiscoverySnapshot {
        DiscoverySnapshot::from_state(&self.lock_inner().state)
    }

    /// Applies one typed Apple-platform event to the Rust-owned ride lifecycle.
    ///
    /// # Errors
    ///
    /// Returns [`MobileRideSessionInputError::InvalidSessionIdentifier`] when a callback carries
    /// a session identifier that was not produced by this Rust boundary.
    pub fn reduce_ride_session(
        &self,
        input: MobileRideSessionInputDto,
    ) -> Result<MobileRideSessionDecisionDto, MobileRideSessionInputError> {
        let input = input.try_into()?;
        let mut mobile = self.lock_inner();
        let decision = mobile.state.ride_session.transition(input);
        let (state, output) = MobileRideSessionDecisionDto::from_core(decision);
        mobile.state.ride_session = state;
        Ok(output)
    }

    /// Returns the opaque Rust-owned marker for a ride that can be reconciled after relaunch.
    ///
    /// # Errors
    ///
    /// Returns an error only when Rust cannot encode its own marker schema.
    pub fn export_ride_session_marker(
        &self,
    ) -> Result<Option<Vec<u8>>, MobileRideSessionMarkerError> {
        self.lock_inner()
            .state
            .ride_session
            .marker()
            .map(|marker| marker.encode().map_err(Into::into))
            .transpose()
    }

    /// Compares an opaque persisted marker with a platform identity without exposing marker
    /// parsing to Swift.
    ///
    /// # Errors
    ///
    /// Returns an error when the persisted bytes are invalid or unsupported.
    #[allow(clippy::needless_pass_by_value)] // UniFFI exports own boundary values.
    pub fn ride_session_marker_matches_platform_identifier(
        &self,
        marker: Vec<u8>,
        platform_identifier: String,
    ) -> Result<bool, MobileRideSessionMarkerError> {
        let marker = CoreRideSessionMarker::decode(&marker)?;
        Ok(marker.identity().platform_identifier() == platform_identifier)
    }

    /// Reconciles a persisted marker with the platform identity restored by `CoreBluetooth`.
    ///
    /// # Errors
    ///
    /// Returns an error when the persisted bytes are invalid or unsupported. Invalid bytes do not
    /// mutate the current Rust-owned session state.
    #[allow(clippy::needless_pass_by_value)] // UniFFI exports own boundary values.
    pub fn recover_ride_session_marker(
        &self,
        marker: Vec<u8>,
        restored_platform_identifier: Option<String>,
    ) -> Result<MobileRideSessionDecisionDto, MobileRideSessionMarkerError> {
        let marker = CoreRideSessionMarker::decode(&marker)?;
        let decision =
            CoreRideSessionLifecycle::recover(marker, restored_platform_identifier.as_deref());
        let (state, output) = MobileRideSessionDecisionDto::from_core(decision);
        self.lock_inner().state.ride_session = state;
        Ok(output)
    }

    /// Returns the current Rust-owned ride-session snapshot.
    #[must_use]
    pub fn ride_session_snapshot(&self) -> MobileRideSessionSnapshotDto {
        (&self.lock_inner().state.ride_session).into()
    }
}

impl CutoutSessionStateHandle {
    fn lock_inner(&self) -> MutexGuard<'_, MobileSessionState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Device-detection resolution exposed across the `UniFFI` boundary.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct DeviceDetectionResolutionRecord {
    /// Resolved protocol family, when known.
    pub protocol_family: Option<MobileProtocolFamilyDto>,

    /// Strong wire evidence reported incompatible protocol families.
    pub protocol_conflict: bool,

    /// Veteran/NOSFET protocol-native model id, when decoded.
    pub veteran_protocol_model_id: Option<u16>,

    /// Raw advertised-name bytes retained by the detector.
    pub advertised_name: Option<Vec<u8>>,

    /// Raw model-banner bytes retained by the detector.
    pub model_banner: Option<Vec<u8>>,

    /// Raw firmware-banner bytes retained by the detector.
    pub firmware_banner: Option<Vec<u8>>,

    /// Raw IMU-banner bytes retained by the detector.
    pub imu_banner: Option<Vec<u8>>,

    /// Probe that was issued but did not produce a matching response.
    pub missing_probe_response: Option<MobilePendingProbeDto>,

    /// Probe that produced malformed identity evidence.
    pub malformed_probe_response: Option<MobilePendingProbeDto>,
}

/// Pending probe state exposed across the `UniFFI` boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePendingProbeDto {
    /// Begode `N` probe awaiting a model/name response.
    BegodeName,

    /// Begode `V` probe awaiting a firmware response.
    BegodeFirmware,

    /// Begode `M` probe awaiting an IMU response.
    BegodeImu,
}

/// GATT write mode for an authorized identification query.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileIdentificationProbeWriteModeDto {
    /// Write without waiting for a GATT response acknowledgement.
    WithoutResponse,
}

/// One Rust-authorized, bounded identification query write.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileIdentificationProbeWriteDto {
    /// Characteristic UUID bytes.
    pub characteristic: Vec<u8>,

    /// Protocol-owned bounded payload bytes.
    pub payload: Vec<u8>,

    /// Required GATT write mode.
    pub mode: MobileIdentificationProbeWriteModeDto,
}

/// Result of requesting a non-mutating identification probe.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileIdentificationProbeOutcomeDto {
    /// The selected device is already identified and requires no query.
    NoProbeNeeded,

    /// The selected device does not support this identification exchange.
    Unsupported,

    /// Ordered authorized writes that the transport must execute.
    Writes {
        /// Bounded query writes.
        writes: Vec<MobileIdentificationProbeWriteDto>,
    },

    /// An earlier identification query is still awaiting a response.
    AlreadyPending,
}

impl MobileIdentificationProbeWriteDto {
    fn from_begode_probe(probe: &cutout_protocols::EncodedIdentificationProbe) -> Self {
        debug_assert_eq!(probe.mode, WriteMode::WithoutResponse);
        Self {
            characteristic: BEGODE_DATA_CHANNEL.as_bytes().to_vec(),
            payload: probe.payload.as_slice().to_vec(),
            mode: MobileIdentificationProbeWriteModeDto::WithoutResponse,
        }
    }

    #[cfg(test)]
    fn begode(payload: &[u8]) -> Self {
        Self {
            characteristic: BEGODE_DATA_CHANNEL.as_bytes().to_vec(),
            payload: payload.to_vec(),
            mode: MobileIdentificationProbeWriteModeDto::WithoutResponse,
        }
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Begins the complete ordered non-mutating identification query sequence.
    pub fn begin_identification_probe_at(
        &self,
        started_at_ms: u64,
    ) -> MobileIdentificationProbeOutcomeDto {
        let probes = begode_identification_probes();
        let mut state = self.lock_inner();
        let selected_identifier = state
            .state
            .discovery()
            .selected_platform_identifier
            .as_deref();
        let selected_candidate = selected_identifier.and_then(|identifier| {
            state
                .state
                .discovery()
                .picker_candidates()
                .into_iter()
                .find(|candidate| candidate.platform_identifier == identifier)
        });
        match selected_candidate {
            Some(candidate)
                if candidate.electric_unicycle_model
                    == Some(CoreDiscoveryElectricUnicycleModel::Aero) =>
            {
                return MobileIdentificationProbeOutcomeDto::NoProbeNeeded;
            }
            Some(candidate)
                if candidate.connection_route
                    == Some(CoreDiscoveryConnectionRoute::VescOnewheel) =>
            {
                return MobileIdentificationProbeOutcomeDto::Unsupported;
            }
            Some(candidate)
                if candidate.support == CoreDiscoveryCandidateSupport::ProbeRecommended
                    || candidate.electric_unicycle_model
                        == Some(CoreDiscoveryElectricUnicycleModel::Falcon) => {}
            Some(_) => {
                return MobileIdentificationProbeOutcomeDto::Unsupported;
            }
            None if selected_identifier.is_some() => {
                return MobileIdentificationProbeOutcomeDto::Unsupported;
            }
            None => {}
        }
        if state
            .detector
            .next_probe_expiry(&state.state, CoreDuration::from_milliseconds(0))
            .is_some()
        {
            return MobileIdentificationProbeOutcomeDto::AlreadyPending;
        }
        let MobileSessionState { state, detector } = &mut *state;
        for probe in &probes {
            let _ = detector.observe_probe_write_at(
                state,
                probe.probe,
                MonotonicTimestamp::new(started_at_ms),
            );
        }
        MobileIdentificationProbeOutcomeDto::Writes {
            writes: probes
                .iter()
                .map(MobileIdentificationProbeWriteDto::from_begode_probe)
                .collect(),
        }
    }

    /// Observes raw advertisement-name bytes from the mobile BLE stack.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI exports owned bytes")]
    pub fn observe_advertisement(&self, name: Option<Vec<u8>>) -> DeviceDetectionResolutionRecord {
        self.observe(DeviceDetectionEvent::Advertisement {
            name: name.as_deref(),
        })
    }

    /// Observes the current mobile GATT fingerprint snapshot.
    pub fn observe_gatt(
        &self,
        fingerprints: Vec<MobileGattFingerprintDto>,
    ) -> DeviceDetectionResolutionRecord {
        let fingerprints = fingerprints
            .into_iter()
            .map(GattFingerprint::from)
            .collect::<Vec<_>>();
        self.observe(DeviceDetectionEvent::Gatt {
            gatt: &fingerprints,
        })
    }

    /// Observes raw notification bytes from the mobile BLE stack.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI exports owned bytes")]
    pub fn observe_notification(&self, bytes: Vec<u8>) -> DeviceDetectionResolutionRecord {
        self.observe(DeviceDetectionEvent::Notification { bytes: &bytes })
    }

    /// Records that the caller issued a Begode `N` name probe.
    pub fn observe_begode_name_probe(&self) -> DeviceDetectionResolutionRecord {
        self.observe_begode_name_probe_at(0)
    }

    /// Records a Begode `N` name probe with its monotonic write time.
    pub fn observe_begode_name_probe_at(
        &self,
        started_at_ms: u64,
    ) -> DeviceDetectionResolutionRecord {
        self.observe_probe_write_at(PendingProbe::BegodeName, started_at_ms)
    }

    /// Records that the caller issued a Begode `V` firmware probe.
    pub fn observe_begode_firmware_probe(&self) -> DeviceDetectionResolutionRecord {
        self.observe_begode_firmware_probe_at(0)
    }

    /// Records a Begode `V` firmware probe with its monotonic write time.
    pub fn observe_begode_firmware_probe_at(
        &self,
        started_at_ms: u64,
    ) -> DeviceDetectionResolutionRecord {
        self.observe_probe_write_at(PendingProbe::BegodeFirmware, started_at_ms)
    }

    /// Records that the caller issued a Begode `M` IMU probe.
    pub fn observe_begode_imu_probe(&self) -> DeviceDetectionResolutionRecord {
        self.observe_begode_imu_probe_at(0)
    }

    /// Records a Begode `M` IMU probe with its monotonic write time.
    pub fn observe_begode_imu_probe_at(
        &self,
        started_at_ms: u64,
    ) -> DeviceDetectionResolutionRecord {
        self.observe_probe_write_at(PendingProbe::BegodeImu, started_at_ms)
    }

    /// Records that the Begode `N` name probe did not produce a matching response.
    pub fn observe_begode_name_probe_timeout(&self) -> DeviceDetectionResolutionRecord {
        self.observe(DeviceDetectionEvent::ProbeTimeout {
            probe: PendingProbe::BegodeName,
        })
    }

    /// Records that the Begode `V` firmware probe did not produce a matching response.
    pub fn observe_begode_firmware_probe_timeout(&self) -> DeviceDetectionResolutionRecord {
        self.observe(DeviceDetectionEvent::ProbeTimeout {
            probe: PendingProbe::BegodeFirmware,
        })
    }

    /// Records that the Begode `M` IMU probe did not produce a matching response.
    pub fn observe_begode_imu_probe_timeout(&self) -> DeviceDetectionResolutionRecord {
        self.observe(DeviceDetectionEvent::ProbeTimeout {
            probe: PendingProbe::BegodeImu,
        })
    }

    /// Expires pending Begode probes strictly older than the response timeout.
    pub fn expire_begode_probe_responses(
        &self,
        now_ms: u64,
        timeout_ms: u64,
    ) -> Vec<MobilePendingProbeDto> {
        let mut state = self.lock_inner();
        let MobileSessionState { state, detector } = &mut *state;
        detector
            .expire_pending_probes(
                state,
                MonotonicTimestamp::new(now_ms),
                CoreDuration::from_milliseconds(timeout_ms),
            )
            .into_iter()
            .map(MobilePendingProbeDto::from)
            .collect()
    }

    /// Marks every pending Begode probe as missing.
    pub fn mark_begode_probe_responses_missing(&self) -> Vec<MobilePendingProbeDto> {
        let mut state = self.lock_inner();
        let MobileSessionState { state, detector } = &mut *state;
        detector
            .mark_pending_probes_missing(state)
            .into_iter()
            .map(MobilePendingProbeDto::from)
            .collect()
    }

    /// Returns the next strict Begode probe-expiration deadline.
    pub fn next_begode_probe_expiry(&self, timeout_ms: u64) -> Option<u64> {
        let state = self.lock_inner();
        state
            .detector
            .next_probe_expiry(&state.state, CoreDuration::from_milliseconds(timeout_ms))
            .map(MonotonicTimestamp::as_milliseconds)
    }

    /// Returns the current detection resolution.
    pub fn resolution(&self) -> DeviceDetectionResolutionRecord {
        let state = self.lock_inner();
        state.detector.resolution(&state.state).into()
    }

    /// Clears device-specific detection state while preserving discovery observations.
    pub fn reset_device_detection(&self) {
        let mut state = self.lock_inner();
        state.state.reset_device_identity();
        state.detector = DeviceDetectionSession::default();
    }
}

impl CutoutSessionStateHandle {
    fn observe(&self, event: DeviceDetectionEvent<'_>) -> DeviceDetectionResolutionRecord {
        let mut state = self.lock_inner();
        let MobileSessionState { state, detector } = &mut *state;
        detector.observe(state, event).into()
    }

    fn observe_probe_write_at(
        &self,
        probe: PendingProbe,
        started_at_ms: u64,
    ) -> DeviceDetectionResolutionRecord {
        let mut state = self.lock_inner();
        let MobileSessionState { state, detector } = &mut *state;
        detector
            .observe_probe_write_at(state, probe, MonotonicTimestamp::new(started_at_ms))
            .into()
    }
}

/// Build a mobile discovery candidate from advertisement evidence.
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI exports owned sequences"
)]
#[uniffi::export]
pub fn mobile_discovery_candidate_from_advertisement(
    platform_identifier: String,
    local_name: Option<String>,
    advertised_service_uuids: Vec<DiscoveryServiceUuid>,
) -> DiscoveryCandidate {
    let display_name = local_name.unwrap_or_else(|| "Unknown Bluetooth device".to_owned());
    let advertised_service_uuids = advertised_service_uuids
        .into_iter()
        .filter_map(|uuid| CoreBluetoothServiceUuid::try_from(uuid.bytes).ok())
        .collect::<Vec<_>>();
    if advertised_service_uuids.contains(&CoreBluetoothServiceUuid::EUC_SERIAL_FFE0) {
        return DiscoveryCandidate {
            platform_identifier,
            display_name,
            product_category: "Electric unicycle".to_owned(),
            evidence: "FFE0/FFE1 transport hint".to_owned(),
            detail: "Read-only protocol probe recommended".to_owned(),
            is_picker_candidate: true,
            support: DiscoveryCandidateSupport::ProbeRecommended,
            recommended_action: DiscoveryCandidateSupport::ProbeRecommended.recommended_action(),
            section: DiscoveryCandidateSupport::ProbeRecommended.picker_section(),
            connection_route: None,
            electric_unicycle_model: None,
            disabled_reason: Some("Read-only protocol probe recommended".to_owned()),
        };
    }

    if advertised_service_uuids.iter().any(|uuid| {
        *uuid == CoreBluetoothServiceUuid::VESC_SERIAL_FFF0
            || *uuid == CoreBluetoothServiceUuid::VESC_NORDIC_UART
    }) {
        return DiscoveryCandidate {
            platform_identifier,
            display_name,
            product_category: "VESC Onewheel".to_owned(),
            evidence: "FFF0 transport hint".to_owned(),
            detail: "VESC protocol route".to_owned(),
            is_picker_candidate: true,
            support: DiscoveryCandidateSupport::ProvisionalRoute,
            recommended_action: DiscoveryCandidateSupport::ProvisionalRoute.recommended_action(),
            section: DiscoveryCandidateSupport::ProvisionalRoute.picker_section(),
            connection_route: Some(DiscoveryConnectionRoute::VescOnewheel),
            electric_unicycle_model: None,
            disabled_reason: None,
        };
    }

    DiscoveryCandidate {
        platform_identifier,
        display_name,
        product_category: "Unknown rideable".to_owned(),
        evidence: "advertisement observed".to_owned(),
        detail: "Not yet supported".to_owned(),
        is_picker_candidate: false,
        support: DiscoveryCandidateSupport::RejectedNoise,
        recommended_action: DiscoveryCandidateSupport::RejectedNoise.recommended_action(),
        section: DiscoveryCandidateSupport::RejectedNoise.picker_section(),
        connection_route: None,
        electric_unicycle_model: None,
        disabled_reason: Some("Rejected noise".to_owned()),
    }
}

/// Manual picker placeholder for future record/capture flow.
#[uniffi::export]
#[must_use]
pub fn mobile_manual_discovery_candidate() -> DiscoveryCandidate {
    DiscoveryCandidate {
        platform_identifier: "manual-add".to_owned(),
        display_name: "Manual add / record unknown device".to_owned(),
        product_category: "Unknown rideable".to_owned(),
        evidence: "Manual placeholder".to_owned(),
        detail: "Capture flow later".to_owned(),
        is_picker_candidate: true,
        support: DiscoveryCandidateSupport::ManualPlaceholder,
        recommended_action: DiscoveryCandidateSupport::ManualPlaceholder.recommended_action(),
        section: DiscoveryCandidateSupport::ManualPlaceholder.picker_section(),
        connection_route: None,
        electric_unicycle_model: None,
        disabled_reason: Some("Capture flow later".to_owned()),
    }
}

/// Canonicalizes one discovered device display name without opening the database.
///
/// Empty and identifier-equal names are returned as absent.
///
/// # Errors
///
/// Returns a typed database error when an input exceeds the stored text bound.
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns boundary strings"
)]
pub fn normalize_device_display_name(
    platform_identifier: String,
    display_name: String,
) -> Result<Option<String>, MobileRideDatabaseError> {
    persistence::normalize_device_display_name(&platform_identifier, &display_name)
        .map_err(map_ride_database_error)
}

/// Ambiguous picker candidate that requires user confirmation before routing.
#[uniffi::export]
#[must_use]
pub fn mobile_ambiguous_discovery_candidate(
    platform_identifier: String,
    display_name: String,
    detail: String,
) -> DiscoveryCandidate {
    DiscoveryCandidate {
        platform_identifier,
        display_name,
        product_category: "Electric unicycle".to_owned(),
        evidence: "Ambiguous identity evidence".to_owned(),
        detail,
        is_picker_candidate: true,
        support: DiscoveryCandidateSupport::Ambiguous,
        recommended_action: DiscoveryCandidateSupport::Ambiguous.recommended_action(),
        section: DiscoveryCandidateSupport::Ambiguous.picker_section(),
        connection_route: None,
        electric_unicycle_model: None,
        disabled_reason: Some("Needs user confirmation".to_owned()),
    }
}

/// Conflicting picker candidate that must not route automatically.
#[uniffi::export]
#[must_use]
pub fn mobile_conflicting_discovery_candidate(
    platform_identifier: String,
    display_name: String,
    detail: String,
) -> DiscoveryCandidate {
    DiscoveryCandidate {
        platform_identifier,
        display_name,
        product_category: "Electric unicycle".to_owned(),
        evidence: "Conflicting identity evidence".to_owned(),
        detail,
        is_picker_candidate: true,
        support: DiscoveryCandidateSupport::Conflicting,
        recommended_action: DiscoveryCandidateSupport::Conflicting.recommended_action(),
        section: DiscoveryCandidateSupport::Conflicting.picker_section(),
        connection_route: None,
        electric_unicycle_model: None,
        disabled_reason: Some("Conflicting identity evidence".to_owned()),
    }
}

/// Build a mobile discovery candidate from Begode/Gotway protocol identity evidence.
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "UniFFI exports owned records"
)]
#[uniffi::export]
pub fn mobile_discovery_candidate_from_begode_identity_probe(
    platform_identifier: String,
    display_name: String,
    probe: MobileBegodeIdentityProbeDto,
) -> DiscoveryCandidate {
    let evidence = mobile_begode_identity_probe_evidence(&probe);
    let reported_model = probe.reported_model.as_deref();
    let reported_code_name = probe.reported_code_name.as_deref();
    let missing_probe_response = probe.missing_probe_response;
    let malformed_probe_response = probe.malformed_probe_response;
    let support = mobile_begode_identity_probe_support(
        reported_model,
        reported_code_name,
        missing_probe_response,
        malformed_probe_response,
    );
    let supported = support == DiscoveryCandidateSupport::Supported;
    let detail = mobile_begode_identity_probe_detail(supported, &probe);

    DiscoveryCandidate {
        platform_identifier,
        display_name,
        product_category: "Electric unicycle".to_owned(),
        evidence,
        detail,
        is_picker_candidate: true,
        support,
        recommended_action: support.recommended_action(),
        section: support.picker_section(),
        connection_route: supported.then_some(DiscoveryConnectionRoute::ElectricUnicycle),
        electric_unicycle_model: supported.then_some(DiscoveryElectricUnicycleModel::Falcon),
        disabled_reason: mobile_begode_identity_probe_disabled_reason(
            support,
            reported_code_name,
            missing_probe_response,
            malformed_probe_response,
        ),
    }
}

/// Build a mobile discovery candidate from caller-owned detection output.
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "UniFFI exports owned records"
)]
#[uniffi::export]
pub fn mobile_discovery_candidate_from_detection_resolution(
    platform_identifier: String,
    display_name: String,
    resolution: DeviceDetectionResolutionRecord,
) -> DiscoveryCandidate {
    let has_detection_evidence = resolution.protocol_family.is_some()
        || resolution.protocol_conflict
        || resolution.veteran_protocol_model_id.is_some()
        || resolution.model_banner.is_some()
        || resolution.firmware_banner.is_some()
        || resolution.imu_banner.is_some()
        || resolution.missing_probe_response.is_some()
        || resolution.malformed_probe_response.is_some();

    if !has_detection_evidence {
        return DiscoveryCandidate {
            platform_identifier,
            display_name,
            product_category: "Unknown rideable".to_owned(),
            evidence: "No protocol identity evidence".to_owned(),
            detail: "No protocol identity evidence".to_owned(),
            is_picker_candidate: false,
            support: DiscoveryCandidateSupport::RejectedNoise,
            recommended_action: DiscoveryCandidateSupport::RejectedNoise.recommended_action(),
            section: DiscoveryCandidateSupport::RejectedNoise.picker_section(),
            connection_route: None,
            electric_unicycle_model: None,
            disabled_reason: Some("Rejected noise".to_owned()),
        };
    }

    if resolution.protocol_conflict {
        return DiscoveryCandidate {
            platform_identifier,
            display_name,
            product_category: "Electric unicycle".to_owned(),
            evidence: "Conflicting protocol family evidence".to_owned(),
            detail: "Conflicting protocol family evidence".to_owned(),
            is_picker_candidate: true,
            support: DiscoveryCandidateSupport::Conflicting,
            recommended_action: DiscoveryCandidateSupport::Conflicting.recommended_action(),
            section: DiscoveryCandidateSupport::Conflicting.picker_section(),
            connection_route: None,
            electric_unicycle_model: None,
            disabled_reason: Some("Conflicting identity evidence".to_owned()),
        };
    }

    if let Some(model_id) = resolution.veteran_protocol_model_id {
        return mobile_discovery_candidate_from_veteran_protocol_identity(
            platform_identifier,
            display_name,
            model_id,
        );
    }

    if matches!(
        resolution.protocol_family.as_ref(),
        Some(MobileProtocolFamilyDto::VeteranLeaperkimNosfet)
    ) {
        return DiscoveryCandidate {
            platform_identifier,
            display_name,
            product_category: "Electric unicycle".to_owned(),
            evidence: "Veteran/NOSFET protocol family".to_owned(),
            detail: "Veteran/NOSFET model not confirmed".to_owned(),
            is_picker_candidate: true,
            support: DiscoveryCandidateSupport::UnknownRecordable,
            recommended_action: DiscoveryCandidateSupport::UnknownRecordable.recommended_action(),
            section: DiscoveryCandidateSupport::UnknownRecordable.picker_section(),
            connection_route: None,
            electric_unicycle_model: None,
            disabled_reason: Some("Veteran/NOSFET model not confirmed".to_owned()),
        };
    }

    if matches!(
        resolution.protocol_family.as_ref(),
        Some(MobileProtocolFamilyDto::Vesc)
    ) {
        return DiscoveryCandidate {
            platform_identifier,
            display_name,
            product_category: "VESC Onewheel".to_owned(),
            evidence: "VESC protocol family".to_owned(),
            detail: "VESC read-only route".to_owned(),
            is_picker_candidate: true,
            support: DiscoveryCandidateSupport::ProvisionalRoute,
            recommended_action: DiscoveryCandidateSupport::ProvisionalRoute.recommended_action(),
            section: DiscoveryCandidateSupport::ProvisionalRoute.picker_section(),
            connection_route: Some(DiscoveryConnectionRoute::VescOnewheel),
            electric_unicycle_model: None,
            disabled_reason: None,
        };
    }

    mobile_discovery_candidate_from_begode_identity_probe(
        platform_identifier,
        display_name,
        MobileBegodeIdentityProbeDto {
            reported_model: match (
                resolution.missing_probe_response,
                resolution.malformed_probe_response,
            ) {
                (Some(MobilePendingProbeDto::BegodeName), _)
                | (_, Some(MobilePendingProbeDto::BegodeName)) => None,
                _ => resolution
                    .model_banner
                    .as_deref()
                    .and_then(|banner| std::str::from_utf8(banner).ok())
                    .map(ToOwned::to_owned),
            },
            reported_code_name: match (
                resolution.missing_probe_response,
                resolution.malformed_probe_response,
            ) {
                (Some(MobilePendingProbeDto::BegodeFirmware), _)
                | (_, Some(MobilePendingProbeDto::BegodeFirmware)) => None,
                _ => resolution
                    .firmware_banner
                    .as_deref()
                    .and_then(|banner| std::str::from_utf8(banner).ok())
                    .map(ToOwned::to_owned),
            },
            reported_imu: match (
                resolution.missing_probe_response,
                resolution.malformed_probe_response,
            ) {
                (Some(MobilePendingProbeDto::BegodeImu), _)
                | (_, Some(MobilePendingProbeDto::BegodeImu)) => None,
                _ => resolution
                    .imu_banner
                    .as_deref()
                    .and_then(|banner| std::str::from_utf8(banner).ok())
                    .map(ToOwned::to_owned),
            },
            reported_firmware_version: None,
            reported_serial: None,
            nominal_voltage_hint_mv: None,
            missing_probe_response: resolution.missing_probe_response,
            malformed_probe_response: resolution.malformed_probe_response,
        },
    )
}

/// Build a mobile discovery candidate from caller-owned Begode detection output.
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI exports owned records"
)]
#[uniffi::export]
pub fn mobile_discovery_candidate_from_begode_detection_resolution(
    platform_identifier: String,
    display_name: String,
    resolution: DeviceDetectionResolutionRecord,
) -> DiscoveryCandidate {
    mobile_discovery_candidate_from_detection_resolution(
        platform_identifier,
        display_name,
        resolution,
    )
}

/// Build a mobile discovery candidate from Veteran/NOSFET protocol identity.
#[must_use]
#[uniffi::export]
pub fn mobile_discovery_candidate_from_veteran_protocol_identity(
    platform_identifier: String,
    display_name: String,
    model_id: u16,
) -> DiscoveryCandidate {
    let resolution = identify_known_model(&StagedIdentityInput {
        advertised_name: None,
        gatt: &[] as &[GattFingerprint],
        stream_family: ProtocolFamilyClassification::Pending,
        banner_model: IdentityBannerEvidence::Missing,
        protocol_model: ProtocolModelIdentityEvidence::model_id(
            ProtocolFamily::VeteranLeaperkimNosfet,
            model_id,
        ),
    });

    let Some(model) = resolution.model else {
        return DiscoveryCandidate {
            platform_identifier,
            display_name,
            product_category: "Electric unicycle".to_owned(),
            evidence: "Veteran protocol model id".to_owned(),
            detail: format!("Unknown Veteran/NOSFET model id {model_id}"),
            is_picker_candidate: true,
            support: DiscoveryCandidateSupport::UnknownRecordable,
            recommended_action: DiscoveryCandidateSupport::UnknownRecordable.recommended_action(),
            section: DiscoveryCandidateSupport::UnknownRecordable.picker_section(),
            connection_route: None,
            electric_unicycle_model: None,
            disabled_reason: Some(format!("Unknown Veteran/NOSFET model id {model_id}")),
        };
    };

    let electric_unicycle_model = match (
        model.protocol_family,
        model.wire_model_id.map(|wire_model_id| wire_model_id.value),
    ) {
        (ProtocolFamily::VeteranLeaperkimNosfet, Some(43)) => {
            Some(DiscoveryElectricUnicycleModel::Aero)
        }
        _ => None,
    };
    let supported = electric_unicycle_model.is_some();
    let support = if supported {
        DiscoveryCandidateSupport::Supported
    } else {
        DiscoveryCandidateSupport::Unsupported
    };

    DiscoveryCandidate {
        platform_identifier,
        display_name,
        product_category: "Electric unicycle".to_owned(),
        evidence: "Veteran protocol model id".to_owned(),
        detail: match resolution.outcome {
            StagedIdentityOutcome::Matched => {
                format!("{} confirmed by model id {model_id}", model.model)
            }
            _ => format!("Veteran/NOSFET model id {model_id} confirmed"),
        },
        is_picker_candidate: true,
        support,
        recommended_action: support.recommended_action(),
        section: support.picker_section(),
        connection_route: supported.then_some(DiscoveryConnectionRoute::ElectricUnicycle),
        electric_unicycle_model,
        disabled_reason: (!supported).then(|| "Model not supported".to_owned()),
    }
}

fn mobile_begode_identity_probe_support(
    reported_model: Option<&str>,
    reported_code_name: Option<&str>,
    missing_probe_response: Option<MobilePendingProbeDto>,
    malformed_probe_response: Option<MobilePendingProbeDto>,
) -> DiscoveryCandidateSupport {
    let resolution = reported_model.map(|model| {
        identify_known_model(&StagedIdentityInput {
            advertised_name: None,
            gatt: &[] as &[GattFingerprint],
            stream_family: ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon),
            banner_model: IdentityBannerEvidence::model(model),
            protocol_model: ProtocolModelIdentityEvidence::Missing,
        })
    });

    match resolution.map(|resolution| resolution.outcome) {
        Some(StagedIdentityOutcome::Matched) => DiscoveryCandidateSupport::Supported,
        Some(StagedIdentityOutcome::Ambiguous) => DiscoveryCandidateSupport::Ambiguous,
        Some(StagedIdentityOutcome::Conflict) => DiscoveryCandidateSupport::Conflicting,
        Some(_) => DiscoveryCandidateSupport::Unsupported,
        None if reported_code_name.is_some() => DiscoveryCandidateSupport::UnknownRecordable,
        None if missing_probe_response.is_some() => DiscoveryCandidateSupport::UnknownRecordable,
        None if malformed_probe_response.is_some() => DiscoveryCandidateSupport::UnknownRecordable,
        None => DiscoveryCandidateSupport::UnknownRecordable,
    }
}

fn mobile_begode_identity_probe_disabled_reason(
    support: DiscoveryCandidateSupport,
    reported_code_name: Option<&str>,
    missing_probe_response: Option<MobilePendingProbeDto>,
    malformed_probe_response: Option<MobilePendingProbeDto>,
) -> Option<String> {
    match (
        support,
        reported_code_name,
        missing_probe_response,
        malformed_probe_response,
    ) {
        (DiscoveryCandidateSupport::Supported, _, _, _) => None,
        (DiscoveryCandidateSupport::Conflicting, _, _, _) => {
            Some("Conflicting identity evidence".to_owned())
        }
        (DiscoveryCandidateSupport::Ambiguous, _, _, _) => {
            Some("Needs user confirmation".to_owned())
        }
        (DiscoveryCandidateSupport::UnknownRecordable, Some(_), _, _) => {
            Some("Unresolved Begode code banner".to_owned())
        }
        (DiscoveryCandidateSupport::UnknownRecordable, _, Some(_), _) => {
            Some("Missing Begode probe response".to_owned())
        }
        (DiscoveryCandidateSupport::UnknownRecordable, _, _, Some(_)) => {
            Some("Malformed Begode probe response".to_owned())
        }
        (DiscoveryCandidateSupport::UnknownRecordable, None, None, None) => {
            Some("Begode model not confirmed".to_owned())
        }
        _ => Some("Not yet supported".to_owned()),
    }
}

fn mobile_begode_identity_probe_evidence(probe: &MobileBegodeIdentityProbeDto) -> String {
    let mut parts = Vec::new();
    if let Some(model) = probe.reported_model.as_deref() {
        parts.push(format!("model={model}"));
    }
    if let Some(code_name) = probe.reported_code_name.as_deref() {
        parts.push(format!("code={code_name}"));
    }
    if let Some(imu) = probe.reported_imu.as_deref() {
        parts.push(format!("imu={imu}"));
    }
    if let Some(firmware_version) = probe.reported_firmware_version.as_deref() {
        parts.push(format!("firmware={firmware_version}"));
    }
    if let Some(serial) = probe.reported_serial.as_deref() {
        parts.push(format!("serial={serial}"));
    }
    if let Some(voltage_hint_mv) = probe.nominal_voltage_hint_mv {
        parts.push(format!("voltage_hint={voltage_hint_mv}mV"));
    }
    if let Some(missing_probe_response) = probe.missing_probe_response {
        parts.push(format!("missing_probe_response={missing_probe_response:?}"));
    }
    if let Some(malformed_probe_response) = probe.malformed_probe_response {
        parts.push(format!(
            "malformed_probe_response={malformed_probe_response:?}"
        ));
    }
    if parts.is_empty() {
        "Begode protocol identity probe".to_owned()
    } else {
        parts.join(", ")
    }
}

fn mobile_begode_identity_probe_detail(
    supported: bool,
    probe: &MobileBegodeIdentityProbeDto,
) -> String {
    let mut parts = Vec::new();
    if let Some(model) = probe.reported_model.as_deref() {
        parts.push(format!("reported model {model}"));
    }
    if let Some(code_name) = probe.reported_code_name.as_deref() {
        parts.push(format!("code {code_name}"));
    }
    if let Some(imu) = probe.reported_imu.as_deref() {
        parts.push(format!("imu {imu}"));
    }
    if let Some(firmware_version) = probe.reported_firmware_version.as_deref() {
        parts.push(format!("firmware {firmware_version}"));
    }
    if let Some(serial) = probe.reported_serial.as_deref() {
        parts.push(format!("serial {serial}"));
    }
    if let Some(voltage_hint_mv) = probe.nominal_voltage_hint_mv {
        parts.push(format!("voltage hint {voltage_hint_mv}mV"));
    }
    if let Some(missing_probe_response) = probe.missing_probe_response {
        parts.push(format!("missing {missing_probe_response:?} response"));
    }
    if let Some(malformed_probe_response) = probe.malformed_probe_response {
        parts.push(format!("malformed {malformed_probe_response:?} response"));
    }
    if supported {
        if parts.is_empty() {
            "Begode/Falcon confirmed by protocol evidence".to_owned()
        } else {
            format!("Begode/Falcon confirmed by {}", parts.join(", "))
        }
    } else if parts.is_empty() {
        "Begode/GotWay identity probe collected; model not confirmed".to_owned()
    } else {
        format!(
            "Begode/GotWay identity probe collected; {}",
            parts.join(", ")
        )
    }
}

/// Mobile DTO command kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCommandDto {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request telemetry.
    RequestTelemetry,

    /// Request firmware information.
    RequestFirmwareInfo,

    /// Request battery information.
    RequestBatteryInfo,

    /// Request diagnostics.
    RequestDiagnostics,

    /// Request historical fault information.
    RequestFaultHistory,

    /// Request current settings without changing device state.
    RequestSettings,

    /// Reset the NOSFET Aero trip meter; stationary-only.
    ResetTripMeter,

    /// Set NOSFET/Veteran tilt-back speed in whole km/h.
    SetAeroTiltbackSpeed(MobileAeroSpeedSettingDto),

    /// Set NOSFET/Veteran PWM warning percentage.
    SetAeroPwmPercent(MobileAeroPwmPercentDto),

    /// Set NOSFET/Veteran speed alarm in whole km/h.
    SetAeroAlarmSpeed(MobileAeroSpeedSettingDto),

    /// Set NOSFET/Veteran pedal-zero angle adjustment in tenths of a degree.
    SetAeroAngleAdjustment(MobileAeroAngleAdjustmentDto),

    /// Set the documented NOSFET/Veteran high beam; stationary-only.
    SetAeroHighBeam(MobileLightStateDto),

    /// Set the device lights.
    SetLights(MobileLightStateDto),

    /// Set the documented pedal response; unverified models refuse this before transport.
    SetPedalMode(MobilePedalModeKindDto),

    /// Set the documented Falcon roll-angle sensitivity; stationary-only.
    SetRollAngle(MobileRollAngleKindDto),

    /// Set the documented Begode speed-alarm mode; stationary-only.
    SetSpeedAlarmMode(MobileSpeedAlarmModeKindDto),

    /// Set Begode max speed through the timed `W` submenu.
    SetBegodeMaxSpeed(MobileBegodeMaxSpeedDto),

    /// Set Begode beeper volume through the timed `W` submenu.
    SetBegodeBeeperVolume(MobileBegodeBeeperVolumeDto),

    /// Set Begode LED mode through the timed `W` submenu.
    SetBegodeLedMode(MobileBegodeLedModeDto),

    /// Enable or disable acceleration assist; unsupported models refuse this before transport.
    SetAccelerationAssist(MobileAccelerationAssistStateDto),

    /// Set the taillight independently; unsupported models refuse this before transport.
    SetTaillight(MobileLightStateDto),

    /// Sound a horn or alert.
    SoundHorn,
}

/// NOSFET/Veteran speed setting in whole km/h.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileAeroSpeedSettingDto {
    /// Whole kilometres per hour, in the wheel's 1..=99 range.
    pub kilometres_per_hour: u8,
}

impl From<CoreAeroSpeedSetting> for MobileAeroSpeedSettingDto {
    fn from(setting: CoreAeroSpeedSetting) -> Self {
        Self {
            kilometres_per_hour: setting.kilometres_per_hour(),
        }
    }
}

/// NOSFET/Veteran PWM warning percentage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileAeroPwmPercentDto {
    /// PWM percentage, from 0 through 100.
    pub percent: u8,
}

/// NOSFET/Veteran pedal-zero angle adjustment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileAeroAngleAdjustmentDto {
    /// Signed tenths of a degree, from -100 through 100.
    pub tenths_of_degree: i8,
}

impl From<CoreAeroPwmPercent> for MobileAeroPwmPercentDto {
    fn from(setting: CoreAeroPwmPercent) -> Self {
        Self {
            percent: setting.percent(),
        }
    }
}

impl From<CoreAeroAngleAdjustment> for MobileAeroAngleAdjustmentDto {
    fn from(setting: CoreAeroAngleAdjustment) -> Self {
        Self {
            tenths_of_degree: setting.tenths_of_degree(),
        }
    }
}

/// Mobile DTO light state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileLightStateDto {
    /// Lights off.
    Off,

    /// Lights on.
    On,

    /// Begode strobe/running-light mode.
    Strobe,
}

/// Mobile DTO acceleration-assist state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileAccelerationAssistStateDto {
    /// Acceleration assist is disabled.
    Disabled,

    /// Acceleration assist is enabled.
    Enabled,
}

impl From<MobileAccelerationAssistStateDto> for AccelerationAssistStateDto {
    fn from(state: MobileAccelerationAssistStateDto) -> Self {
        match state {
            MobileAccelerationAssistStateDto::Disabled => Self::Disabled,
            MobileAccelerationAssistStateDto::Enabled => Self::Enabled,
        }
    }
}

impl From<MobileAccelerationAssistStateDto> for CoreAccelerationAssistState {
    fn from(state: MobileAccelerationAssistStateDto) -> Self {
        match state {
            MobileAccelerationAssistStateDto::Disabled => Self::Disabled,
            MobileAccelerationAssistStateDto::Enabled => Self::Enabled,
        }
    }
}

impl From<CoreAccelerationAssistState> for MobileAccelerationAssistStateDto {
    fn from(state: CoreAccelerationAssistState) -> Self {
        match state {
            CoreAccelerationAssistState::Disabled => Self::Disabled,
            CoreAccelerationAssistState::Enabled => Self::Enabled,
        }
    }
}

/// Documented pedal stiffness mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePedalModeKindDto {
    /// Firm pedal response.
    Hard,

    /// Mid-range pedal response.
    Medium,

    /// Soft pedal response.
    Soft,
}

impl From<CorePedalMode> for MobilePedalModeKindDto {
    fn from(mode: CorePedalMode) -> Self {
        match mode {
            CorePedalMode::Hard => Self::Hard,
            CorePedalMode::Medium => Self::Medium,
            CorePedalMode::Soft => Self::Soft,
        }
    }
}

impl From<MobilePedalModeKindDto> for CorePedalMode {
    fn from(mode: MobilePedalModeKindDto) -> Self {
        match mode {
            MobilePedalModeKindDto::Hard => Self::Hard,
            MobilePedalModeKindDto::Medium => Self::Medium,
            MobilePedalModeKindDto::Soft => Self::Soft,
        }
    }
}

/// Documented Falcon roll-angle sensitivity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRollAngleKindDto {
    /// Low roll-angle sensitivity.
    Low,

    /// Medium roll-angle sensitivity.
    Medium,

    /// High roll-angle sensitivity.
    High,
}

impl From<CoreRollAngle> for MobileRollAngleKindDto {
    fn from(angle: CoreRollAngle) -> Self {
        match angle {
            CoreRollAngle::Low => Self::Low,
            CoreRollAngle::Medium => Self::Medium,
            CoreRollAngle::High => Self::High,
        }
    }
}

impl From<MobileRollAngleKindDto> for CoreRollAngle {
    fn from(angle: MobileRollAngleKindDto) -> Self {
        match angle {
            MobileRollAngleKindDto::Low => Self::Low,
            MobileRollAngleKindDto::Medium => Self::Medium,
            MobileRollAngleKindDto::High => Self::High,
        }
    }
}

/// Documented Begode speed-alarm mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSpeedAlarmModeKindDto {
    /// Both speed alarms are enabled.
    Both,

    /// Only the first-stage speed alarm is enabled.
    StageOneOnly,

    /// Speed alarms are disabled.
    Off,

    /// Firmware-controlled PWM tiltback mode.
    PwmTiltback,
}

/// Begode max-speed input for the timed `W` submenu.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBegodeMaxSpeedDto {
    /// Whole kilometres per hour, limited by the two-digit protocol field.
    pub kilometres_per_hour: u8,
}

/// Begode beeper-volume input for the timed `W` submenu.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBegodeBeeperVolumeDto {
    /// Protocol volume level, 1 through 9.
    pub level: u8,
}

/// Begode LED-mode input for the timed `W` submenu.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBegodeLedModeDto {
    /// Protocol LED mode, 0 through 9.
    pub mode: u8,
}

impl From<CoreSpeedAlarmMode> for MobileSpeedAlarmModeKindDto {
    fn from(mode: CoreSpeedAlarmMode) -> Self {
        match mode {
            CoreSpeedAlarmMode::Both => Self::Both,
            CoreSpeedAlarmMode::StageOneOnly => Self::StageOneOnly,
            CoreSpeedAlarmMode::Off => Self::Off,
            CoreSpeedAlarmMode::PwmTiltback => Self::PwmTiltback,
        }
    }
}

impl From<MobileSpeedAlarmModeKindDto> for CoreSpeedAlarmMode {
    fn from(mode: MobileSpeedAlarmModeKindDto) -> Self {
        match mode {
            MobileSpeedAlarmModeKindDto::Both => Self::Both,
            MobileSpeedAlarmModeKindDto::StageOneOnly => Self::StageOneOnly,
            MobileSpeedAlarmModeKindDto::Off => Self::Off,
            MobileSpeedAlarmModeKindDto::PwmTiltback => Self::PwmTiltback,
        }
    }
}

/// Validation state for a setting write exposed to mobile consumers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSettingWriteSupportDto {
    /// Available for an explicit user-initiated write through the guarded session.
    Supported,

    /// An encoder exists, but validation evidence is incomplete.
    Unverified,

    /// No safe write is available for the model.
    Unsupported,
}

/// Product-shaped write capabilities for an electric-unicycle session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileEucSettingsCapabilitiesDto {
    /// Pedal-mode/settings write support.
    pub pedal_mode: MobileSettingWriteSupportDto,

    /// Falcon roll-angle sensitivity write support.
    pub roll_angle: MobileSettingWriteSupportDto,

    /// Begode speed-alarm mode write support.
    pub speed_alarm_mode: MobileSettingWriteSupportDto,

    /// Begode max-speed write support.
    pub begode_max_speed: MobileSettingWriteSupportDto,

    /// Begode beeper-volume write support.
    pub begode_beeper_volume: MobileSettingWriteSupportDto,

    /// Begode LED-mode write support.
    pub begode_led_mode: MobileSettingWriteSupportDto,

    /// Acceleration-assist/behavior write support.
    pub acceleration_assist: MobileSettingWriteSupportDto,

    /// Headlight or validated high-beam write support.
    pub headlight: MobileSettingWriteSupportDto,

    /// NOSFET/Veteran high-beam write support.
    pub aero_high_beam: MobileSettingWriteSupportDto,

    /// Separate taillight write support.
    pub taillight: MobileSettingWriteSupportDto,

    /// NOSFET Aero trip-meter reset support.
    pub reset_trip_meter: MobileSettingWriteSupportDto,

    /// NOSFET/Veteran tilt-back speed write support.
    pub aero_tiltback_speed: MobileSettingWriteSupportDto,

    /// NOSFET/Veteran PWM warning write support.
    pub aero_pwm_percent: MobileSettingWriteSupportDto,

    /// NOSFET/Veteran speed-alarm write support.
    pub aero_alarm_speed: MobileSettingWriteSupportDto,

    /// NOSFET/Veteran pedal-zero angle write support.
    pub aero_angle_adjustment: MobileSettingWriteSupportDto,
}

impl MobileEucSettingsCapabilitiesDto {
    const fn aero() -> Self {
        Self {
            pedal_mode: MobileSettingWriteSupportDto::Supported,
            roll_angle: MobileSettingWriteSupportDto::Unsupported,
            speed_alarm_mode: MobileSettingWriteSupportDto::Unsupported,
            begode_max_speed: MobileSettingWriteSupportDto::Unsupported,
            begode_beeper_volume: MobileSettingWriteSupportDto::Unsupported,
            begode_led_mode: MobileSettingWriteSupportDto::Unsupported,
            acceleration_assist: MobileSettingWriteSupportDto::Unsupported,
            headlight: MobileSettingWriteSupportDto::Supported,
            aero_high_beam: MobileSettingWriteSupportDto::Supported,
            taillight: MobileSettingWriteSupportDto::Unsupported,
            reset_trip_meter: MobileSettingWriteSupportDto::Supported,
            aero_tiltback_speed: MobileSettingWriteSupportDto::Supported,
            aero_pwm_percent: MobileSettingWriteSupportDto::Supported,
            aero_alarm_speed: MobileSettingWriteSupportDto::Supported,
            aero_angle_adjustment: MobileSettingWriteSupportDto::Supported,
        }
    }

    const fn falcon() -> Self {
        Self {
            pedal_mode: MobileSettingWriteSupportDto::Supported,
            roll_angle: MobileSettingWriteSupportDto::Supported,
            speed_alarm_mode: MobileSettingWriteSupportDto::Supported,
            begode_max_speed: MobileSettingWriteSupportDto::Supported,
            begode_beeper_volume: MobileSettingWriteSupportDto::Supported,
            begode_led_mode: MobileSettingWriteSupportDto::Supported,
            acceleration_assist: MobileSettingWriteSupportDto::Unsupported,
            headlight: MobileSettingWriteSupportDto::Supported,
            aero_high_beam: MobileSettingWriteSupportDto::Unsupported,
            taillight: MobileSettingWriteSupportDto::Unsupported,
            reset_trip_meter: MobileSettingWriteSupportDto::Unsupported,
            aero_tiltback_speed: MobileSettingWriteSupportDto::Unsupported,
            aero_pwm_percent: MobileSettingWriteSupportDto::Unsupported,
            aero_alarm_speed: MobileSettingWriteSupportDto::Unsupported,
            aero_angle_adjustment: MobileSettingWriteSupportDto::Unsupported,
        }
    }
}

/// Returns Rust-owned setting capabilities for a detected EUC model.
#[uniffi::export]
#[must_use]
pub const fn mobile_euc_settings_capabilities(
    model: DiscoveryElectricUnicycleModel,
) -> MobileEucSettingsCapabilitiesDto {
    match model {
        DiscoveryElectricUnicycleModel::Aero => MobileEucSettingsCapabilitiesDto::aero(),
        DiscoveryElectricUnicycleModel::Falcon => MobileEucSettingsCapabilitiesDto::falcon(),
    }
}

/// Lifecycle phase for a typed mobile setting write.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSettingStateKindDto {
    /// No current value or write request is known.
    Unknown,

    /// A current value is available without a pending write.
    Current,

    /// A write was accepted and awaits matching readback.
    Pending,

    /// Readback confirmed the requested value.
    Confirmed,

    /// The write was refused before transport.
    Refused,

    /// No matching readback arrived before timeout.
    TimedOut,

    /// Transport or session failure prevented completion.
    Failed,
}

/// Provenance for the current value in a mobile setting state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSettingValueSourceDto {
    /// Value was observed in live device telemetry or readback.
    LiveReadback,

    /// Value came from a capture or replay fixture.
    CaptureReplay,

    /// Value was supplied by a user request and is not device-confirmed.
    UserRequest,

    /// Value provenance is unknown.
    Unknown,
}

/// Typed headlight state exposed by a mobile EUC session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileLightSettingStateDto {
    /// Current lifecycle phase.
    pub kind: MobileSettingStateKindDto,

    /// Most recent current light value, when known.
    pub current: Option<MobileLightStateDto>,

    /// Requested light value, when a write is pending or terminal.
    pub requested: Option<MobileLightStateDto>,

    /// Provenance for the current value.
    pub source: MobileSettingValueSourceDto,

    /// Monotonic time at which the write was accepted.
    pub submitted_at_ms: Option<u64>,

    /// Monotonic time at which matching readback arrived.
    pub confirmed_at_ms: Option<u64>,

    /// Typed refusal reason, when the write was refused.
    pub refusal_reason: Option<MobileControlRefusalReasonDto>,
}

/// Typed NOSFET/Veteran speed-setting lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileAeroSpeedSettingStateDto {
    /// Current lifecycle phase.
    pub kind: MobileSettingStateKindDto,

    /// Most recent current speed, when known.
    pub current: Option<MobileAeroSpeedSettingDto>,

    /// Requested speed, when a write is pending or terminal.
    pub requested: Option<MobileAeroSpeedSettingDto>,

    /// Provenance for the current value.
    pub source: MobileSettingValueSourceDto,

    /// Monotonic time at which the write was accepted.
    pub submitted_at_ms: Option<u64>,

    /// Monotonic time at which matching readback arrived.
    pub confirmed_at_ms: Option<u64>,

    /// Typed refusal reason, when the write was refused.
    pub refusal_reason: Option<MobileControlRefusalReasonDto>,
}

impl MobileAeroSpeedSettingStateDto {
    fn unknown() -> Self {
        Self {
            kind: MobileSettingStateKindDto::Unknown,
            current: None,
            requested: None,
            source: MobileSettingValueSourceDto::Unknown,
            submitted_at_ms: None,
            confirmed_at_ms: None,
            refusal_reason: None,
        }
    }
}

/// Typed NOSFET/Veteran PWM-warning lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileAeroPwmSettingStateDto {
    /// Current lifecycle phase.
    pub kind: MobileSettingStateKindDto,

    /// Most recent current PWM warning percentage, when known.
    pub current: Option<MobileAeroPwmPercentDto>,

    /// Requested PWM warning percentage, when a write is pending or terminal.
    pub requested: Option<MobileAeroPwmPercentDto>,

    /// Provenance for the current value.
    pub source: MobileSettingValueSourceDto,

    /// Monotonic time at which the write was accepted.
    pub submitted_at_ms: Option<u64>,

    /// Monotonic time at which matching readback arrived.
    pub confirmed_at_ms: Option<u64>,

    /// Typed refusal reason, when the write was refused.
    pub refusal_reason: Option<MobileControlRefusalReasonDto>,
}

impl MobileAeroPwmSettingStateDto {
    fn unknown() -> Self {
        Self {
            kind: MobileSettingStateKindDto::Unknown,
            current: None,
            requested: None,
            source: MobileSettingValueSourceDto::Unknown,
            submitted_at_ms: None,
            confirmed_at_ms: None,
            refusal_reason: None,
        }
    }
}

/// Typed NOSFET/Veteran angle-adjustment lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileAeroAngleAdjustmentStateDto {
    /// Current lifecycle phase.
    pub kind: MobileSettingStateKindDto,

    /// Most recent current angle adjustment, when known.
    pub current: Option<MobileAeroAngleAdjustmentDto>,

    /// Requested angle adjustment, when a write is pending or terminal.
    pub requested: Option<MobileAeroAngleAdjustmentDto>,

    /// Provenance for the current value.
    pub source: MobileSettingValueSourceDto,

    /// Monotonic time at which the write was accepted.
    pub submitted_at_ms: Option<u64>,

    /// Monotonic time at which matching readback arrived.
    pub confirmed_at_ms: Option<u64>,

    /// Typed refusal reason, when the write was refused.
    pub refusal_reason: Option<MobileControlRefusalReasonDto>,
}

impl MobileAeroAngleAdjustmentStateDto {
    fn unknown() -> Self {
        Self {
            kind: MobileSettingStateKindDto::Unknown,
            current: None,
            requested: None,
            source: MobileSettingValueSourceDto::Unknown,
            submitted_at_ms: None,
            confirmed_at_ms: None,
            refusal_reason: None,
        }
    }
}

impl MobileLightSettingStateDto {
    fn unknown() -> Self {
        Self {
            kind: MobileSettingStateKindDto::Unknown,
            current: None,
            requested: None,
            source: MobileSettingValueSourceDto::Unknown,
            submitted_at_ms: None,
            confirmed_at_ms: None,
            refusal_reason: None,
        }
    }
}

/// Typed pedal-mode lifecycle state exposed by a mobile EUC session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePedalModeSettingStateDto {
    /// Current lifecycle phase.
    pub kind: MobileSettingStateKindDto,

    /// Most recent current pedal mode, when known.
    pub current: Option<MobilePedalModeKindDto>,

    /// Requested pedal mode, when a write is pending or terminal.
    pub requested: Option<MobilePedalModeKindDto>,

    /// Provenance for the current value.
    pub source: MobileSettingValueSourceDto,

    /// Monotonic time at which the write was accepted.
    pub submitted_at_ms: Option<u64>,

    /// Monotonic time at which matching readback arrived.
    pub confirmed_at_ms: Option<u64>,

    /// Typed refusal reason, when the write was refused.
    pub refusal_reason: Option<MobileControlRefusalReasonDto>,
}

impl MobilePedalModeSettingStateDto {
    fn unknown() -> Self {
        Self {
            kind: MobileSettingStateKindDto::Unknown,
            current: None,
            requested: None,
            source: MobileSettingValueSourceDto::Unknown,
            submitted_at_ms: None,
            confirmed_at_ms: None,
            refusal_reason: None,
        }
    }
}

/// Typed Falcon roll-angle lifecycle state exposed by a mobile EUC session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRollAngleSettingStateDto {
    /// Current lifecycle phase.
    pub kind: MobileSettingStateKindDto,

    /// Most recent current roll-angle sensitivity, when known.
    pub current: Option<MobileRollAngleKindDto>,

    /// Requested roll-angle sensitivity, when a write is pending or terminal.
    pub requested: Option<MobileRollAngleKindDto>,

    /// Provenance for the current value.
    pub source: MobileSettingValueSourceDto,

    /// Monotonic time at which the write was accepted.
    pub submitted_at_ms: Option<u64>,

    /// Monotonic time at which matching readback arrived.
    pub confirmed_at_ms: Option<u64>,

    /// Typed refusal reason, when the write was refused.
    pub refusal_reason: Option<MobileControlRefusalReasonDto>,
}

impl MobileRollAngleSettingStateDto {
    fn unknown() -> Self {
        Self {
            kind: MobileSettingStateKindDto::Unknown,
            current: None,
            requested: None,
            source: MobileSettingValueSourceDto::Unknown,
            submitted_at_ms: None,
            confirmed_at_ms: None,
            refusal_reason: None,
        }
    }
}

/// Typed Begode speed-alarm lifecycle state exposed by a mobile EUC session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSpeedAlarmModeSettingStateDto {
    /// Current lifecycle phase.
    pub kind: MobileSettingStateKindDto,

    /// Most recent current speed-alarm mode, when known.
    pub current: Option<MobileSpeedAlarmModeKindDto>,

    /// Requested speed-alarm mode, when a write is pending or terminal.
    pub requested: Option<MobileSpeedAlarmModeKindDto>,

    /// Provenance for the current value.
    pub source: MobileSettingValueSourceDto,

    /// Monotonic time at which the write was accepted.
    pub submitted_at_ms: Option<u64>,

    /// Monotonic time at which matching readback arrived.
    pub confirmed_at_ms: Option<u64>,

    /// Typed refusal reason, when the write was refused.
    pub refusal_reason: Option<MobileControlRefusalReasonDto>,
}

impl MobileSpeedAlarmModeSettingStateDto {
    fn unknown() -> Self {
        Self {
            kind: MobileSettingStateKindDto::Unknown,
            current: None,
            requested: None,
            source: MobileSettingValueSourceDto::Unknown,
            submitted_at_ms: None,
            confirmed_at_ms: None,
            refusal_reason: None,
        }
    }
}

/// Typed acceleration-assist lifecycle state exposed by a mobile EUC session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileAccelerationAssistSettingStateDto {
    /// Current lifecycle phase.
    pub kind: MobileSettingStateKindDto,

    /// Most recent current assist state, when known.
    pub current: Option<MobileAccelerationAssistStateDto>,

    /// Requested assist state, when a write is pending or terminal.
    pub requested: Option<MobileAccelerationAssistStateDto>,

    /// Provenance for the current value.
    pub source: MobileSettingValueSourceDto,

    /// Monotonic time at which the write was accepted.
    pub submitted_at_ms: Option<u64>,

    /// Monotonic time at which matching readback arrived.
    pub confirmed_at_ms: Option<u64>,

    /// Typed refusal reason, when the write was refused.
    pub refusal_reason: Option<MobileControlRefusalReasonDto>,
}

impl MobileAccelerationAssistSettingStateDto {
    fn unknown() -> Self {
        Self {
            kind: MobileSettingStateKindDto::Unknown,
            current: None,
            requested: None,
            source: MobileSettingValueSourceDto::Unknown,
            submitted_at_ms: None,
            confirmed_at_ms: None,
            refusal_reason: None,
        }
    }
}

impl From<MobileLightStateDto> for LightStateDto {
    fn from(state: MobileLightStateDto) -> Self {
        match state {
            MobileLightStateDto::Off => Self::Off,
            MobileLightStateDto::On => Self::On,
            MobileLightStateDto::Strobe => Self::Strobe,
        }
    }
}

/// Mobile DTO input kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionInputKindDto {
    /// Link-up input.
    LinkUp,

    /// Link-down input.
    LinkDown,

    /// Notification input.
    Notification,

    /// Timer tick input.
    Tick,

    /// Device command input.
    Command,
}

/// Mobile DTO input.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionInputDto {
    /// Input kind.
    pub kind: MobileSessionInputKindDto,

    /// Monotonic timestamp.
    pub monotonic_ms: MobileMonotonicMillisDto,

    /// Maximum write length, when known.
    pub max_write_len: Option<MobileTransportWriteLimitDto>,

    /// Transport channel bytes for notification inputs.
    pub channel: Vec<u8>,

    /// Owned notification bytes.
    pub bytes: Vec<u8>,

    /// Command for command inputs.
    pub command: Option<MobileCommandDto>,
}

/// Mobile DTO monotonic timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMonotonicMillisDto {
    /// Timestamp value in milliseconds.
    pub milliseconds: u64,
}

impl MobileMonotonicMillisDto {
    const fn from_core_ffi_timestamp(timestamp: MonotonicMillisDto) -> Self {
        Self {
            milliseconds: timestamp.milliseconds,
        }
    }

    fn into_core_ffi(self) -> MonotonicMillisDto {
        MonotonicMillisDto {
            milliseconds: self.milliseconds,
        }
    }

    fn into_core(self) -> MonotonicTimestamp {
        MonotonicTimestamp::new(self.milliseconds)
    }
}

/// Mobile DTO wall-clock Unix timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileWallClockUnixMillisDto {
    /// Timestamp value in Unix epoch milliseconds.
    pub milliseconds: u64,
}

impl MobileWallClockUnixMillisDto {
    fn into_core(self) -> WallClockUnixTimestamp {
        WallClockUnixTimestamp::new(self.milliseconds)
    }
}

/// Mobile maximum transport write payload length.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileTransportWriteLimitDto {
    /// Length in bytes.
    pub bytes: u16,
}

impl MobileTransportWriteLimitDto {
    const fn into_core_ffi(self) -> TransportWriteLimitDto {
        TransportWriteLimitDto { bytes: self.bytes }
    }
}

impl From<TransportWriteLimitDto> for MobileTransportWriteLimitDto {
    fn from(value: TransportWriteLimitDto) -> Self {
        Self { bytes: value.bytes }
    }
}

/// Mobile notification payload length.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileNotificationByteLenDto {
    /// Length in bytes.
    pub bytes: u64,
}

impl From<NotificationByteLenDto> for MobileNotificationByteLenDto {
    fn from(value: NotificationByteLenDto) -> Self {
        Self {
            bytes: value.bytes as u64,
        }
    }
}

/// Mobile protocol payload body length.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePayloadBodyLenDto {
    /// Length in bytes.
    pub bytes: u64,
}

impl From<PayloadBodyLenDto> for MobilePayloadBodyLenDto {
    fn from(value: PayloadBodyLenDto) -> Self {
        Self {
            bytes: value.bytes as u64,
        }
    }
}

/// Mobile semantic event count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSemanticEventCountDto {
    /// Count of emitted semantic events.
    pub count: u64,
}

impl From<SemanticEventCountDto> for MobileSemanticEventCountDto {
    fn from(value: SemanticEventCountDto) -> Self {
        Self {
            count: value.count as u64,
        }
    }
}

/// Mobile dropped parser byte count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserDroppedBytesDto {
    /// Count of dropped bytes.
    pub bytes: u64,
}

impl From<ParserDroppedBytesDto> for MobileParserDroppedBytesDto {
    fn from(value: ParserDroppedBytesDto) -> Self {
        Self { bytes: value.bytes }
    }
}

/// Mobile parser diagnostic event count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserDiagnosticCountDto {
    /// Count of parser diagnostic events.
    pub count: u64,
}

impl From<ParserDiagnosticCountDto> for MobileParserDiagnosticCountDto {
    fn from(value: ParserDiagnosticCountDto) -> Self {
        Self { count: value.count }
    }
}

/// Mobile output kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionOutputKindDto {
    /// Subscribe transport action.
    Subscribe,

    /// Write transport action.
    Write,

    /// Non-transport event.
    Event,

    /// Disconnect transport action.
    Disconnect,

    /// Typed protocol notification ingest outcome.
    NotificationIngest,

    /// Read-only settings response.
    SettingsReadback,

    /// Read-only fault-history response.
    FaultHistoryReadback,

    /// Read-only BMS or pack-health response.
    BmsSnapshot,
}

/// Mobile output DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionOutputDto {
    /// Output kind.
    pub kind: MobileSessionOutputKindDto,

    /// Transport channel bytes.
    pub channel: Vec<u8>,

    /// Transport payload bytes.
    pub bytes: Vec<u8>,

    /// Typed parser-first ingest outcome.
    pub ingest: Option<MobileNotificationIngestOutcomeDto>,

    /// Typed read-only settings response.
    pub settings_readback: Option<MobileSettingsReadbackDto>,

    /// Typed read-only fault-history response.
    pub fault_history_readback: Option<MobileFaultHistoryReadbackDto>,

    /// Typed read-only BMS or pack-health response.
    pub bms_snapshot: Option<MobileBmsSnapshotDto>,

    /// Full protocol-native raw telemetry.
    pub raw_telemetry: Option<MobileRawTelemetryReadbackDto>,

    /// Veteran/NOSFET protocol model id when an Aero-family session decoded it.
    pub veteran_protocol_model_id: Option<u16>,
}

/// Mobile notification ingest outcome kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileNotificationIngestOutcomeKindDto {
    /// Notification produced semantic protocol events.
    SemanticEvents,

    /// Notification bytes are a valid partial frame.
    BufferedFragment,

    /// Notification was recognized but failed parser validation.
    ParserDiagnostic,

    /// Notification carried a known reserved/opaque payload.
    KnownReserved,

    /// Notification reached a known parser gap.
    ParserGap,

    /// Notification was intentionally ignored.
    Ignored,
}

/// Mobile ignored notification reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileIgnoredNotificationReasonDto {
    /// Notification arrived on a channel the selected protocol does not consume.
    WrongChannel,

    /// Notification could not be associated with a supported protocol family.
    UnsupportedFamily,

    /// Notification was classified to a family but not to a supported channel.
    UnsupportedChannel,

    /// Notification was accepted by a known family but no semantic mapping exists yet.
    AcceptedButUnmapped,

    /// Notification advanced frame-boundary search without completing a frame.
    SeekingFrameBoundary,

    /// Notification was classified and intentionally dropped by policy.
    IntentionallyDropped,
}

/// Mobile parser error kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileParserErrorKindDto {
    /// A frame claimed or accumulated more bytes than allowed.
    OversizedFrame,

    /// A frame checksum did not match its payload.
    BadChecksum,

    /// Input bytes could not form a valid frame.
    MalformedFrame,

    /// A parser deadline elapsed before the expected data arrived.
    Timeout,

    /// A reply could not be matched to an in-flight request.
    UnmatchedReply,
}

/// Mobile parser error DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserErrorDto {
    /// Error kind.
    pub kind: MobileParserErrorKindDto,

    /// Claimed or observed frame length.
    pub claimed: Option<MobileParserFrameLenDto>,

    /// Configured maximum accepted frame length.
    pub max: Option<MobileParserFrameLenDto>,

    /// Elapsed monotonic time.
    pub elapsed_ms: Option<MobileMonotonicMillisDto>,

    /// Timeout threshold in monotonic time.
    pub timeout_ms: Option<MobileMonotonicMillisDto>,
}

/// Mobile parser frame length DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserFrameLenDto {
    /// Length in bytes.
    pub bytes: u64,
}

/// Mobile reserved payload evidence DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileReservedPayloadEvidenceDto {
    /// Selector/page identifier when present.
    pub selector: Option<u8>,

    /// Tag/opcode when present.
    pub tag: Option<u16>,

    /// Reserved payload body length.
    pub body_len: MobilePayloadBodyLenDto,

    /// Bounded raw payload retained for capture correlation.
    pub retained_payload: Vec<u8>,

    /// Evidence verification status.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile parser gap evidence DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserGapEvidenceDto {
    /// Selector/page identifier when present.
    pub selector: Option<u8>,

    /// Tag/opcode when present.
    pub tag: Option<u16>,

    /// Unparsed body length.
    pub body_len: MobilePayloadBodyLenDto,

    /// Bounded raw payload retained for capture correlation.
    pub retained_payload: Vec<u8>,
}

/// Mobile notification evidence DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileNotificationEvidenceDto {
    /// Protocol family that accepted or classified the notification.
    pub family: MobileProtocolFamilyDto,

    /// GATT channel UUID bytes.
    pub channel: Vec<u8>,

    /// Notification payload length without retaining payload bytes.
    pub len: MobileNotificationByteLenDto,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MobileMonotonicMillisDto,
}

/// Mobile ignored notification evidence DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileIgnoredNotificationEvidenceDto {
    /// Protocol family when classification got that far.
    pub family: Option<MobileProtocolFamilyDto>,

    /// GATT channel UUID bytes.
    pub channel: Vec<u8>,

    /// Notification payload length without retaining payload bytes.
    pub len: MobileNotificationByteLenDto,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MobileMonotonicMillisDto,

    /// Bounded raw payload retained for capture correlation.
    pub retained_payload: Vec<u8>,
}

/// Mobile parser-first notification ingest outcome DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileNotificationIngestOutcomeDto {
    /// Outcome kind.
    pub kind: MobileNotificationIngestOutcomeKindDto,

    /// Accepted notification evidence without raw payload bytes.
    pub notification: Option<MobileNotificationEvidenceDto>,

    /// Number of semantic events emitted from this notification.
    pub event_count: Option<MobileSemanticEventCountDto>,

    /// Parser error for diagnostic outcomes.
    pub parser_error: Option<MobileParserErrorDto>,

    /// Reserved payload evidence for known-reserved outcomes.
    pub reserved: Option<MobileReservedPayloadEvidenceDto>,

    /// Parser gap evidence for parser-gap outcomes.
    pub gap: Option<MobileParserGapEvidenceDto>,

    /// Ignored notification evidence.
    pub ignored: Option<MobileIgnoredNotificationEvidenceDto>,

    /// Reason an ignored notification did not enter a decoder path.
    pub ignored_reason: Option<MobileIgnoredNotificationReasonDto>,
}

/// Mobile step-error kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionStepErrorKindDto {
    /// Command was refused.
    CommandRefused,

    /// Falcon profile was not supported.
    UnsupportedFalconProfile,
}

/// Typed reason a mobile control request was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileControlRefusalReasonDto {
    /// Command safety classification does not allow this control path.
    WrongSafetyClass,

    /// The required arming token was not supplied.
    MissingArm,

    /// The arming token belongs to another model.
    WrongModel,

    /// The arming token has expired.
    ExpiredArm,

    /// The requested value exceeds the configured current limit.
    CurrentLimitExceeded,

    /// The command is unsupported by this model or session.
    UnsupportedCommand,

    /// A previous timed settings sequence is still in progress.
    Busy,
}

/// Mobile session step error DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionStepErrorDto {
    /// Error kind.
    pub kind: MobileSessionStepErrorKindDto,

    /// Command associated with the error, if any.
    pub command: Option<MobileCommandDto>,

    /// Refusal reason, if any.
    pub reason: Option<MobileControlRefusalReasonDto>,
}

/// Mobile result of one session step.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionStepResultDto {
    /// Owned outputs from the step.
    pub outputs: Vec<MobileSessionOutputDto>,

    /// Stable error from the step, if any.
    pub error: Option<MobileSessionStepErrorDto>,
}

#[derive(Clone, Copy, Debug)]
struct MobileSettingTracker<Value> {
    state: CoreSettingState<Value>,
}

impl<Value> Default for MobileSettingTracker<Value>
where
    Value: Copy + Eq,
{
    fn default() -> Self {
        Self {
            state: CoreSettingState::unknown(),
        }
    }
}

impl<Value> MobileSettingTracker<Value>
where
    Value: Copy + Eq,
{
    fn observe_step(&mut self, kind: MobileSessionInputKindDto, now: MonotonicTimestamp) {
        if kind == MobileSessionInputKindDto::LinkDown {
            self.state = CoreSettingState::unknown();
        }

        if kind == MobileSessionInputKindDto::Tick {
            self.state
                .timeout_if_elapsed(now, SETTING_WRITE_CONFIRMATION_TIMEOUT);
        }
    }

    fn observe_write(
        &mut self,
        requested: Value,
        submitted_at: MonotonicTimestamp,
        result: &MobileSessionStepResultDto,
    ) {
        observe_setting_write(&mut self.state, requested, submitted_at, result);
    }

    fn observe_readback(&mut self, value: Value, observed_at: MonotonicTimestamp) {
        let _ = self
            .state
            .observe(value, CoreSettingValueSource::LiveReadback, observed_at);
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct MobileEucSettingTrackers {
    headlight: MobileSettingTracker<CoreLightState>,
    aero_high_beam: MobileSettingTracker<CoreLightState>,
    aero_tiltback_speed: MobileSettingTracker<CoreAeroSpeedSetting>,
    aero_pwm_percent: MobileSettingTracker<CoreAeroPwmPercent>,
    aero_alarm_speed: MobileSettingTracker<CoreAeroSpeedSetting>,
    aero_angle_adjustment: MobileSettingTracker<CoreAeroAngleAdjustment>,
    pedal_mode: MobileSettingTracker<CorePedalMode>,
    roll_angle: MobileSettingTracker<CoreRollAngle>,
    speed_alarm_mode: MobileSettingTracker<CoreSpeedAlarmMode>,
    acceleration_assist: MobileSettingTracker<CoreAccelerationAssistState>,
    taillight: MobileSettingTracker<CoreLightState>,
}

impl MobileEucSettingTrackers {
    fn observe_step(&mut self, input: &MobileSessionInputDto, result: &MobileSessionStepResultDto) {
        let now = input.monotonic_ms.into_core();
        self.headlight.observe_step(input.kind, now);
        self.aero_high_beam.observe_step(input.kind, now);
        self.aero_tiltback_speed.observe_step(input.kind, now);
        self.aero_pwm_percent.observe_step(input.kind, now);
        self.aero_alarm_speed.observe_step(input.kind, now);
        self.aero_angle_adjustment.observe_step(input.kind, now);
        self.pedal_mode.observe_step(input.kind, now);
        self.roll_angle.observe_step(input.kind, now);
        self.speed_alarm_mode.observe_step(input.kind, now);
        self.acceleration_assist.observe_step(input.kind, now);
        self.taillight.observe_step(input.kind, now);

        self.observe_command(input.command, now, result);
        for output in &result.outputs {
            if let Some(readback) = output.settings_readback.as_ref() {
                self.observe_readback(readback, now);
            }
        }
    }

    fn observe_command(
        &mut self,
        command: Option<MobileCommandDto>,
        now: MonotonicTimestamp,
        result: &MobileSessionStepResultDto,
    ) {
        match command {
            Some(MobileCommandDto::SetLights(requested)) => {
                self.headlight.observe_write(requested.into(), now, result);
            }
            Some(MobileCommandDto::SetAeroHighBeam(requested)) => {
                self.aero_high_beam
                    .observe_write(requested.into(), now, result);
            }
            Some(MobileCommandDto::SetAeroTiltbackSpeed(requested)) => {
                if let Some(requested) = CoreAeroSpeedSetting::new(requested.kilometres_per_hour) {
                    self.aero_tiltback_speed
                        .observe_write(requested, now, result);
                }
            }
            Some(MobileCommandDto::SetAeroPwmPercent(requested)) => {
                if let Some(requested) = CoreAeroPwmPercent::new(requested.percent) {
                    self.aero_pwm_percent.observe_write(requested, now, result);
                }
            }
            Some(MobileCommandDto::SetAeroAlarmSpeed(requested)) => {
                if let Some(requested) = CoreAeroSpeedSetting::new(requested.kilometres_per_hour) {
                    self.aero_alarm_speed.observe_write(requested, now, result);
                }
            }
            Some(MobileCommandDto::SetAeroAngleAdjustment(requested)) => {
                if let Some(requested) = CoreAeroAngleAdjustment::new(requested.tenths_of_degree) {
                    self.aero_angle_adjustment
                        .observe_write(requested, now, result);
                }
            }
            Some(MobileCommandDto::SetPedalMode(requested)) => {
                self.pedal_mode.observe_write(requested.into(), now, result);
            }
            Some(MobileCommandDto::SetRollAngle(requested)) => {
                self.roll_angle.observe_write(requested.into(), now, result);
            }
            Some(MobileCommandDto::SetSpeedAlarmMode(requested)) => {
                self.speed_alarm_mode
                    .observe_write(requested.into(), now, result);
            }
            Some(MobileCommandDto::SetAccelerationAssist(requested)) => {
                self.acceleration_assist
                    .observe_write(requested.into(), now, result);
            }
            Some(MobileCommandDto::SetTaillight(requested)) => {
                self.taillight.observe_write(requested.into(), now, result);
            }
            _ => {}
        }
    }

    fn observe_readback(&mut self, readback: &MobileSettingsReadbackDto, now: MonotonicTimestamp) {
        if let Some(light_state) = readback.euc_garage.light_state {
            self.headlight.observe_readback(light_state.into(), now);
        }
        if let Some(entry) =
            settings_entry(&readback.entries, VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH)
                .and_then(aero_speed_setting_from_entry)
        {
            self.aero_tiltback_speed.observe_readback(entry, now);
        }
        if let Some(entry) = settings_entry(&readback.entries, VETERAN_FIELD_SPEED_ALERT_DECI_KMH)
            .and_then(aero_speed_setting_from_entry)
        {
            self.aero_alarm_speed.observe_readback(entry, now);
        }
        if let Some(pedal_mode) = readback
            .euc_garage
            .pedal_mode
            .as_ref()
            .and_then(|pedal_mode| pedal_mode.mode.map(CorePedalMode::from))
        {
            self.pedal_mode.observe_readback(pedal_mode, now);
        }
        if let Some(roll_angle) = readback
            .euc_garage
            .roll_angle
            .as_ref()
            .and_then(|roll_angle| roll_angle.angle.map(CoreRollAngle::from))
        {
            self.roll_angle.observe_readback(roll_angle, now);
        }
        if let Some(speed_alarm_mode) = readback
            .euc_garage
            .speed_alarm_mode
            .as_ref()
            .and_then(|mode| mode.mode.map(CoreSpeedAlarmMode::from))
        {
            self.speed_alarm_mode
                .observe_readback(speed_alarm_mode, now);
        }
    }

    fn headlight(&self) -> MobileLightSettingStateDto {
        mobile_light_setting_state(self.headlight.state)
    }

    fn aero_high_beam(&self) -> MobileLightSettingStateDto {
        mobile_light_setting_state(self.aero_high_beam.state)
    }

    fn aero_tiltback_speed(&self) -> MobileAeroSpeedSettingStateDto {
        mobile_aero_speed_setting_state(self.aero_tiltback_speed.state)
    }

    fn aero_pwm_percent(&self) -> MobileAeroPwmSettingStateDto {
        mobile_aero_pwm_setting_state(self.aero_pwm_percent.state)
    }

    fn aero_alarm_speed(&self) -> MobileAeroSpeedSettingStateDto {
        mobile_aero_speed_setting_state(self.aero_alarm_speed.state)
    }

    fn aero_angle_adjustment(&self) -> MobileAeroAngleAdjustmentStateDto {
        mobile_aero_angle_setting_state(self.aero_angle_adjustment.state)
    }

    fn pedal_mode(&self) -> MobilePedalModeSettingStateDto {
        mobile_pedal_mode_setting_state(self.pedal_mode.state)
    }

    fn roll_angle(&self) -> MobileRollAngleSettingStateDto {
        mobile_roll_angle_setting_state(self.roll_angle.state)
    }

    fn speed_alarm_mode(&self) -> MobileSpeedAlarmModeSettingStateDto {
        mobile_speed_alarm_mode_setting_state(self.speed_alarm_mode.state)
    }

    fn acceleration_assist(&self) -> MobileAccelerationAssistSettingStateDto {
        mobile_acceleration_assist_setting_state(self.acceleration_assist.state)
    }

    fn taillight(&self) -> MobileLightSettingStateDto {
        mobile_light_setting_state(self.taillight.state)
    }
}

fn observe_setting_write<Value>(
    state: &mut CoreSettingState<Value>,
    requested: Value,
    submitted_at: MonotonicTimestamp,
    result: &MobileSessionStepResultDto,
) where
    Value: Copy + Eq,
{
    match result.error.as_ref() {
        None => state.submit(requested, submitted_at),
        Some(error) if error.kind == MobileSessionStepErrorKindDto::CommandRefused => {
            state.submit(requested, submitted_at);
            if let Some(reason) = error.reason {
                state.refuse(reason.into());
            } else {
                state.fail();
            }
        }
        Some(_) => state.fail(),
    }
}

fn mobile_aero_speed_setting_state(
    state: CoreSettingState<CoreAeroSpeedSetting>,
) -> MobileAeroSpeedSettingStateDto {
    let mut snapshot = MobileAeroSpeedSettingStateDto::unknown();
    match state {
        CoreSettingState::Unknown => {}
        CoreSettingState::Current(value) => {
            snapshot.kind = MobileSettingStateKindDto::Current;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
        }
        CoreSettingState::Pending {
            current,
            requested,
            submitted_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Pending;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
            snapshot.submitted_at_ms = Some(submitted_at.as_milliseconds());
        }
        CoreSettingState::Confirmed {
            value,
            confirmed_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Confirmed;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
            snapshot.confirmed_at_ms = Some(confirmed_at.as_milliseconds());
        }
        CoreSettingState::Refused {
            current,
            requested,
            reason,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Refused;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
            snapshot.refusal_reason = Some(reason.into());
        }
        CoreSettingState::TimedOut { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::TimedOut;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
        }
        CoreSettingState::Failed { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::Failed;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
        }
    }
    snapshot
}

fn mobile_aero_pwm_setting_state(
    state: CoreSettingState<CoreAeroPwmPercent>,
) -> MobileAeroPwmSettingStateDto {
    let mut snapshot = MobileAeroPwmSettingStateDto::unknown();
    match state {
        CoreSettingState::Unknown => {}
        CoreSettingState::Current(value) => {
            snapshot.kind = MobileSettingStateKindDto::Current;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
        }
        CoreSettingState::Pending {
            current,
            requested,
            submitted_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Pending;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
            snapshot.submitted_at_ms = Some(submitted_at.as_milliseconds());
        }
        CoreSettingState::Confirmed {
            value,
            confirmed_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Confirmed;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
            snapshot.confirmed_at_ms = Some(confirmed_at.as_milliseconds());
        }
        CoreSettingState::Refused {
            current,
            requested,
            reason,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Refused;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
            snapshot.refusal_reason = Some(reason.into());
        }
        CoreSettingState::TimedOut { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::TimedOut;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
        }
        CoreSettingState::Failed { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::Failed;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
        }
    }
    snapshot
}

fn mobile_aero_angle_setting_state(
    state: CoreSettingState<CoreAeroAngleAdjustment>,
) -> MobileAeroAngleAdjustmentStateDto {
    let mut snapshot = MobileAeroAngleAdjustmentStateDto::unknown();
    match state {
        CoreSettingState::Unknown => {}
        CoreSettingState::Current(value) => {
            snapshot.kind = MobileSettingStateKindDto::Current;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
        }
        CoreSettingState::Pending {
            current,
            requested,
            submitted_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Pending;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
            snapshot.submitted_at_ms = Some(submitted_at.as_milliseconds());
        }
        CoreSettingState::Confirmed {
            value,
            confirmed_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Confirmed;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
            snapshot.confirmed_at_ms = Some(confirmed_at.as_milliseconds());
        }
        CoreSettingState::Refused {
            current,
            requested,
            reason,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Refused;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
            snapshot.refusal_reason = Some(reason.into());
        }
        CoreSettingState::TimedOut { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::TimedOut;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
        }
        CoreSettingState::Failed { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::Failed;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
        }
    }
    snapshot
}

fn mobile_light_setting_state(
    state: CoreSettingState<CoreLightState>,
) -> MobileLightSettingStateDto {
    let mut snapshot = MobileLightSettingStateDto::unknown();
    match state {
        CoreSettingState::Unknown => {}
        CoreSettingState::Current(value) => {
            snapshot.kind = MobileSettingStateKindDto::Current;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
        }
        CoreSettingState::Pending {
            current,
            requested,
            submitted_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Pending;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
            snapshot.submitted_at_ms = Some(submitted_at.as_milliseconds());
        }
        CoreSettingState::Confirmed {
            value,
            confirmed_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Confirmed;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
            snapshot.confirmed_at_ms = Some(confirmed_at.as_milliseconds());
        }
        CoreSettingState::Refused {
            current,
            requested,
            reason,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Refused;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
            snapshot.refusal_reason = Some(reason.into());
        }
        CoreSettingState::TimedOut { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::TimedOut;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
        }
        CoreSettingState::Failed { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::Failed;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
        }
    }
    snapshot
}

fn mobile_pedal_mode_setting_state(
    state: CoreSettingState<CorePedalMode>,
) -> MobilePedalModeSettingStateDto {
    let mut snapshot = MobilePedalModeSettingStateDto::unknown();
    match state {
        CoreSettingState::Unknown => {}
        CoreSettingState::Current(value) => {
            snapshot.kind = MobileSettingStateKindDto::Current;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
        }
        CoreSettingState::Pending {
            current,
            requested,
            submitted_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Pending;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
            snapshot.submitted_at_ms = Some(submitted_at.as_milliseconds());
        }
        CoreSettingState::Confirmed {
            value,
            confirmed_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Confirmed;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
            snapshot.confirmed_at_ms = Some(confirmed_at.as_milliseconds());
        }
        CoreSettingState::Refused {
            current,
            requested,
            reason,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Refused;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
            snapshot.refusal_reason = Some(reason.into());
        }
        CoreSettingState::TimedOut { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::TimedOut;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
        }
        CoreSettingState::Failed { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::Failed;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
        }
    }
    snapshot
}

fn mobile_roll_angle_setting_state(
    state: CoreSettingState<CoreRollAngle>,
) -> MobileRollAngleSettingStateDto {
    let mut snapshot = MobileRollAngleSettingStateDto::unknown();
    match state {
        CoreSettingState::Unknown => {}
        CoreSettingState::Current(value) => {
            snapshot.kind = MobileSettingStateKindDto::Current;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
        }
        CoreSettingState::Pending {
            current,
            requested,
            submitted_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Pending;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
            snapshot.submitted_at_ms = Some(submitted_at.as_milliseconds());
        }
        CoreSettingState::Confirmed {
            value,
            confirmed_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Confirmed;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
            snapshot.confirmed_at_ms = Some(confirmed_at.as_milliseconds());
        }
        CoreSettingState::Refused {
            current,
            requested,
            reason,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Refused;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
            snapshot.refusal_reason = Some(reason.into());
        }
        CoreSettingState::TimedOut { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::TimedOut;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
        }
        CoreSettingState::Failed { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::Failed;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
        }
    }
    snapshot
}

fn mobile_speed_alarm_mode_setting_state(
    state: CoreSettingState<CoreSpeedAlarmMode>,
) -> MobileSpeedAlarmModeSettingStateDto {
    let mut snapshot = MobileSpeedAlarmModeSettingStateDto::unknown();
    match state {
        CoreSettingState::Unknown => {}
        CoreSettingState::Current(value) => {
            snapshot.kind = MobileSettingStateKindDto::Current;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
        }
        CoreSettingState::Pending {
            current,
            requested,
            submitted_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Pending;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
            snapshot.submitted_at_ms = Some(submitted_at.as_milliseconds());
        }
        CoreSettingState::Confirmed {
            value,
            confirmed_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Confirmed;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
            snapshot.confirmed_at_ms = Some(confirmed_at.as_milliseconds());
        }
        CoreSettingState::Refused {
            current,
            requested,
            reason,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Refused;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
            snapshot.refusal_reason = Some(reason.into());
        }
        CoreSettingState::TimedOut { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::TimedOut;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
        }
        CoreSettingState::Failed { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::Failed;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
        }
    }
    snapshot
}

fn mobile_acceleration_assist_setting_state(
    state: CoreSettingState<CoreAccelerationAssistState>,
) -> MobileAccelerationAssistSettingStateDto {
    let mut snapshot = MobileAccelerationAssistSettingStateDto::unknown();
    match state {
        CoreSettingState::Unknown => {}
        CoreSettingState::Current(value) => {
            snapshot.kind = MobileSettingStateKindDto::Current;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
        }
        CoreSettingState::Pending {
            current,
            requested,
            submitted_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Pending;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
            snapshot.submitted_at_ms = Some(submitted_at.as_milliseconds());
        }
        CoreSettingState::Confirmed {
            value,
            confirmed_at,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Confirmed;
            snapshot.current = Some(value.value.into());
            snapshot.source = value.source.into();
            snapshot.confirmed_at_ms = Some(confirmed_at.as_milliseconds());
        }
        CoreSettingState::Refused {
            current,
            requested,
            reason,
        } => {
            snapshot.kind = MobileSettingStateKindDto::Refused;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
            snapshot.refusal_reason = Some(reason.into());
        }
        CoreSettingState::TimedOut { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::TimedOut;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = Some(requested.into());
        }
        CoreSettingState::Failed { current, requested } => {
            snapshot.kind = MobileSettingStateKindDto::Failed;
            snapshot.current = current.map(|value| value.value.into());
            snapshot.source = current
                .map_or(CoreSettingValueSource::Unknown, |value| value.source)
                .into();
            snapshot.requested = requested.map(Into::into);
        }
    }
    snapshot
}

impl From<CoreSettingValueSource> for MobileSettingValueSourceDto {
    fn from(source: CoreSettingValueSource) -> Self {
        match source {
            CoreSettingValueSource::LiveReadback => Self::LiveReadback,
            CoreSettingValueSource::CaptureReplay => Self::CaptureReplay,
            CoreSettingValueSource::UserRequest => Self::UserRequest,
            CoreSettingValueSource::Unknown => Self::Unknown,
        }
    }
}

impl From<MobileLightStateDto> for CoreLightState {
    fn from(state: MobileLightStateDto) -> Self {
        match state {
            MobileLightStateDto::Off => Self::Off,
            MobileLightStateDto::On => Self::On,
            MobileLightStateDto::Strobe => Self::Strobe,
        }
    }
}

impl From<CoreLightState> for MobileLightStateDto {
    fn from(state: CoreLightState) -> Self {
        match state {
            CoreLightState::Off => Self::Off,
            CoreLightState::On => Self::On,
            CoreLightState::Strobe => Self::Strobe,
        }
    }
}

impl From<MobileControlRefusalReasonDto> for CoreControlRefusalReason {
    fn from(reason: MobileControlRefusalReasonDto) -> Self {
        match reason {
            MobileControlRefusalReasonDto::WrongSafetyClass => Self::WrongSafetyClass,
            MobileControlRefusalReasonDto::MissingArm => Self::MissingArm,
            MobileControlRefusalReasonDto::WrongModel => Self::WrongModel,
            MobileControlRefusalReasonDto::ExpiredArm => Self::ExpiredArm,
            MobileControlRefusalReasonDto::CurrentLimitExceeded => Self::CurrentLimitExceeded,
            MobileControlRefusalReasonDto::UnsupportedCommand => Self::UnsupportedCommand,
            MobileControlRefusalReasonDto::Busy => Self::Busy,
        }
    }
}

impl From<CoreControlRefusalReason> for MobileControlRefusalReasonDto {
    fn from(reason: CoreControlRefusalReason) -> Self {
        match reason {
            CoreControlRefusalReason::WrongSafetyClass => Self::WrongSafetyClass,
            CoreControlRefusalReason::MissingArm => Self::MissingArm,
            CoreControlRefusalReason::WrongModel => Self::WrongModel,
            CoreControlRefusalReason::ExpiredArm => Self::ExpiredArm,
            CoreControlRefusalReason::CurrentLimitExceeded => Self::CurrentLimitExceeded,
            CoreControlRefusalReason::UnsupportedCommand => Self::UnsupportedCommand,
            CoreControlRefusalReason::Busy => Self::Busy,
        }
    }
}

/// Fixed-unit duration used by the charging estimator boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDurationDto {
    /// Duration in milliseconds.
    pub milliseconds: u64,
}

/// Source of usable pack capacity for charging estimates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargeCapacitySourceDto {
    /// Capacity selected from a verified protocol profile.
    ProtocolProfile,

    /// Capacity measured from the physical pack.
    HardwareMeasured,

    /// Capacity inferred from incomplete evidence.
    Estimated,
}

/// Estimate confidence exposed to mobile presentation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, uniffi::Enum)]
pub enum MobileEstimateConfidenceDto {
    /// Weak or substantially inferred evidence.
    Low,

    /// Useful evidence with material uncertainty.
    Medium,

    /// Verified and stable evidence.
    High,
}

/// Kind of charge-time estimate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileEstimateKindDto {
    /// Time at the currently observed charging rate.
    AtPresentCurrent,

    /// Time integrated from a verified charge profile.
    ProfileBackedTimeToFull,

    /// Time adjusted from observed live taper behavior.
    ObservedTaperTimeToFull,
}

/// Reason a charge estimate is unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargeEstimateUnavailableReasonDto {
    /// The device is not explicitly charging.
    NotCharging,

    /// No battery current was supplied.
    CurrentMissing,

    /// Current direction or charge semantics are unverified.
    CurrentDirectionUnverified,

    /// Current is too small to produce a useful estimate.
    CurrentTooSmall,

    /// No usable SOC evidence was supplied.
    BatteryLevelMissing,

    /// No verified usable capacity was supplied.
    CapacityMissing,

    /// The supplied profile is missing or unverified.
    UnsupportedProfile,

    /// Current samples are too variable.
    UnstableCurrent,

    /// A sample is older than the freshness policy.
    StaleInput,

    /// Temperature is outside the conservative model.
    TemperatureOutOfModel,

    /// The pack is full or near full.
    FullOrNearFull,

    /// Independent charging inputs disagree.
    ContradictoryInputs,
}

/// Reason the bounded estimator accumulator was reset.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargeEstimateResetReasonDto {
    /// The active session changed.
    SessionChanged,

    /// Charging stopped.
    ChargingStopped,

    /// A stale sample gap interrupted the window.
    StaleGap,

    /// A timestamp moved backwards.
    TimestampOrder,

    /// Current provenance changed.
    CurrentEvidenceChanged,

    /// Usable capacity changed.
    CapacityChanged,

    /// The selected charge profile changed.
    ProfileChanged,

    /// The caller explicitly reset the estimator.
    Manual,
}

/// Why the estimator has not produced a valid result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargeEstimateErrorDto {
    /// Input timestamps were invalid.
    TimestampOrder,

    /// Checked arithmetic could not represent the result.
    ArithmeticOverflow,
}

/// Explicit usable capacity/profile configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargeProfileDto {
    /// Stable active session identity.
    pub session_id: u64,

    /// Verified battery/charger profile identity.
    pub profile_id: u32,

    /// Usable capacity in milliamp-hours.
    pub capacity_milliamp_hours: u32,

    /// Capacity provenance.
    pub capacity_source: MobileChargeCapacitySourceDto,

    /// Capacity verification state.
    pub verification: MobileVerificationStatusDto,

    /// Independent charge-flow/polarity verification from LIBCU-521.
    pub charge_flow_verification: MobileVerificationStatusDto,
}

/// Current-rate summary used by an available estimate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCurrentRateSummaryDto {
    /// Smoothed charging current magnitude in milliamps.
    pub mean_milliamps: i32,

    /// Minimum admitted charging current magnitude.
    pub minimum_milliamps: i32,

    /// Maximum admitted charging current magnitude.
    pub maximum_milliamps: i32,

    /// Current range divided by mean, in permille.
    pub variability_permille: u16,
}

/// Recent voltage-sag evidence emitted by the Rust estimator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileVoltageSagEstimateDto {
    /// Estimated loaded-minus-no-load pack voltage in millivolts.
    pub delta_millivolts: i32,

    /// Latest observed current used to project sag.
    pub load_current: BatteryCurrentReading,

    /// Effective pack resistance learned from observed load steps, in milliohms.
    pub effective_resistance_milliohms: u32,

    /// Number of admitted load-step observations.
    pub observations: u16,

    /// Confidence in this sag evidence.
    pub confidence: MobileEstimateConfidenceDto,

    /// Timestamp at which the evidence was calculated.
    pub calculated_at: MobileMonotonicMillisDto,

    /// Timestamp after which the evidence is stale.
    pub valid_until: MobileMonotonicMillisDto,
}

/// Persistable learned voltage-sag model for one stable EUC identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileVoltageSagModelDto {
    /// Persisted schema version.
    pub schema_version: u16,

    /// Learned effective pack resistance in milliohms.
    pub effective_resistance_milliohms: u32,

    /// Number of admitted load-step observations.
    pub observations: u16,

    /// Whether every admitted step came from hardware-verified telemetry.
    pub hardware_verified: bool,
}

/// Charge estimate result with conservative bounds and provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargeTimeEstimateDto {
    /// Conservative lower duration.
    pub lower: MobileDurationDto,

    /// Expected duration at the admitted current rate.
    pub expected: MobileDurationDto,

    /// Conservative upper duration.
    pub upper: MobileDurationDto,

    /// Estimate semantics.
    pub kind: MobileEstimateKindDto,

    /// Combined evidence confidence.
    pub confidence: MobileEstimateConfidenceDto,

    /// Current-rate evidence.
    pub current_rate: MobileCurrentRateSummaryDto,

    /// SOC value used by the calculation.
    pub battery_level: BatteryLevelReading,

    /// Whether SOC was reported or profile-estimated.
    pub battery_level_basis: MobileBatteryLevelBasisDto,

    /// Profile identity when SOC was profile-estimated.
    pub battery_profile_id: Option<u32>,

    /// Capacity provenance.
    pub capacity_source: MobileChargeCapacitySourceDto,

    /// Sag evidence incorporated into the bounds.
    pub voltage_sag: Option<MobileVoltageSagEstimateDto>,

    /// Host calculation timestamp.
    pub calculated_at: MobileMonotonicMillisDto,

    /// Timestamp after which this result is stale.
    pub valid_until: MobileMonotonicMillisDto,
}

/// SOC basis used by a charge estimate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileBatteryLevelBasisDto {
    /// SOC was reported by the device.
    Reported,

    /// SOC was derived from a verified voltage/profile basis.
    ProfileEstimated,
}

/// Mobile charging-state value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargeModeDto {
    /// The device reports that charging is active.
    Charging,

    /// The device reports that charging is not active.
    NotCharging,
}

/// Mobile charging-state reading with provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargeModeReadingDto {
    /// Charging-state value.
    pub value: MobileChargeModeDto,

    /// Value source.
    pub source: MobileValueSourceDto,

    /// Value quality.
    pub quality: MobileValueQualityDto,

    /// Value verification status.
    pub verification: MobileVerificationStatusDto,
}

/// Typed state for Swift/dashboard presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargeEstimateStateDto {
    /// Current state kind.
    pub kind: MobileChargeEstimateStateKindDto,

    /// Current estimate when available.
    pub estimate: Option<MobileChargeTimeEstimateDto>,

    /// Latest observed load-step sag, independent of charge-estimate availability.
    pub voltage_sag: Option<MobileVoltageSagEstimateDto>,

    /// Unavailable reason, when applicable.
    pub unavailable_reason: Option<MobileChargeEstimateUnavailableReasonDto>,

    /// Invariant/arithmetic error, when applicable.
    pub error: Option<MobileChargeEstimateErrorDto>,

    /// Most recent reset reason.
    pub reset_reason: Option<MobileChargeEstimateResetReasonDto>,

    /// Number of admitted samples.
    pub samples: u16,

    /// Observation duration covered by the bounded window.
    pub observed_for: MobileDurationDto,
}

/// Presentation state kind for a charge estimate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargeEstimateStateKindDto {
    /// Samples are still being collected.
    CollectingSamples,

    /// A valid estimate is available.
    Available,

    /// The current input cannot produce an estimate.
    Unavailable,

    /// The input stream has gone stale.
    Stale,

    /// An invariant or arithmetic error occurred.
    Failed,
}

/// Input for one Rust-owned charge estimate update.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargeEstimateInputDto {
    /// Host evaluation timestamp.
    pub at: MobileMonotonicMillisDto,

    /// Latest typed telemetry snapshot.
    pub snapshot: MobileTelemetrySnapshotDto,

    /// Maximum age and allowed gap for telemetry samples.
    pub freshness: MobileDurationDto,
}

/// Mobile telemetry snapshot DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileTelemetrySnapshotDto {
    /// Snapshot timestamp.
    pub at_ms: Option<MobileMonotonicMillisDto>,

    /// Reported or calculated speed.
    pub speed: Option<SpeedReading>,

    /// Conservative operating state inferred from currently available telemetry.
    pub operating_state: RideOperatingState,

    /// Protocol-decoded controller operating mode.
    pub vesc_operating_mode: Option<MobileVescRideOperatingModeDto>,

    /// Protocol-decoded VESC ride warning, when the active protocol reports one.
    pub vesc_warning: Option<MobileVescRideWarningDto>,

    /// Protocol-decoded reason the controller stopped balancing.
    pub vesc_stop_reason: Option<MobileVescRideStopReasonDto>,

    /// Reported voltage.
    pub voltage: Option<VoltageReading>,

    /// Reported battery current.
    pub battery_current: Option<BatteryCurrentReading>,

    /// Explicit protocol charging state with provenance.
    pub charge_mode: Option<MobileChargeModeReadingDto>,

    /// Reported motor current.
    pub motor_current: Option<PhaseCurrentReading>,

    /// Reported power.
    pub power: Option<PowerReading>,

    /// Signed power/current flow direction when known enough for conservative UI labels.
    pub power_flow: Option<PowerFlowDirection>,

    /// Voltage sag/loaded-pack delta when modeled by the product contract.
    pub voltage_sag: Option<VoltageDeltaReading>,

    /// Reported controller temperature.
    pub controller_temperature: Option<TemperatureReading>,

    /// Reported motor temperature.
    pub motor_temperature: Option<TemperatureReading>,

    /// Reported battery temperature.
    pub battery_temperature: Option<TemperatureReading>,

    /// Reported PWM duty in permille.
    pub pwm: Option<DutyCycle>,

    /// Reported distance.
    pub distance: Option<DistanceReading>,

    /// Reported trip distance when the active protocol exposes it separately.
    pub trip_distance: Option<DistanceReading>,

    /// Limp-home/range estimate when modeled by the product contract.
    pub limp_home_range: Option<DistanceReading>,

    /// Reported pitch.
    pub pitch: Option<AngleReading>,

    /// Reported balance-loop target angle.
    pub balance_angle: Option<AngleReading>,

    /// Reported roll.
    pub roll: Option<AngleReading>,

    /// Footpad/sensor state.
    pub footpad: Option<MobileFootpadTelemetryDto>,

    /// Reported battery level.
    pub battery_level_reported: Option<BatteryLevelReading>,

    /// Estimated battery percent.
    pub battery_level_estimated: Option<BatteryLevelReading>,
}

/// Mobile footpad telemetry DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileFootpadContactState {
    /// Neither footpad contact is active.
    None,

    /// Only the left footpad contact is active.
    Left,

    /// Only the right footpad contact is active.
    Right,

    /// Both footpad contacts are active.
    Both,
}

/// Mobile footpad telemetry DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileFootpadTelemetryDto {
    /// Protocol-specific footpad state bitfield/nibble.
    pub state: u8,

    /// Semantically decoded contact state when the protocol defines one.
    pub contact_state: Option<MobileFootpadContactState>,

    /// First footpad ADC reading in protocol units, scaled by 1000.
    pub adc1_milliunits: Option<i32>,

    /// Second footpad ADC reading in protocol units, scaled by 1000.
    pub adc2_milliunits: Option<i32>,
}

/// Raw phone location sample forwarded by the mobile platform.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobilePhoneLocationSampleDto {
    pub wall_clock_unix_ms: u64,
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
    pub altitude_meters: f64,
    pub horizontal_accuracy_meters: Option<f64>,
    pub vertical_accuracy_meters: Option<f64>,
    pub speed_meters_per_second: Option<f64>,
    pub speed_accuracy_meters_per_second: Option<f64>,
    pub course_degrees: Option<f64>,
    pub course_accuracy_degrees: Option<f64>,
}

impl MobilePhoneLocationSampleDto {
    /// Normalizes Core Location sentinel values into typed absence.
    fn canonical(self) -> Option<Self> {
        let location = PevcapPhoneLocation {
            wall_clock_unix_ms: self.wall_clock_unix_ms,
            latitude_degrees: self.latitude_degrees,
            longitude_degrees: self.longitude_degrees,
            altitude_meters: self.altitude_meters,
            horizontal_accuracy_meters: self.horizontal_accuracy_meters,
            vertical_accuracy_meters: self.vertical_accuracy_meters,
            speed_meters_per_second: self.speed_meters_per_second,
            speed_accuracy_meters_per_second: self.speed_accuracy_meters_per_second,
            course_degrees: self.course_degrees,
            course_accuracy_degrees: self.course_accuracy_degrees,
        }
        .canonical()
        .ok()?;
        Some(Self {
            wall_clock_unix_ms: location.wall_clock_unix_ms,
            latitude_degrees: location.latitude_degrees,
            longitude_degrees: location.longitude_degrees,
            altitude_meters: location.altitude_meters,
            horizontal_accuracy_meters: location.horizontal_accuracy_meters,
            vertical_accuracy_meters: location.vertical_accuracy_meters,
            speed_meters_per_second: location.speed_meters_per_second,
            speed_accuracy_meters_per_second: location.speed_accuracy_meters_per_second,
            course_degrees: location.course_degrees,
            course_accuracy_degrees: location.course_accuracy_degrees,
        })
    }

    fn pevcap_location(self) -> PevcapPhoneLocation {
        PevcapPhoneLocation {
            wall_clock_unix_ms: self.wall_clock_unix_ms,
            latitude_degrees: self.latitude_degrees,
            longitude_degrees: self.longitude_degrees,
            altitude_meters: self.altitude_meters,
            horizontal_accuracy_meters: self.horizontal_accuracy_meters,
            vertical_accuracy_meters: self.vertical_accuracy_meters,
            speed_meters_per_second: self.speed_meters_per_second,
            speed_accuracy_meters_per_second: self.speed_accuracy_meters_per_second,
            course_degrees: self.course_degrees,
            course_accuracy_degrees: self.course_accuracy_degrees,
        }
    }
}

/// Rust-owned phone location snapshot returned to the mobile UI and capture path.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobilePhoneLocationSnapshotDto {
    pub latest_sample: Option<MobilePhoneLocationSampleDto>,
    pub gps_speed: Option<SpeedReading>,
}

/// Rust-owned phone location state. Swift only gathers and forwards Core Location values.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobilePhoneLocationState {
    latest_sample: Mutex<Option<MobilePhoneLocationSampleDto>>,
}

/// Source of a Rust-owned canonical ride.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideSourceDto {
    /// A ride recorded from live location updates.
    Live,
    /// A ride reconstructed from a PEVCAP artifact.
    PevcapImport,
}

impl From<MobileRideSourceDto> for persistence::RideSource {
    fn from(source: MobileRideSourceDto) -> Self {
        match source {
            MobileRideSourceDto::Live => Self::Live,
            MobileRideSourceDto::PevcapImport => Self::PevcapImport,
        }
    }
}

impl From<persistence::RideSource> for MobileRideSourceDto {
    fn from(source: persistence::RideSource) -> Self {
        match source {
            persistence::RideSource::Live => Self::Live,
            persistence::RideSource::PevcapImport => Self::PevcapImport,
        }
    }
}

/// Lifecycle state of a canonical ride.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideLifecycleStateDto {
    /// Ride is being assembled.
    Draft,
    /// Ride is receiving samples.
    Active,
    /// Ride is temporarily paused.
    Paused,
    /// Ride has stopped but is not yet saved.
    Stopped,
    /// Ride ended because transport or capture was interrupted.
    Interrupted,
    /// Ride was discarded.
    Discarded,
    /// Ride was durably saved.
    Saved,
    /// Ride was imported from an artifact.
    Imported,
}

impl From<ride_maps::RideLifecycleState> for MobileRideLifecycleStateDto {
    fn from(state: ride_maps::RideLifecycleState) -> Self {
        match state {
            ride_maps::RideLifecycleState::Draft => Self::Draft,
            ride_maps::RideLifecycleState::Active => Self::Active,
            ride_maps::RideLifecycleState::Paused => Self::Paused,
            ride_maps::RideLifecycleState::Stopped => Self::Stopped,
            ride_maps::RideLifecycleState::Interrupted => Self::Interrupted,
            ride_maps::RideLifecycleState::Discarded => Self::Discarded,
            ride_maps::RideLifecycleState::Saved => Self::Saved,
            ride_maps::RideLifecycleState::Imported => Self::Imported,
        }
    }
}

/// Lifecycle event applied to a canonical ride.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideEventDto {
    /// Start a draft ride.
    Start,
    /// Pause an active ride.
    Pause,
    /// Resume a paused ride.
    Resume,
    /// Stop an active or paused ride.
    Stop,
    /// Mark a ride interrupted.
    Interrupt,
    /// Discard a ride.
    Discard,
    /// Save a stopped or interrupted ride.
    Save,
    /// Mark an imported ride.
    Import,
}

impl From<MobileRideEventDto> for ride_maps::RideEvent {
    fn from(event: MobileRideEventDto) -> Self {
        match event {
            MobileRideEventDto::Start => Self::Start,
            MobileRideEventDto::Pause => Self::Pause,
            MobileRideEventDto::Resume => Self::Resume,
            MobileRideEventDto::Stop => Self::Stop,
            MobileRideEventDto::Interrupt => Self::Interrupt,
            MobileRideEventDto::Discard => Self::Discard,
            MobileRideEventDto::Save => Self::Save,
            MobileRideEventDto::Import => Self::Import,
        }
    }
}

/// Location sample accepted by the Rust-owned ride database.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideLocationDto {
    /// Latitude in WGS84 decimal degrees.
    pub latitude_degrees: f64,
    /// Longitude in WGS84 decimal degrees.
    pub longitude_degrees: f64,
    /// Monotonic sample timestamp in milliseconds.
    pub monotonic_milliseconds: u64,
    /// Wall-clock Unix timestamp in milliseconds.
    pub wall_clock_unix_milliseconds: u64,
    /// Horizontal accuracy in millimetres, when provided.
    pub horizontal_accuracy_millimetres: Option<u32>,
    /// Origin of this sample.
    pub source: MobileRideSourceDto,
}

/// Stable ride identifier returned by Rust.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideIdDto {
    /// UUID string created by Rust.
    pub value: String,
}

/// Result of admitting a location sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideLocationAdmissionDto {
    /// Sample was durably appended.
    Accepted,
    /// Sample repeated the latest durable sample.
    Duplicate,
    /// Sample was older than the latest durable sample.
    OutOfOrder,
    /// Sample accuracy exceeded the Rust admission threshold.
    AccuracyTooLow,
    /// Sample implied an unrealistic travel speed.
    UnrealisticJump,
}

impl From<ride_maps::LocationAdmission> for MobileRideLocationAdmissionDto {
    fn from(admission: ride_maps::LocationAdmission) -> Self {
        match admission {
            ride_maps::LocationAdmission::Accepted => Self::Accepted,
            ride_maps::LocationAdmission::Duplicate => Self::Duplicate,
            ride_maps::LocationAdmission::OutOfOrder => Self::OutOfOrder,
            ride_maps::LocationAdmission::AccuracyTooLow => Self::AccuracyTooLow,
            ride_maps::LocationAdmission::UnrealisticJump => Self::UnrealisticJump,
        }
    }
}

/// Error returned by the Rust-owned live map recording core.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileRideMapCoreErrorDto {
    /// A live ride is already open.
    #[error("a ride is already recording")]
    AlreadyRecording,
    /// No live ride is open.
    #[error("no active ride")]
    NoActiveRide,
    /// The requested lifecycle event is not valid for the current state.
    #[error("invalid ride transition")]
    InvalidTransition,
    /// The supplied location values are invalid.
    #[error("invalid location")]
    InvalidLocation,
    /// The route display budget, viewport, or privacy policy is invalid.
    #[error("invalid route projection")]
    InvalidRouteProjection,
    /// A live route projection was cancelled by its caller.
    #[error("live route projection cancelled")]
    Cancelled,
    /// The canonical database rejected the operation.
    #[error("ride map storage failure: {0}")]
    Storage(String),
}

/// Telemetry provenance projected onto a live route point.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideMapCoreTelemetryStateDto {
    /// No vehicle telemetry has been associated with the recording.
    GpsOnly,
    /// A vehicle is associated, but no fresh telemetry sample is available.
    AssociatedNoTelemetry,
    /// A fresh vehicle telemetry sample is available.
    AssociatedFresh,
    /// The associated vehicle telemetry has gone stale.
    AssociatedStale,
    /// A newer Rust telemetry state is not known to this mobile binding.
    Unknown,
}

/// Result of observing a confirmed vehicle telemetry timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideMapTelemetryObservationDto {
    /// The timestamp became the newest telemetry evidence.
    Observed,
    /// The timestamp was already observed.
    AlreadyObserved,
    /// The ride has no confirmed association.
    NotAssociated,
    /// The timestamp moved backwards.
    TimestampOutOfOrder,
    /// The ride is not open for telemetry.
    RideNotOpen,
    /// A newer Rust observation is not known to this mobile binding.
    Unknown,
}

impl From<ride_maps::TelemetryObservation> for MobileRideMapTelemetryObservationDto {
    fn from(observation: ride_maps::TelemetryObservation) -> Self {
        match observation {
            ride_maps::TelemetryObservation::Observed => Self::Observed,
            ride_maps::TelemetryObservation::AlreadyObserved => Self::AlreadyObserved,
            ride_maps::TelemetryObservation::NotAssociated => Self::NotAssociated,
            ride_maps::TelemetryObservation::TimestampOutOfOrder => Self::TimestampOutOfOrder,
            ride_maps::TelemetryObservation::RideNotOpen => Self::RideNotOpen,
            _ => Self::Unknown,
        }
    }
}

impl From<ride_maps::RouteTelemetryState> for MobileRideMapCoreTelemetryStateDto {
    fn from(state: ride_maps::RouteTelemetryState) -> Self {
        match state {
            ride_maps::RouteTelemetryState::GpsOnly => Self::GpsOnly,
            ride_maps::RouteTelemetryState::AssociatedNoTelemetry => Self::AssociatedNoTelemetry,
            ride_maps::RouteTelemetryState::AssociatedFresh => Self::AssociatedFresh,
            ride_maps::RouteTelemetryState::AssociatedStale => Self::AssociatedStale,
            _ => Self::Unknown,
        }
    }
}

/// One Rust-owned live route point.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideMapCorePointDto {
    /// Stable sequence within the ride.
    pub sequence: u64,
    /// Segment sequence within the ride.
    pub segment_id: u64,
    /// Rust-owned reason this segment began.
    pub start_reason: MobileRideSegmentStartReasonDto,
    /// Latitude in WGS84 decimal degrees.
    pub latitude_degrees: f64,
    /// Longitude in WGS84 decimal degrees.
    pub longitude_degrees: f64,
    /// Wall-clock Unix timestamp in milliseconds.
    pub wall_clock_unix_ms: u64,
    /// Monotonic timestamp in milliseconds.
    pub monotonic_ms: u64,
    /// Horizontal accuracy in metres, when the source provided it.
    pub horizontal_accuracy_meters: Option<f64>,
    /// Vehicle telemetry provenance.
    pub telemetry_state: MobileRideMapCoreTelemetryStateDto,
}

/// One bounded page of Rust-owned live route points.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideMapCorePointBatchDto {
    /// Points after the supplied cursor.
    pub points: Vec<MobileRideMapCorePointDto>,
    /// Cursor for the next page, when more points remain.
    pub next_cursor: Option<u64>,
    /// Whether another page is available.
    pub has_more: bool,
}

/// Summary projected for an active map recording.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideMapCoreSummaryDto {
    /// Number of accepted route points.
    pub point_count: u64,
    /// Accumulated route distance in metres.
    pub distance_meters: f64,
    /// Elapsed recording duration in milliseconds.
    pub duration_milliseconds: u64,
}

/// Snapshot of the Rust-owned live map recording.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideMapCoreSnapshotDto {
    /// Rust-created ride identifier.
    pub ride_id: String,
    /// Current durable lifecycle state.
    pub state: MobileRideLifecycleStateDto,
    /// Current route summary.
    pub summary: MobileRideMapCoreSummaryDto,
    /// Rust-owned number of admitted route segments.
    pub segment_count: u64,
    /// Associated vehicle platform identifier, when available.
    pub associated_vehicle: Option<String>,
    /// Whether canonical start/end annotations are meaningful for this lifecycle state.
    pub recorded_bounds_available: bool,
}

/// Result of associating a connected vehicle with the active recording.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideMapCoreAssociationDto {
    /// The connection was associated with the recording.
    Associated,
    /// The same connection was already associated.
    AlreadyAssociated,
    /// The recording has no candidate vehicle to reconcile.
    CandidateMissing,
    /// The connected identity conflicts with the candidate identity.
    IdentityMismatch,
    /// The connection timestamp moved backwards.
    TimestampOutOfOrder,
    /// No recording is currently open.
    RideNotOpen,
    /// A newer Rust association outcome is not known to this mobile binding.
    Unknown,
}

impl From<ride_maps::VehicleAssociation> for MobileRideMapCoreAssociationDto {
    fn from(association: ride_maps::VehicleAssociation) -> Self {
        match association {
            ride_maps::VehicleAssociation::Associated => Self::Associated,
            ride_maps::VehicleAssociation::AlreadyAssociated => Self::AlreadyAssociated,
            ride_maps::VehicleAssociation::CandidateMissing => Self::CandidateMissing,
            ride_maps::VehicleAssociation::IdentityMismatch => Self::IdentityMismatch,
            ride_maps::VehicleAssociation::TimestampOutOfOrder => Self::TimestampOutOfOrder,
            ride_maps::VehicleAssociation::RideNotOpen => Self::RideNotOpen,
            _ => Self::Unknown,
        }
    }
}

/// Result of admitting one Core Location sample into the live recording.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideMapDecisionReasonDto {
    /// The ride is paused, stopped, or otherwise not recording.
    RideNotRecording,
    /// The sample repeats the latest accepted location.
    DuplicateLocation,
    /// The sample timestamp is not newer than the latest accepted sample.
    TimestampOutOfOrder,
    /// The reported horizontal accuracy exceeds the admission threshold.
    AccuracyTooLow,
    /// The sample implies an impossible travel speed.
    UnrealisticJump,
}

/// Result of admitting one Core Location sample into the live recording.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum MobileRideMapCoreDecisionDto {
    /// The location was accepted into the canonical route.
    Accepted {
        /// The resulting route point.
        point: MobileRideMapCorePointDto,
        /// Whether this point starts a new route segment.
        segment_started: bool,
    },
    /// The sample passed in-memory admission and is queued for durable persistence.
    Pending {
        /// The point already admitted to the Rust-owned in-memory route.
        point: MobileRideMapCorePointDto,
        /// Whether this point starts a new route segment.
        segment_started: bool,
    },
    /// The location was rejected as invalid input.
    Rejected {
        /// Stable admission reason.
        reason: MobileRideMapDecisionReasonDto,
    },
    /// The location was ignored because the ride is not recording.
    Ignored {
        /// Stable admission reason.
        reason: MobileRideMapDecisionReasonDto,
    },
    /// Durable persistence failed after in-memory admission.
    StorageError {
        /// Bounded diagnostic suitable for logging at the mobile boundary.
        message: String,
    },
}

/// Durable ride summary projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideSummaryDto {
    /// Number of accepted location samples.
    pub point_count: u64,
    /// Accumulated path distance in millimetres.
    pub distance_millimetres: u64,
    /// Rust-derived average speed in millimetres per second, when meaningful.
    pub average_speed_millimetres_per_second: Option<u64>,
}

/// Stable cursor for a subsequent ride-history page.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideCursorDto {
    pub created_at_milliseconds: u64,
    pub ride_id: MobileRideIdDto,
}

/// Rust-owned filters for bounded ride-history queries.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideHistoryFilterDto {
    pub created_after_milliseconds: Option<u64>,
    pub vehicle_identity: Option<String>,
    pub search_text: Option<String>,
}

/// Rust-owned bounds for the history overview's contextual route projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideHistoryContextBudgetDto {
    /// Maximum number of history records loaded for contextual projection.
    pub history_page_limit: u32,
    /// Maximum number of non-selected routes returned.
    pub max_routes: u32,
    /// Maximum display points returned for one contextual route.
    pub per_route_budget: u32,
    /// Maximum display points returned across all contextual routes.
    pub total_point_budget: u32,
}

/// Rust-owned limits used by the mobile map presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideMapLimitsDto {
    /// Maximum live-tail points retained by the in-memory map projection.
    pub live_tail_point_limit: u32,
    /// Maximum historical display points returned by one projection.
    pub history_preview_point_limit: u32,
    /// Default number of rides loaded per history page.
    pub history_page_limit: u32,
    /// Default number of surrounding routes in history context.
    pub history_context_max_routes: u32,
    /// Default display points per surrounding route.
    pub history_context_per_route_budget: u32,
    /// Default aggregate display points in history context.
    pub history_context_total_point_budget: u32,
    /// Duration covered by the recent-history filter.
    pub history_recent_window_milliseconds: u64,
}

/// Inputs for a bounded history overview context projection.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideHistoryContextOptionsDto {
    /// Rust-owned history filters applied before route projection.
    pub filter: MobileRideHistoryFilterDto,
    /// Selected ride omitted from the subdued contextual routes, when present.
    pub selected_ride_id: Option<MobileRideIdDto>,
    /// Rust-enforced history and display bounds.
    pub budget: MobileRideHistoryContextBudgetDto,
    /// Optional inclusive viewport shared by every contextual route.
    pub viewport: Option<MobileGeoBoundsDto>,
    /// Privacy policy applied before coordinates cross the FFI boundary.
    pub privacy: MobileRideMapRoutePrivacyPolicyDto,
}

/// One bounded ride-history projection.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideRecordDto {
    pub id: MobileRideIdDto,
    pub source: MobileRideSourceDto,
    pub state: MobileRideLifecycleStateDto,
    pub created_at_milliseconds: u64,
    /// Monotonic timestamp at which the ride became active, when known.
    pub monotonic_created_at_milliseconds: Option<u64>,
    /// Latest monotonic lifecycle or location timestamp, when known.
    pub monotonic_last_event_milliseconds: Option<u64>,
    /// Monotonic timestamp at which the current pause began, when paused.
    pub paused_at_milliseconds: Option<u64>,
    /// Accumulated paused duration in milliseconds.
    pub paused_duration_milliseconds: u64,
    /// Frozen active duration for terminal rides in milliseconds.
    pub completed_duration_milliseconds: u64,
    pub updated_at_milliseconds: u64,
    pub duration_milliseconds: u64,
    pub summary: MobileRideSummaryDto,
    pub segment_count: u64,
    pub candidate_vehicle: Option<String>,
    pub associated_vehicle: Option<String>,
    /// Persisted display name for the candidate vehicle, when available.
    pub candidate_vehicle_name: Option<String>,
    /// Persisted display name for the associated vehicle, when available.
    pub associated_vehicle_name: Option<String>,
    pub associated_at_milliseconds: Option<u64>,
    pub last_telemetry_at_milliseconds: Option<u64>,
}

/// One vehicle identity available to the history filter.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideHistoryVehicleOptionDto {
    /// Stable platform-local identity used as the filter value.
    pub platform_identifier: String,
    /// Display name persisted in the Rust device table, when available.
    pub display_name: Option<String>,
}

/// One bounded page of ride-history projections.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRidePageDto {
    pub rides: Vec<MobileRideRecordDto>,
    pub next_cursor: Option<MobileRideCursorDto>,
}

/// One bounded contextual route paired with its stable ride identifier.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideHistoryContextRouteDto {
    pub ride_id: MobileRideIdDto,
    pub projection: MobileRideMapRouteProjectionDto,
}

/// Bounded route context for a history overview.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideHistoryContextProjectionDto {
    /// Contextual routes in newest-first history order.
    pub routes: Vec<MobileRideHistoryContextRouteDto>,
    /// Number of records loaded from the bounded history page.
    pub source_history_route_count: u64,
    /// Number of eligible routes after selected-ride exclusion.
    pub context_route_count: u64,
    /// Total display points returned across contextual routes.
    pub total_display_point_count: u64,
    /// Whether route or aggregate point budgets omitted eligible context data.
    pub routes_omitted_by_budget: bool,
    /// Whether the bounded history page has another page after it.
    pub history_page_has_more: bool,
}

/// Stable cursor for a subsequent route-point page.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRoutePointCursorDto {
    pub sequence: u64,
}

/// One canonical route point with its stable ride sequence.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileRoutePointDto {
    pub sequence: u64,
    pub segment_id: u64,
    pub start_reason: MobileRideSegmentStartReasonDto,
    pub location: MobileRideLocationDto,
    pub telemetry_state: MobileRideMapCoreTelemetryStateDto,
}

/// One bounded page of canonical route points.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileRoutePointPageDto {
    pub points: Vec<MobileRoutePointDto>,
    pub next_cursor: Option<MobileRoutePointCursorDto>,
}

/// Privacy classification attached to a projected route coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideMapRoutePrivacyClassDto {
    Precise,
    GridRedacted,
}

/// Privacy policy applied before coordinates cross the mobile boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideMapRoutePrivacyPolicyDto {
    Precise,
    Grid { grid_e7: u32 },
}

/// Why a Rust-owned route segment began.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideSegmentStartReasonDto {
    /// The first accepted point in a ride.
    Initial,
    /// A paused ride resumed.
    Resume,
    /// A location gap exceeded the route continuity threshold.
    BackgroundGap,
    /// An imported artifact established the first route segment.
    ImportBoundary,
}

/// Rust-owned options for a bounded route display projection.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideMapRouteProjectionOptionsDto {
    /// Optional inclusive viewport. Reversed bounds are rejected.
    pub viewport: Option<MobileGeoBoundsDto>,
    /// Maximum number of display points to return.
    pub budget: u32,
    /// Privacy policy applied to every returned coordinate.
    pub privacy: MobileRideMapRoutePrivacyPolicyDto,
}

/// One bounded, privacy-classified Rust route display point.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideMapRouteDisplayPointDto {
    /// Stable sequence within the canonical ride.
    pub sequence: u64,
    /// Canonical segment sequence within the ride.
    pub segment_id: u64,
    /// Privacy-projected latitude in WGS84 decimal degrees.
    pub latitude_degrees: f64,
    /// Privacy-projected longitude in WGS84 decimal degrees.
    pub longitude_degrees: f64,
    /// Classification applied before this point crossed the boundary.
    pub privacy_class: MobileRideMapRoutePrivacyClassDto,
}

/// Bounded metadata for one visible projected route segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRideMapSegmentDisplayMetadataDto {
    /// Canonical segment identity within the ride.
    pub segment_id: u64,
    /// Rust-owned reason this segment began.
    pub start_reason: MobileRideSegmentStartReasonDto,
    /// Number of projected points retained for this segment.
    pub visible_point_count: u64,
    /// Number of points in the canonical source segment, when known.
    pub canonical_point_count: Option<u64>,
    /// First retained projected point sequence.
    pub first_visible_sequence: Option<u64>,
    /// Last retained projected point sequence.
    pub last_visible_sequence: Option<u64>,
}

/// Rust-computed camera region for a bounded route display projection.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideMapCameraRegionDto {
    /// Camera center latitude in WGS84 degrees.
    pub center_latitude_degrees: f64,
    /// Camera center longitude in WGS84 degrees.
    pub center_longitude_degrees: f64,
    /// Padded camera latitude span in degrees.
    pub latitude_span_degrees: f64,
    /// Padded camera longitude span in degrees.
    pub longitude_span_degrees: f64,
}

/// Bounded Rust route display projection.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileRideMapRouteProjectionDto {
    /// Evenly sampled points visible in the requested viewport.
    pub points: Vec<MobileRideMapRouteDisplayPointDto>,
    /// Bounded metadata for the visible projected segment runs.
    pub segments: Vec<MobileRideMapSegmentDisplayMetadataDto>,
    /// Total canonical point count before viewport filtering or display LOD.
    pub source_point_count: u64,
    /// Total canonical segment count before viewport filtering or display LOD.
    pub source_segment_count: u64,
    /// Number of canonical points inside the requested viewport before display LOD.
    pub candidate_point_count: u64,
    /// Number of canonical segments with points inside the requested viewport.
    pub candidate_segment_count: u64,
    /// Number of segments represented by the bounded display points.
    pub displayed_segment_count: u64,
    /// Total canonical segments started by a background location gap.
    pub background_gap_count: u64,
    /// Canonical first-point sequence, when the route has points.
    pub canonical_start_sequence: Option<u64>,
    /// Canonical last-point sequence, when the route has points.
    pub canonical_end_sequence: Option<u64>,
    /// Whether the canonical first point lies in the requested viewport.
    pub canonical_start_visible: bool,
    /// Whether the canonical last point lies in the requested viewport.
    pub canonical_end_visible: bool,
    /// Rust-computed camera region for the bounded display points.
    pub camera_region: Option<MobileRideMapCameraRegionDto>,
}

/// Bounded startup state produced by Rust after recovering interrupted rides.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBootstrapSnapshotDto {
    pub recovered_rides: Vec<MobileRideIdDto>,
}

/// Runtime `SQLite` capabilities observed by Rust.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSqliteCapabilitiesDto {
    /// `SQLite` major version.
    pub major: u32,
    /// `SQLite` minor version.
    pub minor: u32,
    /// `SQLite` patch version.
    pub patch: u32,
    /// Whether R*Tree is compiled in.
    pub has_rtree: bool,
    /// Whether FTS5 is compiled in.
    pub has_fts5: bool,
}

/// Supported PEVCAP artifact encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePevcapEncodingDto {
    /// Review-friendly line-delimited JSON.
    Jsonl,
    /// Compact binary PEVCAP container.
    Binary,
}

impl From<MobilePevcapEncodingDto> for CorePevcapEncoding {
    fn from(encoding: MobilePevcapEncodingDto) -> Self {
        match encoding {
            MobilePevcapEncodingDto::Jsonl => Self::Jsonl,
            MobilePevcapEncodingDto::Binary => Self::Binary,
        }
    }
}

impl From<CorePevcapEncoding> for MobilePevcapEncodingDto {
    fn from(encoding: CorePevcapEncoding) -> Self {
        match encoding {
            CorePevcapEncoding::Jsonl => Self::Jsonl,
            CorePevcapEncoding::Binary => Self::Binary,
        }
    }
}

/// Durable outcome selected by PEVCAP preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePevcapImportOutcomeDto {
    /// Confirmation produces a canonical ride and a managed capture.
    RideAndCapture,
    /// Confirmation produces only a managed capture because no route locations exist.
    CaptureOnly,
}

impl From<persistence::PevcapImportOutcome> for MobilePevcapImportOutcomeDto {
    fn from(outcome: persistence::PevcapImportOutcome) -> Self {
        match outcome {
            persistence::PevcapImportOutcome::RideAndCapture => Self::RideAndCapture,
            persistence::PevcapImportOutcome::CaptureOnly => Self::CaptureOnly,
        }
    }
}

/// Non-fatal PEVCAP preflight warning.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobilePevcapImportWarningDto {
    /// No phone route locations were present.
    NoRouteLocations,
}

/// Bounded PEVCAP facts presented before explicit confirmation.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobilePevcapImportPreviewDto {
    pub source_path: String,
    pub encoding: MobilePevcapEncodingDto,
    pub artifact_digest: String,
    pub artifact_size: u64,
    pub record_count: u64,
    pub location_count: u64,
    pub duration_milliseconds: u64,
    pub outcome: MobilePevcapImportOutcomeDto,
    pub warnings: Vec<MobilePevcapImportWarningDto>,
}

/// Durable result of importing one PEVCAP artifact.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePevcapImportReceiptDto {
    /// Rust-created ride UUID, when route locations were present.
    pub ride_id: Option<MobileRideIdDto>,
    /// SHA-256 digest of the source artifact.
    pub artifact_digest: String,
    /// Immutable application-managed artifact path.
    pub managed_artifact_path: String,
    /// Whether confirmation produced a ride and capture or only a capture.
    pub outcome: MobilePevcapImportOutcomeDto,
    /// Number of transport records read.
    pub record_count: u64,
    /// Number of phone locations admitted.
    pub location_count: u64,
    /// Whether this artifact digest was already imported.
    pub duplicate: bool,
}

/// Coordinate used by map and trail DTOs.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileMapCoordinateDto {
    /// Latitude in WGS84 decimal degrees.
    pub latitude_degrees: f64,
    /// Longitude in WGS84 decimal degrees.
    pub longitude_degrees: f64,
}

/// Stable trail identifier returned by Rust.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileTrailIdDto {
    /// UUID string created by Rust.
    pub value: String,
}

/// One indexed trail segment.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileTrailSegmentDto {
    /// Stable trail identifier owning this segment.
    pub trail_id: MobileTrailIdDto,
    /// Segment sequence within its trail.
    pub sequence: u32,
    /// Segment start coordinate.
    pub start: MobileMapCoordinateDto,
    /// Segment end coordinate.
    pub end: MobileMapCoordinateDto,
}

/// Validated WGS84 viewport bounds. A minimum longitude greater than the maximum crosses the
/// antimeridian.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MobileGeoBoundsDto {
    pub minimum_latitude_degrees: f64,
    pub maximum_latitude_degrees: f64,
    pub minimum_longitude_degrees: f64,
    pub maximum_longitude_degrees: f64,
}

/// Stable cursor for a subsequent trail-segment page.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileTrailSegmentCursorDto {
    pub trail_id: MobileTrailIdDto,
    pub sequence: u32,
}

/// One bounded page of indexed trail segments.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileTrailSegmentPageDto {
    pub segments: Vec<MobileTrailSegmentDto>,
    pub next_cursor: Option<MobileTrailSegmentCursorDto>,
}

/// One indexed charging/food map point.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileMapPointDto {
    /// Stable point identifier.
    pub id: String,
    /// User-visible point name.
    pub name: String,
    /// Point coordinate.
    pub coordinate: MobileMapCoordinateDto,
}

/// Stable cursor for a subsequent map-point page.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMapPointCursorDto {
    pub id: String,
}

/// One bounded page of indexed map points.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileMapPointPageDto {
    pub points: Vec<MobileMapPointDto>,
    pub next_cursor: Option<MobileMapPointCursorDto>,
}

/// Stable error categories for the Rust-owned ride database boundary.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileRideDatabaseError {
    /// The database path cannot be opened.
    #[error("invalid database path")]
    InvalidPath,
    /// A different database is already owned by this process.
    #[error("a different database is already open")]
    AlreadyOpenForDifferentPath,
    /// A supplied ride or trail identifier is not a Rust-created UUID.
    #[error("invalid ride or trail identifier")]
    InvalidIdentifier,
    /// The database schema is newer than this build supports.
    #[error("unsupported database schema")]
    UnsupportedSchemaVersion,
    /// The file is `SQLite` but belongs to another application.
    #[error("invalid Cutout database identity")]
    InvalidDatabaseIdentity,
    /// The database failed `SQLite`'s integrity check.
    #[error("database integrity check failed")]
    IntegrityCheckFailed,
    /// A growing query was not bounded by a supported limit.
    #[error("invalid query limit")]
    InvalidQueryLimit,
    /// Geographic query bounds were non-finite, out of range, or reversed.
    #[error("invalid geographic bounds")]
    InvalidGeographicBounds,
    /// The history context page or display budgets are outside Rust's supported bounds.
    #[error("invalid history context budget")]
    InvalidHistoryContextBudget,
    /// The route display budget, viewport, or privacy policy is invalid.
    #[error("invalid route projection")]
    InvalidRouteProjection,
    /// A cancellable route projection was cancelled.
    #[error("route projection cancelled")]
    Cancelled,
    /// PEVCAP preflight rejected an artifact that exceeded a hard resource limit.
    #[error("PEVCAP resource limit exceeded")]
    PevcapLimitExceeded,
    /// The PEVCAP source changed after the preview was presented.
    #[error("PEVCAP source changed after preflight")]
    PevcapPreviewChanged,
    /// Another confirmation for the same artifact is already active.
    #[error("PEVCAP import is already in progress")]
    PevcapImportInProgress,
    /// A coordinate failed WGS84 validation.
    #[error("invalid coordinate")]
    InvalidCoordinate,
    /// The requested ride does not exist.
    #[error("ride was not found")]
    NotFound,
    /// The requested lifecycle transition is invalid.
    #[error("invalid ride transition")]
    InvalidTransition,
    /// The ride is not accepting location samples.
    #[error("ride is not accepting samples")]
    InvalidRideState,
    /// The bounded Rust worker queue is full.
    #[error("ride database queue is full")]
    QueueFull,
    /// The Rust worker is no longer available.
    #[error("ride database worker stopped")]
    WorkerStopped,
    /// An internal storage failure occurred.
    #[error("ride database storage failure")]
    StorageFailure,
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "matches and consumes the storage error enum"
)]
fn map_ride_database_error(error: persistence::StorageError) -> MobileRideDatabaseError {
    match error {
        persistence::StorageError::InvalidPath => MobileRideDatabaseError::InvalidPath,
        persistence::StorageError::AlreadyOpenForDifferentPath => {
            MobileRideDatabaseError::AlreadyOpenForDifferentPath
        }
        persistence::StorageError::UnsupportedSchemaVersion(_) => {
            MobileRideDatabaseError::UnsupportedSchemaVersion
        }
        persistence::StorageError::InvalidDatabaseIdentity => {
            MobileRideDatabaseError::InvalidDatabaseIdentity
        }
        persistence::StorageError::IntegrityCheckFailed(_) => {
            MobileRideDatabaseError::IntegrityCheckFailed
        }
        persistence::StorageError::InvalidQueryLimit(_) => {
            MobileRideDatabaseError::InvalidQueryLimit
        }
        persistence::StorageError::InvalidGeographicBounds => {
            MobileRideDatabaseError::InvalidGeographicBounds
        }
        persistence::StorageError::InvalidHistoryContextBudget { .. } => {
            MobileRideDatabaseError::InvalidHistoryContextBudget
        }
        persistence::StorageError::Cancelled => MobileRideDatabaseError::Cancelled,
        persistence::StorageError::PevcapLimitExceeded { .. } => {
            MobileRideDatabaseError::PevcapLimitExceeded
        }
        persistence::StorageError::PevcapPreviewChanged => {
            MobileRideDatabaseError::PevcapPreviewChanged
        }
        persistence::StorageError::PevcapImportInProgress => {
            MobileRideDatabaseError::PevcapImportInProgress
        }
        persistence::StorageError::NotFound => MobileRideDatabaseError::NotFound,
        persistence::StorageError::Transition(_) => MobileRideDatabaseError::InvalidTransition,
        persistence::StorageError::InvalidRideState(_) => MobileRideDatabaseError::InvalidRideState,
        persistence::StorageError::QueueFull => MobileRideDatabaseError::QueueFull,
        persistence::StorageError::WorkerStopped
        | persistence::StorageError::ResponseDropped
        | persistence::StorageError::WorkerStart(_)
        | persistence::StorageError::DeadlineExceeded => MobileRideDatabaseError::WorkerStopped,
        persistence::StorageError::Sqlite(_)
        | persistence::StorageError::Io(_)
        | persistence::StorageError::InvalidStoredValue { .. }
        | persistence::StorageError::InvalidSqliteVersion(_)
        | persistence::StorageError::PevcapImport(_)
        | persistence::StorageError::SystemClock(_)
        | persistence::StorageError::SpatialCapabilityUnavailable
        | persistence::StorageError::SpatialSchemaInitialization(_) => {
            MobileRideDatabaseError::StorageFailure
        }
    }
}

fn mobile_ride_record_dto(ride: &persistence::RideRecord) -> MobileRideRecordDto {
    let summary = ride.summary();
    MobileRideRecordDto {
        id: MobileRideIdDto {
            value: ride.id().uuid().to_string(),
        },
        source: ride.source().into(),
        state: ride.state().into(),
        created_at_milliseconds: ride.created_at_milliseconds(),
        monotonic_created_at_milliseconds: ride.monotonic_created_at_milliseconds(),
        monotonic_last_event_milliseconds: ride.monotonic_last_event_milliseconds(),
        paused_at_milliseconds: ride.paused_at_milliseconds(),
        paused_duration_milliseconds: ride.paused_duration_milliseconds(),
        completed_duration_milliseconds: ride.completed_duration_milliseconds(),
        updated_at_milliseconds: ride.updated_at_milliseconds(),
        duration_milliseconds: ride.duration_milliseconds(),
        summary: MobileRideSummaryDto {
            point_count: summary.point_count().as_u64(),
            distance_millimetres: summary.distance_millimetres(),
            average_speed_millimetres_per_second: ride.average_speed_millimetres_per_second(),
        },
        segment_count: ride.segment_count(),
        candidate_vehicle: ride.candidate_vehicle().map(str::to_owned),
        associated_vehicle: ride.associated_vehicle().map(str::to_owned),
        candidate_vehicle_name: ride.candidate_vehicle_name().map(str::to_owned),
        associated_vehicle_name: ride.associated_vehicle_name().map(str::to_owned),
        associated_at_milliseconds: ride.associated_at_milliseconds(),
        last_telemetry_at_milliseconds: ride.last_telemetry_at_milliseconds(),
    }
}

fn parse_mobile_ride_id(
    id: &MobileRideIdDto,
) -> Result<persistence::RideId, MobileRideDatabaseError> {
    Uuid::parse_str(&id.value)
        .map(persistence::RideId::from_uuid)
        .map_err(|_| MobileRideDatabaseError::InvalidIdentifier)
}

fn parse_mobile_trail_id(
    id: &MobileTrailIdDto,
) -> Result<persistence::TrailId, MobileRideDatabaseError> {
    Uuid::parse_str(&id.value)
        .map(persistence::TrailId::from_uuid)
        .map_err(|_| MobileRideDatabaseError::InvalidIdentifier)
}

fn mobile_map_coordinate(
    coordinate: MobileMapCoordinateDto,
) -> Result<ride_maps::Coordinate, MobileRideDatabaseError> {
    ride_maps::Coordinate::from_degrees(coordinate.latitude_degrees, coordinate.longitude_degrees)
        .map_err(|_| MobileRideDatabaseError::InvalidCoordinate)
}

fn mobile_map_coordinate_dto(coordinate: ride_maps::Coordinate) -> MobileMapCoordinateDto {
    MobileMapCoordinateDto {
        latitude_degrees: coordinate.latitude_degrees(),
        longitude_degrees: coordinate.longitude_degrees(),
    }
}

fn mobile_geo_bounds(
    bounds: MobileGeoBoundsDto,
) -> Result<persistence::GeoBounds, MobileRideDatabaseError> {
    persistence::GeoBounds::new(
        bounds.minimum_latitude_degrees,
        bounds.maximum_latitude_degrees,
        bounds.minimum_longitude_degrees,
        bounds.maximum_longitude_degrees,
    )
    .map_err(map_ride_database_error)
}

fn mobile_route_projection_options_for_database(
    options: &MobileRideMapRouteProjectionOptionsDto,
) -> Result<
    (
        Option<ride_maps::RouteViewport>,
        ride_maps::RouteDisplayBudget,
        ride_maps::RoutePrivacyPolicy,
    ),
    MobileRideDatabaseError,
> {
    let viewport = options
        .viewport
        .map(|bounds| {
            let minimum = ride_maps::Coordinate::from_degrees(
                bounds.minimum_latitude_degrees,
                bounds.minimum_longitude_degrees,
            )
            .map_err(|_| MobileRideDatabaseError::InvalidRouteProjection)?;
            let maximum = ride_maps::Coordinate::from_degrees(
                bounds.maximum_latitude_degrees,
                bounds.maximum_longitude_degrees,
            )
            .map_err(|_| MobileRideDatabaseError::InvalidRouteProjection)?;
            ride_maps::RouteViewport::new(
                minimum.latitude(),
                maximum.latitude(),
                minimum.longitude(),
                maximum.longitude(),
            )
            .ok_or(MobileRideDatabaseError::InvalidRouteProjection)
        })
        .transpose()?;
    let budget = usize::try_from(options.budget)
        .ok()
        .and_then(ride_maps::RouteDisplayBudget::new)
        .ok_or(MobileRideDatabaseError::InvalidRouteProjection)?;
    let privacy = match options.privacy {
        MobileRideMapRoutePrivacyPolicyDto::Precise => ride_maps::RoutePrivacyPolicy::Precise,
        MobileRideMapRoutePrivacyPolicyDto::Grid { grid_e7 } => {
            ride_maps::RoutePrivacyPolicy::grid(
                ride_maps::RoutePrivacyGridE7::new(grid_e7)
                    .ok_or(MobileRideDatabaseError::InvalidRouteProjection)?,
            )
        }
    };
    Ok((viewport, budget, privacy))
}

fn mobile_route_projection_options(
    options: &MobileRideMapRouteProjectionOptionsDto,
) -> Result<
    (
        Option<ride_maps::RouteViewport>,
        ride_maps::RouteDisplayBudget,
        ride_maps::RoutePrivacyPolicy,
    ),
    MobileRideMapCoreErrorDto,
> {
    mobile_route_projection_options_for_database(options)
        .map_err(|_| MobileRideMapCoreErrorDto::InvalidRouteProjection)
}

fn mobile_route_display_point_dto(
    point: ride_maps::RouteDisplayPoint,
) -> MobileRideMapRouteDisplayPointDto {
    MobileRideMapRouteDisplayPointDto {
        sequence: point.sequence().as_u64(),
        segment_id: point.segment_id().value(),
        latitude_degrees: point.coordinate().latitude_degrees(),
        longitude_degrees: point.coordinate().longitude_degrees(),
        privacy_class: match point.privacy_class() {
            ride_maps::RoutePrivacyClass::Precise => MobileRideMapRoutePrivacyClassDto::Precise,
            ride_maps::RoutePrivacyClass::GridRedacted => {
                MobileRideMapRoutePrivacyClassDto::GridRedacted
            }
        },
    }
}

fn mobile_segment_start_reason_dto(
    reason: ride_maps::RideSegmentStartReason,
) -> MobileRideSegmentStartReasonDto {
    match reason {
        ride_maps::RideSegmentStartReason::Initial => MobileRideSegmentStartReasonDto::Initial,
        ride_maps::RideSegmentStartReason::Resume => MobileRideSegmentStartReasonDto::Resume,
        ride_maps::RideSegmentStartReason::BackgroundGap => {
            MobileRideSegmentStartReasonDto::BackgroundGap
        }
        ride_maps::RideSegmentStartReason::ImportBoundary => {
            MobileRideSegmentStartReasonDto::ImportBoundary
        }
    }
}

fn mobile_segment_start_reason(
    reason: MobileRideSegmentStartReasonDto,
) -> ride_maps::RideSegmentStartReason {
    match reason {
        MobileRideSegmentStartReasonDto::Initial => ride_maps::RideSegmentStartReason::Initial,
        MobileRideSegmentStartReasonDto::Resume => ride_maps::RideSegmentStartReason::Resume,
        MobileRideSegmentStartReasonDto::BackgroundGap => {
            ride_maps::RideSegmentStartReason::BackgroundGap
        }
        MobileRideSegmentStartReasonDto::ImportBoundary => {
            ride_maps::RideSegmentStartReason::ImportBoundary
        }
    }
}

fn mobile_segment_display_metadata_dto(
    segment: ride_maps::RouteSegmentDisplayMetadata,
) -> MobileRideMapSegmentDisplayMetadataDto {
    MobileRideMapSegmentDisplayMetadataDto {
        segment_id: segment.segment_id().value(),
        start_reason: mobile_segment_start_reason_dto(segment.start_reason()),
        visible_point_count: segment.visible_point_count(),
        canonical_point_count: segment
            .canonical_point_count()
            .map(ride_maps::RidePointCount::as_u64),
        first_visible_sequence: segment
            .first_visible_sequence()
            .map(ride_maps::RidePointSequence::as_u64),
        last_visible_sequence: segment
            .last_visible_sequence()
            .map(ride_maps::RidePointSequence::as_u64),
    }
}

fn mobile_route_projection_dto(
    projection: &persistence::RoutePointProjection,
) -> MobileRideMapRouteProjectionDto {
    MobileRideMapRouteProjectionDto {
        points: projection
            .points()
            .iter()
            .copied()
            .map(mobile_route_display_point_dto)
            .collect(),
        segments: projection
            .segments()
            .iter()
            .copied()
            .map(mobile_segment_display_metadata_dto)
            .collect(),
        source_point_count: projection.source_point_count(),
        source_segment_count: projection.source_segment_count(),
        candidate_point_count: projection.candidate_point_count(),
        candidate_segment_count: projection.candidate_segment_count(),
        displayed_segment_count: projection.displayed_segment_count(),
        background_gap_count: projection.background_gap_count(),
        canonical_start_sequence: projection
            .endpoint_metadata()
            .start_sequence()
            .map(ride_maps::RidePointSequence::as_u64),
        canonical_end_sequence: projection
            .endpoint_metadata()
            .end_sequence()
            .map(ride_maps::RidePointSequence::as_u64),
        canonical_start_visible: projection.endpoint_metadata().start_visible(),
        canonical_end_visible: projection.endpoint_metadata().end_visible(),
        camera_region: projection.camera_region().map(mobile_camera_region_dto),
    }
}

fn mobile_camera_region_dto(
    region: cutout_ride_maps::RouteCameraRegion,
) -> MobileRideMapCameraRegionDto {
    MobileRideMapCameraRegionDto {
        center_latitude_degrees: region.center_latitude_degrees(),
        center_longitude_degrees: region.center_longitude_degrees(),
        latitude_span_degrees: region.latitude_span_degrees(),
        longitude_span_degrees: region.longitude_span_degrees(),
    }
}

fn mobile_history_context_projection_dto(
    projection: &persistence::HistoryContextProjection,
) -> MobileRideHistoryContextProjectionDto {
    MobileRideHistoryContextProjectionDto {
        routes: projection
            .routes()
            .iter()
            .map(|route| MobileRideHistoryContextRouteDto {
                ride_id: MobileRideIdDto {
                    value: route.ride_id().uuid().to_string(),
                },
                projection: mobile_route_projection_dto(route.projection()),
            })
            .collect(),
        source_history_route_count: projection.source_history_route_count(),
        context_route_count: projection.context_route_count(),
        total_display_point_count: projection.total_display_point_count(),
        routes_omitted_by_budget: projection.routes_omitted_by_budget(),
        history_page_has_more: projection.history_page_has_more(),
    }
}

fn mobile_segment_count(
    points: &[ride_maps::RideMapPoint],
    viewport: Option<ride_maps::RouteViewport>,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<u64, MobileRideMapCoreErrorDto> {
    let mut previous_segment = None;
    let mut count = 0usize;
    for point in points.iter().copied() {
        if is_cancelled() {
            return Err(MobileRideMapCoreErrorDto::Cancelled);
        }
        if !viewport.is_none_or(|viewport| viewport.contains(point.sample().coordinate())) {
            continue;
        }
        let segment_id = point.segment_id();
        if previous_segment != Some(segment_id) {
            count += 1;
            previous_segment = Some(segment_id);
        }
    }
    Ok(u64::try_from(count).unwrap_or(u64::MAX))
}

fn mobile_point_count(
    points: &[ride_maps::RideMapPoint],
    viewport: Option<ride_maps::RouteViewport>,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<u64, MobileRideMapCoreErrorDto> {
    let mut count = 0usize;
    for point in points.iter().copied() {
        if is_cancelled() {
            return Err(MobileRideMapCoreErrorDto::Cancelled);
        }
        if viewport.is_none_or(|viewport| viewport.contains(point.sample().coordinate())) {
            count += 1;
        }
    }
    Ok(u64::try_from(count).unwrap_or(u64::MAX))
}

fn mobile_displayed_segment_count(points: &[MobileRideMapRouteDisplayPointDto]) -> u64 {
    let count = ride_maps::count_segment_runs(
        points
            .iter()
            .map(|point| ride_maps::RideMapSegmentId::new(point.segment_id)),
    );
    u64::try_from(count).unwrap_or(u64::MAX)
}

fn mobile_route_display_points(
    points: impl IntoIterator<Item = ride_maps::RouteDisplayPoint>,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<Vec<MobileRideMapRouteDisplayPointDto>, MobileRideMapCoreErrorDto> {
    let mut display_points = Vec::new();
    for point in points {
        if is_cancelled() {
            return Err(MobileRideMapCoreErrorDto::Cancelled);
        }
        display_points.push(mobile_route_display_point_dto(point));
    }
    if is_cancelled() {
        return Err(MobileRideMapCoreErrorDto::Cancelled);
    }
    Ok(display_points)
}

fn mobile_query_limit(value: u32) -> Result<persistence::QueryLimit, MobileRideDatabaseError> {
    persistence::QueryLimit::new(value).map_err(map_ride_database_error)
}

fn mobile_history_query(filter: &MobileRideHistoryFilterDto) -> persistence::RideHistoryQuery {
    persistence::RideHistoryQuery::new(
        filter.created_after_milliseconds,
        filter.vehicle_identity.as_deref(),
        filter.search_text.as_deref(),
    )
}

fn mobile_pevcap_preview(
    preview: &persistence::PevcapImportPreview,
) -> MobilePevcapImportPreviewDto {
    MobilePevcapImportPreviewDto {
        source_path: preview.source_path().to_string_lossy().into_owned(),
        encoding: preview.encoding().into(),
        artifact_digest: preview.artifact_digest().to_owned(),
        artifact_size: preview.artifact_size(),
        record_count: preview.record_count(),
        location_count: preview.location_count(),
        duration_milliseconds: preview.duration_milliseconds(),
        outcome: preview.outcome().into(),
        warnings: preview
            .warnings()
            .iter()
            .map(|warning| match warning {
                persistence::PevcapImportWarning::NoRouteLocations => {
                    MobilePevcapImportWarningDto::NoRouteLocations
                }
            })
            .collect(),
    }
}

fn mobile_pevcap_receipt(
    receipt: persistence::PevcapImportReceipt,
) -> MobilePevcapImportReceiptDto {
    MobilePevcapImportReceiptDto {
        ride_id: receipt.ride_id.map(|ride_id| MobileRideIdDto {
            value: ride_id.uuid().to_string(),
        }),
        artifact_digest: receipt.artifact_digest,
        managed_artifact_path: receipt.managed_artifact_path.to_string_lossy().into_owned(),
        outcome: receipt.outcome.into(),
        record_count: receipt.record_count,
        location_count: receipt.location_count,
        duplicate: receipt.duplicate,
    }
}

fn mobile_ride_location(
    location: MobileRideLocationDto,
) -> Result<ride_maps::LocationSample, MobileRideDatabaseError> {
    let coordinate =
        ride_maps::Coordinate::from_degrees(location.latitude_degrees, location.longitude_degrees)
            .map_err(|_| MobileRideDatabaseError::InvalidCoordinate)?;
    Ok(ride_maps::LocationSample::new(
        coordinate,
        ride_maps::MonotonicMilliseconds::new(location.monotonic_milliseconds),
        ride_maps::WallClockUnixMilliseconds::new(location.wall_clock_unix_milliseconds),
        location.horizontal_accuracy_millimetres,
        match location.source {
            MobileRideSourceDto::Live => ride_maps::LocationSource::Live,
            MobileRideSourceDto::PevcapImport => ride_maps::LocationSource::PevcapImport,
        },
    ))
}

fn mobile_ride_location_dto(location: ride_maps::LocationSample) -> MobileRideLocationDto {
    MobileRideLocationDto {
        latitude_degrees: location.coordinate().latitude_degrees(),
        longitude_degrees: location.coordinate().longitude_degrees(),
        monotonic_milliseconds: location.monotonic_milliseconds().as_u64(),
        wall_clock_unix_milliseconds: location.wall_clock_unix_milliseconds().as_u64(),
        horizontal_accuracy_millimetres: location.horizontal_accuracy_millimetres(),
        source: match location.source() {
            ride_maps::LocationSource::Live => MobileRideSourceDto::Live,
            ride_maps::LocationSource::PevcapImport => MobileRideSourceDto::PevcapImport,
        },
    }
}

/// Rust-owned synchronous ride database handle for mobile clients.
#[derive(Debug, uniffi::Object)]
pub struct RideDatabaseHandle {
    inner: persistence::RideDatabase,
}

/// Cooperative cancellation for one durable route projection.
#[derive(Debug, uniffi::Object)]
pub struct MobileRouteProjectionCancellation {
    inner: persistence::RouteProjectionCancellation,
}

#[uniffi::export]
impl MobileRouteProjectionCancellation {
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: persistence::RouteProjectionCancellation::new(),
        })
    }

    pub fn cancel(&self) {
        self.inner.cancel();
    }
}

/// Cooperative cancellation for one live in-memory route projection.
///
/// This token is intentionally separate from [`MobileRouteProjectionCancellation`]: live
/// projections do not use the `SQLite` worker or its interrupt handle.
#[derive(Debug, uniffi::Object)]
pub struct MobileLiveRouteProjectionCancellation {
    cancelled: Arc<AtomicBool>,
}

#[uniffi::export]
impl MobileLiveRouteProjectionCancellation {
    /// Creates an active live-projection cancellation token.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Requests cancellation of the associated live projection.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl MobileLiveRouteProjectionCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Returns the Rust-owned bounds used by mobile map projections and history pages.
#[uniffi::export]
#[must_use]
pub fn mobile_ride_map_limits() -> MobileRideMapLimitsDto {
    MobileRideMapLimitsDto {
        live_tail_point_limit: u32::try_from(ride_maps::MAX_LIVE_ROUTE_POINTS).unwrap_or(u32::MAX),
        history_preview_point_limit: u32::try_from(ride_maps::MAX_ROUTE_DISPLAY_POINTS)
            .unwrap_or(u32::MAX),
        history_page_limit: persistence::DEFAULT_HISTORY_PAGE_LIMIT,
        history_context_max_routes: persistence::DEFAULT_HISTORY_CONTEXT_ROUTES,
        history_context_per_route_budget: persistence::DEFAULT_HISTORY_CONTEXT_POINTS_PER_ROUTE,
        history_context_total_point_budget: persistence::DEFAULT_HISTORY_CONTEXT_TOTAL_POINTS,
        history_recent_window_milliseconds: persistence::DEFAULT_HISTORY_RECENT_WINDOW_MILLISECONDS,
    }
}

/// Acquires the process-wide Rust-owned ride database service for `path`.
///
/// # Errors
///
/// Returns a stable database error when the path cannot be acquired, migrated, or validated.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)]
pub fn open_ride_database(
    path: String,
) -> Result<Arc<RideDatabaseHandle>, MobileRideDatabaseError> {
    persistence::RideDatabase::open(Path::new(&path))
        .map(|inner| Arc::new(RideDatabaseHandle { inner }))
        .map_err(map_ride_database_error)
}

#[uniffi::export]
impl RideDatabaseHandle {
    /// Returns the stable identity of the process-wide database service.
    #[must_use]
    pub fn service_id(&self) -> String {
        self.inner.service_id().to_string()
    }

    /// Returns the bounded startup recovery state captured while acquiring this service.
    #[must_use]
    pub fn bootstrap_snapshot(&self) -> MobileBootstrapSnapshotDto {
        MobileBootstrapSnapshotDto {
            recovered_rides: self
                .inner
                .bootstrap()
                .recovered_rides()
                .iter()
                .map(|id| MobileRideIdDto {
                    value: id.uuid().to_string(),
                })
                .collect(),
        }
    }

    /// Lists one bounded page of ride history in stable newest-first order.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the cursor, limit, worker, or stored page is invalid.
    pub fn list_rides(
        &self,
        cursor: Option<MobileRideCursorDto>,
        limit: u32,
    ) -> Result<MobileRidePageDto, MobileRideDatabaseError> {
        self.list_rides_filtered(
            cursor,
            MobileRideHistoryFilterDto {
                created_after_milliseconds: None,
                vehicle_identity: None,
                search_text: None,
            },
            limit,
        )
    }

    /// Lists one bounded page of ride history using Rust-owned filters.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the cursor, filter, limit, worker, or stored page is
    /// invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary filter DTOs"
    )]
    pub fn list_rides_filtered(
        &self,
        cursor: Option<MobileRideCursorDto>,
        filter: MobileRideHistoryFilterDto,
        limit: u32,
    ) -> Result<MobileRidePageDto, MobileRideDatabaseError> {
        let cursor = cursor
            .map(|cursor| {
                parse_mobile_ride_id(&cursor.ride_id).map(|ride_id| {
                    persistence::RideCursor::new(cursor.created_at_milliseconds, ride_id)
                })
            })
            .transpose()?;
        let limit = mobile_query_limit(limit)?;
        self.inner
            .list_rides_filtered(cursor, limit, mobile_history_query(&filter))
            .map(|page| MobileRidePageDto {
                rides: page.rides().iter().map(mobile_ride_record_dto).collect(),
                next_cursor: page.next_cursor().map(|cursor| MobileRideCursorDto {
                    created_at_milliseconds: cursor.created_at_milliseconds(),
                    ride_id: MobileRideIdDto {
                        value: cursor.ride_id().uuid().to_string(),
                    },
                }),
            })
            .map_err(map_ride_database_error)
    }

    /// Lists every vehicle identity referenced by visible ride history.
    ///
    /// This is a separate Rust-owned lookup rather than a projection of the first history page,
    /// so the filter remains complete when history contains more than one page or the current
    /// device is disconnected.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the worker cannot query the device identities.
    pub fn list_ride_history_vehicle_options(
        &self,
    ) -> Result<Vec<MobileRideHistoryVehicleOptionDto>, MobileRideDatabaseError> {
        self.inner
            .list_ride_history_vehicle_options()
            .map(|options| {
                options
                    .into_iter()
                    .map(|option| MobileRideHistoryVehicleOptionDto {
                        platform_identifier: option.platform_identifier().to_owned(),
                        display_name: option.display_name().map(str::to_owned),
                    })
                    .collect()
            })
            .map_err(map_ride_database_error)
    }

    /// Finds one visible ride by stable identifier without scanning history pages.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the identifier, worker, or stored record is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary identifiers"
    )]
    pub fn find_ride(
        &self,
        ride_id: MobileRideIdDto,
    ) -> Result<Option<MobileRideRecordDto>, MobileRideDatabaseError> {
        let ride_id = parse_mobile_ride_id(&ride_id)?;
        self.inner
            .find_ride(ride_id)
            .map(|ride| ride.as_ref().map(mobile_ride_record_dto))
            .map_err(map_ride_database_error)
    }

    /// Loads one bounded page of canonical route points in stable sequence order.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the ride, cursor, limit, worker, or stored page is invalid.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn route_points(
        &self,
        ride_id: MobileRideIdDto,
        cursor: Option<MobileRoutePointCursorDto>,
        limit: u32,
    ) -> Result<MobileRoutePointPageDto, MobileRideDatabaseError> {
        let ride_id = parse_mobile_ride_id(&ride_id)?;
        let cursor = cursor.map(|cursor| persistence::RoutePointCursor::new(cursor.sequence));
        let limit = mobile_query_limit(limit)?;
        self.inner
            .route_points(ride_id, cursor, limit)
            .map(|page| MobileRoutePointPageDto {
                points: page
                    .points()
                    .iter()
                    .map(|point| MobileRoutePointDto {
                        sequence: point.sequence(),
                        segment_id: point.segment_id(),
                        location: mobile_ride_location_dto(point.sample()),
                        start_reason: mobile_segment_start_reason_dto(point.start_reason()),
                        telemetry_state: point.telemetry_state().into(),
                    })
                    .collect(),
                next_cursor: page.next_cursor().map(|cursor| MobileRoutePointCursorDto {
                    sequence: cursor.sequence(),
                }),
            })
            .map_err(map_ride_database_error)
    }

    /// Projects bounded contextual routes for a history overview.
    ///
    /// The Rust worker owns history paging, selected-route exclusion, aggregate/per-route
    /// display bounds, viewport filtering, privacy, endpoint, and segment metadata. Swift only
    /// receives the bounded display projections.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the history filter, context budget, projection options,
    /// or database worker is invalid or unavailable.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn project_history_context(
        &self,
        options: MobileRideHistoryContextOptionsDto,
    ) -> Result<MobileRideHistoryContextProjectionDto, MobileRideDatabaseError> {
        let selected_ride = options
            .selected_ride_id
            .as_ref()
            .map(parse_mobile_ride_id)
            .transpose()?;
        let route_options = MobileRideMapRouteProjectionOptionsDto {
            viewport: options.viewport,
            budget: options.budget.per_route_budget,
            privacy: options.privacy,
        };
        let (viewport, _, privacy) = mobile_route_projection_options_for_database(&route_options)?;
        let budget = persistence::HistoryContextBudget::new(
            options.budget.history_page_limit,
            options.budget.max_routes,
            options.budget.per_route_budget,
            options.budget.total_point_budget,
        )
        .map_err(map_ride_database_error)?;
        self.inner
            .project_history_context(
                mobile_history_query(&options.filter),
                selected_ride,
                budget,
                viewport,
                privacy,
            )
            .map(|projection| mobile_history_context_projection_dto(&projection))
            .map_err(map_ride_database_error)
    }

    /// Returns runtime `SQLite` capabilities.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the worker cannot report capabilities.
    pub fn capabilities(&self) -> Result<MobileSqliteCapabilitiesDto, MobileRideDatabaseError> {
        self.inner
            .capabilities()
            .map(|capabilities| {
                let version = capabilities.sqlite_version();
                MobileSqliteCapabilitiesDto {
                    major: version.major(),
                    minor: version.minor(),
                    patch: version.patch(),
                    has_rtree: capabilities.has_rtree(),
                    has_fts5: capabilities.has_fts5(),
                }
            })
            .map_err(map_ride_database_error)
    }

    /// Validates a PEVCAP artifact and returns bounded facts for explicit confirmation.
    ///
    /// # Errors
    ///
    /// Returns a stable database error when the path, encoding, replay, or limits are invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary path strings"
    )]
    pub fn preflight_pevcap(
        &self,
        path: String,
        encoding: MobilePevcapEncodingDto,
    ) -> Result<MobilePevcapImportPreviewDto, MobileRideDatabaseError> {
        self.inner
            .preflight_pevcap(Path::new(&path), encoding.into())
            .map(|preview| mobile_pevcap_preview(&preview))
            .map_err(map_ride_database_error)
    }

    /// Confirms a previously reviewed PEVCAP preview and imports it into managed storage.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the preview changed or the import cannot be committed.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns the confirmation DTO"
    )]
    pub fn confirm_pevcap_import(
        &self,
        preview: MobilePevcapImportPreviewDto,
        created_at_milliseconds: u64,
    ) -> Result<MobilePevcapImportReceiptDto, MobileRideDatabaseError> {
        let current = self
            .inner
            .preflight_pevcap(Path::new(&preview.source_path), preview.encoding.into())
            .map_err(map_ride_database_error)?;
        if mobile_pevcap_preview(&current) != preview {
            return Err(MobileRideDatabaseError::PevcapPreviewChanged);
        }
        self.inner
            .confirm_pevcap_import(&current, created_at_milliseconds)
            .map(mobile_pevcap_receipt)
            .map_err(map_ride_database_error)
    }

    /// Creates an indexed trail definition.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the trail cannot be stored or indexed.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn create_trail(&self, name: String) -> Result<MobileTrailIdDto, MobileRideDatabaseError> {
        self.inner
            .create_trail(&name)
            .map(|id| MobileTrailIdDto {
                value: id.uuid().to_string(),
            })
            .map_err(map_ride_database_error)
    }

    /// Appends and spatially indexes one trail segment.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the trail or spatial index rejects the segment.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn append_trail_segment(
        &self,
        trail_id: MobileTrailIdDto,
        sequence: u32,
        start: MobileMapCoordinateDto,
        end: MobileMapCoordinateDto,
    ) -> Result<(), MobileRideDatabaseError> {
        let trail_id = parse_mobile_trail_id(&trail_id)?;
        let start = mobile_map_coordinate(start)?;
        let end = mobile_map_coordinate(end)?;
        self.inner
            .append_trail_segment(trail_id, sequence, start, end)
            .map_err(map_ride_database_error)
    }

    /// Queries indexed trail segments intersecting a WGS84 bounding box.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the bounds, cursor, limit, or worker is invalid.
    pub fn trail_segments_in_bounds(
        &self,
        bounds: MobileGeoBoundsDto,
        cursor: Option<MobileTrailSegmentCursorDto>,
        limit: u32,
    ) -> Result<MobileTrailSegmentPageDto, MobileRideDatabaseError> {
        let bounds = mobile_geo_bounds(bounds)?;
        let cursor = cursor
            .map(|cursor| {
                parse_mobile_trail_id(&cursor.trail_id)
                    .map(|trail_id| persistence::TrailSegmentCursor::new(trail_id, cursor.sequence))
            })
            .transpose()?;
        let limit = mobile_query_limit(limit)?;
        self.inner
            .trail_segments_in_bounds(bounds, cursor, limit)
            .map(|page| MobileTrailSegmentPageDto {
                segments: page
                    .segments()
                    .iter()
                    .map(|segment| MobileTrailSegmentDto {
                        trail_id: MobileTrailIdDto {
                            value: segment.trail_id.uuid().to_string(),
                        },
                        sequence: segment.sequence,
                        start: mobile_map_coordinate_dto(segment.start),
                        end: mobile_map_coordinate_dto(segment.end),
                    })
                    .collect(),
                next_cursor: page
                    .next_cursor()
                    .map(|cursor| MobileTrailSegmentCursorDto {
                        trail_id: MobileTrailIdDto {
                            value: cursor.trail_id().uuid().to_string(),
                        },
                        sequence: cursor.sequence(),
                    }),
            })
            .map_err(map_ride_database_error)
    }

    /// Stores and spatially indexes one charging/food map point.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the point or spatial index rejects the value.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn create_map_point(
        &self,
        name: String,
        coordinate: MobileMapCoordinateDto,
    ) -> Result<String, MobileRideDatabaseError> {
        let coordinate = mobile_map_coordinate(coordinate)?;
        self.inner
            .create_map_point(&name, coordinate)
            .map(|id| id.uuid().to_string())
            .map_err(map_ride_database_error)
    }

    /// Queries indexed map points intersecting a WGS84 bounding box.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the bounds, cursor, limit, or worker is invalid.
    pub fn map_points_in_bounds(
        &self,
        bounds: MobileGeoBoundsDto,
        cursor: Option<MobileMapPointCursorDto>,
        limit: u32,
    ) -> Result<MobileMapPointPageDto, MobileRideDatabaseError> {
        let bounds = mobile_geo_bounds(bounds)?;
        let cursor = cursor
            .map(|cursor| {
                Uuid::parse_str(&cursor.id)
                    .map(persistence::MapPointId::from_uuid)
                    .map(persistence::MapPointCursor::new)
                    .map_err(|_| MobileRideDatabaseError::InvalidIdentifier)
            })
            .transpose()?;
        let limit = mobile_query_limit(limit)?;
        self.inner
            .map_points_in_bounds(bounds, cursor, limit)
            .map(|page| MobileMapPointPageDto {
                points: page
                    .points()
                    .iter()
                    .map(|point| MobileMapPointDto {
                        id: point.id.uuid().to_string(),
                        name: point.name.clone(),
                        coordinate: mobile_map_coordinate_dto(point.coordinate),
                    })
                    .collect(),
                next_cursor: page.next_cursor().map(|cursor| MobileMapPointCursorDto {
                    id: cursor.id().uuid().to_string(),
                }),
            })
            .map_err(map_ride_database_error)
    }

    /// Rebuilds all derived spatial indexes from their canonical tables.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the spatial schema or worker is unavailable.
    pub fn rebuild_spatial_indexes(&self) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .rebuild_spatial_indexes()
            .map_err(map_ride_database_error)
    }

    /// Writes a consistent `SQLite` backup to a caller-selected file.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the destination or worker cannot complete the backup.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary path strings"
    )]
    pub fn backup_to(&self, path: String) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .backup_to(Path::new(&path))
            .map_err(map_ride_database_error)
    }

    /// Exports one ride summary as a versioned JSON document.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the ride, destination, or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary DTOs and paths"
    )]
    pub fn export_ride_json(
        &self,
        id: MobileRideIdDto,
        path: String,
    ) -> Result<(), MobileRideDatabaseError> {
        let id = parse_mobile_ride_id(&id)?;
        self.inner
            .export_ride_json(id, Path::new(&path))
            .map_err(map_ride_database_error)
    }

    /// Loads the selected platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the worker cannot read the selection.
    pub fn selected_device(&self) -> Result<Option<String>, MobileRideDatabaseError> {
        self.inner
            .selected_device()
            .map_err(map_ride_database_error)
    }

    /// Stores the selected platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the identifier or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn save_selected_device(
        &self,
        platform_identifier: String,
        updated_at_milliseconds: u64,
    ) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .save_selected_device(&platform_identifier, updated_at_milliseconds)
            .map_err(map_ride_database_error)
    }

    /// Stores an optional display name and selects the device atomically.
    ///
    /// Blank or identifier-equal names are ignored by the Rust persistence boundary.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when an identity, name, or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn remember_selected_device(
        &self,
        platform_identifier: String,
        display_name: Option<String>,
        updated_at_milliseconds: u64,
    ) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .remember_selected_device(
                &platform_identifier,
                display_name.as_deref(),
                updated_at_milliseconds,
            )
            .map_err(map_ride_database_error)
    }

    /// Stores a display name for a platform-local device identity.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the identity, display name, or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn save_device_name(
        &self,
        platform_identifier: String,
        display_name: String,
        updated_at_milliseconds: u64,
    ) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .save_device_name(&platform_identifier, &display_name, updated_at_milliseconds)
            .map_err(map_ride_database_error)
    }

    /// Migrates a legacy display name through Rust's canonical device-name policy.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the identity, name, or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn migrate_device_name(
        &self,
        platform_identifier: String,
        display_name: String,
        updated_at_milliseconds: u64,
    ) -> Result<Option<String>, MobileRideDatabaseError> {
        self.inner
            .migrate_device_name(&platform_identifier, &display_name, updated_at_milliseconds)
            .map_err(map_ride_database_error)
    }

    /// Loads a stored display name for a platform-local device identity.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the identity or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn device_name(
        &self,
        platform_identifier: String,
    ) -> Result<Option<String>, MobileRideDatabaseError> {
        self.inner
            .device_name(&platform_identifier)
            .map_err(map_ride_database_error)
    }

    /// Clears the selected platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the worker cannot clear the selection.
    pub fn clear_selected_device(&self) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .clear_selected_device()
            .map_err(map_ride_database_error)
    }

    /// Loads a learned voltage-sag model for one device identity.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the identity or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn voltage_sag_model(
        &self,
        device_identity: String,
    ) -> Result<Option<MobileVoltageSagModelDto>, MobileRideDatabaseError> {
        self.inner
            .voltage_sag_model(&device_identity)
            .map(|model| {
                model.map(|model| MobileVoltageSagModelDto {
                    schema_version: model.schema_version,
                    effective_resistance_milliohms: model.effective_resistance_milliohms,
                    observations: model.observations,
                    hardware_verified: model.hardware_verified,
                })
            })
            .map_err(map_ride_database_error)
    }

    /// Stores a learned voltage-sag model for one device identity.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the model, identity, or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn save_voltage_sag_model(
        &self,
        device_identity: String,
        model: MobileVoltageSagModelDto,
        learned_at_milliseconds: u64,
    ) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .save_voltage_sag_model(
                &device_identity,
                persistence::VoltageSagModelRecord {
                    schema_version: model.schema_version,
                    effective_resistance_milliohms: model.effective_resistance_milliohms,
                    observations: model.observations,
                    hardware_verified: model.hardware_verified,
                    last_learned_wall_clock_milliseconds: learned_at_milliseconds,
                },
            )
            .map_err(map_ride_database_error)
    }

    /// Removes a learned voltage-sag model.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the identity or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn remove_voltage_sag_model(
        &self,
        device_identity: String,
    ) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .remove_voltage_sag_model(&device_identity)
            .map_err(map_ride_database_error)
    }

    /// Loads opaque Rust-owned ride-session marker bytes.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the worker cannot read the marker.
    pub fn ride_session_marker(&self) -> Result<Option<Vec<u8>>, MobileRideDatabaseError> {
        self.inner
            .ride_session_marker()
            .map_err(map_ride_database_error)
    }

    /// Stores opaque Rust-owned ride-session marker bytes.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the marker or worker is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns opaque boundary bytes"
    )]
    pub fn save_ride_session_marker(&self, marker: Vec<u8>) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .save_ride_session_marker(&marker)
            .map_err(map_ride_database_error)
    }

    /// Clears opaque Rust-owned ride-session marker bytes.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the worker cannot clear the marker.
    pub fn clear_ride_session_marker(&self) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .clear_ride_session_marker()
            .map_err(map_ride_database_error)
    }

    /// Creates a draft ride and returns its Rust-created identifier.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the worker cannot create the ride.
    pub fn create_ride(
        &self,
        source: MobileRideSourceDto,
        created_at_milliseconds: u64,
    ) -> Result<MobileRideIdDto, MobileRideDatabaseError> {
        self.inner
            .create_ride(source.into(), created_at_milliseconds)
            .map(|id| MobileRideIdDto {
                value: id.uuid().to_string(),
            })
            .map_err(map_ride_database_error)
    }

    /// Replaces Rust-owned map association and telemetry metadata for a ride.
    ///
    /// Every argument is written as given. A `None` argument clears that stored field, so callers
    /// must provide a complete metadata snapshot rather than a partial update.
    fn create_ride_with_monotonic_start(
        &self,
        source: MobileRideSourceDto,
        created_at_milliseconds: u64,
        monotonic_created_at_milliseconds: Option<u64>,
    ) -> Result<MobileRideIdDto, MobileRideDatabaseError> {
        self.inner
            .create_ride_with_monotonic_start(
                source.into(),
                created_at_milliseconds,
                monotonic_created_at_milliseconds,
            )
            .map(|id| MobileRideIdDto {
                value: id.uuid().to_string(),
            })
            .map_err(map_ride_database_error)
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "owned UniFFI boundary value is converted once"
    )]
    fn create_started_ride_with_monotonic_start(
        &self,
        source: MobileRideSourceDto,
        created_at_milliseconds: u64,
        monotonic_created_at_milliseconds: Option<u64>,
        candidate_vehicle: Option<String>,
    ) -> Result<MobileRideIdDto, MobileRideDatabaseError> {
        self.inner
            .create_started_ride_with_monotonic_start(
                source.into(),
                created_at_milliseconds,
                monotonic_created_at_milliseconds,
                candidate_vehicle.as_deref(),
            )
            .map(|id| MobileRideIdDto {
                value: id.uuid().to_string(),
            })
            .map_err(map_ride_database_error)
    }

    fn monotonic_created_at_milliseconds(
        &self,
        id: &MobileRideIdDto,
    ) -> Result<Option<u64>, MobileRideDatabaseError> {
        let id = parse_mobile_ride_id(id)?;
        self.inner
            .find_ride(id)
            .map(|ride| ride.and_then(|ride| ride.monotonic_created_at_milliseconds()))
            .map_err(map_ride_database_error)
    }

    /// Persists Rust-owned map association and telemetry metadata for a ride.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the metadata or worker rejects the update.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary DTOs and strings"
    )]
    pub fn update_ride_map_metadata(
        &self,
        id: MobileRideIdDto,
        candidate_vehicle: Option<String>,
        associated_vehicle: Option<String>,
        associated_at_milliseconds: Option<u64>,
        last_telemetry_at_milliseconds: Option<u64>,
    ) -> Result<(), MobileRideDatabaseError> {
        let id = parse_mobile_ride_id(&id)?;
        self.inner
            .update_ride_map_metadata(
                id,
                candidate_vehicle.as_deref(),
                associated_vehicle.as_deref(),
                associated_at_milliseconds,
                last_telemetry_at_milliseconds,
            )
            .map_err(map_ride_database_error)
    }

    /// Applies a lifecycle event to a Rust-created ride.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the transition or worker rejects the event.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn transition(
        &self,
        id: MobileRideIdDto,
        event: MobileRideEventDto,
        monotonic_at_milliseconds: u64,
    ) -> Result<MobileRideLifecycleStateDto, MobileRideDatabaseError> {
        let id = parse_mobile_ride_id(&id)?;
        self.inner
            .transition_at(id, event.into(), monotonic_at_milliseconds)
            .map(Into::into)
            .map_err(map_ride_database_error)
    }

    /// Appends a location sample and reports the Rust admission decision.
    ///
    /// A successful result can be `Accepted`, `Duplicate`, `OutOfOrder`, `AccuracyTooLow`, or
    /// `UnrealisticJump`; only `Accepted` adds the point to the route.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the sample, ride, or worker rejects the append.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn append_location(
        &self,
        id: MobileRideIdDto,
        location: MobileRideLocationDto,
    ) -> Result<MobileRideLocationAdmissionDto, MobileRideDatabaseError> {
        self.append_location_with_segment(id, location, 0)
    }

    /// Appends a location sample with its Rust-owned route segment identity and reports admission.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the sample, ride, or worker rejects the append.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn append_location_with_segment(
        &self,
        id: MobileRideIdDto,
        location: MobileRideLocationDto,
        segment_id: u64,
    ) -> Result<MobileRideLocationAdmissionDto, MobileRideDatabaseError> {
        let id = parse_mobile_ride_id(&id)?;
        let location = mobile_ride_location(location)?;
        self.inner
            .append_location_with_segment(id, location, segment_id)
            .map(Into::into)
            .map_err(map_ride_database_error)
    }

    /// Appends a location sample with segment and telemetry provenance and reports admission.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the sample, ride, or worker rejects the append.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn append_location_with_segment_and_telemetry(
        &self,
        id: MobileRideIdDto,
        location: MobileRideLocationDto,
        segment_id: u64,
        telemetry_state: MobileRideMapCoreTelemetryStateDto,
    ) -> Result<MobileRideLocationAdmissionDto, MobileRideDatabaseError> {
        let id = parse_mobile_ride_id(&id)?;
        let location = mobile_ride_location(location)?;
        self.inner
            .append_location_with_segment_and_telemetry(
                id,
                location,
                segment_id,
                map_ride_telemetry_state(telemetry_state)
                    .map_err(|_| MobileRideDatabaseError::StorageFailure)?,
            )
            .map(Into::into)
            .map_err(map_ride_database_error)
    }

    /// Enqueues a location sample for ordered background persistence and returns durable admission.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the durable append is rejected by the worker. A
    /// successful result still reports duplicate, out-of-order, or other admission outcomes.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn enqueue_location_with_segment_and_telemetry(
        &self,
        id: MobileRideIdDto,
        location: MobileRideLocationDto,
        segment_id: u64,
        telemetry_state: MobileRideMapCoreTelemetryStateDto,
    ) -> Result<MobileRideLocationAdmissionDto, MobileRideDatabaseError> {
        let id = parse_mobile_ride_id(&id)?;
        let location = mobile_ride_location(location)?;
        self.inner
            .append_location_with_segment_and_telemetry(
                id,
                location,
                segment_id,
                map_ride_telemetry_state(telemetry_state)
                    .map_err(|_| MobileRideDatabaseError::StorageFailure)?,
            )
            .map(Into::into)
            .map_err(map_ride_database_error)
    }

    /// Projects one durable route through Rust-owned viewport, LOD, and privacy policy.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the ride ID, options, or worker is invalid.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn project_route_points(
        &self,
        ride_id: MobileRideIdDto,
        options: MobileRideMapRouteProjectionOptionsDto,
    ) -> Result<MobileRideMapRouteProjectionDto, MobileRideDatabaseError> {
        let ride_id = parse_mobile_ride_id(&ride_id)?;
        let (viewport, budget, privacy) = mobile_route_projection_options_for_database(&options)?;
        self.inner
            .project_route_points(ride_id, viewport, budget, privacy)
            .map(|projection| mobile_route_projection_dto(&projection))
            .map_err(map_ride_database_error)
    }

    /// Projects one durable route while honoring cooperative cancellation.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the ride ID, options, cancellation token, or worker is
    /// invalid.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn project_route_points_cancellable(
        &self,
        ride_id: MobileRideIdDto,
        options: MobileRideMapRouteProjectionOptionsDto,
        cancellation: Arc<MobileRouteProjectionCancellation>,
    ) -> Result<MobileRideMapRouteProjectionDto, MobileRideDatabaseError> {
        let ride_id = parse_mobile_ride_id(&ride_id)?;
        let (viewport, budget, privacy) = mobile_route_projection_options_for_database(&options)?;
        self.inner
            .project_route_points_cancellable(
                ride_id,
                viewport,
                budget,
                privacy,
                &cancellation.inner,
            )
            .map(|projection| mobile_route_projection_dto(&projection))
            .map_err(map_ride_database_error)
    }

    /// Loads the durable summary projection for a ride.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the ride or worker cannot provide its summary.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn summary(
        &self,
        id: MobileRideIdDto,
    ) -> Result<MobileRideSummaryDto, MobileRideDatabaseError> {
        let id = parse_mobile_ride_id(&id)?;
        self.inner
            .summary_with_duration(id)
            .map(|(summary, duration_milliseconds)| MobileRideSummaryDto {
                point_count: summary.point_count().as_u64(),
                distance_millimetres: summary.distance_millimetres(),
                average_speed_millimetres_per_second: summary
                    .average_speed_millimetres_per_second(duration_milliseconds)
                    .map(ride_maps::AverageSpeedMillimetresPerSecond::as_u64),
            })
            .map_err(map_ride_database_error)
    }

    /// Stops the process-wide worker and invalidates every handle to it.
    ///
    /// This is an explicit process-wide teardown operation. Callers must not use any
    /// `RideDatabaseHandle` after shutdown; subsequent requests from other handles return
    /// `WorkerStopped` until the process opens a new database service.
    ///
    /// # Errors
    ///
    /// Returns a typed database error when the worker cannot stop cleanly.
    pub fn shutdown(&self) -> Result<(), MobileRideDatabaseError> {
        self.inner
            .clone()
            .shutdown()
            .map_err(map_ride_database_error)
    }
}

impl RideDatabaseHandle {
    #[allow(
        clippy::needless_pass_by_value,
        reason = "owned UniFFI boundary value is converted once"
    )]
    fn transition_at(
        &self,
        id: MobileRideIdDto,
        event: MobileRideEventDto,
        monotonic_at_milliseconds: u64,
    ) -> Result<(), MobileRideDatabaseError> {
        let id = parse_mobile_ride_id(&id)?;
        self.inner
            .transition_at(id, event.into(), monotonic_at_milliseconds)
            .map(|_| ())
            .map_err(map_ride_database_error)
    }

    fn create_started_live_ride(
        &self,
        created_at_milliseconds: u64,
        monotonic_created_at_milliseconds: u64,
        candidate_vehicle: Option<&str>,
    ) -> Result<MobileRideIdDto, MobileRideDatabaseError> {
        self.inner
            .create_started_live_ride(
                created_at_milliseconds,
                monotonic_created_at_milliseconds,
                candidate_vehicle,
            )
            .map(|id| MobileRideIdDto {
                value: id.uuid().to_string(),
            })
            .map_err(map_ride_database_error)
    }

    fn newest_recoverable_ride(
        &self,
    ) -> Result<Option<MobileRideRecordDto>, MobileRideDatabaseError> {
        self.inner
            .newest_recoverable_ride()
            .map(|ride| ride.as_ref().map(mobile_ride_record_dto))
            .map_err(map_ride_database_error)
    }
}

/// Rust-owned live ride map state. The mutex protects callbacks arriving from different Apple
/// delegate queues while the database handle keeps durable lifecycle and route writes in Rust.
fn queue_location(
    database: &RideDatabaseHandle,
    id: &MobileRideIdDto,
    location: MobileRideLocationDto,
    segment_id: ride_maps::RideMapSegmentId,
    start_reason: ride_maps::RideSegmentStartReason,
    telemetry_state: MobileRideMapCoreTelemetryStateDto,
) -> Result<persistence::PendingLocationWrite, MobileRideDatabaseError> {
    let id = parse_mobile_ride_id(id)?;
    let location = mobile_ride_location(location)?;
    database
        .inner
        .queue_location(
            id,
            location,
            segment_id,
            start_reason,
            map_ride_telemetry_state(telemetry_state)
                .map_err(|_| MobileRideDatabaseError::StorageFailure)?,
        )
        .map_err(map_ride_database_error)
}

#[derive(Debug, uniffi::Object)]
pub struct MobileRideMapCore {
    inner: Mutex<MobileRideMapCoreInner>,
}

const MAX_PENDING_LOCATION_WRITES: usize = 64;

#[derive(Debug)]
struct MobileRideMapCoreInner {
    database: Option<Arc<RideDatabaseHandle>>,
    active_ride_id: Option<MobileRideIdDto>,
    settled_ride_id: Option<MobileRideIdDto>,
    recorder: ride_maps::RideMapRecorder,
    admission_recorder: ride_maps::RideMapRecorder,
    pending_location_writes: VecDeque<PendingMapLocationWrite>,
    initialization_error: Option<MobileRideMapCoreErrorDto>,
}

#[derive(Debug)]
struct PendingMapLocationWrite {
    ride_id: MobileRideIdDto,
    sample: ride_maps::LocationSample,
    point: MobileRideMapCorePointDto,
    segment_started: bool,
    write: persistence::PendingLocationWrite,
}

impl MobileRideMapCoreInner {
    fn transition_state(
        &mut self,
        event: MobileRideEventDto,
        at_milliseconds: u64,
    ) -> Result<(MobileRideIdDto, ride_maps::RideLifecycleState), MobileRideMapCoreErrorDto> {
        let Some(id) = self.active_ride_id.clone() else {
            return Err(MobileRideMapCoreErrorDto::NoActiveRide);
        };
        let Some(current) = self.recorder.state() else {
            return Err(MobileRideMapCoreErrorDto::NoActiveRide);
        };
        let next = current
            .apply(event.into())
            .map_err(|_| MobileRideMapCoreErrorDto::InvalidTransition)?;
        if let Some(database) = self.database.as_ref() {
            database
                .transition_at(id.clone(), event, at_milliseconds)
                .map_err(map_core_error)?;
        }
        Ok((id, next))
    }

    fn ingest_location(
        &mut self,
        monotonic_ms: u64,
        wall_clock_unix_ms: u64,
        latitude_degrees: f64,
        longitude_degrees: f64,
        horizontal_accuracy_meters: f64,
    ) -> Result<MobileRideMapCoreDecisionDto, MobileRideMapCoreErrorDto> {
        let Some(id) = self.active_ride_id.clone() else {
            return Err(MobileRideMapCoreErrorDto::NoActiveRide);
        };
        if self.admission_recorder.state() != Some(ride_maps::RideLifecycleState::Active) {
            return Ok(MobileRideMapCoreDecisionDto::Ignored {
                reason: MobileRideMapDecisionReasonDto::RideNotRecording,
            });
        }
        let location = MobileRideLocationDto {
            latitude_degrees,
            longitude_degrees,
            monotonic_milliseconds: monotonic_ms,
            wall_clock_unix_milliseconds: wall_clock_unix_ms,
            horizontal_accuracy_millimetres: Some(horizontal_accuracy_millimetres(
                horizontal_accuracy_meters,
            )?),
            source: MobileRideSourceDto::Live,
        };
        if wall_clock_unix_ms == 0 {
            return Err(MobileRideMapCoreErrorDto::InvalidLocation);
        }
        let sample = mobile_ride_location(location).map_err(map_core_error)?;
        match self.admission_recorder.check_sample(&sample) {
            ride_maps::LocationAdmission::Duplicate => {
                return Ok(MobileRideMapCoreDecisionDto::Ignored {
                    reason: MobileRideMapDecisionReasonDto::DuplicateLocation,
                });
            }
            ride_maps::LocationAdmission::OutOfOrder => {
                return Ok(MobileRideMapCoreDecisionDto::Rejected {
                    reason: MobileRideMapDecisionReasonDto::TimestampOutOfOrder,
                });
            }
            ride_maps::LocationAdmission::AccuracyTooLow => {
                return Ok(MobileRideMapCoreDecisionDto::Rejected {
                    reason: MobileRideMapDecisionReasonDto::AccuracyTooLow,
                });
            }
            ride_maps::LocationAdmission::UnrealisticJump => {
                return Ok(MobileRideMapCoreDecisionDto::Rejected {
                    reason: MobileRideMapDecisionReasonDto::UnrealisticJump,
                });
            }
            ride_maps::LocationAdmission::Accepted => {}
        }
        let telemetry_state =
            self.admission_recorder
                .telemetry_state_at(ride_maps::MonotonicMilliseconds::new(
                    location.monotonic_milliseconds,
                ));
        let sequence = self.admission_recorder.point_count();
        let (segment_id, segment_started, start_reason) =
            self.admission_recorder.next_sample_metadata(sample);
        let point = Self::point_from_location(
            location,
            sequence,
            segment_id,
            start_reason,
            telemetry_state,
        );
        if let Some(database) = self.database.as_ref() {
            if self.pending_location_writes.len() >= MAX_PENDING_LOCATION_WRITES {
                return Ok(MobileRideMapCoreDecisionDto::StorageError {
                    message: "ride location write queue is full".to_owned(),
                });
            }
            let write = queue_location(
                database,
                &id,
                location,
                segment_id,
                start_reason,
                telemetry_state.into(),
            )
            .map_err(map_core_error)?;
            self.admission_recorder.record_sample(sample);
            self.pending_location_writes
                .push_back(PendingMapLocationWrite {
                    ride_id: id,
                    sample,
                    point,
                    segment_started,
                    write,
                });
            return Ok(MobileRideMapCoreDecisionDto::Pending {
                point,
                segment_started,
            });
        }
        self.admission_recorder.record_sample(sample);
        self.recorder = self.admission_recorder.clone();
        Ok(MobileRideMapCoreDecisionDto::Accepted {
            point,
            segment_started,
        })
    }

    fn new(database: Option<Arc<RideDatabaseHandle>>) -> Self {
        let mut state = Self {
            database,
            active_ride_id: None,
            settled_ride_id: None,
            recorder: ride_maps::RideMapRecorder::new(),
            admission_recorder: ride_maps::RideMapRecorder::new(),
            pending_location_writes: VecDeque::new(),
            initialization_error: None,
        };
        if let Err(error) = state.restore_active_ride() {
            state.initialization_error = Some(error);
        }
        state
    }

    fn restored_route_samples(
        database: &RideDatabaseHandle,
        ride_id: &MobileRideIdDto,
        point_count: u64,
    ) -> Result<Vec<ride_maps::RideMapPoint>, MobileRideMapCoreErrorDto> {
        let tail_start = point_count.saturating_sub(ride_maps::MAX_LIVE_ROUTE_POINTS as u64);
        let mut cursor = tail_start
            .checked_sub(1)
            .map(|sequence| MobileRoutePointCursorDto { sequence });
        let mut samples = Vec::new();
        loop {
            let page = database
                .route_points(ride_id.clone(), cursor, 500)
                .map_err(map_core_error)?;
            samples.extend(
                page.points
                    .into_iter()
                    .map(|point| {
                        mobile_ride_location(point.location).and_then(|sample| {
                            map_ride_telemetry_state(point.telemetry_state)
                                .map_err(|_| MobileRideDatabaseError::StorageFailure)
                                .map(|telemetry_state| {
                                    ride_maps::RideMapPoint::new_with_start_reason(
                                        sample,
                                        ride_maps::RideMapSegmentId::new(point.segment_id),
                                        telemetry_state,
                                        mobile_segment_start_reason(point.start_reason),
                                    )
                                })
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(map_core_error)?,
            );
            if samples.len() > ride_maps::MAX_LIVE_ROUTE_POINTS {
                let excess = samples.len() - ride_maps::MAX_LIVE_ROUTE_POINTS;
                samples.drain(..excess);
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(samples);
            }
        }
    }

    fn restore_active_ride(&mut self) -> Result<(), MobileRideMapCoreErrorDto> {
        let Some(database) = self.database.as_ref() else {
            return Ok(());
        };
        let Some(ride) = database.newest_recoverable_ride().map_err(map_core_error)? else {
            return Ok(());
        };
        let background_gap_count = database
            .project_route_points(
                ride.id.clone(),
                MobileRideMapRouteProjectionOptionsDto {
                    viewport: None,
                    budget: 1,
                    privacy: MobileRideMapRoutePrivacyPolicyDto::Precise,
                },
            )
            .map_err(map_core_error)?
            .background_gap_count;
        let samples = Self::restored_route_samples(database, &ride.id, ride.summary.point_count)?;
        let last_restored_monotonic = samples
            .last()
            .map(|sample| sample.sample().monotonic_milliseconds().as_u64());
        let restored_start_milliseconds = ride
            .monotonic_created_at_milliseconds
            .or_else(|| {
                last_restored_monotonic.map(|last| last.saturating_sub(ride.duration_milliseconds))
            })
            .unwrap_or(0);
        let timing_last = ride
            .monotonic_last_event_milliseconds
            .or(last_restored_monotonic)
            .unwrap_or(restored_start_milliseconds);
        let timing = ride_maps::RideRecordingTiming::new(
            ride_maps::MonotonicMilliseconds::new(timing_last),
            ride.paused_at_milliseconds
                .map(ride_maps::MonotonicMilliseconds::new),
            ride_maps::RideDurationMilliseconds::new(ride.paused_duration_milliseconds),
            ride_maps::RideDurationMilliseconds::new(ride.completed_duration_milliseconds),
        );
        self.recorder = ride_maps::RideMapRecorder::restored_with_metadata_and_summary_and_timing(
            map_ride_lifecycle_state(ride.state),
            ride_maps::MonotonicMilliseconds::new(restored_start_milliseconds),
            ride_maps::RideMapMetadata {
                candidate_vehicle: ride
                    .candidate_vehicle
                    .as_deref()
                    .and_then(ride_maps::VehicleIdentity::new),
                associated_vehicle: ride
                    .associated_vehicle
                    .as_deref()
                    .and_then(ride_maps::VehicleIdentity::new),
                associated_at_milliseconds: ride
                    .associated_at_milliseconds
                    .map(ride_maps::MonotonicMilliseconds::new),
                last_telemetry_at_milliseconds: ride
                    .last_telemetry_at_milliseconds
                    .map(ride_maps::MonotonicMilliseconds::new),
                background_gap_count: ride_maps::BackgroundGapCount::new(background_gap_count),
            },
            samples,
            ride_maps::RideSummary::from_stored(
                ride_maps::RidePointCount::new(ride.summary.point_count),
                ride.summary.distance_millimetres,
            ),
            timing,
        );
        self.active_ride_id = Some(ride.id);
        self.admission_recorder = self.recorder.clone();
        Ok(())
    }

    fn point_from_location(
        location: MobileRideLocationDto,
        sequence: u64,
        segment_id: ride_maps::RideMapSegmentId,
        start_reason: ride_maps::RideSegmentStartReason,
        telemetry_state: ride_maps::RouteTelemetryState,
    ) -> MobileRideMapCorePointDto {
        MobileRideMapCorePointDto {
            sequence,
            segment_id: segment_id.value(),
            start_reason: mobile_segment_start_reason_dto(start_reason),
            latitude_degrees: location.latitude_degrees,
            longitude_degrees: location.longitude_degrees,
            wall_clock_unix_ms: location.wall_clock_unix_milliseconds,
            monotonic_ms: location.monotonic_milliseconds,
            horizontal_accuracy_meters: location
                .horizontal_accuracy_millimetres
                .map(|value| f64::from(value) / 1_000.0),
            telemetry_state: telemetry_state.into(),
        }
    }

    fn summary(&self, state: MobileRideLifecycleStateDto) -> MobileRideMapCoreSummaryDto {
        let persisted = (!matches!(
            state,
            MobileRideLifecycleStateDto::Active | MobileRideLifecycleStateDto::Paused
        ))
        .then_some(self.active_ride_id.as_ref())
        .flatten()
        .and_then(|id| {
            self.database
                .as_ref()
                .and_then(|database| database.summary(id.clone()).ok())
        });
        let (point_count, distance_meters) = persisted.map_or_else(
            || {
                let summary = self.recorder.summary();
                (
                    summary.point_count().as_u64(),
                    millimetres_to_meters(summary.distance_millimetres()),
                )
            },
            |summary| {
                (
                    summary.point_count,
                    millimetres_to_meters(summary.distance_millimetres),
                )
            },
        );
        MobileRideMapCoreSummaryDto {
            point_count,
            distance_meters,
            duration_milliseconds: self.recorder.duration_milliseconds().as_u64(),
        }
    }

    fn snapshot(&self, state: MobileRideLifecycleStateDto) -> MobileRideMapCoreSnapshotDto {
        MobileRideMapCoreSnapshotDto {
            ride_id: self
                .active_ride_id
                .as_ref()
                .map_or_else(String::new, |id| id.value.clone()),
            state,
            summary: self.summary(state),
            segment_count: self.recorder.segment_count().as_u64(),
            associated_vehicle: self.recorder.associated_vehicle().map(str::to_owned),
            recorded_bounds_available: matches!(
                state,
                MobileRideLifecycleStateDto::Stopped
                    | MobileRideLifecycleStateDto::Interrupted
                    | MobileRideLifecycleStateDto::Discarded
                    | MobileRideLifecycleStateDto::Saved
                    | MobileRideLifecycleStateDto::Imported
            ),
        }
    }

    fn snapshot_at(
        &self,
        state: MobileRideLifecycleStateDto,
        at_milliseconds: u64,
    ) -> MobileRideMapCoreSnapshotDto {
        let mut snapshot = self.snapshot(state);
        snapshot.summary.duration_milliseconds = self
            .recorder
            .duration_milliseconds_at(ride_maps::MonotonicMilliseconds::new(at_milliseconds))
            .as_u64();
        snapshot
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "caller-owned candidate identity is retained"
    )]
    fn start_gps_only(
        &mut self,
        at_ms: u64,
        last_connected_vehicle: Option<String>,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        if self.recorder.state().is_some_and(|current| {
            !matches!(
                current,
                ride_maps::RideLifecycleState::Stopped
                    | ride_maps::RideLifecycleState::Interrupted
                    | ride_maps::RideLifecycleState::Saved
                    | ride_maps::RideLifecycleState::Discarded
            )
        }) {
            return Err(MobileRideMapCoreErrorDto::AlreadyRecording);
        }
        let mut staged_recorder = self.recorder.clone();
        staged_recorder
            .start(
                ride_maps::MonotonicMilliseconds::new(at_ms),
                last_connected_vehicle
                    .as_deref()
                    .and_then(ride_maps::VehicleIdentity::new),
            )
            .map_err(|_| MobileRideMapCoreErrorDto::AlreadyRecording)?;
        let id = if let Some(database) = self.database.as_ref() {
            let wall_clock_milliseconds: u64 = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| MobileRideMapCoreErrorDto::Storage(error.to_string()))?
                .as_millis()
                .try_into()
                .map_err(|error: std::num::TryFromIntError| {
                    MobileRideMapCoreErrorDto::Storage(error.to_string())
                })?;
            database
                .create_started_live_ride(
                    wall_clock_milliseconds,
                    at_ms,
                    last_connected_vehicle.as_deref(),
                )
                .map_err(map_core_error)?
        } else {
            MobileRideIdDto {
                value: Uuid::new_v4().to_string(),
            }
        };
        self.recorder = staged_recorder.clone();
        self.admission_recorder = staged_recorder;
        self.active_ride_id = Some(id);
        self.settled_ride_id = None;
        self.pending_location_writes.clear();
        Ok(self.snapshot(MobileRideLifecycleStateDto::Active))
    }
}

fn map_ride_lifecycle_state(state: MobileRideLifecycleStateDto) -> ride_maps::RideLifecycleState {
    match state {
        MobileRideLifecycleStateDto::Draft => ride_maps::RideLifecycleState::Draft,
        MobileRideLifecycleStateDto::Active => ride_maps::RideLifecycleState::Active,
        MobileRideLifecycleStateDto::Paused => ride_maps::RideLifecycleState::Paused,
        MobileRideLifecycleStateDto::Stopped => ride_maps::RideLifecycleState::Stopped,
        MobileRideLifecycleStateDto::Interrupted => ride_maps::RideLifecycleState::Interrupted,
        MobileRideLifecycleStateDto::Discarded => ride_maps::RideLifecycleState::Discarded,
        MobileRideLifecycleStateDto::Saved => ride_maps::RideLifecycleState::Saved,
        MobileRideLifecycleStateDto::Imported => ride_maps::RideLifecycleState::Imported,
    }
}

fn map_ride_telemetry_state(
    state: MobileRideMapCoreTelemetryStateDto,
) -> Result<ride_maps::RouteTelemetryState, &'static str> {
    match state {
        MobileRideMapCoreTelemetryStateDto::GpsOnly => Ok(ride_maps::RouteTelemetryState::GpsOnly),
        MobileRideMapCoreTelemetryStateDto::AssociatedNoTelemetry => {
            Ok(ride_maps::RouteTelemetryState::AssociatedNoTelemetry)
        }
        MobileRideMapCoreTelemetryStateDto::AssociatedFresh => {
            Ok(ride_maps::RouteTelemetryState::AssociatedFresh)
        }
        MobileRideMapCoreTelemetryStateDto::AssociatedStale => {
            Ok(ride_maps::RouteTelemetryState::AssociatedStale)
        }
        MobileRideMapCoreTelemetryStateDto::Unknown => Err("unsupported telemetry state"),
    }
}

fn millimetres_to_meters(value: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f64 / 1_000.0
    }
}

#[allow(clippy::missing_errors_doc)]
fn map_ride_map_error(error: &MobileRideDatabaseError) -> MobileRideMapCoreErrorDto {
    MobileRideMapCoreErrorDto::Storage(error.to_string())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn horizontal_accuracy_millimetres(value: f64) -> Result<u32, MobileRideMapCoreErrorDto> {
    if !value.is_finite() || value < 0.0 {
        return Err(MobileRideMapCoreErrorDto::InvalidLocation);
    }
    let millimetres = value * 1_000.0;
    if !millimetres.is_finite() || millimetres > f64::from(u32::MAX) {
        return Err(MobileRideMapCoreErrorDto::InvalidLocation);
    }
    Ok(millimetres as u32)
}

fn empty_map_point_batch() -> MobileRideMapCorePointBatchDto {
    MobileRideMapCorePointBatchDto {
        points: Vec::new(),
        next_cursor: None,
        has_more: false,
    }
}

#[uniffi::export]
impl MobileRideMapCore {
    /// Creates a Rust-owned map state without durable storage, for deterministic UI tests.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(MobileRideMapCoreInner::new(None)),
        })
    }

    /// Creates a Rust-owned map state backed by the process-wide ride database.
    #[uniffi::constructor]
    #[must_use]
    pub fn with_database(database: Arc<RideDatabaseHandle>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(MobileRideMapCoreInner::new(Some(database))),
        })
    }

    /// Returns the active ride snapshot, if one exists.
    ///
    /// Returns the active ride snapshot evaluated at the supplied monotonic timestamp.
    ///
    /// Requiring the caller's current monotonic timestamp makes live duration independent of
    /// Core Location callback frequency.
    pub fn current_snapshot(&self, at_ms: u64) -> Option<MobileRideMapCoreSnapshotDto> {
        let state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .active_ride_id
            .as_ref()
            .zip(state.recorder.state())
            .map(|(_, active)| state.snapshot_at(active.into(), at_ms))
    }

    /// Returns a storage error encountered while restoring the previous ride projection.
    #[must_use]
    pub fn initialization_error(&self) -> Option<MobileRideMapCoreErrorDto> {
        let state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        state.initialization_error.clone()
    }

    /// Starts a GPS-only ride and retains the last connected vehicle as a candidate.
    ///
    /// `at_ms` is a monotonic timestamp from the caller; durable history ordering uses a separate
    /// wall-clock timestamp captured by Rust.
    ///
    /// # Errors
    ///
    /// Returns an error when another ride is open or durable storage rejects creation.
    pub fn start_gps_only(
        &self,
        at_ms: u64,
        last_connected_vehicle: Option<String>,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        state.start_gps_only(at_ms, last_connected_vehicle)
    }

    /// Ensures a live map ride exists for a connected vehicle and associates it.
    ///
    /// A fresh connection starts a new live ride when no open ride exists. An already-open
    /// GPS-only ride is associated with this vehicle, preserving the route recorded before the
    /// Bluetooth connection was available.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is invalid, no ride can be created, or association
    /// metadata cannot be persisted.
    pub fn ensure_recording_for_vehicle(
        &self,
        platform_identifier: String,
        at_ms: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if state.recorder.state().is_none_or(|current| {
            matches!(
                current,
                ride_maps::RideLifecycleState::Stopped
                    | ride_maps::RideLifecycleState::Interrupted
                    | ride_maps::RideLifecycleState::Saved
                    | ride_maps::RideLifecycleState::Discarded
            )
        }) {
            state.start_gps_only(at_ms, Some(platform_identifier.clone()))?;
        }

        let mut staged = state.admission_recorder.clone();
        let mut durable_staged = state.recorder.clone();
        let Some(identity) = ride_maps::VehicleIdentity::new(&platform_identifier) else {
            return Ok(state.snapshot(
                state
                    .recorder
                    .state()
                    .ok_or(MobileRideMapCoreErrorDto::NoActiveRide)?
                    .into(),
            ));
        };
        let association =
            staged.observe_vehicle(&identity, ride_maps::MonotonicMilliseconds::new(at_ms));
        if association == ride_maps::VehicleAssociation::Associated {
            let _ = durable_staged
                .observe_vehicle(&identity, ride_maps::MonotonicMilliseconds::new(at_ms));
            if let (Some(database), Some(id)) =
                (state.database.as_ref(), state.active_ride_id.clone())
            {
                database
                    .update_ride_map_metadata(
                        id,
                        staged.candidate_vehicle().map(str::to_owned),
                        Some(platform_identifier),
                        staged
                            .associated_at_milliseconds()
                            .map(ride_maps::MonotonicMilliseconds::as_u64),
                        staged
                            .last_telemetry_at_milliseconds()
                            .map(ride_maps::MonotonicMilliseconds::as_u64),
                    )
                    .map_err(map_core_error)?;
            }
        }
        state.recorder = durable_staged;
        state.admission_recorder = staged;
        let lifecycle = state
            .recorder
            .state()
            .ok_or(MobileRideMapCoreErrorDto::NoActiveRide)?;
        Ok(state.snapshot(lifecycle.into()))
    }

    /// Pauses the active ride.
    ///
    /// # Errors
    ///
    /// Returns an error when no active ride exists or the lifecycle transition is invalid.
    pub fn pause(
        &self,
        at_ms: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        self.transition_at(MobileRideEventDto::Pause, at_ms)
    }

    /// Evaluates the pause transition at the supplied monotonic timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when no active ride exists or the transition is invalid.
    pub fn pause_at(
        &self,
        at_ms: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        self.transition_at(MobileRideEventDto::Pause, at_ms)
    }

    /// Resumes the paused ride.
    ///
    /// # Errors
    ///
    /// Returns an error when no paused ride exists or durable storage rejects the transition.
    pub fn resume(
        &self,
        at_ms: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        self.transition_at(MobileRideEventDto::Resume, at_ms)
    }

    /// Evaluates the resume transition at the supplied monotonic timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when no paused ride exists or the transition is invalid.
    pub fn resume_at(
        &self,
        at_ms: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        self.transition_at(MobileRideEventDto::Resume, at_ms)
    }

    /// Stops the active or paused ride.
    ///
    /// # Errors
    ///
    /// Returns an error when no open ride exists or durable storage rejects the transition.
    pub fn stop(
        &self,
        at_ms: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        self.transition_at(MobileRideEventDto::Stop, at_ms)
    }

    /// Evaluates the stop transition at the supplied monotonic timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when no open ride exists or the transition is invalid.
    pub fn stop_at(
        &self,
        at_ms: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        self.transition_at(MobileRideEventDto::Stop, at_ms)
    }

    /// Saves a stopped ride and removes it from the active projection.
    ///
    /// # Errors
    ///
    /// Returns an error when no stopped ride exists or durable storage rejects the transition.
    /// Pending writes are retained until they settle so the saved recorder matches `SQLite`.
    pub fn save(&self) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let saved_ride_id = state.active_ride_id.clone();
        let snapshot = state.transition_inner(MobileRideEventDto::Save)?;
        state.active_ride_id = None;
        state.settled_ride_id = if state.pending_location_writes.is_empty() {
            None
        } else {
            saved_ride_id
        };
        state.admission_recorder = state.recorder.clone();
        Ok(snapshot)
    }

    /// Discards the stopped ride and removes it from the active projection.
    ///
    /// # Errors
    ///
    /// Returns an error when no stopped ride exists or durable storage rejects the transition.
    /// Discard intentionally cancels pending in-memory results for the discarded ride.
    pub fn discard(&self) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let snapshot = state.transition_inner(MobileRideEventDto::Discard)?;
        state.active_ride_id = None;
        state.settled_ride_id = None;
        state.pending_location_writes.clear();
        state.admission_recorder = state.recorder.clone();
        Ok(snapshot)
    }

    /// Associates a connected vehicle with the active recording.
    ///
    /// # Errors
    ///
    /// Returns an error when durable association metadata cannot be persisted.
    #[allow(clippy::needless_pass_by_value)]
    pub fn observe_vehicle_connection(
        &self,
        platform_identifier: String,
        at_ms: u64,
    ) -> Result<MobileRideMapCoreAssociationDto, MobileRideMapCoreErrorDto> {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let mut staged = state.admission_recorder.clone();
        let mut durable_staged = state.recorder.clone();
        let Some(identity) = ride_maps::VehicleIdentity::new(&platform_identifier) else {
            return Ok(ride_maps::VehicleAssociation::CandidateMissing.into());
        };
        let association =
            staged.observe_vehicle(&identity, ride_maps::MonotonicMilliseconds::new(at_ms));
        if association == ride_maps::VehicleAssociation::Associated {
            let _ = durable_staged
                .observe_vehicle(&identity, ride_maps::MonotonicMilliseconds::new(at_ms));
            if let (Some(database), Some(id)) =
                (state.database.as_ref(), state.active_ride_id.clone())
            {
                database
                    .update_ride_map_metadata(
                        id,
                        staged.candidate_vehicle().map(str::to_owned),
                        Some(platform_identifier.clone()),
                        staged
                            .associated_at_milliseconds()
                            .map(ride_maps::MonotonicMilliseconds::as_u64),
                        staged
                            .last_telemetry_at_milliseconds()
                            .map(ride_maps::MonotonicMilliseconds::as_u64),
                    )
                    .map_err(map_core_error)?;
            }
        }
        state.recorder = durable_staged;
        state.admission_recorder = staged;
        Ok(association.into())
    }

    /// Records a confirmed vehicle telemetry timestamp without backfilling route points.
    ///
    /// # Errors
    ///
    /// Returns an error when durable telemetry metadata cannot be persisted.
    pub fn observe_telemetry(
        &self,
        at_ms: u64,
    ) -> Result<MobileRideMapTelemetryObservationDto, MobileRideMapCoreErrorDto> {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let mut staged = state.admission_recorder.clone();
        let mut durable_staged = state.recorder.clone();
        let observation = staged.observe_telemetry(ride_maps::MonotonicMilliseconds::new(at_ms));
        if observation == ride_maps::TelemetryObservation::Observed {
            let _ = durable_staged.observe_telemetry(ride_maps::MonotonicMilliseconds::new(at_ms));
            if let (Some(database), Some(id)) =
                (state.database.as_ref(), state.active_ride_id.clone())
            {
                database
                    .update_ride_map_metadata(
                        id,
                        staged.candidate_vehicle().map(str::to_owned),
                        staged.associated_vehicle().map(str::to_owned),
                        staged
                            .associated_at_milliseconds()
                            .map(ride_maps::MonotonicMilliseconds::as_u64),
                        staged
                            .last_telemetry_at_milliseconds()
                            .map(ride_maps::MonotonicMilliseconds::as_u64),
                    )
                    .map_err(map_core_error)?;
            }
        }
        state.recorder = durable_staged;
        state.admission_recorder = staged;
        Ok(observation.into())
    }

    /// Admits one Core Location sample into the active recording.
    ///
    /// # Errors
    ///
    /// Returns an error when there is no active ride, the location is invalid, or durable
    /// persistence rejects the sample.
    pub fn ingest_location(
        &self,
        monotonic_ms: u64,
        wall_clock_unix_ms: u64,
        latitude_degrees: f64,
        longitude_degrees: f64,
        horizontal_accuracy_meters: f64,
    ) -> Result<MobileRideMapCoreDecisionDto, MobileRideMapCoreErrorDto> {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        state.ingest_location(
            monotonic_ms,
            wall_clock_unix_ms,
            latitude_degrees,
            longitude_degrees,
            horizontal_accuracy_meters,
        )
    }

    /// Forwards a complete Core Location callback while keeping timestamp admission in Rust.
    ///
    /// The callback supplies receipt anchors once; Rust derives monotonic timestamps from each
    /// source timestamp, rejects future/invalid samples, and runs the canonical admission policy.
    /// No platform-side queue or per-sample timestamp state is required.
    ///
    /// # Errors
    ///
    /// Returns an error when durable persistence rejects an admitted sample.
    pub fn ingest_location_batch(
        &self,
        receipt_monotonic_ms: u64,
        receipt_wall_clock_unix_ms: u64,
        samples: Vec<MobilePhoneLocationSampleDto>,
    ) -> Result<Vec<MobileRideMapCoreDecisionDto>, MobileRideMapCoreErrorDto> {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let mut decisions = Vec::with_capacity(samples.len());
        for sample in samples {
            let Some(sample) = sample.canonical() else {
                continue;
            };
            let Some(horizontal_accuracy_meters) = sample.horizontal_accuracy_meters else {
                continue;
            };
            let Some(elapsed_ms) =
                receipt_wall_clock_unix_ms.checked_sub(sample.wall_clock_unix_ms)
            else {
                // A source timestamp newer than the callback receipt is invalid.
                continue;
            };
            let Some(monotonic_ms) = receipt_monotonic_ms.checked_sub(elapsed_ms) else {
                continue;
            };
            match state.ingest_location(
                monotonic_ms,
                sample.wall_clock_unix_ms,
                sample.latitude_degrees,
                sample.longitude_degrees,
                horizontal_accuracy_meters,
            ) {
                Ok(decision) => decisions.push(decision),
                // No active ride remains; every remaining sample would return NoActiveRide,
                // so stop without discarding decisions already collected.
                Err(MobileRideMapCoreErrorDto::NoActiveRide) => break,
                Err(error) => return Err(error),
            }
        }
        Ok(decisions)
    }

    ///
    /// A pending write is removed when its worker result becomes available. Results belonging to
    /// a ride that is neither active nor settling after save are discarded so a late completion
    /// cannot affect a newly started ride.
    pub fn poll_location_writes(&self) -> Vec<MobileRideMapCoreDecisionDto> {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let current_ride_id = state
            .active_ride_id
            .clone()
            .or_else(|| state.settled_ride_id.clone());
        let mut completed = Vec::new();
        let mut remaining = VecDeque::with_capacity(state.pending_location_writes.len());
        while let Some(mut pending) = state.pending_location_writes.pop_front() {
            let Some(result) = pending.write.try_result() else {
                remaining.push_back(pending);
                continue;
            };
            if current_ride_id.as_ref() != Some(&pending.ride_id) {
                continue;
            }
            completed.push(match result {
                Ok(result) if result.admission() == ride_maps::LocationAdmission::Accepted => {
                    state.recorder.record_sample(pending.sample);
                    let mut point = pending.point;
                    if let Some(sequence) = result.sequence() {
                        point.sequence = sequence;
                    }
                    MobileRideMapCoreDecisionDto::Accepted {
                        point,
                        segment_started: pending.segment_started,
                    }
                }
                Ok(result) if result.admission() == ride_maps::LocationAdmission::Duplicate => {
                    MobileRideMapCoreDecisionDto::Ignored {
                        reason: MobileRideMapDecisionReasonDto::DuplicateLocation,
                    }
                }
                Ok(result) if result.admission() == ride_maps::LocationAdmission::OutOfOrder => {
                    MobileRideMapCoreDecisionDto::Rejected {
                        reason: MobileRideMapDecisionReasonDto::TimestampOutOfOrder,
                    }
                }
                Ok(result)
                    if result.admission() == ride_maps::LocationAdmission::AccuracyTooLow =>
                {
                    MobileRideMapCoreDecisionDto::Rejected {
                        reason: MobileRideMapDecisionReasonDto::AccuracyTooLow,
                    }
                }
                Ok(result)
                    if result.admission() == ride_maps::LocationAdmission::UnrealisticJump =>
                {
                    MobileRideMapCoreDecisionDto::Rejected {
                        reason: MobileRideMapDecisionReasonDto::UnrealisticJump,
                    }
                }
                Ok(_) => MobileRideMapCoreDecisionDto::StorageError {
                    message: "unknown location admission result".to_owned(),
                },
                Err(error) => MobileRideMapCoreDecisionDto::StorageError {
                    message: error.to_string(),
                },
            });
        }
        state.pending_location_writes = remaining;
        if state.active_ride_id.is_none() && state.pending_location_writes.is_empty() {
            state.settled_ride_id = None;
        }
        // Rebuild the admission projection from the durable projection plus only writes that are
        // still pending. This removes a provisional point after a durable rejection.
        let mut rebuilt = state.recorder.clone();
        for pending in &state.pending_location_writes {
            rebuilt.record_sample(pending.sample);
        }
        state.admission_recorder = rebuilt;
        completed
    }

    /// Returns a bounded page of active route points.
    ///
    /// Durable recordings are paged directly from `SQLite` so the bridge never materializes the
    /// complete route just to answer a preview request.
    ///
    /// # Errors
    ///
    /// Returns an error when durable route paging fails.
    pub fn points_after(
        &self,
        after_cursor: Option<u64>,
        limit: u32,
    ) -> Result<MobileRideMapCorePointBatchDto, MobileRideMapCoreErrorDto> {
        let state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if limit == 0 {
            return Ok(empty_map_point_batch());
        }
        let page_size = limit.min(500);
        if let (Some(database), Some(ride_id)) =
            (state.database.as_ref(), state.active_ride_id.clone())
        {
            let page = database
                .route_points(
                    ride_id,
                    after_cursor.map(|sequence| MobileRoutePointCursorDto { sequence }),
                    page_size,
                )
                .map_err(map_core_error)?;
            let points = page
                .points
                .into_iter()
                .map(|point| {
                    map_ride_telemetry_state(point.telemetry_state)
                        .map_err(|_| {
                            MobileRideMapCoreErrorDto::Storage(
                                "unsupported telemetry state".to_owned(),
                            )
                        })
                        .map(|telemetry_state| {
                            MobileRideMapCoreInner::point_from_location(
                                point.location,
                                point.sequence,
                                ride_maps::RideMapSegmentId::new(point.segment_id),
                                mobile_segment_start_reason(point.start_reason),
                                telemetry_state,
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| {
                    MobileRideMapCoreErrorDto::Storage("unsupported telemetry state".to_owned())
                })?;
            return Ok(MobileRideMapCorePointBatchDto {
                points,
                next_cursor: page.next_cursor.map(|cursor| cursor.sequence),
                has_more: page.next_cursor.is_some(),
            });
        }

        let start = after_cursor.map_or(state.recorder.first_point_sequence(), |cursor| {
            ride_maps::RidePointSequence::new(cursor.saturating_add(1))
        });
        let first_point_sequence = state.recorder.first_point_sequence();
        let mut points: Vec<_> = state
            .recorder
            .points()
            .iter()
            .copied()
            .enumerate()
            .map(|(offset, sample)| (first_point_sequence.saturating_add(offset as u64), sample))
            .filter(|(sequence, _)| *sequence >= start)
            .take(page_size as usize + 1)
            .map(|(sequence, sample)| {
                MobileRideMapCoreInner::point_from_location(
                    mobile_ride_location_dto(sample.sample()),
                    sequence.as_u64(),
                    sample.segment_id(),
                    sample.segment_start_reason(),
                    sample.telemetry_state(),
                )
            })
            .collect();
        let has_more = points.len() > page_size as usize;
        if has_more {
            points.pop();
        }
        let next_cursor = has_more.then(|| points.last().map_or(0, |point| point.sequence));
        Ok(MobileRideMapCorePointBatchDto {
            points,
            next_cursor,
            has_more,
        })
    }

    /// Projects the Rust-owned in-memory route tail into bounded display points.
    ///
    /// # Errors
    ///
    /// Returns [`MobileRideMapCoreErrorDto::InvalidRouteProjection`] when the options are invalid.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn project_points(
        &self,
        options: MobileRideMapRouteProjectionOptionsDto,
    ) -> Result<MobileRideMapRouteProjectionDto, MobileRideMapCoreErrorDto> {
        project_live_route_points(self, &options, || false)
    }

    /// Projects the Rust-owned route tail while honoring live-projection cancellation.
    ///
    /// The recorder snapshot is copied while the core mutex is held, then all route scanning and
    /// projection work happens after the mutex is released. This keeps a long presentation
    /// projection from blocking location admission or lifecycle transitions.
    ///
    /// # Errors
    ///
    /// Returns [`MobileRideMapCoreErrorDto::Cancelled`] when the caller requests cancellation,
    /// or [`MobileRideMapCoreErrorDto::InvalidRouteProjection`] for invalid options.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary DTOs")]
    pub fn project_points_cancellable(
        &self,
        options: MobileRideMapRouteProjectionOptionsDto,
        cancellation: Arc<MobileLiveRouteProjectionCancellation>,
    ) -> Result<MobileRideMapRouteProjectionDto, MobileRideMapCoreErrorDto> {
        project_live_route_points(self, &options, || cancellation.is_cancelled())
    }
}

fn project_live_route_points(
    core: &MobileRideMapCore,
    options: &MobileRideMapRouteProjectionOptionsDto,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<MobileRideMapRouteProjectionDto, MobileRideMapCoreErrorDto> {
    if is_cancelled() {
        return Err(MobileRideMapCoreErrorDto::Cancelled);
    }
    let (viewport, budget, privacy) = mobile_route_projection_options(options)?;
    let (points, first_sequence, source_point_count, source_segment_count, background_gap_count) = {
        let state = core.inner.lock().unwrap_or_else(PoisonError::into_inner);
        (
            state.recorder.points().to_vec(),
            state.recorder.first_point_sequence(),
            state.recorder.point_count(),
            state.recorder.segment_count().as_u64(),
            state.recorder.background_gap_count().as_u64(),
        )
    };
    if is_cancelled() {
        return Err(MobileRideMapCoreErrorDto::Cancelled);
    }
    let endpoint_metadata = ride_maps::route_endpoint_metadata(
        points.iter().copied().enumerate().map(|(offset, point)| {
            (
                first_sequence.saturating_add(u64::try_from(offset).unwrap_or(u64::MAX)),
                point,
            )
        }),
        source_point_count,
        viewport,
    );
    let candidate_segment_count = mobile_segment_count(&points, viewport, &mut is_cancelled)?;
    let candidate_point_count = mobile_point_count(&points, viewport, &mut is_cancelled)?;
    let projected_points = ride_maps::project_route_points_cancellable(
        &points,
        first_sequence,
        viewport,
        budget,
        privacy,
        &mut is_cancelled,
    )
    .map_err(|_| MobileRideMapCoreErrorDto::Cancelled)?;
    let camera_region =
        ride_maps::route_camera_region(projected_points.iter().map(|point| point.coordinate()))
            .map(mobile_camera_region_dto);
    let segments = ride_maps::route_segment_display_metadata(projected_points.iter().copied())
        .into_iter()
        .map(mobile_segment_display_metadata_dto)
        .collect();
    let points = mobile_route_display_points(projected_points, &mut is_cancelled)?;
    let displayed_segment_count = mobile_displayed_segment_count(&points);
    Ok(MobileRideMapRouteProjectionDto {
        points,
        segments,
        source_point_count,
        source_segment_count,
        candidate_point_count,
        candidate_segment_count,
        displayed_segment_count,
        background_gap_count,
        canonical_start_sequence: endpoint_metadata
            .start_sequence()
            .map(ride_maps::RidePointSequence::as_u64),
        canonical_end_sequence: endpoint_metadata
            .end_sequence()
            .map(ride_maps::RidePointSequence::as_u64),
        canonical_start_visible: endpoint_metadata.start_visible(),
        canonical_end_visible: endpoint_metadata.end_visible(),
        camera_region,
    })
}

impl MobileRideMapCoreInner {
    fn transition_inner(
        &mut self,
        event: MobileRideEventDto,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        let at_milliseconds = self
            .recorder
            .recording_timing()
            .last_monotonic_milliseconds()
            .as_u64();
        let (_, next) = self.transition_state(event, at_milliseconds)?;
        self.recorder.apply_transition(next);
        self.admission_recorder.apply_transition(next);
        Ok(self.snapshot(next.into()))
    }
    fn transition_inner_at(
        &mut self,
        event: MobileRideEventDto,
        at_milliseconds: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        let (_, next) = self.transition_state(event, at_milliseconds)?;
        self.recorder
            .apply_transition_at(next, ride_maps::MonotonicMilliseconds::new(at_milliseconds));
        self.admission_recorder
            .apply_transition_at(next, ride_maps::MonotonicMilliseconds::new(at_milliseconds));
        Ok(self.snapshot_at(next.into(), at_milliseconds))
    }
}

impl MobileRideMapCore {
    fn transition_at(
        &self,
        event: MobileRideEventDto,
        at_milliseconds: u64,
    ) -> Result<MobileRideMapCoreSnapshotDto, MobileRideMapCoreErrorDto> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.transition_inner_at(event, at_milliseconds)
    }
}

fn map_core_error(error: MobileRideDatabaseError) -> MobileRideMapCoreErrorDto {
    match error {
        MobileRideDatabaseError::InvalidTransition | MobileRideDatabaseError::InvalidRideState => {
            MobileRideMapCoreErrorDto::InvalidTransition
        }
        other => map_ride_map_error(&other),
    }
}

#[uniffi::export]
impl MobilePhoneLocationState {
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Clears the latest sample so a new capture cannot inherit an older location context.
    pub fn clear(&self) {
        *self
            .latest_sample
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    pub fn ingest(&self, sample: MobilePhoneLocationSampleDto) -> MobilePhoneLocationSnapshotDto {
        *self
            .latest_sample
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(sample);
        phone_location_snapshot(Some(sample))
    }

    #[must_use]
    pub fn current_snapshot(&self) -> MobilePhoneLocationSnapshotDto {
        let sample = *self
            .latest_sample
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        phone_location_snapshot(sample)
    }
}

/// VESC controller state for ride UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVescControllerStateDto {
    /// Controller is armed and balancing/riding data is relevant.
    Armed,

    /// Controller is not armed.
    Disarmed,

    /// Controller state is not known from the current readback.
    Unknown,
}

/// VESC controller operating mode for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVescRideOperatingModeDto {
    /// The protocol reported an unsupported mode value.
    Unknown,
    /// Normal upright balancing mode.
    Normal,
    /// Upside-down darkride mode.
    Darkride,
    /// Hand-test mode.
    Handtest,
    /// Flywheel test mode.
    Flywheel,
}

impl From<RideOperatingModeDto> for MobileVescRideOperatingModeDto {
    fn from(mode: RideOperatingModeDto) -> Self {
        match mode {
            RideOperatingModeDto::Unknown => Self::Unknown,
            RideOperatingModeDto::Normal => Self::Normal,
            RideOperatingModeDto::Darkride => Self::Darkride,
            RideOperatingModeDto::Handtest => Self::Handtest,
            RideOperatingModeDto::Flywheel => Self::Flywheel,
        }
    }
}

/// VESC ride warning state for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVescRideWarningDto {
    /// No ride warning is active.
    None,

    /// Controller input voltage is below its configured warning threshold.
    LowVoltage,
    /// Controller input voltage is above its configured warning threshold.
    HighVoltage,
    /// Controller MOSFET temperature reached its warning threshold.
    MosfetTemperature,
    /// Motor temperature reached its warning threshold.
    MotorTemperature,
    /// Motor current reached its configured warning threshold.
    Current,

    /// The controller is applying duty-based pushback.
    DutyPushback,
    /// The controller is applying temperature-based pushback.
    TemperaturePushback,
    /// The controller reports active wheel slip.
    Wheelslip,
    /// The controller reports a sensor warning.
    Sensors,
    /// The package reports a low battery warning.
    LowBattery,
    /// The package reports an error warning.
    Error,
}

/// Reason a VESC float controller stopped balancing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVescRideStopReasonDto {
    /// No stop condition is active.
    None,
    /// Board pitch exceeded the allowed range.
    Pitch,
    /// Board roll exceeded the allowed range.
    Roll,
    /// One half of the footpad switch caused the stop.
    SwitchHalf,
    /// The full footpad switch caused the stop.
    SwitchFull,
    /// Reverse-stop logic caused the stop.
    Reverse,
    /// Quick-stop logic caused the stop.
    QuickStop,
}

impl From<RideStopReasonDto> for MobileVescRideStopReasonDto {
    fn from(reason: RideStopReasonDto) -> Self {
        match reason {
            RideStopReasonDto::None => Self::None,
            RideStopReasonDto::Pitch => Self::Pitch,
            RideStopReasonDto::Roll => Self::Roll,
            RideStopReasonDto::SwitchHalf => Self::SwitchHalf,
            RideStopReasonDto::SwitchFull => Self::SwitchFull,
            RideStopReasonDto::Reverse => Self::Reverse,
            RideStopReasonDto::QuickStop => Self::QuickStop,
        }
    }
}

impl From<RideWarningDto> for MobileVescRideWarningDto {
    fn from(warning: RideWarningDto) -> Self {
        match warning {
            RideWarningDto::None => Self::None,
            RideWarningDto::LowVoltage => Self::LowVoltage,
            RideWarningDto::HighVoltage => Self::HighVoltage,
            RideWarningDto::MosfetTemperature => Self::MosfetTemperature,
            RideWarningDto::MotorTemperature => Self::MotorTemperature,
            RideWarningDto::Current => Self::Current,
            RideWarningDto::DutyPushback => Self::DutyPushback,
            RideWarningDto::TemperaturePushback => Self::TemperaturePushback,
            RideWarningDto::Wheelslip => Self::Wheelslip,
            RideWarningDto::Sensors => Self::Sensors,
            RideWarningDto::LowBattery => Self::LowBattery,
            RideWarningDto::Error => Self::Error,
        }
    }
}

/// VESC vehicle category independent of the installed protocol package.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVescVehicleKindDto {
    /// Float/Onewheel-style VESC build.
    Float,

    /// E-bike or emoto VESC build.
    Bike,

    /// Skateboard VESC build.
    Skateboard,

    /// Electric unicycle VESC build.
    ElectricUnicycle,

    /// Vehicle category is not known yet.
    Unknown,
}

/// VESC sub-protocol available on the device.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVescSubProtocolDto {
    /// Refloat sub-protocol is present.
    Refloat,

    /// Bike/e-moto VESC sub-protocol is present.
    Bike,

    /// Electric-skateboard VESC sub-protocol is present.
    Eskate,

    /// Generic VESC telemetry/protocol only.
    Generic,
}

/// VESC write guardrail shown by the debug surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVescWriteGuardrailDto {
    /// Debug screen is read-only for the current state.
    ReadOnly,

    /// Command is not supported by the product contract.
    UnsupportedCommand,

    /// Command was refused by safety policy.
    PolicyRefusal,

    /// Command passed policy, but no encoder/write path exists yet.
    AuthorizedButUnimplemented,

    /// Writes require parked state plus explicit confirmation.
    ParkedAndConfirmed,

    /// Guardrail state is unknown.
    Unknown,
}

/// VESC debug/config snapshot for mobile UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileVescDebugSnapshotDto {
    /// Profile or setup label.
    pub profile_title: String,

    /// VESC implementation and firmware label.
    pub transport_detail: String,

    /// Current duty cycle.
    pub duty_cycle: Option<DutyCycle>,

    /// Maximum duty observed in the session.
    pub max_seen_duty_cycle: Option<DutyCycle>,

    /// Pack voltage.
    pub pack_voltage: Option<VoltageReading>,

    /// Battery current limit.
    pub battery_current_limit: Option<BatteryCurrentReading>,

    /// Motor/phase current limit.
    pub motor_current_limit: Option<PhaseCurrentReading>,

    /// Last fault label from read-only state.
    pub last_fault: Option<String>,

    /// Input app label.
    pub input_app: Option<String>,

    /// CAN status label.
    pub can_status: Option<String>,

    /// Logging state label.
    pub logging: Option<String>,

    /// Current write guardrail.
    pub write_guardrail: MobileVescWriteGuardrailDto,
}

/// Confidence level for BMS topology mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileBmsTopologyConfidenceDto {
    /// Topology and group mapping were verified from known device evidence.
    Verified,

    /// Topology is plausible but not fully confirmed.
    Inferred,

    /// BMS data exists, but group mapping is not trustworthy yet.
    Unverified,
}

/// Alert level for BMS groups and decoded fault summaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileBmsAlertLevelDto {
    /// Value is in the nominal range.
    Nominal,

    /// Value deserves attention but is not yet critical.
    Warning,

    /// Value is critical or should block stronger claims in the UI.
    Critical,

    /// Data exists but cannot be classified yet.
    Unknown,
}

/// Topology summary for a BMS snapshot.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBmsTopologyDto {
    /// Human-readable topology label such as `20S4P split pack`.
    pub layout_label: String,

    /// Series group count when known.
    pub series_group_count: Option<u16>,

    /// Parallel count when known.
    pub parallel_count: Option<u16>,

    /// Number of packs or modules visible in the snapshot.
    pub pack_count: u8,

    /// Number of BMS controllers reporting in the snapshot.
    pub bms_count: u8,

    /// Confidence in the current topology mapping.
    pub confidence: MobileBmsTopologyConfidenceDto,
}

/// Per-group BMS reading for cells or grouped series strings.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBmsGroupSnapshotDto {
    /// One-based group index used in the UI.
    pub index: u16,

    /// Optional explicit label such as `left pack`.
    pub label: Option<String>,

    /// Group voltage.
    pub voltage: Option<VoltageReading>,

    /// Group temperature.
    pub temperature: Option<TemperatureReading>,

    /// Estimated internal resistance.
    pub resistance: Option<Resistance>,

    /// Whether balancing is active for this group when known.
    pub is_balancing: Option<bool>,

    /// Alert level for coloring and prioritization.
    pub alert_level: MobileBmsAlertLevelDto,

    /// Optional UI-safe detail string such as a trend note.
    pub detail: Option<String>,
}

/// Decoded BMS fault or advisory.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBmsFaultDto {
    /// Fault code or bitmask label.
    pub code: String,

    /// Human-readable fault label.
    pub label: String,

    /// Severity for operator-facing treatment.
    pub alert_level: MobileBmsAlertLevelDto,
}

/// Typed BMS snapshot for mobile pack and cells screens.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileBmsSnapshotDto {
    /// Whether BMS or pack-health data is available for display.
    pub availability: MobileReadbackAvailabilityDto,

    /// Topology summary for this reading.
    pub topology: MobileBmsTopologyDto,

    /// BMS page selector that produced this snapshot.
    pub page_selector: Option<u8>,

    /// Protocol tag/opcode that produced this snapshot.
    pub page_tag: Option<u16>,

    /// BMS page kind that produced this snapshot.
    pub page_kind: Option<String>,

    /// Verification for the BMS page interpretation.
    pub page_verification: Option<MobileVerificationStatusDto>,

    /// State of charge or usable energy percent when known.
    pub energy_percent: Option<BatteryLevelReading>,

    /// Pack voltage.
    pub voltage: Option<VoltageReading>,

    /// Pack current.
    pub current: Option<BatteryCurrentReading>,

    /// First page-specific BMS pack current.
    pub bms_pack_current_0: Option<BatteryCurrentReading>,

    /// Second page-specific BMS pack current.
    pub bms_pack_current_1: Option<BatteryCurrentReading>,

    /// Cell-group voltage delta.
    pub cell_delta: Option<VoltageDeltaReading>,

    /// One-based index of the lowest group when known.
    pub lowest_group_index: Option<u16>,

    /// Highest observed temperature.
    pub highest_temperature: Option<TemperatureReading>,

    /// Page-specific BMS temperature readings.
    pub temperatures: Vec<TemperatureReading>,

    /// Human-readable label for the hottest area.
    pub highest_temperature_label: Option<String>,

    /// Pack-level balancing summary.
    pub balancing_summary: Option<String>,

    /// Additional balancing detail.
    pub balancing_detail: Option<String>,

    /// Pack-level fault summary.
    pub fault_summary: Option<String>,

    /// Additional fault detail.
    pub fault_detail: Option<String>,

    /// Per-group readings.
    pub groups: Vec<MobileBmsGroupSnapshotDto>,

    /// Decoded faults or advisories.
    pub faults: Vec<MobileBmsFaultDto>,

    /// Optional unsupported-device capture action title.
    pub capture_action_title: Option<String>,

    /// Optional state label for the capture action.
    pub capture_action_state: Option<String>,
}

impl From<BatteryInfoDto> for MobileBmsSnapshotDto {
    fn from(battery: BatteryInfoDto) -> Self {
        Self::from_page(MobileReadbackAvailabilityDto::Available, battery)
    }
}

impl From<BatteryReadbackDto> for MobileBmsSnapshotDto {
    fn from(readback: BatteryReadbackDto) -> Self {
        let availability = readback.availability.into();
        match (availability, readback.page) {
            (MobileReadbackAvailabilityDto::Available, Some(page)) => {
                Self::from_page(availability, page)
            }
            _ => Self::empty_readback(availability),
        }
    }
}

impl MobileBmsSnapshotDto {
    fn from_page(availability: MobileReadbackAvailabilityDto, battery: BatteryInfoDto) -> Self {
        let page_identity = BmsPageIdentity::from_page(battery.page);
        let groups = bms_groups_from_cell_voltages(&battery.cell_voltages, page_identity);
        let temperatures = bms_temperatures(&battery.temperatures);
        Self {
            availability,
            topology: MobileBmsTopologyDto::from_observed_groups(groups.len()),
            page_selector: Some(battery.page.id.selector),
            page_tag: battery.page.id.namespace.map(|namespace| namespace.value),
            page_kind: Some(bms_page_kind_label(battery.page.kind).to_owned()),
            page_verification: Some(battery.page.verification.into()),
            energy_percent: battery
                .level_reported
                .or(battery.level_estimated)
                .map(Into::into),
            voltage: battery.voltage.map(Into::into),
            current: battery.current.map(Into::into),
            bms_pack_current_0: battery.bms_pack_current_0.map(Into::into),
            bms_pack_current_1: battery.bms_pack_current_1.map(Into::into),
            cell_delta: cell_voltage_delta(&battery.cell_voltages),
            lowest_group_index: lowest_cell_voltage_group_index(
                &battery.cell_voltages,
                page_identity,
            ),
            highest_temperature: highest_battery_temperature(
                battery.temperature,
                battery.temperatures,
            ),
            temperatures,
            highest_temperature_label: None,
            balancing_summary: None,
            balancing_detail: None,
            fault_summary: None,
            fault_detail: None,
            groups,
            faults: Vec::new(),
            capture_action_title: None,
            capture_action_state: None,
        }
    }

    fn empty_readback(availability: MobileReadbackAvailabilityDto) -> Self {
        Self {
            availability,
            topology: MobileBmsTopologyDto::unknown_readback(),
            page_selector: None,
            page_tag: None,
            page_kind: None,
            page_verification: None,
            energy_percent: None,
            voltage: None,
            current: None,
            bms_pack_current_0: None,
            bms_pack_current_1: None,
            cell_delta: None,
            lowest_group_index: None,
            highest_temperature: None,
            temperatures: Vec::new(),
            highest_temperature_label: None,
            balancing_summary: None,
            balancing_detail: None,
            fault_summary: None,
            fault_detail: None,
            groups: Vec::new(),
            faults: Vec::new(),
            capture_action_title: None,
            capture_action_state: None,
        }
    }
}

fn bms_page_kind_label(kind: BatteryPageKindDto) -> &'static str {
    match kind {
        BatteryPageKindDto::Metadata => "metadata",
        BatteryPageKindDto::CellVoltage => "cell voltage",
        BatteryPageKindDto::Temperature => "temperature",
        BatteryPageKindDto::Raw => "raw",
    }
}

fn bms_groups_from_cell_voltages(
    cell_voltages: &[VoltageReadingDto],
    page_identity: BmsPageIdentity,
) -> Vec<MobileBmsGroupSnapshotDto> {
    cell_voltages
        .iter()
        .enumerate()
        .filter_map(|(index, voltage)| {
            let group_index = page_identity.group_index(index)?;
            Some(MobileBmsGroupSnapshotDto {
                index: group_index.as_mobile_dto(),
                label: Some(format!("group {group_index}")),
                voltage: Some((*voltage).into()),
                temperature: None,
                resistance: None,
                is_balancing: None,
                alert_level: MobileBmsAlertLevelDto::Nominal,
                detail: None,
            })
        })
        .collect()
}

fn bms_temperatures(temperatures: &[Option<TemperatureReadingDto>]) -> Vec<TemperatureReading> {
    temperatures
        .iter()
        .flatten()
        .copied()
        .map(Into::into)
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BmsPageIdentity {
    page_selector: BmsPageSelector,
    cell_bank: Option<BegodeCellPageBank>,
}

impl BmsPageIdentity {
    fn from_page(page: cutout_core::BmsStatusPage) -> Self {
        Self::from_tag_and_selector(
            page.id
                .namespace
                .map(cutout_core::BmsStatusPageNamespace::into_core),
            page.id.selector,
        )
    }

    fn from_tag_and_selector(page_tag: Option<ProtocolTag>, page_selector: u8) -> Self {
        Self {
            page_selector: BmsPageSelector::from_mobile_dto(page_selector),
            cell_bank: BegodeCellPageBank::from_protocol_tag(page_tag),
        }
    }

    fn first_group_index(self) -> BmsGroupIndex {
        match self.cell_bank {
            Some(bank) => bank.first_group_index_for_page(self.page_selector),
            None => BmsGroupIndex::FIRST,
        }
    }

    fn group_index(self, page_offset: usize) -> Option<BmsGroupIndex> {
        self.first_group_index().offset(page_offset)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BmsPageSelector(u8);

impl BmsPageSelector {
    fn from_mobile_dto(selector: u8) -> Self {
        Self(selector)
    }

    fn cell_page_offset(self, values_per_page: BmsPageGroupCount) -> Option<BmsGroupOffset> {
        self.0
            .checked_mul(values_per_page.get())
            .map(BmsGroupOffset::new)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BegodeCellPageBank {
    First,
    Second,
}

impl BegodeCellPageBank {
    const VALUES_PER_PAGE: BmsPageGroupCount = BmsPageGroupCount::new(8);
    const VALUES_PER_BANK: BmsGroupOffset = BmsGroupOffset::new(32);

    fn from_protocol_tag(tag: Option<ProtocolTag>) -> Option<Self> {
        match tag.map(BegodeBmsPageTag::from_protocol_tag) {
            Some(BegodeBmsPageTag::FirstCellBank) => Some(Self::First),
            Some(BegodeBmsPageTag::SecondCellBank) => Some(Self::Second),
            Some(BegodeBmsPageTag::Summary | BegodeBmsPageTag::Unknown) | None => None,
        }
    }

    fn first_group_index_for_page(self, page_selector: BmsPageSelector) -> BmsGroupIndex {
        let bank_offset = match self {
            Self::First => BmsGroupOffset::ZERO,
            Self::Second => Self::VALUES_PER_BANK,
        };
        let bank_base = BmsGroupIndex::FIRST
            .offset_by(bank_offset)
            .unwrap_or(BmsGroupIndex::FIRST);
        page_selector
            .cell_page_offset(Self::VALUES_PER_PAGE)
            .and_then(|offset| bank_base.offset_by(offset))
            .unwrap_or(bank_base)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BegodeBmsPageTag {
    Summary,
    FirstCellBank,
    SecondCellBank,
    Unknown,
}

impl BegodeBmsPageTag {
    fn from_protocol_tag(tag: ProtocolTag) -> Self {
        match tag.get() {
            0x01 => Self::Summary,
            0x02 => Self::FirstCellBank,
            0x03 => Self::SecondCellBank,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BmsPageGroupCount(u8);

impl BmsPageGroupCount {
    const fn new(count: u8) -> Self {
        Self(count)
    }

    const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BmsGroupOffset(u8);

impl BmsGroupOffset {
    const ZERO: Self = Self(0);

    const fn new(offset: u8) -> Self {
        Self(offset)
    }

    const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BmsGroupIndex(u8);

impl BmsGroupIndex {
    const FIRST: Self = Self(1);
    #[cfg(test)]
    const MAX: Self = Self(u8::MAX);

    fn as_mobile_dto(self) -> u16 {
        self.0.into()
    }

    fn offset(self, page_offset: usize) -> Option<Self> {
        u8::try_from(page_offset)
            .ok()
            .and_then(|offset| self.0.checked_add(offset))
            .map(Self)
    }

    fn offset_by(self, offset: BmsGroupOffset) -> Option<Self> {
        self.0.checked_add(offset.get()).map(Self)
    }
}

impl fmt::Display for BmsGroupIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

fn cell_voltage_delta(cell_voltages: &[VoltageReadingDto]) -> Option<VoltageDeltaReading> {
    let min = cell_voltages.iter().map(|voltage| voltage.value).min()?;
    let max = cell_voltages.iter().map(|voltage| voltage.value).max()?;
    let first = cell_voltages.first()?;
    Some(VoltageDeltaReading {
        value: VoltageDelta {
            value: max.saturating_sub(min),
        },
        source: MobileValueSourceDto::Calculated,
        quality: MobileValueQualityDto::Known,
        verification: first.verification.into(),
    })
}

fn lowest_cell_voltage_group_index(
    cell_voltages: &[VoltageReadingDto],
    page_identity: BmsPageIdentity,
) -> Option<u16> {
    cell_voltages
        .iter()
        .enumerate()
        .min_by_key(|(_, voltage)| voltage.value)
        .and_then(|(index, _)| page_identity.group_index(index))
        .map(BmsGroupIndex::as_mobile_dto)
}

impl MobileBmsTopologyDto {
    fn from_observed_groups(group_count: usize) -> Self {
        if group_count == 0 {
            return Self::unknown_readback();
        }
        Self {
            layout_label: format!("{group_count} observed BMS groups"),
            series_group_count: None,
            parallel_count: None,
            pack_count: 1,
            bms_count: 1,
            confidence: MobileBmsTopologyConfidenceDto::Unverified,
        }
    }

    fn unknown_readback() -> Self {
        Self {
            layout_label: "unknown BMS topology".to_owned(),
            series_group_count: None,
            parallel_count: None,
            pack_count: 0,
            bms_count: 0,
            confidence: MobileBmsTopologyConfidenceDto::Unverified,
        }
    }
}

/// Conservative signed power/current flow direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum PowerFlowDirection {
    /// Positive discharge from pack to controller/motor.
    Discharge,

    /// No measurable signed pack flow.
    Zero,

    /// Negative signed pack flow while explicit protocol charge state says charging.
    Charging,

    /// Negative signed pack flow while the wheel is moving.
    Regeneration,

    /// Negative signed flow without enough motion or plug context to label charge or regen.
    NegativeUnknown,
}

/// Conservative EUC ride operating state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum RideOperatingState {
    /// No live evidence has established whether the EUC is parked, riding, or charging.
    Unknown,

    /// Explicit telemetry indicates the EUC is parked.
    Parked,

    /// Live telemetry indicates the EUC is stationary without explicit parked/off evidence.
    Standing,

    /// Telemetry indicates the EUC is moving under ride context.
    Riding,

    /// Explicit charge/plug evidence indicates the EUC is charging.
    Charging,
}

macro_rules! mobile_quantity {
    ($quantity:ident, $reading:ident, $raw:ty, $quantity_doc:literal, $reading_doc:literal) => {
        #[doc = $quantity_doc]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
        pub struct $quantity {
            /// Fixed-unit value owned by this quantity type.
            pub value: $raw,
        }

        #[doc = $reading_doc]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
        pub struct $reading {
            /// Semantic quantity value.
            pub value: $quantity,

            /// Value source.
            pub source: MobileValueSourceDto,

            /// Value quality.
            pub quality: MobileValueQualityDto,

            /// Value verification status.
            pub verification: MobileVerificationStatusDto,
        }
    };
}

mobile_quantity!(Speed, SpeedReading, i32, "Speed.", "Speed reading.");
mobile_quantity!(Voltage, VoltageReading, i32, "Voltage.", "Voltage reading.");
mobile_quantity!(
    BatteryCurrent,
    BatteryCurrentReading,
    i32,
    "Battery current.",
    "Battery current reading."
);
mobile_quantity!(
    PhaseCurrent,
    PhaseCurrentReading,
    i32,
    "Phase current.",
    "Phase current reading."
);
mobile_quantity!(Power, PowerReading, i64, "Power.", "Power reading.");
mobile_quantity!(
    Temperature,
    TemperatureReading,
    i32,
    "Temperature.",
    "Temperature reading."
);
mobile_quantity!(
    Distance,
    DistanceReading,
    u64,
    "Distance.",
    "Distance reading."
);
mobile_quantity!(Angle, AngleReading, i32, "Angle.", "Angle reading.");
mobile_quantity!(
    BatteryLevel,
    BatteryLevelReading,
    u8,
    "Battery level.",
    "Battery level reading."
);
mobile_quantity!(
    VoltageDelta,
    VoltageDeltaReading,
    i32,
    "Voltage delta.",
    "Voltage delta reading."
);

/// Electrical resistance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct Resistance {
    /// Fixed-unit resistance value.
    pub value: u16,
}

/// PWM duty cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct DutyCycle {
    /// Permille duty cycle.
    pub permille: i16,
}

/// Mobile parser diagnostics DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserDiagnosticsDto {
    /// Bytes dropped while recovering from malformed or excessive input.
    pub dropped_bytes: MobileParserDroppedBytesDto,

    /// Parser resynchronization attempts.
    pub resyncs: MobileParserDiagnosticCountDto,

    /// Malformed frame count.
    pub malformed_frames: MobileParserDiagnosticCountDto,

    /// Bad checksum count.
    pub bad_checksums: MobileParserDiagnosticCountDto,

    /// Parser timeout count.
    pub timeouts: MobileParserDiagnosticCountDto,

    /// Oversized frame count.
    pub oversized_frames: MobileParserDiagnosticCountDto,

    /// Unmatched reply count.
    pub unmatched_replies: MobileParserDiagnosticCountDto,
}

/// Mobile Falcon construction profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileFalconProfileDto {
    /// Default known Falcon profile.
    Default,

    /// Deliberate unsupported sentinel used to keep binding errors typed.
    Unsupported,
}

/// Mobile protocol-family identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileProtocolFamilyDto {
    /// Veteran/LeaperKim/NOSFET frame family.
    VeteranLeaperkimNosfet,

    /// Begode/Gotway frame family.
    BegodeGotway,

    /// VESC UART/CAN-derived family.
    Vesc,
}

/// Mobile value source.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileValueSourceDto {
    /// Value was reported directly by the device.
    Reported,

    /// Value was calculated from other values.
    Calculated,

    /// Value was estimated from incomplete evidence.
    Estimated,
}

/// Mobile value quality.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileValueQualityDto {
    /// Value is directly supported by observed data.
    Known,

    /// Value is inferred from partial or indirect evidence.
    Inferred,
}

/// Mobile verification status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVerificationStatusDto {
    /// Not yet verified.
    Unverified,

    /// Inferred from partial evidence.
    Inferred,

    /// Verified against source-attributed protocol documentation.
    SourceVerified,

    /// Verified against actual Bluetooth hardware.
    HardwareVerified,

    /// Verified against source-attributed documentation and hardware.
    SourceAndHardwareVerified,
}

/// Availability of a read-only response for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileReadbackAvailabilityDto {
    /// The device reported the requested readback.
    Available,

    /// The readback is expected for this device/profile but was not available.
    Unavailable,

    /// The readback is not supported for this device/profile.
    Unsupported,
}

/// Raw numeric field from a read-only settings response.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRawFieldValueDto {
    /// Protocol-family field identifier.
    pub id: u16,

    /// Sign-extended value exactly as reported by the protocol layer.
    pub value: i64,
}

/// Protocol-native floating field preserving its exact IEEE-754 bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRawFloatFieldValueDto {
    pub id: u16,
    pub value_bits: u32,
}

/// Full protocol-native telemetry decoded from a notification.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRawTelemetryReadbackDto {
    pub fields: Vec<MobileRawFieldValueDto>,
    pub float_fields: Vec<MobileRawFloatFieldValueDto>,
}

/// Generic read-only settings entry for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSettingsEntryDto {
    /// Raw field value with protocol identity preserved.
    pub field: MobileRawFieldValueDto,

    /// Source of the settings value.
    pub source: MobileValueSourceDto,

    /// Confidence in the settings value.
    pub quality: MobileValueQualityDto,

    /// Verification state for the settings value.
    pub verification: MobileVerificationStatusDto,
}

/// Product-shaped EUC garage settings projection for mobile UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileEucGarageSettingsDto {
    /// Whether this projected settings readback is available.
    pub availability: MobileReadbackAvailabilityDto,

    /// Beep margin speed setting, when understood.
    pub beep_margin: Option<SpeedReading>,

    /// Tiltback speed setting, when understood.
    pub tiltback: Option<SpeedReading>,

    /// Pedal mode setting, when understood.
    pub pedal_mode: Option<MobilePedalModeDto>,

    /// Falcon roll-angle sensitivity, when understood.
    pub roll_angle: Option<MobileRollAngleDto>,

    /// Begode speed-alarm mode, when understood.
    pub speed_alarm_mode: Option<MobileSpeedAlarmModeDto>,

    /// Light state reported by wheel telemetry, when the protocol provides it.
    pub light_state: Option<MobileLightStateDto>,

    /// Auto-shutdown time remaining, in reported seconds.
    pub auto_shutdown_seconds: Option<u64>,

    /// Charge mode reported by wheel telemetry.
    pub charge_mode: Option<MobileChargeModeReadingDto>,
}

/// Read-only pedal mode projection for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePedalModeDto {
    /// Protocol-native raw pedal-mode value.
    pub raw_mode: Option<u16>,

    /// Documented mode when the protocol-native value has a known mapping.
    pub mode: Option<MobilePedalModeKindDto>,
}

/// Read-only Falcon roll-angle projection for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRollAngleDto {
    /// Protocol-native raw roll-angle value.
    pub raw_angle: Option<u16>,

    /// Documented roll-angle sensitivity when the value has a known mapping.
    pub angle: Option<MobileRollAngleKindDto>,
}

/// Read-only Begode speed-alarm projection for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSpeedAlarmModeDto {
    /// Protocol-native raw speed-alarm mode value.
    pub raw_mode: Option<u16>,

    /// Documented speed-alarm mode when the value has a known mapping.
    pub mode: Option<MobileSpeedAlarmModeKindDto>,
}

/// Bounded read-only settings response for mobile UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSettingsReadbackDto {
    /// Whether the requested settings readback is available for display.
    pub availability: MobileReadbackAvailabilityDto,

    /// Product-shaped EUC garage settings projection.
    pub euc_garage: MobileEucGarageSettingsDto,

    /// Present settings entries.
    pub entries: Vec<MobileSettingsEntryDto>,
}

/// Last reported fault for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileFaultHistoryEntryDto {
    /// Protocol-specific fault code without proven semantic mapping.
    pub code: MobileFaultCodeDto,

    /// Source of the fault code.
    pub source: MobileValueSourceDto,

    /// Confidence in the fault-code interpretation.
    pub quality: MobileValueQualityDto,

    /// Verification state for the fault-code interpretation.
    pub verification: MobileVerificationStatusDto,
}

/// Protocol-specific fault code for mobile UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileFaultCodeDto {
    /// Raw protocol field/value pair for an unknown fault code.
    pub raw: MobileRawFieldValueDto,
}

/// Read-only last-fault history for mobile UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileFaultHistoryReadbackDto {
    /// Whether fault history is available for display.
    pub availability: MobileReadbackAvailabilityDto,

    /// Last reported fault, if any.
    pub last_fault: Option<MobileFaultHistoryEntryDto>,

    /// Distance since the last fault, if reported separately.
    pub since_distance: Option<DistanceReading>,
}

/// Mobile GATT characteristic role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileGattRoleDto {
    /// Characteristic supports reads.
    Read,

    /// Characteristic supports writes with response.
    Write,

    /// Characteristic supports writes without response.
    WriteWithoutResponse,

    /// Characteristic supports notifications.
    Notify,

    /// Characteristic supports indications.
    Indicate,
}

/// Mobile GATT service/characteristic fingerprint.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileGattFingerprintDto {
    /// Service UUID bytes.
    pub service: Vec<u8>,

    /// Characteristic UUID bytes.
    pub characteristic: Vec<u8>,

    /// Observed characteristic roles.
    pub roles: Vec<MobileGattRoleDto>,

    /// Verification status for this fingerprint.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile verified string field.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileVerifiedStringDto {
    /// Field value.
    pub value: String,

    /// Verification status for the value.
    pub verification: MobileVerificationStatusDto,
}

/// Mobile resolved identity metadata.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileResolvedIdentityDto {
    /// Resolved protocol family, when known.
    pub protocol_family: Option<MobileProtocolFamilyDto>,

    /// Resolved model name, when known.
    pub model: Option<MobileVerifiedStringDto>,

    /// Resolved firmware string, when known.
    pub firmware: Option<MobileVerifiedStringDto>,
}

impl From<&DeviceDetectionResolution> for DeviceDetectionResolutionRecord {
    fn from(resolution: &DeviceDetectionResolution) -> Self {
        Self {
            protocol_family: mobile_protocol_family_from_detection(resolution.protocol),
            protocol_conflict: resolution.protocol == ProtocolFamilyState::Conflict,
            veteran_protocol_model_id: match resolution.staged.protocol_model {
                ProtocolModelIdentityEvidence::ModelId(identity)
                    if identity.family == ProtocolFamily::VeteranLeaperkimNosfet =>
                {
                    Some(identity.model_id)
                }
                ProtocolModelIdentityEvidence::Missing
                | ProtocolModelIdentityEvidence::Malformed
                | ProtocolModelIdentityEvidence::ModelId(_) => None,
            },
            advertised_name: resolution
                .advertised_name
                .as_ref()
                .map(|name| name.as_bytes().to_vec()),
            model_banner: resolution
                .model_banner
                .as_ref()
                .map(|banner| banner.as_bytes().to_vec()),
            firmware_banner: resolution
                .firmware_banner
                .as_ref()
                .map(|banner| banner.as_bytes().to_vec()),
            imu_banner: resolution
                .imu_banner
                .as_ref()
                .map(|banner| banner.as_bytes().to_vec()),
            missing_probe_response: resolution
                .missing_probe_response
                .map(MobilePendingProbeDto::from),
            malformed_probe_response: resolution
                .malformed_probe_response
                .map(MobilePendingProbeDto::from),
        }
    }
}

impl From<DeviceDetectionResolution> for DeviceDetectionResolutionRecord {
    fn from(resolution: DeviceDetectionResolution) -> Self {
        Self::from(&resolution)
    }
}

const fn mobile_protocol_family_from_detection(
    protocol: ProtocolFamilyState,
) -> Option<MobileProtocolFamilyDto> {
    match protocol {
        ProtocolFamilyState::Unknown | ProtocolFamilyState::Conflict => None,
        ProtocolFamilyState::VeteranLeaperkimNosfet => {
            Some(MobileProtocolFamilyDto::VeteranLeaperkimNosfet)
        }
        ProtocolFamilyState::BegodeGotway => Some(MobileProtocolFamilyDto::BegodeGotway),
    }
}

impl From<PendingProbe> for MobilePendingProbeDto {
    fn from(probe: PendingProbe) -> Self {
        match probe {
            PendingProbe::BegodeName => Self::BegodeName,
            PendingProbe::BegodeFirmware => Self::BegodeFirmware,
            PendingProbe::BegodeImu => Self::BegodeImu,
        }
    }
}

/// Mobile Falcon construction error.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileSessionConstructorError {
    /// Requested Falcon profile is not supported.
    #[error("unsupported Falcon profile")]
    UnsupportedFalconProfile,
}

const CAPTURE_WRITER_QUEUE_CAPACITY: usize = 256;
const CAPTURE_WRITER_BUFFER_BYTES: u64 = 128 * 1024;
const CAPTURE_WRITER_FLUSH_INTERVAL: Duration = Duration::from_millis(500);
const CAPTURE_WRITER_SYNC_INTERVAL: Duration = Duration::from_secs(3);

/// Rust-owned status for the bounded capture writer queue.
#[derive(Clone, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureWriterStatusDto {
    /// Messages accepted by the queue and not yet written.
    pub queued_messages: u64,
    /// Highest number of accepted messages waiting to be written.
    pub peak_queued_messages: u64,
    /// Messages rejected because the queue was full or closed.
    pub dropped_messages: u64,
    /// Bytes written to the capture file.
    pub bytes_written: u64,
    /// Total successful write payload bytes, including header rewrites.
    pub physical_bytes_written: u64,
    /// Whether the writer has encountered an unrecoverable error.
    pub failed: bool,
    /// Last writer error, if one exists.
    pub last_error: Option<String>,
}

#[derive(Debug)]
struct CaptureWriterState {
    queued_messages: AtomicU64,
    peak_queued_messages: AtomicU64,
    dropped_messages: AtomicU64,
    bytes_written: AtomicU64,
    physical_bytes_written: AtomicU64,
    failed: AtomicBool,
    last_error: Mutex<Option<String>>,
}

impl Default for CaptureWriterState {
    fn default() -> Self {
        Self {
            queued_messages: AtomicU64::new(0),
            peak_queued_messages: AtomicU64::new(0),
            dropped_messages: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            physical_bytes_written: AtomicU64::new(0),
            failed: AtomicBool::new(false),
            last_error: Mutex::new(None),
        }
    }
}

impl CaptureWriterState {
    fn fail(&self, error: impl Into<String>) {
        self.failed.store(true, Ordering::Release);
        *self
            .last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(error.into());
    }

    fn status(&self) -> MobileCaptureWriterStatusDto {
        MobileCaptureWriterStatusDto {
            queued_messages: self.queued_messages.load(Ordering::Acquire),
            peak_queued_messages: self.peak_queued_messages.load(Ordering::Acquire),
            dropped_messages: self.dropped_messages.load(Ordering::Acquire),
            bytes_written: self.bytes_written.load(Ordering::Acquire),
            physical_bytes_written: self.physical_bytes_written.load(Ordering::Acquire),
            failed: self.failed.load(Ordering::Acquire),
            last_error: self
                .last_error
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct CaptureMetadata {
    advertised_services: Vec<GattChannel>,
    gatt_fingerprints: Vec<GattFingerprint>,
    resolved_identity: Option<PevcapResolvedIdentity>,
    annotations: Vec<String>,
}

enum CaptureWriterMessage {
    Record,
    Location(PevcapLocationSample),
    Metadata(CaptureMetadata),
    Flush(SyncSender<Result<(), String>>),
    Finish(SyncSender<Result<(), String>>),
}

#[derive(Debug)]
struct CaptureRecordPool {
    records: Mutex<VecDeque<PevcapRecord>>,
}

impl CaptureRecordPool {
    fn new(capacity: usize) -> Self {
        Self {
            records: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn take(&self) -> Option<PevcapRecord> {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }
}

#[derive(Debug)]
struct CaptureWriter {
    sender: SyncSender<CaptureWriterMessage>,
    records: Arc<CaptureRecordPool>,
    state: Arc<CaptureWriterState>,
    join: Option<JoinHandle<()>>,
}

impl CaptureWriter {
    fn start(
        path: PathBuf,
        wall_clock_start_unix_ms: WallClockUnixTimestamp,
        platform_id: &str,
        write_limit: Option<TransportWriteLimit>,
        metadata: &CaptureMetadata,
    ) -> Result<Self, String> {
        let header = capture_header(wall_clock_start_unix_ms, platform_id, write_limit, metadata)?;
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| error.to_string())?;
        let (sender, receiver) = sync_channel(CAPTURE_WRITER_QUEUE_CAPACITY);
        let records = Arc::new(CaptureRecordPool::new(CAPTURE_WRITER_QUEUE_CAPACITY));
        let state = Arc::new(CaptureWriterState::default());
        let thread_records = Arc::clone(&records);
        let thread_state = Arc::clone(&state);
        let join = thread::Builder::new()
            .name("cutout-pevcap-writer".into())
            .spawn(move || {
                run_capture_writer(&path, header, &receiver, &thread_records, &thread_state);
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            sender,
            records,
            state,
            join: Some(join),
        })
    }

    fn try_send(&self, message: CaptureWriterMessage) -> bool {
        let queued_messages = self.state.queued_messages.fetch_add(1, Ordering::AcqRel) + 1;
        match self.sender.try_send(message) {
            Ok(()) => {
                self.state
                    .peak_queued_messages
                    .fetch_max(queued_messages, Ordering::AcqRel);
                true
            }
            Err(TrySendError::Full(_)) => {
                self.state.queued_messages.fetch_sub(1, Ordering::AcqRel);
                self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
                self.state.fail("capture writer queue is full");
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.state.queued_messages.fetch_sub(1, Ordering::AcqRel);
                self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
                self.state.fail("capture writer stopped");
                false
            }
        }
    }

    fn try_send_record(&self, record: PevcapRecord) -> bool {
        let mut records = self
            .records
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if records.len() == records.capacity() {
            self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
            self.state.fail("capture writer queue is full");
            return false;
        }
        records.push_back(record);
        let queued_messages = self.state.queued_messages.fetch_add(1, Ordering::AcqRel) + 1;
        match self.sender.try_send(CaptureWriterMessage::Record) {
            Ok(()) => {
                self.state
                    .peak_queued_messages
                    .fetch_max(queued_messages, Ordering::AcqRel);
                true
            }
            Err(TrySendError::Full(CaptureWriterMessage::Record)) => {
                records.pop_back();
                self.state.queued_messages.fetch_sub(1, Ordering::AcqRel);
                self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
                self.state.fail("capture writer queue is full");
                false
            }
            Err(TrySendError::Disconnected(CaptureWriterMessage::Record)) => {
                records.pop_back();
                self.state.queued_messages.fetch_sub(1, Ordering::AcqRel);
                self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
                self.state.fail("capture writer stopped");
                false
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                unreachable!("record send returned a different message")
            }
        }
    }

    fn flush(&self) -> Result<(), String> {
        let (sender, receiver) = sync_channel(0);
        if !self.try_send(CaptureWriterMessage::Flush(sender)) {
            return Err(self
                .state
                .status()
                .last_error
                .unwrap_or_else(|| "capture writer flush failed".into()));
        }
        receiver
            .recv()
            .map_err(|_| "capture writer stopped before flush".to_string())?
    }

    fn finish(mut self) -> Result<(), String> {
        let (sender, receiver) = sync_channel(0);
        if !self.try_send(CaptureWriterMessage::Finish(sender)) {
            return Err(self
                .state
                .status()
                .last_error
                .unwrap_or_else(|| "capture writer finish failed".into()));
        }
        let result = receiver
            .recv()
            .map_err(|_| "capture writer stopped before finish".to_string())?;
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| "capture writer thread panicked".to_string())?;
        }
        result
    }
}

fn capture_header(
    wall_clock_start_unix_ms: WallClockUnixTimestamp,
    platform_id: &str,
    write_limit: Option<TransportWriteLimit>,
    metadata: &CaptureMetadata,
) -> Result<PevcapHeader, String> {
    let annotations = metadata
        .annotations
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    PevcapHeader::new(
        wall_clock_start_unix_ms,
        platform_id,
        write_limit,
        &metadata.advertised_services,
        &metadata.gatt_fingerprints,
        None,
        metadata.resolved_identity.clone(),
        env!("CARGO_PKG_VERSION"),
        [0; 32],
        &annotations,
    )
    .map_err(|error| format!("invalid capture header: {error}"))
}

fn run_capture_writer(
    path: &Path,
    mut header: PevcapHeader,
    receiver: &Receiver<CaptureWriterMessage>,
    records: &CaptureRecordPool,
    state: &CaptureWriterState,
) {
    let result = write_capture_stream(path, &mut header, receiver, records, state);
    if let Err(error) = result {
        state.fail(error);
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the capture writer keeps its flush and finish state machine in one loop"
)]
fn write_capture_stream(
    path: &Path,
    header: &mut PevcapHeader,
    receiver: &Receiver<CaptureWriterMessage>,
    records: &CaptureRecordPool,
    state: &CaptureWriterState,
) -> Result<(), String> {
    let file = File::create(path).map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    let header_bytes = write_line(
        &mut writer,
        &header.to_jsonl_line().map_err(|error| error.to_string())?,
    )?;
    state
        .physical_bytes_written
        .fetch_add(header_bytes as u64, Ordering::AcqRel);
    writer.flush().map_err(|error| error.to_string())?;
    writer
        .get_mut()
        .sync_data()
        .map_err(|error| error.to_string())?;
    let mut bytes_since_flush = 0_u64;
    let mut last_flush = Instant::now();
    let mut last_sync = Instant::now();
    let mut pending_metadata = None;

    while let Ok(message) = receiver.recv() {
        state.queued_messages.fetch_sub(1, Ordering::AcqRel);
        match message {
            CaptureWriterMessage::Record => {
                let record = records
                    .take()
                    .ok_or_else(|| "capture record slot was empty".to_string())?;
                let line = record.to_jsonl_line().map_err(|error| error.to_string())?;
                write_capture_event_line(
                    &mut writer,
                    &line,
                    state,
                    &mut bytes_since_flush,
                    &mut last_flush,
                    &mut last_sync,
                )?;
            }
            CaptureWriterMessage::Location(location) => {
                let line = location
                    .to_jsonl_line()
                    .map_err(|error| error.to_string())?;
                write_capture_event_line(
                    &mut writer,
                    &line,
                    state,
                    &mut bytes_since_flush,
                    &mut last_flush,
                    &mut last_sync,
                )?;
            }
            CaptureWriterMessage::Metadata(metadata) => {
                pending_metadata = Some(metadata);
            }
            CaptureWriterMessage::Flush(reply) => {
                let result = rewrite_pending_capture_metadata(
                    path,
                    &mut writer,
                    header,
                    &mut pending_metadata,
                    state,
                )
                .and_then(|rewrote_metadata| {
                    if rewrote_metadata {
                        bytes_since_flush = 0;
                        last_flush = Instant::now();
                        last_sync = last_flush;
                        Ok(())
                    } else {
                        maybe_flush(
                            &mut writer,
                            &mut bytes_since_flush,
                            &mut last_flush,
                            &mut last_sync,
                            true,
                        )
                    }
                });
                reply_capture_writer_result(result, &reply)?;
            }
            CaptureWriterMessage::Finish(reply) => {
                let result = rewrite_pending_capture_metadata(
                    path,
                    &mut writer,
                    header,
                    &mut pending_metadata,
                    state,
                )
                .and_then(|rewrote_metadata| {
                    if rewrote_metadata {
                        Ok(())
                    } else {
                        maybe_flush(
                            &mut writer,
                            &mut bytes_since_flush,
                            &mut last_flush,
                            &mut last_sync,
                            true,
                        )
                    }
                });
                return reply_capture_writer_result(result, &reply);
            }
        }
    }
    rewrite_pending_capture_metadata(path, &mut writer, header, &mut pending_metadata, state)?;
    writer.flush().map_err(|error| error.to_string())?;
    writer
        .get_mut()
        .sync_data()
        .map_err(|error| error.to_string())
}

fn write_capture_event_line(
    writer: &mut BufWriter<File>,
    line: &str,
    state: &CaptureWriterState,
    bytes_since_flush: &mut u64,
    last_flush: &mut Instant,
    last_sync: &mut Instant,
) -> Result<(), String> {
    let bytes = write_line(writer, line)? as u64;
    state.bytes_written.fetch_add(bytes, Ordering::AcqRel);
    state
        .physical_bytes_written
        .fetch_add(bytes, Ordering::AcqRel);
    *bytes_since_flush = (*bytes_since_flush).saturating_add(bytes);
    maybe_flush(writer, bytes_since_flush, last_flush, last_sync, false)
}

fn reply_capture_writer_result(
    result: Result<(), String>,
    reply: &SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let failure = result.as_ref().err().cloned();
    let _ = reply.send(result);
    failure.map_or(Ok(()), Err)
}

fn rewrite_pending_capture_metadata(
    path: &Path,
    writer: &mut BufWriter<File>,
    header: &mut PevcapHeader,
    pending_metadata: &mut Option<CaptureMetadata>,
    state: &CaptureWriterState,
) -> Result<bool, String> {
    let Some(metadata) = pending_metadata.take() else {
        return Ok(false);
    };
    *header = capture_header(
        header.wall_clock_start_unix_ms,
        header.platform_id.as_str(),
        header.write_limit,
        &metadata,
    )?;
    let bytes = rewrite_capture_header(path, writer, header)?;
    state
        .physical_bytes_written
        .fetch_add(bytes, Ordering::AcqRel);
    Ok(true)
}

fn write_line(writer: &mut BufWriter<File>, line: &str) -> Result<usize, String> {
    writer
        .write_all(line.as_bytes())
        .and_then(|()| writer.write_all(b"\n"))
        .map(|()| line.len() + 1)
        .map_err(|error| error.to_string())
}

fn maybe_flush(
    writer: &mut BufWriter<File>,
    bytes_since_flush: &mut u64,
    last_flush: &mut Instant,
    last_sync: &mut Instant,
    force_sync: bool,
) -> Result<(), String> {
    let now = Instant::now();
    if *bytes_since_flush >= CAPTURE_WRITER_BUFFER_BYTES
        || now.duration_since(*last_flush) >= CAPTURE_WRITER_FLUSH_INTERVAL
        || force_sync
    {
        writer.flush().map_err(|error| error.to_string())?;
        *bytes_since_flush = 0;
        *last_flush = now;
    }
    if force_sync || now.duration_since(*last_sync) >= CAPTURE_WRITER_SYNC_INTERVAL {
        writer
            .get_mut()
            .sync_data()
            .map_err(|error| error.to_string())?;
        *last_sync = now;
    }
    Ok(())
}

fn rewrite_capture_header(
    path: &Path,
    writer: &mut BufWriter<File>,
    header: &PevcapHeader,
) -> Result<u64, String> {
    writer.flush().map_err(|error| error.to_string())?;
    writer
        .get_mut()
        .sync_data()
        .map_err(|error| error.to_string())?;
    let input = File::open(path).map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(input);
    let mut old_header = Vec::new();
    reader
        .read_until(b'\n', &mut old_header)
        .map_err(|error| error.to_string())?;
    let temp_path = path.with_extension("jsonl.tmp");
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp_path)
        .map_err(|error| error.to_string())?;
    let header_bytes = write_line_to_file(
        &mut output,
        &header.to_jsonl_line().map_err(|error| error.to_string())?,
    )?;
    let copied_bytes =
        std::io::copy(&mut reader, &mut output).map_err(|error| error.to_string())?;
    output.sync_data().map_err(|error| error.to_string())?;
    drop(output);
    fs::rename(&temp_path, path).map_err(|error| error.to_string())?;
    let file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    *writer = BufWriter::new(file);
    Ok(header_bytes as u64 + copied_bytes)
}

fn write_line_to_file(file: &mut File, line: &str) -> Result<usize, String> {
    file.write_all(line.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .map(|()| line.len() + 1)
        .map_err(|error| error.to_string())
}

/// Mobile-facing builder for a PEVCAP capture export.
#[derive(Debug, uniffi::Object)]
pub struct MobilePevcapCaptureBuilder {
    wall_clock_start_unix_ms: WallClockUnixTimestamp,
    platform_id: String,
    write_limit: Option<TransportWriteLimit>,
    advertised_services: Mutex<Vec<GattChannel>>,
    gatt_fingerprints: Mutex<Vec<GattFingerprint>>,
    resolved_identity: Mutex<Option<PevcapResolvedIdentity>>,
    annotations: Mutex<Vec<String>>,
    writer: Mutex<Option<CaptureWriter>>,
    writer_state: Mutex<Option<Arc<CaptureWriterState>>>,
}

#[uniffi::export]
impl MobilePevcapCaptureBuilder {
    /// Creates an empty PEVCAP capture builder.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(
        wall_clock_start_unix_ms: MobileWallClockUnixMillisDto,
        platform_id: String,
        write_limit: Option<MobileTransportWriteLimitDto>,
    ) -> Arc<Self> {
        Arc::new(Self {
            wall_clock_start_unix_ms: wall_clock_start_unix_ms.into_core(),
            platform_id,
            write_limit: write_limit.map(|value| TransportWriteLimit::from_bytes(value.bytes)),
            advertised_services: Mutex::new(Vec::new()),
            gatt_fingerprints: Mutex::new(Vec::new()),
            resolved_identity: Mutex::new(None),
            annotations: Mutex::new(Vec::new()),
            writer: Mutex::new(None),
            writer_state: Mutex::new(None),
        })
    }

    /// Adds an advertised service UUID observed by the mobile BLE stack.
    #[allow(clippy::needless_pass_by_value)]
    pub fn add_advertised_service(&self, service: Vec<u8>) -> bool {
        let mut services = self
            .advertised_services
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if services.len() < cutout_core::PEVCAP_MAX_ADVERTISED_SERVICES {
            services.push(mobile_gatt_channel(&service));
        }
        drop(services);
        self.send_metadata_update()
    }

    /// Adds an observed GATT service/characteristic fingerprint.
    pub fn add_gatt_fingerprint(&self, fingerprint: MobileGattFingerprintDto) -> bool {
        let mut fingerprints = self
            .gatt_fingerprints
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if fingerprints.len() < cutout_core::PEVCAP_MAX_GATT_FINGERPRINTS {
            fingerprints.push(fingerprint.into());
        }
        drop(fingerprints);
        self.send_metadata_update()
    }

    /// Sets the resolved model/firmware identity for the capture.
    pub fn set_resolved_identity(&self, identity: MobileResolvedIdentityDto) -> bool {
        *self
            .resolved_identity
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(identity.into());
        self.send_metadata_update()
    }

    /// Adds a capture annotation, preserving key/value text exactly.
    pub fn add_annotation(&self, annotation: String) -> bool {
        let mut annotations = self
            .annotations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if annotations.len() < cutout_core::PEVCAP_MAX_ANNOTATIONS {
            annotations.push(annotation);
        }
        drop(annotations);
        self.send_metadata_update()
    }

    /// Starts the Rust-owned streaming writer for a JSONL capture.
    pub fn start_writer(&self, path: String) -> bool {
        let metadata = self.metadata();
        let writer = match CaptureWriter::start(
            PathBuf::from(path),
            self.wall_clock_start_unix_ms,
            &self.platform_id,
            self.write_limit,
            &metadata,
        ) {
            Ok(writer) => writer,
            Err(error) => {
                let state = Arc::new(CaptureWriterState::default());
                state.fail(error);
                *self
                    .writer_state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(state);
                return false;
            }
        };
        *self
            .writer_state
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&writer.state));
        *self.writer.lock().unwrap_or_else(PoisonError::into_inner) = Some(writer);
        true
    }

    /// Flushes buffered capture bytes and syncs them to durable storage.
    pub fn flush_writer(&self) -> bool {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        writer.as_ref().is_some_and(|writer| writer.flush().is_ok())
    }

    /// Finishes the Rust-owned streaming writer.
    pub fn finish_writer(&self) -> bool {
        let writer = self
            .writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        writer.is_none_or(|writer| writer.finish().is_ok())
    }

    /// Returns bounded writer queue instrumentation.
    #[must_use]
    pub fn writer_status(&self) -> MobileCaptureWriterStatusDto {
        if let Some(writer) = self
            .writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            return writer.state.status();
        }
        self.writer_state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map_or_else(MobileCaptureWriterStatusDto::default, |state| {
                state.status()
            })
    }

    /// Records a link-up lifecycle event.
    pub fn record_link_up(
        &self,
        monotonic_ms: MobileMonotonicMillisDto,
        max_write_len: Option<MobileTransportWriteLimitDto>,
    ) -> bool {
        self.send_record(PevcapRecord::link_up(
            monotonic_ms.into_core(),
            max_write_len.map(|value| TransportWriteLimit::from_bytes(value.bytes)),
        ))
    }

    /// Records a link-down lifecycle event.
    pub fn record_link_down(&self, monotonic_ms: MobileMonotonicMillisDto) -> bool {
        self.send_record(PevcapRecord::link_down(monotonic_ms.into_core()))
    }

    /// Records outbound write-without-response bytes.
    #[allow(clippy::needless_pass_by_value)]
    pub fn record_write_without_response(
        &self,
        monotonic_ms: MobileMonotonicMillisDto,
        characteristic: Vec<u8>,
        bytes: Vec<u8>,
    ) -> bool {
        self.send_record(PevcapRecord::outbound_write(
            monotonic_ms.into_core(),
            mobile_gatt_channel(&characteristic),
            WriteMode::WithoutResponse,
            bytes,
        ))
    }

    /// Records inbound notification bytes.
    #[allow(clippy::needless_pass_by_value)]
    pub fn record_notification(
        &self,
        monotonic_ms: MobileMonotonicMillisDto,
        characteristic: Vec<u8>,
        service: Vec<u8>,
        bytes: Vec<u8>,
    ) -> bool {
        self.send_record(PevcapRecord::inbound_notification(
            monotonic_ms.into_core(),
            mobile_gatt_channel(&characteristic),
            mobile_gatt_channel(&service),
            bytes,
        ))
    }

    /// Records an inbound notification with Rust-decoded telemetry and phone location context.
    #[allow(clippy::needless_pass_by_value)]
    pub fn record_notification_with_context(
        &self,
        monotonic_ms: MobileMonotonicMillisDto,
        characteristic: Vec<u8>,
        service: Vec<u8>,
        bytes: Vec<u8>,
        telemetry: Option<MobileRawTelemetryReadbackDto>,
        phone_location: Option<MobilePhoneLocationSampleDto>,
    ) -> bool {
        let mut record = PevcapRecord::inbound_notification(
            monotonic_ms.into_core(),
            mobile_gatt_channel(&characteristic),
            mobile_gatt_channel(&service),
            bytes,
        );
        if let Some(telemetry) = telemetry {
            record = record.with_telemetry(raw_telemetry_from_mobile(telemetry));
        }
        if let Some(location) = phone_location.and_then(MobilePhoneLocationSampleDto::canonical) {
            record = record.with_phone_location(location.pevcap_location());
        }
        self.send_record(record)
    }

    /// Records an independent Core Location sample in the PEVCAP location stream.
    ///
    /// The sample is kept separate from transport records so it remains available even when no
    /// BLE notification is received at the same instant. The writer queue is bounded; `false`
    /// means the sample was rejected by canonical validation or could not be queued.
    pub fn record_location_sample(
        &self,
        receipt_monotonic_ms: MobileMonotonicMillisDto,
        sample: MobilePhoneLocationSampleDto,
        simulated: Option<bool>,
        produced_by_accessory: Option<bool>,
    ) -> bool {
        let Some(sample) = sample.canonical() else {
            return false;
        };
        let Ok(location) = PevcapLocationSample::new(
            receipt_monotonic_ms.into_core(),
            sample.pevcap_location(),
            simulated,
            produced_by_accessory,
        ) else {
            return false;
        };
        self.send_location(location)
    }
}

impl MobilePevcapCaptureBuilder {
    fn metadata(&self) -> CaptureMetadata {
        CaptureMetadata {
            advertised_services: self
                .advertised_services
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            gatt_fingerprints: self
                .gatt_fingerprints
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            resolved_identity: self
                .resolved_identity
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            annotations: self
                .annotations
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        }
    }

    fn send_metadata_update(&self) -> bool {
        let metadata = self.metadata();
        self.writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_none_or(|writer| writer.try_send(CaptureWriterMessage::Metadata(metadata)))
    }

    fn send_record(&self, record: PevcapRecord) -> bool {
        self.writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|writer| writer.try_send_record(record))
    }

    fn send_location(&self, location: PevcapLocationSample) -> bool {
        self.writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|writer| writer.try_send(CaptureWriterMessage::Location(location)))
    }
}

impl From<MobileProtocolFamilyDto> for ProtocolFamily {
    fn from(family: MobileProtocolFamilyDto) -> Self {
        match family {
            MobileProtocolFamilyDto::VeteranLeaperkimNosfet => Self::VeteranLeaperkimNosfet,
            MobileProtocolFamilyDto::BegodeGotway => Self::BegodeGotway,
            MobileProtocolFamilyDto::Vesc => Self::Vesc,
        }
    }
}

impl From<ProtocolFamilyDto> for MobileProtocolFamilyDto {
    fn from(family: ProtocolFamilyDto) -> Self {
        match family {
            ProtocolFamilyDto::VeteranLeaperkimNosfet => Self::VeteranLeaperkimNosfet,
            ProtocolFamilyDto::BegodeGotway => Self::BegodeGotway,
            ProtocolFamilyDto::Vesc => Self::Vesc,
        }
    }
}

impl From<ValueSourceDto> for MobileValueSourceDto {
    fn from(source: ValueSourceDto) -> Self {
        match source {
            ValueSourceDto::Reported => Self::Reported,
            ValueSourceDto::Calculated => Self::Calculated,
            ValueSourceDto::Estimated => Self::Estimated,
        }
    }
}

impl From<ValueSource> for MobileValueSourceDto {
    fn from(source: ValueSource) -> Self {
        match source {
            ValueSource::Reported => Self::Reported,
            ValueSource::Calculated => Self::Calculated,
            ValueSource::Estimated => Self::Estimated,
        }
    }
}

impl From<ValueQualityDto> for MobileValueQualityDto {
    fn from(quality: ValueQualityDto) -> Self {
        match quality {
            ValueQualityDto::Known => Self::Known,
            ValueQualityDto::Inferred => Self::Inferred,
        }
    }
}

impl From<ValueQuality> for MobileValueQualityDto {
    fn from(quality: ValueQuality) -> Self {
        match quality {
            ValueQuality::Known => Self::Known,
            ValueQuality::Inferred => Self::Inferred,
        }
    }
}

impl From<VerificationStatusDto> for MobileVerificationStatusDto {
    fn from(status: VerificationStatusDto) -> Self {
        match status {
            VerificationStatusDto::Unverified => Self::Unverified,
            VerificationStatusDto::Inferred => Self::Inferred,
            VerificationStatusDto::SourceVerified => Self::SourceVerified,
            VerificationStatusDto::HardwareVerified => Self::HardwareVerified,
            VerificationStatusDto::SourceAndHardwareVerified => Self::SourceAndHardwareVerified,
        }
    }
}

impl From<VerificationStatus> for MobileVerificationStatusDto {
    fn from(status: VerificationStatus) -> Self {
        match status {
            VerificationStatus::Unverified => Self::Unverified,
            VerificationStatus::Inferred => Self::Inferred,
            VerificationStatus::SourceVerified => Self::SourceVerified,
            VerificationStatus::HardwareVerified => Self::HardwareVerified,
            VerificationStatus::SourceAndHardwareVerified => Self::SourceAndHardwareVerified,
        }
    }
}

impl From<MobileVerificationStatusDto> for VerificationStatus {
    fn from(status: MobileVerificationStatusDto) -> Self {
        match status {
            MobileVerificationStatusDto::Unverified => Self::Unverified,
            MobileVerificationStatusDto::Inferred => Self::Inferred,
            MobileVerificationStatusDto::SourceVerified => Self::SourceVerified,
            MobileVerificationStatusDto::HardwareVerified => Self::HardwareVerified,
            MobileVerificationStatusDto::SourceAndHardwareVerified => {
                Self::SourceAndHardwareVerified
            }
        }
    }
}

impl From<RawFieldValue> for MobileRawFieldValueDto {
    fn from(field: RawFieldValue) -> Self {
        Self {
            id: field.id,
            value: field.value,
        }
    }
}

impl From<RawFieldValueDto> for MobileRawFieldValueDto {
    fn from(field: RawFieldValueDto) -> Self {
        Self {
            id: field.id,
            value: field.value,
        }
    }
}

impl From<RawTelemetryReadbackDto> for MobileRawTelemetryReadbackDto {
    fn from(raw: RawTelemetryReadbackDto) -> Self {
        Self {
            fields: raw.fields.into_iter().map(Into::into).collect(),
            float_fields: raw
                .float_fields
                .into_iter()
                .map(|field| MobileRawFloatFieldValueDto {
                    id: field.id,
                    value_bits: field.value_bits,
                })
                .collect(),
        }
    }
}

fn raw_telemetry_from_mobile(value: MobileRawTelemetryReadbackDto) -> RawTelemetryReadback {
    let mut raw = RawTelemetryReadback::default();
    for field in value.fields.into_iter().take(raw.fields.capacity()) {
        if raw
            .fields
            .try_push(RawFieldValue::new(field.id, field.value))
            .is_err()
        {
            break;
        }
    }
    for field in value
        .float_fields
        .into_iter()
        .take(raw.float_fields.capacity())
    {
        if raw
            .float_fields
            .try_push(cutout_core::RawFloatFieldValue {
                id: field.id,
                value_bits: field.value_bits,
            })
            .is_err()
        {
            break;
        }
    }
    raw
}

impl From<SettingsEntry> for MobileSettingsEntryDto {
    fn from(entry: SettingsEntry) -> Self {
        Self {
            field: entry.field.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

impl From<SettingsEntryDto> for MobileSettingsEntryDto {
    fn from(entry: SettingsEntryDto) -> Self {
        Self {
            field: entry.field.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

impl MobileEucGarageSettingsDto {
    fn from_entries(
        availability: MobileReadbackAvailabilityDto,
        entries: &[MobileSettingsEntryDto],
    ) -> Self {
        if availability != MobileReadbackAvailabilityDto::Available {
            return Self {
                availability,
                beep_margin: None,
                tiltback: None,
                pedal_mode: None,
                roll_angle: None,
                speed_alarm_mode: None,
                light_state: None,
                auto_shutdown_seconds: None,
                charge_mode: None,
            };
        }

        Self {
            availability,
            beep_margin: settings_speed(
                entries,
                VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
                speed_from_deci_kmh,
            ),
            tiltback: settings_speed(
                entries,
                VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
                speed_from_deci_kmh,
            )
            .or_else(|| settings_speed(entries, BEGODE_FIELD_TILTBACK_SPEED_KMH, speed_from_kmh)),
            pedal_mode: settings_entry(entries, VETERAN_FIELD_PEDALS_MODE)
                .and_then(|entry| u16::try_from(entry.field.value).ok())
                .map(|raw_mode| {
                    mobile_pedal_mode(raw_mode, CorePedalMode::from_veteran_raw(raw_mode))
                })
                .or_else(|| {
                    settings_entry(entries, BEGODE_FIELD_SETTINGS_BITS)
                        .and_then(|entry| u16::try_from(entry.field.value).ok())
                        .map(begode_pedal_mode)
                }),
            roll_angle: settings_entry(entries, BEGODE_FIELD_SETTINGS_BITS)
                .and_then(|entry| u16::try_from(entry.field.value).ok())
                .map(begode_roll_angle),
            speed_alarm_mode: settings_entry(entries, BEGODE_FIELD_SETTINGS_BITS)
                .and_then(|entry| u16::try_from(entry.field.value).ok())
                .map(begode_speed_alarm_mode),
            light_state: settings_entry(entries, BEGODE_FIELD_LED_AND_LIGHT_MODE)
                .and_then(|entry| u16::try_from(entry.field.value).ok())
                .and_then(|value| match value & 0x03 {
                    0 => Some(MobileLightStateDto::Off),
                    1 => Some(MobileLightStateDto::On),
                    _ => None,
                }),
            auto_shutdown_seconds: settings_entry(
                entries,
                VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS,
            )
            .and_then(|entry| u64::try_from(entry.field.value).ok()),
            charge_mode: settings_entry(entries, VETERAN_FIELD_CHARGE_MODE).map(|entry| {
                MobileChargeModeReadingDto {
                    value: if entry.field.value == 0 {
                        MobileChargeModeDto::NotCharging
                    } else {
                        MobileChargeModeDto::Charging
                    },
                    source: entry.source,
                    quality: entry.quality,
                    verification: entry.verification,
                }
            }),
        }
    }
}

fn settings_entry(
    entries: &[MobileSettingsEntryDto],
    field_id: u16,
) -> Option<MobileSettingsEntryDto> {
    entries
        .iter()
        .copied()
        .find(|entry| entry.field.id == field_id)
}

fn aero_speed_setting_from_entry(entry: MobileSettingsEntryDto) -> Option<CoreAeroSpeedSetting> {
    let deci_kmh = u8::try_from(entry.field.value).ok()?;
    (deci_kmh % 10 == 0).then_some(())?;
    CoreAeroSpeedSetting::new(deci_kmh / 10)
}

fn begode_pedal_mode(settings_bits: u16) -> MobilePedalModeDto {
    let raw_mode = (settings_bits >> 13) & 0x03;
    mobile_pedal_mode(
        raw_mode,
        CorePedalMode::from_begode_settings_bits(settings_bits),
    )
}

fn begode_roll_angle(settings_bits: u16) -> MobileRollAngleDto {
    let raw_angle = (settings_bits >> 7) & 0x03;
    MobileRollAngleDto {
        raw_angle: Some(raw_angle),
        angle: CoreRollAngle::from_begode_settings_bits(settings_bits)
            .map(MobileRollAngleKindDto::from),
    }
}

fn begode_speed_alarm_mode(settings_bits: u16) -> MobileSpeedAlarmModeDto {
    let raw_mode = (settings_bits >> 10) & 0x03;
    MobileSpeedAlarmModeDto {
        raw_mode: Some(raw_mode),
        mode: CoreSpeedAlarmMode::from_begode_settings_bits(settings_bits)
            .map(MobileSpeedAlarmModeKindDto::from),
    }
}

fn mobile_pedal_mode(raw_mode: u16, mode: Option<CorePedalMode>) -> MobilePedalModeDto {
    MobilePedalModeDto {
        raw_mode: Some(raw_mode),
        mode: mode.map(MobilePedalModeKindDto::from),
    }
}

fn settings_speed(
    entries: &[MobileSettingsEntryDto],
    field_id: u16,
    convert: fn(i64) -> Option<CoreSpeed>,
) -> Option<SpeedReading> {
    settings_entry(entries, field_id).and_then(|entry| {
        convert(entry.field.value).map(|speed| SpeedReading {
            value: Speed {
                value: speed.as_millimetres_per_second(),
            },
            source: entry.source,
            quality: entry.quality,
            verification: entry.verification,
        })
    })
}

fn phone_location_snapshot(
    sample: Option<MobilePhoneLocationSampleDto>,
) -> MobilePhoneLocationSnapshotDto {
    MobilePhoneLocationSnapshotDto {
        latest_sample: sample,
        gps_speed: sample.and_then(phone_location_speed),
    }
}

fn phone_location_speed(sample: MobilePhoneLocationSampleDto) -> Option<SpeedReading> {
    let speed = sample.speed_meters_per_second?;
    if !speed.is_finite() || speed < 0.0 {
        return None;
    }
    Some(SpeedReading {
        value: Speed {
            value: round_f64_to_i32(speed * 1_000.0),
        },
        source: MobileValueSourceDto::Reported,
        quality: MobileValueQualityDto::Known,
        verification: MobileVerificationStatusDto::SourceVerified,
    })
}

fn round_f64_to_i32(value: f64) -> i32 {
    if value.is_nan() {
        return 0;
    }
    if value <= f64::from(i32::MIN) {
        return i32::MIN;
    }
    if value >= f64::from(i32::MAX) {
        return i32::MAX;
    }
    let rounded = value.round();
    let mut low = i64::from(i32::MIN);
    let mut high = i64::from(i32::MAX);
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let Ok(candidate) = i32::try_from(midpoint) else {
            return 0;
        };
        match f64::from(candidate).total_cmp(&rounded) {
            std::cmp::Ordering::Less => low = midpoint + 1,
            std::cmp::Ordering::Greater => high = midpoint - 1,
            std::cmp::Ordering::Equal => return candidate,
        }
    }
    0
}

fn speed_from_deci_kmh(value: i64) -> Option<CoreSpeed> {
    speed_from_milli_kmh(value.checked_mul(100)?)
}

fn speed_from_kmh(value: i64) -> Option<CoreSpeed> {
    speed_from_milli_kmh(value.checked_mul(1_000)?)
}

fn speed_from_milli_kmh(value: i64) -> Option<CoreSpeed> {
    (value >= 0)
        .then_some(value)?
        .checked_mul(5)?
        .checked_div(18)?
        .try_into()
        .ok()
        .map(CoreSpeed::from_millimetres_per_second)
}

impl From<SettingsReadback> for MobileSettingsReadbackDto {
    fn from(readback: SettingsReadback) -> Self {
        let availability = readback.availability().into();
        let entries: Vec<_> = readback
            .entries()
            .into_iter()
            .flatten()
            .map(Into::into)
            .collect();
        let euc_garage = MobileEucGarageSettingsDto::from_entries(availability, &entries);

        Self {
            availability,
            euc_garage,
            entries,
        }
    }
}

impl From<SettingsReadbackDto> for MobileSettingsReadbackDto {
    fn from(readback: SettingsReadbackDto) -> Self {
        let availability = readback.availability.into();
        let entries: Vec<_> = (availability == MobileReadbackAvailabilityDto::Available)
            .then_some(readback.entries)
            .into_iter()
            .flatten()
            .map(Into::into)
            .collect();
        let euc_garage = MobileEucGarageSettingsDto::from_entries(availability, &entries);

        Self {
            availability,
            euc_garage,
            entries,
        }
    }
}

impl From<SettingsReadbackAvailability> for MobileReadbackAvailabilityDto {
    fn from(availability: SettingsReadbackAvailability) -> Self {
        match availability {
            SettingsReadbackAvailability::Available => Self::Available,
            SettingsReadbackAvailability::Unavailable => Self::Unavailable,
            SettingsReadbackAvailability::Unsupported => Self::Unsupported,
        }
    }
}

impl From<SettingsReadbackAvailabilityDto> for MobileReadbackAvailabilityDto {
    fn from(availability: SettingsReadbackAvailabilityDto) -> Self {
        match availability {
            SettingsReadbackAvailabilityDto::Available => Self::Available,
            SettingsReadbackAvailabilityDto::Unavailable => Self::Unavailable,
            SettingsReadbackAvailabilityDto::Unsupported => Self::Unsupported,
        }
    }
}

impl From<FaultHistoryAvailability> for MobileReadbackAvailabilityDto {
    fn from(availability: FaultHistoryAvailability) -> Self {
        match availability {
            FaultHistoryAvailability::Available => Self::Available,
            FaultHistoryAvailability::Unavailable => Self::Unavailable,
            FaultHistoryAvailability::Unsupported => Self::Unsupported,
        }
    }
}

impl From<FaultHistoryAvailabilityDto> for MobileReadbackAvailabilityDto {
    fn from(availability: FaultHistoryAvailabilityDto) -> Self {
        match availability {
            FaultHistoryAvailabilityDto::Available => Self::Available,
            FaultHistoryAvailabilityDto::Unavailable => Self::Unavailable,
            FaultHistoryAvailabilityDto::Unsupported => Self::Unsupported,
        }
    }
}

impl From<BatteryReadbackAvailabilityDto> for MobileReadbackAvailabilityDto {
    fn from(availability: BatteryReadbackAvailabilityDto) -> Self {
        match availability {
            BatteryReadbackAvailabilityDto::Available => Self::Available,
            BatteryReadbackAvailabilityDto::Unavailable => Self::Unavailable,
            BatteryReadbackAvailabilityDto::Unsupported => Self::Unsupported,
        }
    }
}

impl From<FaultHistoryEntry> for MobileFaultHistoryEntryDto {
    fn from(entry: FaultHistoryEntry) -> Self {
        Self {
            code: entry.code.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

impl From<FaultHistoryEntryDto> for MobileFaultHistoryEntryDto {
    fn from(entry: FaultHistoryEntryDto) -> Self {
        Self {
            code: entry.code.into(),
            source: entry.source.into(),
            quality: entry.quality.into(),
            verification: entry.verification.into(),
        }
    }
}

impl From<FaultCodeDto> for MobileFaultCodeDto {
    fn from(code: FaultCodeDto) -> Self {
        Self {
            raw: code.raw.into(),
        }
    }
}

impl From<FaultCode> for MobileFaultCodeDto {
    fn from(code: FaultCode) -> Self {
        Self {
            raw: code.raw.into(),
        }
    }
}

impl From<FaultHistoryReadback> for MobileFaultHistoryReadbackDto {
    fn from(readback: FaultHistoryReadback) -> Self {
        Self {
            availability: readback.availability().into(),
            last_fault: readback.last_fault().map(Into::into),
            since_distance: readback
                .since_distance()
                .map(DistanceReadingDto::from)
                .map(Into::into),
        }
    }
}

impl From<FaultHistoryReadbackDto> for MobileFaultHistoryReadbackDto {
    fn from(readback: FaultHistoryReadbackDto) -> Self {
        let availability = readback.availability.into();
        let (last_fault, since_distance) =
            if availability == MobileReadbackAvailabilityDto::Available {
                (
                    readback.last_fault.map(Into::into),
                    readback.since_distance.map(Into::into),
                )
            } else {
                (None, None)
            };

        Self {
            availability,
            last_fault,
            since_distance,
        }
    }
}

impl From<MobileGattRoleDto> for GattRoles {
    fn from(role: MobileGattRoleDto) -> Self {
        match role {
            MobileGattRoleDto::Read => Self::empty().with_read(),
            MobileGattRoleDto::Write => Self::empty().with_write(),
            MobileGattRoleDto::WriteWithoutResponse => Self::empty().with_write_without_response(),
            MobileGattRoleDto::Notify => Self::empty().with_notify(),
            MobileGattRoleDto::Indicate => Self::empty().with_indicate(),
        }
    }
}

fn mobile_gatt_roles(roles: Vec<MobileGattRoleDto>) -> GattRoles {
    roles
        .into_iter()
        .fold(GattRoles::empty(), |accumulator, role| match role {
            MobileGattRoleDto::Read => accumulator.with_read(),
            MobileGattRoleDto::Write => accumulator.with_write(),
            MobileGattRoleDto::WriteWithoutResponse => accumulator.with_write_without_response(),
            MobileGattRoleDto::Notify => accumulator.with_notify(),
            MobileGattRoleDto::Indicate => accumulator.with_indicate(),
        })
}

impl From<MobileGattFingerprintDto> for GattFingerprint {
    fn from(fingerprint: MobileGattFingerprintDto) -> Self {
        Self {
            service: mobile_gatt_channel(&fingerprint.service),
            characteristic: mobile_gatt_channel(&fingerprint.characteristic),
            roles: mobile_gatt_roles(fingerprint.roles),
            verification: fingerprint.verification.into(),
        }
    }
}

impl From<MobileVerifiedStringDto> for VerifiedValue<String> {
    fn from(value: MobileVerifiedStringDto) -> Self {
        Self {
            value: value.value,
            verification: value.verification.into(),
        }
    }
}

impl From<MobileResolvedIdentityDto> for PevcapResolvedIdentity {
    fn from(identity: MobileResolvedIdentityDto) -> Self {
        Self {
            protocol_family: identity.protocol_family.map(Into::into),
            model: identity.model.map(Into::into),
            firmware: identity.firmware.map(Into::into),
        }
    }
}

fn mobile_gatt_channel(channel: &[u8]) -> GattChannel {
    GattChannel::from_bytes(mobile_channel_bytes(channel))
}

/// Mobile-facing NOSFET Aero telemetry wrapper with allow-listed light control.
#[derive(Debug, uniffi::Object)]
pub struct AeroBenignControlSession {
    inner: Mutex<ConcreteAeroBenignControlSession>,
    settings: Mutex<MobileEucSettingTrackers>,
}

#[uniffi::export]
impl AeroBenignControlSession {
    /// Creates a NOSFET Aero telemetry session with allow-listed headlight control.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(new_nosfet_aero_benign_control_session()),
            settings: Mutex::new(MobileEucSettingTrackers::default()),
        })
    }

    /// Drives one input and returns owned outputs plus any stable error DTO.
    pub fn ingest_checked(&self, input: MobileSessionInputDto) -> MobileSessionStepResultDto {
        let tracked_input = input.clone();
        let input = SessionInputDto::from(input);
        let result = preserve_refused_command(
            mobile_aero_session_step_result(self.lock_inner().ingest_checked(&input)),
            tracked_input.command,
        );
        self.lock_settings().observe_step(&tracked_input, &result);
        result
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    pub fn drain_outputs(&self) -> Vec<MobileSessionOutputDto> {
        self.lock_inner()
            .drain_outputs()
            .into_iter()
            .map(mobile_aero_session_output)
            .collect()
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    pub fn current_snapshot(&self) -> MobileTelemetrySnapshotDto {
        self.lock_inner().current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    pub fn diagnostics(&self) -> MobileParserDiagnosticsDto {
        self.lock_inner().diagnostics().into()
    }

    /// Returns the capture-backed EUC setting write capabilities.
    pub fn settings_capabilities(&self) -> MobileEucSettingsCapabilitiesDto {
        MobileEucSettingsCapabilitiesDto::aero()
    }

    /// Arms settings writes only when the latest Rust-owned ride state is stationary.
    pub fn arm_settings_writes(
        &self,
        state: RideOperatingState,
        monotonic_ms: MobileMonotonicMillisDto,
    ) -> bool {
        let speed_mm_per_second = self
            .lock_inner()
            .current_snapshot()
            .speed
            .map(|speed| speed.value);
        self.lock_inner().arm_settings_writes(
            mobile_ride_operating_state_dto(state),
            speed_mm_per_second,
            monotonic_ms.milliseconds,
        )
    }

    /// Returns the Rust-owned headlight write lifecycle state.
    pub fn headlight_state(&self) -> MobileLightSettingStateDto {
        self.lock_settings().headlight()
    }

    /// Returns the Rust-owned Aero high-beam write lifecycle state.
    pub fn aero_high_beam_state(&self) -> MobileLightSettingStateDto {
        self.lock_settings().aero_high_beam()
    }

    /// Returns the Rust-owned Aero tilt-back speed lifecycle state.
    pub fn aero_tiltback_speed_state(&self) -> MobileAeroSpeedSettingStateDto {
        self.lock_settings().aero_tiltback_speed()
    }

    /// Returns the Rust-owned Aero PWM-warning lifecycle state.
    pub fn aero_pwm_percent_state(&self) -> MobileAeroPwmSettingStateDto {
        self.lock_settings().aero_pwm_percent()
    }

    /// Returns the Rust-owned Aero alarm-speed lifecycle state.
    pub fn aero_alarm_speed_state(&self) -> MobileAeroSpeedSettingStateDto {
        self.lock_settings().aero_alarm_speed()
    }

    /// Returns the Rust-owned Aero angle-adjustment lifecycle state.
    pub fn aero_angle_adjustment_state(&self) -> MobileAeroAngleAdjustmentStateDto {
        self.lock_settings().aero_angle_adjustment()
    }

    /// Returns the Rust-owned pedal-mode setting lifecycle state.
    pub fn pedal_mode_state(&self) -> MobilePedalModeSettingStateDto {
        self.lock_settings().pedal_mode()
    }

    /// Returns the Rust-owned roll-angle setting lifecycle state.
    pub fn roll_angle_state(&self) -> MobileRollAngleSettingStateDto {
        self.lock_settings().roll_angle()
    }

    /// Returns the Rust-owned speed-alarm setting lifecycle state.
    pub fn speed_alarm_mode_state(&self) -> MobileSpeedAlarmModeSettingStateDto {
        self.lock_settings().speed_alarm_mode()
    }

    /// Returns the Rust-owned acceleration-assist setting lifecycle state.
    pub fn acceleration_assist_state(&self) -> MobileAccelerationAssistSettingStateDto {
        self.lock_settings().acceleration_assist()
    }

    /// Returns the Rust-owned taillight setting lifecycle state.
    pub fn taillight_state(&self) -> MobileLightSettingStateDto {
        self.lock_settings().taillight()
    }
}

impl AeroBenignControlSession {
    fn lock_inner(&self) -> MutexGuard<'_, ConcreteAeroBenignControlSession> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_settings(&self) -> MutexGuard<'_, MobileEucSettingTrackers> {
        self.settings.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for AeroBenignControlSession {
    fn default() -> Self {
        Self {
            inner: Mutex::new(new_nosfet_aero_benign_control_session()),
            settings: Mutex::new(MobileEucSettingTrackers::default()),
        }
    }
}

impl From<MobileSessionInputDto> for SessionInputDto {
    fn from(input: MobileSessionInputDto) -> Self {
        match input.kind {
            MobileSessionInputKindDto::LinkUp => Self::LinkUp {
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
                max_write_len: input
                    .max_write_len
                    .map(MobileTransportWriteLimitDto::into_core_ffi),
            },
            MobileSessionInputKindDto::LinkDown => Self::LinkDown,
            MobileSessionInputKindDto::Notification => Self::Notification {
                channel: mobile_channel_bytes(&input.channel),
                bytes: input.bytes,
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
            },
            MobileSessionInputKindDto::Tick => Self::Tick {
                monotonic_ms: input.monotonic_ms.into_core_ffi(),
            },
            MobileSessionInputKindDto::Command => Self::Command(
                input
                    .command
                    .unwrap_or(MobileCommandDto::RequestTelemetry)
                    .into(),
            ),
        }
    }
}

fn mobile_channel_bytes(channel: &[u8]) -> [u8; 16] {
    let mut bytes = [0; 16];
    let len = channel.len().min(bytes.len());
    bytes[..len].copy_from_slice(&channel[..len]);
    bytes
}

impl From<MobileCommandDto> for DeviceCommandDto {
    fn from(command: MobileCommandDto) -> Self {
        match command {
            MobileCommandDto::RequestIdentity => Self::RequestIdentity,
            MobileCommandDto::RequestTelemetry => Self::RequestTelemetry,
            MobileCommandDto::RequestFirmwareInfo => Self::RequestFirmwareInfo,
            MobileCommandDto::RequestBatteryInfo => Self::RequestBatteryInfo,
            MobileCommandDto::RequestDiagnostics => Self::RequestDiagnostics,
            MobileCommandDto::RequestFaultHistory => Self::RequestFaultHistory,
            MobileCommandDto::RequestSettings => Self::RequestSettings,
            MobileCommandDto::ResetTripMeter => Self::ResetTripMeter,
            MobileCommandDto::SetAeroTiltbackSpeed(speed) => {
                CoreAeroSpeedSetting::new(speed.kilometres_per_hour).map_or(
                    DeviceCommandDto::RequestSettings,
                    DeviceCommandDto::SetAeroTiltbackSpeed,
                )
            }
            MobileCommandDto::SetAeroPwmPercent(percent) => {
                CoreAeroPwmPercent::new(percent.percent).map_or(
                    DeviceCommandDto::RequestSettings,
                    DeviceCommandDto::SetAeroPwmPercent,
                )
            }
            MobileCommandDto::SetAeroAlarmSpeed(speed) => {
                CoreAeroSpeedSetting::new(speed.kilometres_per_hour).map_or(
                    DeviceCommandDto::RequestSettings,
                    DeviceCommandDto::SetAeroAlarmSpeed,
                )
            }
            MobileCommandDto::SetAeroAngleAdjustment(angle) => {
                CoreAeroAngleAdjustment::new(angle.tenths_of_degree).map_or(
                    DeviceCommandDto::RequestSettings,
                    DeviceCommandDto::SetAeroAngleAdjustment,
                )
            }
            MobileCommandDto::SetAeroHighBeam(state) => Self::SetAeroHighBeam(state.into()),
            MobileCommandDto::SetLights(state) => Self::SetLights(state.into()),
            MobileCommandDto::SetPedalMode(mode) => {
                Self::SetPedalMode(CorePedalMode::from(mode).into())
            }
            MobileCommandDto::SetRollAngle(angle) => {
                Self::SetRollAngle(CoreRollAngle::from(angle).into())
            }
            MobileCommandDto::SetSpeedAlarmMode(mode) => {
                Self::SetSpeedAlarmMode(CoreSpeedAlarmMode::from(mode).into())
            }
            MobileCommandDto::SetBegodeMaxSpeed(speed) => {
                CoreBegodeMaxSpeed::new(speed.kilometres_per_hour).map_or(
                    DeviceCommandDto::RequestSettings,
                    DeviceCommandDto::SetBegodeMaxSpeed,
                )
            }
            MobileCommandDto::SetBegodeBeeperVolume(volume) => {
                CoreBegodeBeeperVolume::new(volume.level).map_or(
                    DeviceCommandDto::RequestSettings,
                    DeviceCommandDto::SetBegodeBeeperVolume,
                )
            }
            MobileCommandDto::SetBegodeLedMode(mode) => CoreBegodeLedModeSetting::new(mode.mode)
                .map_or(
                    DeviceCommandDto::RequestSettings,
                    DeviceCommandDto::SetBegodeLedMode,
                ),
            MobileCommandDto::SetAccelerationAssist(state) => {
                Self::SetAccelerationAssist(state.into())
            }
            MobileCommandDto::SetTaillight(state) => Self::SetTaillight(state.into()),
            MobileCommandDto::SoundHorn => Self::SoundHorn,
        }
    }
}

fn mobile_command_from_command_kind(command: CommandKindDto) -> Option<MobileCommandDto> {
    match command {
        CommandKindDto::RequestIdentity => Some(MobileCommandDto::RequestIdentity),
        CommandKindDto::RequestTelemetry => Some(MobileCommandDto::RequestTelemetry),
        CommandKindDto::RequestFirmwareInfo => Some(MobileCommandDto::RequestFirmwareInfo),
        CommandKindDto::RequestBatteryInfo => Some(MobileCommandDto::RequestBatteryInfo),
        CommandKindDto::RequestDiagnostics => Some(MobileCommandDto::RequestDiagnostics),
        CommandKindDto::RequestFaultHistory => Some(MobileCommandDto::RequestFaultHistory),
        CommandKindDto::RequestSettings => Some(MobileCommandDto::RequestSettings),
        CommandKindDto::ResetTripMeter => Some(MobileCommandDto::ResetTripMeter),
        CommandKindDto::SoundHorn => Some(MobileCommandDto::SoundHorn),
        CommandKindDto::SetAeroTiltbackSpeed
        | CommandKindDto::SetAeroPwmPercent
        | CommandKindDto::SetAeroAlarmSpeed
        | CommandKindDto::SetAeroAngleAdjustment
        | CommandKindDto::SetAeroHighBeam
        | CommandKindDto::SetAccelerationAssist
        | CommandKindDto::SetLights
        | CommandKindDto::SetPedalMode
        | CommandKindDto::SetRollAngle
        | CommandKindDto::SetSpeedAlarmMode
        | CommandKindDto::SetBegodeMaxSpeed
        | CommandKindDto::SetBegodeBeeperVolume
        | CommandKindDto::SetBegodeLedMode
        | CommandKindDto::SetTaillight
        | CommandKindDto::SetRawMotorCurrent => None,
    }
}

impl MobileSessionOutputDto {
    fn empty(kind: MobileSessionOutputKindDto) -> Self {
        Self {
            kind,
            channel: Vec::new(),
            bytes: Vec::new(),
            ingest: None,
            settings_readback: None,
            fault_history_readback: None,
            bms_snapshot: None,
            raw_telemetry: None,
            veteran_protocol_model_id: None,
        }
    }

    fn transport(kind: MobileSessionOutputKindDto, channel: Vec<u8>, bytes: Vec<u8>) -> Self {
        let mut output = Self::empty(kind);
        output.channel = channel;
        output.bytes = bytes;
        output
    }

    fn read_only(payload: ReadOnlyOutputPayload) -> Self {
        let mut output = Self::empty(MobileSessionOutputKindDto::Event);
        match payload {
            ReadOnlyOutputPayload::Settings(settings) => {
                output.kind = MobileSessionOutputKindDto::SettingsReadback;
                output.settings_readback = Some(settings.into());
            }
            ReadOnlyOutputPayload::FaultHistory(fault_history) => {
                output.kind = MobileSessionOutputKindDto::FaultHistoryReadback;
                output.fault_history_readback = Some(fault_history.into());
            }
            ReadOnlyOutputPayload::Battery(battery) => {
                output.kind = MobileSessionOutputKindDto::BmsSnapshot;
                output.bms_snapshot = Some(battery.into());
            }
            ReadOnlyOutputPayload::RawTelemetry(raw) => {
                output.raw_telemetry = Some(raw.into());
            }
            ReadOnlyOutputPayload::Firmware(_) | ReadOnlyOutputPayload::Diagnostics(_) => {}
        }
        output
    }
}

impl From<SessionOutputDto> for MobileSessionOutputDto {
    fn from(output: SessionOutputDto) -> Self {
        match output {
            SessionOutputDto::Transport(TransportActionDto::Subscribe { channel }) => {
                Self::transport(
                    MobileSessionOutputKindDto::Subscribe,
                    channel.to_vec(),
                    Vec::new(),
                )
            }
            SessionOutputDto::Transport(TransportActionDto::Write { channel, bytes, .. }) => {
                Self::transport(MobileSessionOutputKindDto::Write, channel.to_vec(), bytes)
            }
            SessionOutputDto::Transport(TransportActionDto::Disconnect) => {
                Self::empty(MobileSessionOutputKindDto::Disconnect)
            }
            SessionOutputDto::ReadOnly(response) => Self::read_only(response.payload),
            SessionOutputDto::Event(_) => Self::empty(MobileSessionOutputKindDto::Event),
            SessionOutputDto::NotificationIngest(outcome) => {
                let mut output = Self::empty(MobileSessionOutputKindDto::NotificationIngest);
                output.ingest = Some(outcome.into());
                output
            }
        }
    }
}
impl From<NotificationIngestOutcomeDto> for MobileNotificationIngestOutcomeDto {
    fn from(outcome: NotificationIngestOutcomeDto) -> Self {
        match outcome {
            NotificationIngestOutcomeDto::SemanticEvents {
                notification,
                event_count,
            } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::SemanticEvents,
                notification: Some(notification.into()),
                event_count: Some(event_count.into()),
                parser_error: None,
                reserved: None,
                gap: None,
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::BufferedFragment(notification) => Self {
                kind: MobileNotificationIngestOutcomeKindDto::BufferedFragment,
                notification: Some(notification.into()),
                event_count: None,
                parser_error: None,
                reserved: None,
                gap: None,
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::ParserDiagnostic {
                notification,
                error,
            } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::ParserDiagnostic,
                notification: Some(notification.into()),
                event_count: None,
                parser_error: Some(error.into()),
                reserved: None,
                gap: None,
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::KnownReserved {
                notification,
                payload,
            } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::KnownReserved,
                notification: Some(notification.into()),
                event_count: None,
                parser_error: None,
                reserved: Some(payload.into()),
                gap: None,
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::ParserGap { notification, gap } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::ParserGap,
                notification: Some(notification.into()),
                event_count: None,
                parser_error: None,
                reserved: None,
                gap: Some(gap.into()),
                ignored: None,
                ignored_reason: None,
            },
            NotificationIngestOutcomeDto::Ignored { evidence, reason } => Self {
                kind: MobileNotificationIngestOutcomeKindDto::Ignored,
                notification: None,
                event_count: None,
                parser_error: None,
                reserved: None,
                gap: None,
                ignored: Some(evidence.into()),
                ignored_reason: Some(reason.into()),
            },
        }
    }
}

impl From<DutyCycleReadingDto> for DutyCycle {
    fn from(measured: DutyCycleReadingDto) -> Self {
        Self {
            permille: measured.value,
        }
    }
}

macro_rules! mobile_quantity_from_reading {
    ($reading_dto:ty, $quantity:ident, $reading:ident) => {
        impl From<$reading_dto> for $reading {
            fn from(reading: $reading_dto) -> Self {
                Self {
                    value: $quantity {
                        value: reading.value,
                    },
                    source: reading.source.into(),
                    quality: reading.quality.into(),
                    verification: reading.verification.into(),
                }
            }
        }
    };
}

mobile_quantity_from_reading!(SpeedReadingDto, Speed, SpeedReading);
mobile_quantity_from_reading!(VoltageReadingDto, Voltage, VoltageReading);
mobile_quantity_from_reading!(
    BatteryCurrentReadingDto,
    BatteryCurrent,
    BatteryCurrentReading
);
mobile_quantity_from_reading!(PhaseCurrentReadingDto, PhaseCurrent, PhaseCurrentReading);
mobile_quantity_from_reading!(PowerReadingDto, Power, PowerReading);
mobile_quantity_from_reading!(TemperatureReadingDto, Temperature, TemperatureReading);
mobile_quantity_from_reading!(DistanceReadingDto, Distance, DistanceReading);
mobile_quantity_from_reading!(AngleReadingDto, Angle, AngleReading);
mobile_quantity_from_reading!(BatteryLevelReadingDto, BatteryLevel, BatteryLevelReading);

fn highest_battery_temperature(
    temperature: Option<TemperatureReadingDto>,
    temperatures: Vec<Option<TemperatureReadingDto>>,
) -> Option<TemperatureReading> {
    temperature
        .into_iter()
        .chain(temperatures.into_iter().flatten())
        .max_by_key(|reading| reading.value)
        .map(Into::into)
}

fn power_flow_from_signed_current(
    current: BatteryCurrentReadingDto,
    operating_state: RideOperatingState,
) -> PowerFlowDirection {
    match current.value.cmp(&0) {
        std::cmp::Ordering::Greater => PowerFlowDirection::Discharge,
        std::cmp::Ordering::Equal => PowerFlowDirection::Zero,
        std::cmp::Ordering::Less => match operating_state {
            RideOperatingState::Charging => PowerFlowDirection::Charging,
            RideOperatingState::Riding => PowerFlowDirection::Regeneration,
            RideOperatingState::Parked
            | RideOperatingState::Standing
            | RideOperatingState::Unknown => PowerFlowDirection::NegativeUnknown,
        },
    }
}

fn ride_operating_state(
    operating_state: Option<RideOperatingStateDto>,
    charge_mode: Option<ChargeModeReadingDto>,
    speed: Option<SpeedReadingDto>,
) -> RideOperatingState {
    match operating_state {
        Some(RideOperatingStateDto::Parked) => return RideOperatingState::Parked,
        Some(RideOperatingStateDto::Standing) => return RideOperatingState::Standing,
        Some(RideOperatingStateDto::Riding) => return RideOperatingState::Riding,
        Some(RideOperatingStateDto::Charging) => return RideOperatingState::Charging,
        Some(RideOperatingStateDto::Unknown) | None => {}
    }
    match charge_mode.map(|mode| mode.value) {
        Some(ChargeModeDto::Charging) => RideOperatingState::Charging,
        Some(ChargeModeDto::NotCharging) | None => match speed.map(|speed| speed.value.cmp(&0)) {
            Some(std::cmp::Ordering::Equal) => RideOperatingState::Standing,
            Some(_) => RideOperatingState::Riding,
            None => RideOperatingState::Unknown,
        },
    }
}

fn mobile_ride_operating_state_dto(state: RideOperatingState) -> RideOperatingStateDto {
    match state {
        RideOperatingState::Unknown => RideOperatingStateDto::Unknown,
        RideOperatingState::Parked => RideOperatingStateDto::Parked,
        RideOperatingState::Standing => RideOperatingStateDto::Standing,
        RideOperatingState::Riding => RideOperatingStateDto::Riding,
        RideOperatingState::Charging => RideOperatingStateDto::Charging,
    }
}

impl From<NotificationEvidenceDto> for MobileNotificationEvidenceDto {
    fn from(evidence: NotificationEvidenceDto) -> Self {
        Self {
            family: evidence.family.into(),
            channel: evidence.channel.to_vec(),
            len: evidence.len.into(),
            monotonic_ms: MobileMonotonicMillisDto::from_core_ffi_timestamp(evidence.monotonic_ms),
        }
    }
}

impl From<IgnoredNotificationEvidenceDto> for MobileIgnoredNotificationEvidenceDto {
    fn from(evidence: IgnoredNotificationEvidenceDto) -> Self {
        Self {
            family: evidence.family.map(Into::into),
            channel: evidence.channel.to_vec(),
            len: evidence.len.into(),
            monotonic_ms: MobileMonotonicMillisDto::from_core_ffi_timestamp(evidence.monotonic_ms),
            retained_payload: evidence.retained_payload,
        }
    }
}

impl From<IgnoredNotificationReasonDto> for MobileIgnoredNotificationReasonDto {
    fn from(reason: IgnoredNotificationReasonDto) -> Self {
        match reason {
            IgnoredNotificationReasonDto::WrongChannel => Self::WrongChannel,
            IgnoredNotificationReasonDto::UnsupportedFamily => Self::UnsupportedFamily,
            IgnoredNotificationReasonDto::UnsupportedChannel => Self::UnsupportedChannel,
            IgnoredNotificationReasonDto::AcceptedButUnmapped => Self::AcceptedButUnmapped,
            IgnoredNotificationReasonDto::SeekingFrameBoundary => Self::SeekingFrameBoundary,
            IgnoredNotificationReasonDto::IntentionallyDropped => Self::IntentionallyDropped,
        }
    }
}

impl From<ParserFrameLenDto> for MobileParserFrameLenDto {
    fn from(value: ParserFrameLenDto) -> Self {
        Self {
            bytes: value.bytes as u64,
        }
    }
}

impl From<ParserErrorDto> for MobileParserErrorDto {
    fn from(error: ParserErrorDto) -> Self {
        match error {
            ParserErrorDto::OversizedFrame { claimed, max } => Self {
                kind: MobileParserErrorKindDto::OversizedFrame,
                claimed: Some(claimed.into()),
                max: Some(max.into()),
                elapsed_ms: None,
                timeout_ms: None,
            },
            ParserErrorDto::BadChecksum => Self {
                kind: MobileParserErrorKindDto::BadChecksum,
                claimed: None,
                max: None,
                elapsed_ms: None,
                timeout_ms: None,
            },
            ParserErrorDto::MalformedFrame => Self {
                kind: MobileParserErrorKindDto::MalformedFrame,
                claimed: None,
                max: None,
                elapsed_ms: None,
                timeout_ms: None,
            },
            ParserErrorDto::Timeout {
                elapsed_ms,
                timeout_ms,
            } => Self {
                kind: MobileParserErrorKindDto::Timeout,
                claimed: None,
                max: None,
                elapsed_ms: Some(MobileMonotonicMillisDto::from_core_ffi_timestamp(
                    elapsed_ms,
                )),
                timeout_ms: Some(MobileMonotonicMillisDto::from_core_ffi_timestamp(
                    timeout_ms,
                )),
            },
            ParserErrorDto::UnmatchedReply => Self {
                kind: MobileParserErrorKindDto::UnmatchedReply,
                claimed: None,
                max: None,
                elapsed_ms: None,
                timeout_ms: None,
            },
        }
    }
}

impl From<ReservedPayloadEvidenceDto> for MobileReservedPayloadEvidenceDto {
    fn from(evidence: ReservedPayloadEvidenceDto) -> Self {
        Self {
            selector: evidence.selector,
            tag: evidence.tag,
            body_len: evidence.body_len.into(),
            retained_payload: evidence.retained_payload,
            verification: evidence.verification.into(),
        }
    }
}

impl From<ParserGapEvidenceDto> for MobileParserGapEvidenceDto {
    fn from(evidence: ParserGapEvidenceDto) -> Self {
        Self {
            selector: evidence.selector,
            tag: evidence.tag,
            body_len: evidence.body_len.into(),
            retained_payload: evidence.retained_payload,
        }
    }
}

impl From<ConcreteSessionStepResultDto> for MobileSessionStepResultDto {
    fn from(result: ConcreteSessionStepResultDto) -> Self {
        Self {
            outputs: result.outputs.into_iter().map(Into::into).collect(),
            error: result.error.map(Into::into),
        }
    }
}

fn preserve_refused_command(
    mut result: MobileSessionStepResultDto,
    command: Option<MobileCommandDto>,
) -> MobileSessionStepResultDto {
    if let Some(MobileSessionStepErrorDto {
        kind: MobileSessionStepErrorKindDto::CommandRefused,
        command: error_command @ None,
        ..
    }) = &mut result.error
    {
        *error_command = command;
    }
    result
}

fn mobile_aero_session_step_result(
    result: ConcreteSessionStepResultDto,
) -> MobileSessionStepResultDto {
    MobileSessionStepResultDto {
        outputs: result
            .outputs
            .into_iter()
            .map(mobile_aero_session_output)
            .collect(),
        error: result.error.map(Into::into),
    }
}

fn mobile_aero_session_output(output: SessionOutputDto) -> MobileSessionOutputDto {
    let veteran_protocol_model_id = match &output {
        SessionOutputDto::ReadOnly(response) => match &response.payload {
            ReadOnlyOutputPayload::Firmware(firmware) => {
                firmware.firmware_major.map(|major| major.value)
            }
            _ => None,
        },
        _ => None,
    };

    MobileSessionOutputDto {
        veteran_protocol_model_id,
        ..MobileSessionOutputDto::from(output)
    }
}

impl From<ConcreteSessionErrorDto> for MobileSessionStepErrorDto {
    fn from(error: ConcreteSessionErrorDto) -> Self {
        match error {
            ConcreteSessionErrorDto::CommandRefused { refusal } => Self {
                kind: MobileSessionStepErrorKindDto::CommandRefused,
                command: mobile_command_from_command_kind(refusal.command),
                reason: Some(refusal.reason.into()),
            },
            ConcreteSessionErrorDto::UnsupportedFalconProfile { .. } => Self {
                kind: MobileSessionStepErrorKindDto::UnsupportedFalconProfile,
                command: None,
                reason: None,
            },
        }
    }
}

impl From<ControlRefusalReasonDto> for MobileControlRefusalReasonDto {
    fn from(reason: ControlRefusalReasonDto) -> Self {
        match reason {
            ControlRefusalReasonDto::WrongSafetyClass => Self::WrongSafetyClass,
            ControlRefusalReasonDto::MissingArm => Self::MissingArm,
            ControlRefusalReasonDto::WrongModel => Self::WrongModel,
            ControlRefusalReasonDto::ExpiredArm => Self::ExpiredArm,
            ControlRefusalReasonDto::CurrentLimitExceeded => Self::CurrentLimitExceeded,
            ControlRefusalReasonDto::UnsupportedCommand => Self::UnsupportedCommand,
            ControlRefusalReasonDto::Busy => Self::Busy,
        }
    }
}

impl From<TelemetrySnapshotDto> for MobileTelemetrySnapshotDto {
    fn from(snapshot: TelemetrySnapshotDto) -> Self {
        let operating_state = ride_operating_state(
            snapshot.operating_state,
            snapshot.charge_mode,
            snapshot.speed,
        );
        Self {
            at_ms: snapshot
                .at_ms
                .map(MobileMonotonicMillisDto::from_core_ffi_timestamp),
            speed: snapshot.speed.map(Into::into),
            operating_state,
            vesc_operating_mode: snapshot.operating_mode.map(Into::into),
            vesc_warning: snapshot.ride_warning.map(Into::into),
            vesc_stop_reason: snapshot.ride_stop_reason.map(Into::into),
            voltage: snapshot.voltage.map(Into::into),
            battery_current: snapshot.battery_current.map(Into::into),
            charge_mode: snapshot.charge_mode.map(Into::into),
            motor_current: snapshot.motor_current.map(Into::into),
            power: snapshot.power.map(Into::into),
            power_flow: snapshot
                .battery_current
                .map(|current| power_flow_from_signed_current(current, operating_state)),
            voltage_sag: None,
            controller_temperature: snapshot.controller_temperature.map(Into::into),
            motor_temperature: snapshot.motor_temperature.map(Into::into),
            battery_temperature: snapshot.battery_temperature.map(Into::into),
            pwm: snapshot.pwm.map(Into::into),
            distance: snapshot.distance.map(Into::into),
            trip_distance: snapshot.trip_distance.map(Into::into),
            limp_home_range: None,
            pitch: snapshot.pitch.map(Into::into),
            balance_angle: snapshot.balance_angle.map(Into::into),
            roll: snapshot.roll.map(Into::into),
            footpad: snapshot.footpad.map(Into::into),
            battery_level_reported: snapshot.battery_level_reported.map(Into::into),
            battery_level_estimated: snapshot.battery_level_estimated.map(Into::into),
        }
    }
}

impl From<ChargeModeReadingDto> for MobileChargeModeReadingDto {
    fn from(reading: ChargeModeReadingDto) -> Self {
        Self {
            value: match reading.value {
                ChargeModeDto::Charging => MobileChargeModeDto::Charging,
                ChargeModeDto::NotCharging => MobileChargeModeDto::NotCharging,
            },
            source: reading.source.into(),
            quality: reading.quality.into(),
            verification: reading.verification.into(),
        }
    }
}

impl From<FootpadTelemetryDto> for MobileFootpadTelemetryDto {
    fn from(footpad: FootpadTelemetryDto) -> Self {
        Self {
            state: footpad.state,
            contact_state: footpad.contact_state.map(Into::into),
            adc1_milliunits: footpad.adc1_milliunits,
            adc2_milliunits: footpad.adc2_milliunits,
        }
    }
}

impl From<FootpadContactStateDto> for MobileFootpadContactState {
    fn from(state: FootpadContactStateDto) -> Self {
        match state {
            FootpadContactStateDto::None => Self::None,
            FootpadContactStateDto::Left => Self::Left,
            FootpadContactStateDto::Right => Self::Right,
            FootpadContactStateDto::Both => Self::Both,
        }
    }
}

impl From<ParserDiagnosticsDto> for MobileParserDiagnosticsDto {
    fn from(diagnostics: ParserDiagnosticsDto) -> Self {
        Self {
            dropped_bytes: diagnostics.dropped_bytes.into(),
            resyncs: diagnostics.resyncs.into(),
            malformed_frames: diagnostics.malformed_frames.into(),
            bad_checksums: diagnostics.bad_checksums.into(),
            timeouts: diagnostics.timeouts.into(),
            oversized_frames: diagnostics.oversized_frames.into(),
            unmatched_replies: diagnostics.unmatched_replies.into(),
        }
    }
}

impl From<MobileFalconProfileDto> for ConcreteFalconProfileDto {
    fn from(profile: MobileFalconProfileDto) -> Self {
        match profile {
            MobileFalconProfileDto::Default => Self::Default,
            MobileFalconProfileDto::Unsupported => Self::Unsupported,
        }
    }
}

#[derive(Debug)]
struct MobileChargeEstimatorState {
    estimator: cutout_core::ChargeEstimator,
    voltage_sag: VoltageSagEstimator,
    profile: Option<MobileChargeProfileDto>,
}

/// Rust-owned charging estimate engine for mobile sessions.
#[derive(Debug, uniffi::Object)]
pub struct MobileChargeEstimator {
    state: Mutex<MobileChargeEstimatorState>,
}

#[uniffi::export]
impl MobileChargeEstimator {
    /// Creates an empty estimator. A verified profile must be configured before use.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(MobileChargeEstimatorState {
                estimator: cutout_core::ChargeEstimator::new(),
                voltage_sag: VoltageSagEstimator::new(),
                profile: None,
            }),
        })
    }

    /// Selects the Rust-owned battery profile for a supported EUC model.
    pub fn configure_electric_unicycle_profile(&self, model: DiscoveryElectricUnicycleModel) {
        match model {
            DiscoveryElectricUnicycleModel::Aero => {
                self.configure_nosfet_aero_30s2p_samsung_50s_profile();
            }
            DiscoveryElectricUnicycleModel::Falcon => {
                self.configure_begode_falcon_24s2p_samsung_50s_profile();
            }
        }
    }

    /// Configures the confirmed NOSFET Aero pack basis: 30s2p Samsung 50S,
    /// with a 10 Ah profile capacity. Charge-flow polarity remains unverified
    /// until the LIBCU-521 hardware matrix is complete.
    pub fn configure_nosfet_aero_30s2p_samsung_50s_profile(&self) {
        self.configure_profile(MobileChargeProfileDto {
            session_id: 43,
            profile_id: 43,
            capacity_milliamp_hours: 10_000,
            capacity_source: MobileChargeCapacitySourceDto::ProtocolProfile,
            verification: MobileVerificationStatusDto::SourceVerified,
            charge_flow_verification: MobileVerificationStatusDto::Unverified,
        });
    }

    /// Configures the current Falcon battery basis: 24s2p Samsung 50S,
    /// 100.8 V full-charge class and approximately 900 Wh nominal energy.
    /// Charge-flow polarity remains unverified until the LIBCU-521 hardware
    /// matrix is complete.
    pub fn configure_begode_falcon_24s2p_samsung_50s_profile(&self) {
        self.configure_profile(MobileChargeProfileDto {
            session_id: 44,
            profile_id: 44,
            capacity_milliamp_hours: 10_000,
            capacity_source: MobileChargeCapacitySourceDto::ProtocolProfile,
            verification: MobileVerificationStatusDto::SourceVerified,
            charge_flow_verification: MobileVerificationStatusDto::Unverified,
        });
    }

    /// Replaces the usable pack profile and resets bounded history.
    pub fn configure_profile(&self, profile: MobileChargeProfileDto) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let incompatible_profile = state.profile.is_some_and(|current| {
            current.profile_id != profile.profile_id
                || current.capacity_milliamp_hours != profile.capacity_milliamp_hours
        });
        state.estimator.reset();
        if incompatible_profile {
            state.voltage_sag.reset();
        } else {
            state.voltage_sag.reset_observations();
        }
        state.profile = Some(profile);
    }

    /// Applies the optional charge basis carried by a device-specific VESC profile.
    pub fn configure_vesc_board_profile(&self, board_profile: VescBoardProfile) {
        if let Some(profile) = board_profile.charge_profile {
            self.configure_profile(profile);
        } else {
            self.clear_profile();
        }
    }

    /// Removes the usable pack profile and resets bounded history.
    pub fn clear_profile(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.estimator.reset();
        state.voltage_sag.reset_observations();
        state.profile = None;
    }

    /// Resets the bounded current and sag windows.
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.estimator.reset();
        state.voltage_sag.reset_observations();
    }

    /// Returns the durable learned resistance for persistence by the platform layer.
    pub fn voltage_sag_model(&self) -> Option<MobileVoltageSagModelDto> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .voltage_sag
            .model()
            .map(|model| MobileVoltageSagModelDto {
                schema_version: 1,
                effective_resistance_milliohms: model.effective_resistance.as_milliohms(),
                observations: model.observations,
                hardware_verified: model.hardware_verified,
            })
    }

    /// Restores a validated resistance model already scoped to the active EUC identity.
    #[must_use]
    pub fn restore_voltage_sag_model(&self, model: MobileVoltageSagModelDto) -> bool {
        if model.schema_version != 1 {
            return false;
        }
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .voltage_sag
            .restore_model(VoltageSagModel::new(
                EffectiveResistance::from_milliohms(model.effective_resistance_milliohms),
                model.observations,
                model.hardware_verified,
            ))
    }

    /// Explicitly clears the durable learned resistance for the active EUC.
    pub fn clear_voltage_sag_model(&self) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .voltage_sag
            .reset();
    }

    /// Admits one typed telemetry sample and returns its presentation state.
    pub fn update(&self, input: MobileChargeEstimateInputDto) -> MobileChargeEstimateStateDto {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let snapshot = input.snapshot;
        let charge_mode = snapshot.charge_mode.map_or_else(
            || Measured::estimated(ChargeMode::NotCharging),
            core_charge_mode,
        );
        let charge_flow_verification = state
            .profile
            .map_or(VerificationStatus::Unverified, |profile| {
                profile.charge_flow_verification.into()
            });
        let flow = core_charge_flow(&snapshot, charge_mode, charge_flow_verification);
        let voltage_sag = match (snapshot.voltage, snapshot.battery_current) {
            (Some(voltage), Some(battery_current)) => state.voltage_sag.update(VoltageSagInput {
                at: MonotonicTimestamp::new(snapshot.at_ms.unwrap_or(input.at).milliseconds),
                flow: flow.value,
                voltage: core_measured_voltage(voltage),
                battery_current: core_measured_battery_current(battery_current),
                freshness: TelemetryFreshness::new(CoreDuration::from_milliseconds(
                    input.freshness.milliseconds,
                )),
            }),
            _ => None,
        };
        let Some(profile) = state.profile else {
            return charge_estimate_state_unavailable(
                MobileChargeEstimateUnavailableReasonDto::CapacityMissing,
                None,
                voltage_sag,
            );
        };
        let battery_level = snapshot
            .battery_level_reported
            .map(core_battery_level)
            .map(BatteryLevelBasis::reported)
            .or_else(|| {
                snapshot.battery_level_estimated.map(|level| {
                    BatteryLevelBasis::profile_estimated(
                        core_battery_level(level),
                        ChargeProfileIdentity::new(profile.profile_id),
                        MobileEstimateConfidenceDto::Medium.into(),
                    )
                })
            })
            .unwrap_or(BatteryLevelBasis::Unavailable);
        let at = MonotonicTimestamp::new(input.at.milliseconds);
        let observed_at = snapshot.at_ms.map_or(at, |timestamp| {
            MonotonicTimestamp::new(timestamp.milliseconds)
        });
        let result = state.estimator.update(ChargeEstimateInput {
            session: ChargeSessionIdentity::new(profile.session_id),
            profile: ChargeProfileIdentity::new(profile.profile_id),
            at,
            observed_at,
            battery_current: snapshot.battery_current.map(core_battery_current),
            charge_mode,
            flow,
            battery_level,
            usable_capacity: UsablePackCapacity::new(
                Capacity::from_milliamp_hours(profile.capacity_milliamp_hours),
                profile.capacity_source.into(),
                profile.verification.into(),
            ),
            battery_temperature: snapshot.battery_temperature.map(core_temperature),
            voltage_sag,
            freshness: TelemetryFreshness::new(CoreDuration::from_milliseconds(
                input.freshness.milliseconds,
            )),
        });
        mobile_charge_estimate_state(result, state.estimator.last_reset_reason(), voltage_sag)
    }
}

impl Default for MobileChargeEstimator {
    fn default() -> Self {
        Self {
            state: Mutex::new(MobileChargeEstimatorState {
                estimator: cutout_core::ChargeEstimator::new(),
                voltage_sag: VoltageSagEstimator::new(),
                profile: None,
            }),
        }
    }
}

fn core_measured<T>(
    value: T,
    source: MobileValueSourceDto,
    quality: MobileValueQualityDto,
    verification: MobileVerificationStatusDto,
) -> Measured<T> {
    Measured {
        value,
        source: match source {
            MobileValueSourceDto::Reported => CoreValueSource::Reported,
            MobileValueSourceDto::Calculated => CoreValueSource::Calculated,
            MobileValueSourceDto::Estimated => CoreValueSource::Estimated,
        },
        quality: match quality {
            MobileValueQualityDto::Known => CoreValueQuality::Known,
            MobileValueQualityDto::Inferred => CoreValueQuality::Inferred,
        },
        verification: verification.into(),
    }
}

fn core_measured_battery_current(reading: BatteryCurrentReading) -> Measured<CoreBatteryCurrent> {
    core_measured(
        CoreBatteryCurrent::from_milliamps(reading.value.value),
        reading.source,
        reading.quality,
        reading.verification,
    )
}

fn core_battery_current(reading: BatteryCurrentReading) -> Measured<CoreBatteryCurrent> {
    core_measured_battery_current(reading)
}

fn core_measured_voltage(reading: VoltageReading) -> Measured<CoreVoltage> {
    core_measured(
        CoreVoltage::from_millivolts(reading.value.value),
        reading.source,
        reading.quality,
        reading.verification,
    )
}

fn core_battery_level(reading: BatteryLevelReading) -> Measured<CoreBatteryLevel> {
    core_measured(
        CoreBatteryLevel::from_percent(reading.value.value),
        reading.source,
        reading.quality,
        reading.verification,
    )
}

fn core_temperature(reading: TemperatureReading) -> Measured<cutout_core::Temperature> {
    core_measured(
        cutout_core::Temperature::from_millicelsius(reading.value.value),
        reading.source,
        reading.quality,
        reading.verification,
    )
}

fn core_charge_mode(reading: MobileChargeModeReadingDto) -> Measured<ChargeMode> {
    core_measured(
        match reading.value {
            MobileChargeModeDto::Charging => ChargeMode::Charging,
            MobileChargeModeDto::NotCharging => ChargeMode::NotCharging,
        },
        reading.source,
        reading.quality,
        reading.verification,
    )
}

fn core_charge_flow(
    snapshot: &MobileTelemetrySnapshotDto,
    charge_mode: Measured<ChargeMode>,
    charge_flow_verification: VerificationStatus,
) -> Measured<ChargeFlow> {
    let flow = if charge_mode.value.is_active() {
        match snapshot.power_flow {
            Some(
                PowerFlowDirection::Discharge
                | PowerFlowDirection::Regeneration
                | PowerFlowDirection::NegativeUnknown,
            ) => ChargeFlow::Unknown,
            Some(PowerFlowDirection::Charging | PowerFlowDirection::Zero) | None => {
                ChargeFlow::Charging
            }
        }
    } else {
        match snapshot.power_flow {
            Some(PowerFlowDirection::Discharge) => ChargeFlow::Discharging,
            Some(PowerFlowDirection::Regeneration) => ChargeFlow::Regeneration,
            Some(PowerFlowDirection::Zero) => ChargeFlow::Zero,
            Some(PowerFlowDirection::Charging) => ChargeFlow::Charging,
            Some(PowerFlowDirection::NegativeUnknown) | None => ChargeFlow::Unknown,
        }
    };
    Measured {
        value: flow,
        source: charge_mode.source,
        quality: charge_mode.quality,
        verification: combine_verification(charge_mode.verification, charge_flow_verification),
    }
}

fn combine_verification(left: VerificationStatus, right: VerificationStatus) -> VerificationStatus {
    let source_verified = matches!(
        left,
        VerificationStatus::SourceVerified | VerificationStatus::SourceAndHardwareVerified
    ) && matches!(
        right,
        VerificationStatus::SourceVerified | VerificationStatus::SourceAndHardwareVerified
    );
    let hardware_verified = matches!(
        left,
        VerificationStatus::HardwareVerified | VerificationStatus::SourceAndHardwareVerified
    ) && matches!(
        right,
        VerificationStatus::HardwareVerified | VerificationStatus::SourceAndHardwareVerified
    );
    match (source_verified, hardware_verified) {
        (true, true) => VerificationStatus::SourceAndHardwareVerified,
        (true, false) => VerificationStatus::SourceVerified,
        (false, true) => VerificationStatus::HardwareVerified,
        (false, false) => VerificationStatus::Unverified,
    }
}

fn mobile_charge_estimate_state(
    state: ChargeEstimateState,
    reset_reason: Option<ChargeEstimateResetReason>,
    voltage_sag: Option<VoltageSagEstimate>,
) -> MobileChargeEstimateStateDto {
    let reset_reason = reset_reason.map(Into::into);
    let voltage_sag = voltage_sag.map(Into::into);
    match state {
        ChargeEstimateState::CollectingSamples {
            samples,
            observed_for,
        } => MobileChargeEstimateStateDto {
            kind: MobileChargeEstimateStateKindDto::CollectingSamples,
            estimate: None,
            voltage_sag,
            unavailable_reason: None,
            error: None,
            reset_reason,
            samples,
            observed_for: mobile_duration(observed_for),
        },
        ChargeEstimateState::Available(estimate) => {
            let estimate = estimate.into();
            MobileChargeEstimateStateDto {
                kind: MobileChargeEstimateStateKindDto::Available,
                estimate: Some(estimate),
                voltage_sag,
                unavailable_reason: None,
                error: None,
                reset_reason,
                samples: 0,
                observed_for: MobileDurationDto { milliseconds: 0 },
            }
        }
        ChargeEstimateState::Unavailable { reason } => MobileChargeEstimateStateDto {
            kind: MobileChargeEstimateStateKindDto::Unavailable,
            estimate: None,
            voltage_sag,
            unavailable_reason: Some(reason.into()),
            error: None,
            reset_reason,
            samples: 0,
            observed_for: MobileDurationDto { milliseconds: 0 },
        },
        ChargeEstimateState::Stale => MobileChargeEstimateStateDto {
            kind: MobileChargeEstimateStateKindDto::Stale,
            estimate: None,
            voltage_sag,
            unavailable_reason: Some(MobileChargeEstimateUnavailableReasonDto::StaleInput),
            error: None,
            reset_reason,
            samples: 0,
            observed_for: MobileDurationDto { milliseconds: 0 },
        },
        ChargeEstimateState::Failed(error) => MobileChargeEstimateStateDto {
            kind: MobileChargeEstimateStateKindDto::Failed,
            estimate: None,
            voltage_sag,
            unavailable_reason: None,
            error: Some(error.into()),
            reset_reason,
            samples: 0,
            observed_for: MobileDurationDto { milliseconds: 0 },
        },
        _ => MobileChargeEstimateStateDto {
            kind: MobileChargeEstimateStateKindDto::Failed,
            estimate: None,
            voltage_sag,
            unavailable_reason: None,
            error: Some(MobileChargeEstimateErrorDto::ArithmeticOverflow),
            reset_reason,
            samples: 0,
            observed_for: MobileDurationDto { milliseconds: 0 },
        },
    }
}

fn charge_estimate_state_unavailable(
    reason: MobileChargeEstimateUnavailableReasonDto,
    reset_reason: Option<MobileChargeEstimateResetReasonDto>,
    voltage_sag: Option<VoltageSagEstimate>,
) -> MobileChargeEstimateStateDto {
    MobileChargeEstimateStateDto {
        kind: MobileChargeEstimateStateKindDto::Unavailable,
        estimate: None,
        voltage_sag: voltage_sag.map(Into::into),
        unavailable_reason: Some(reason),
        error: None,
        reset_reason,
        samples: 0,
        observed_for: MobileDurationDto { milliseconds: 0 },
    }
}

fn mobile_duration(duration: CoreDuration) -> MobileDurationDto {
    MobileDurationDto {
        milliseconds: duration.as_milliseconds(),
    }
}

impl From<ChargeTimeEstimate> for MobileChargeTimeEstimateDto {
    fn from(estimate: ChargeTimeEstimate) -> Self {
        let (battery_level, battery_level_basis, battery_profile_id) =
            match estimate.battery_level_basis {
                BatteryLevelBasis::Reported(level) => (
                    mobile_battery_level(level),
                    MobileBatteryLevelBasisDto::Reported,
                    None,
                ),
                BatteryLevelBasis::ProfileEstimated { level, profile, .. } => (
                    mobile_battery_level(level),
                    MobileBatteryLevelBasisDto::ProfileEstimated,
                    Some(profile.get()),
                ),
                _ => (
                    mobile_battery_level(Measured::estimated(CoreBatteryLevel::from_percent(0))),
                    MobileBatteryLevelBasisDto::ProfileEstimated,
                    None,
                ),
            };
        Self {
            lower: mobile_duration(estimate.lower),
            expected: mobile_duration(estimate.expected),
            upper: mobile_duration(estimate.upper),
            kind: estimate.kind.into(),
            confidence: estimate.confidence.into(),
            current_rate: MobileCurrentRateSummaryDto {
                mean_milliamps: estimate.current_rate.mean.as_milliamps(),
                minimum_milliamps: estimate.current_rate.minimum.as_milliamps(),
                maximum_milliamps: estimate.current_rate.maximum.as_milliamps(),
                variability_permille: estimate.current_rate.variability_permille,
            },
            battery_level,
            battery_level_basis,
            battery_profile_id,
            capacity_source: estimate.capacity_source.into(),
            voltage_sag: estimate.voltage_sag.map(Into::into),
            calculated_at: MobileMonotonicMillisDto {
                milliseconds: estimate.calculated_at.get(),
            },
            valid_until: MobileMonotonicMillisDto {
                milliseconds: estimate.valid_until.get(),
            },
        }
    }
}

fn mobile_battery_level(level: Measured<CoreBatteryLevel>) -> BatteryLevelReading {
    BatteryLevelReading {
        value: BatteryLevel {
            value: level.value.as_percent(),
        },
        source: level.source.into(),
        quality: level.quality.into(),
        verification: level.verification.into(),
    }
}

impl From<VoltageSagEstimate> for MobileVoltageSagEstimateDto {
    fn from(estimate: VoltageSagEstimate) -> Self {
        Self {
            delta_millivolts: estimate.delta.as_millivolts(),
            load_current: BatteryCurrentReading {
                value: BatteryCurrent {
                    value: estimate.load_current.value.as_milliamps(),
                },
                source: estimate.load_current.source.into(),
                quality: estimate.load_current.quality.into(),
                verification: estimate.load_current.verification.into(),
            },
            effective_resistance_milliohms: estimate.effective_resistance.as_milliohms(),
            observations: estimate.observations,
            confidence: estimate.confidence.into(),
            calculated_at: MobileMonotonicMillisDto {
                milliseconds: estimate.calculated_at.get(),
            },
            valid_until: MobileMonotonicMillisDto {
                milliseconds: estimate.valid_until.get(),
            },
        }
    }
}

impl From<MobileChargeCapacitySourceDto> for cutout_core::CapacitySource {
    fn from(source: MobileChargeCapacitySourceDto) -> Self {
        match source {
            MobileChargeCapacitySourceDto::ProtocolProfile => Self::ProtocolProfile,
            MobileChargeCapacitySourceDto::HardwareMeasured => Self::HardwareMeasured,
            MobileChargeCapacitySourceDto::Estimated => Self::Estimated,
        }
    }
}

impl From<MobileEstimateConfidenceDto> for cutout_core::EstimateConfidence {
    fn from(confidence: MobileEstimateConfidenceDto) -> Self {
        match confidence {
            MobileEstimateConfidenceDto::Low => Self::Low,
            MobileEstimateConfidenceDto::Medium => Self::Medium,
            MobileEstimateConfidenceDto::High => Self::High,
        }
    }
}

impl From<MobileEstimateKindDto> for cutout_core::EstimateKind {
    fn from(kind: MobileEstimateKindDto) -> Self {
        match kind {
            MobileEstimateKindDto::AtPresentCurrent => Self::AtPresentCurrent,
            MobileEstimateKindDto::ProfileBackedTimeToFull => Self::ProfileBackedTimeToFull,
            MobileEstimateKindDto::ObservedTaperTimeToFull => Self::ObservedTaperTimeToFull,
        }
    }
}

impl From<cutout_core::CapacitySource> for MobileChargeCapacitySourceDto {
    fn from(source: cutout_core::CapacitySource) -> Self {
        match source {
            cutout_core::CapacitySource::ProtocolProfile => Self::ProtocolProfile,
            cutout_core::CapacitySource::HardwareMeasured => Self::HardwareMeasured,
            _ => Self::Estimated,
        }
    }
}

impl From<cutout_core::EstimateConfidence> for MobileEstimateConfidenceDto {
    fn from(confidence: cutout_core::EstimateConfidence) -> Self {
        match confidence {
            cutout_core::EstimateConfidence::Medium => Self::Medium,
            cutout_core::EstimateConfidence::High => Self::High,
            _ => Self::Low,
        }
    }
}

impl From<cutout_core::EstimateKind> for MobileEstimateKindDto {
    fn from(kind: cutout_core::EstimateKind) -> Self {
        match kind {
            cutout_core::EstimateKind::ProfileBackedTimeToFull => Self::ProfileBackedTimeToFull,
            cutout_core::EstimateKind::ObservedTaperTimeToFull => Self::ObservedTaperTimeToFull,
            _ => Self::AtPresentCurrent,
        }
    }
}

impl From<ChargeEstimateUnavailableReason> for MobileChargeEstimateUnavailableReasonDto {
    fn from(reason: ChargeEstimateUnavailableReason) -> Self {
        match reason {
            ChargeEstimateUnavailableReason::NotCharging => Self::NotCharging,
            ChargeEstimateUnavailableReason::CurrentMissing => Self::CurrentMissing,
            ChargeEstimateUnavailableReason::CurrentDirectionUnverified => {
                Self::CurrentDirectionUnverified
            }
            ChargeEstimateUnavailableReason::CurrentTooSmall => Self::CurrentTooSmall,
            ChargeEstimateUnavailableReason::BatteryLevelMissing => Self::BatteryLevelMissing,
            ChargeEstimateUnavailableReason::CapacityMissing => Self::CapacityMissing,
            ChargeEstimateUnavailableReason::UnsupportedProfile => Self::UnsupportedProfile,
            ChargeEstimateUnavailableReason::UnstableCurrent => Self::UnstableCurrent,
            ChargeEstimateUnavailableReason::StaleInput => Self::StaleInput,
            ChargeEstimateUnavailableReason::TemperatureOutOfModel => Self::TemperatureOutOfModel,
            ChargeEstimateUnavailableReason::FullOrNearFull => Self::FullOrNearFull,
            ChargeEstimateUnavailableReason::ContradictoryInputs => Self::ContradictoryInputs,
            _ => Self::CurrentDirectionUnverified,
        }
    }
}

impl From<ChargeEstimateResetReason> for MobileChargeEstimateResetReasonDto {
    fn from(reason: ChargeEstimateResetReason) -> Self {
        match reason {
            ChargeEstimateResetReason::SessionChanged => Self::SessionChanged,
            ChargeEstimateResetReason::ChargingStopped => Self::ChargingStopped,
            ChargeEstimateResetReason::StaleGap => Self::StaleGap,
            ChargeEstimateResetReason::TimestampOrder => Self::TimestampOrder,
            ChargeEstimateResetReason::CurrentEvidenceChanged => Self::CurrentEvidenceChanged,
            ChargeEstimateResetReason::CapacityChanged => Self::CapacityChanged,
            ChargeEstimateResetReason::ProfileChanged => Self::ProfileChanged,
            _ => Self::Manual,
        }
    }
}

impl From<ChargeEstimateError> for MobileChargeEstimateErrorDto {
    fn from(error: ChargeEstimateError) -> Self {
        match error {
            ChargeEstimateError::TimestampOrder => Self::TimestampOrder,
            _ => Self::ArithmeticOverflow,
        }
    }
}

impl From<ConcreteSessionErrorDto> for MobileSessionConstructorError {
    fn from(error: ConcreteSessionErrorDto) -> Self {
        match error {
            ConcreteSessionErrorDto::CommandRefused { .. }
            | ConcreteSessionErrorDto::UnsupportedFalconProfile { .. } => {
                Self::UnsupportedFalconProfile
            }
        }
    }
}

/// Mobile-facing Begode Falcon telemetry wrapper with allow-listed light control.
#[derive(Debug, uniffi::Object)]
pub struct FalconBenignControlSession {
    inner: Mutex<ConcreteFalconBenignControlSession>,
    settings: Mutex<MobileEucSettingTrackers>,
}

#[uniffi::export]
impl FalconBenignControlSession {
    /// Creates a Begode Falcon telemetry session with allow-listed headlight control.
    ///
    /// # Errors
    ///
    /// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
    /// default profile is unavailable.
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, MobileSessionConstructorError> {
        Self::with_profile(MobileFalconProfileDto::Default)
    }

    /// Creates a Begode Falcon telemetry session with an explicit profile.
    ///
    /// # Errors
    ///
    /// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
    /// selected profile is unavailable.
    #[uniffi::constructor]
    pub fn with_profile(
        profile: MobileFalconProfileDto,
    ) -> Result<Arc<Self>, MobileSessionConstructorError> {
        Ok(Arc::new(Self {
            inner: Mutex::new(try_new_begode_falcon_benign_control_session(
                profile.into(),
            )?),
            settings: Mutex::new(MobileEucSettingTrackers::default()),
        }))
    }

    /// Drives one input and returns owned outputs plus any stable error DTO.
    pub fn ingest_checked(&self, input: MobileSessionInputDto) -> MobileSessionStepResultDto {
        let tracked_input = input.clone();
        let input = SessionInputDto::from(input);
        let result = preserve_refused_command(
            MobileSessionStepResultDto::from(self.lock_inner().ingest_checked(&input)),
            tracked_input.command,
        );
        self.lock_settings().observe_step(&tracked_input, &result);
        result
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    pub fn drain_outputs(&self) -> Vec<MobileSessionOutputDto> {
        self.lock_inner()
            .drain_outputs()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    pub fn current_snapshot(&self) -> MobileTelemetrySnapshotDto {
        self.lock_inner().current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    pub fn diagnostics(&self) -> MobileParserDiagnosticsDto {
        self.lock_inner().diagnostics().into()
    }

    /// Returns the EUC setting write capabilities and their validation state.
    pub fn settings_capabilities(&self) -> MobileEucSettingsCapabilitiesDto {
        MobileEucSettingsCapabilitiesDto::falcon()
    }

    /// Arms settings writes only when the latest Rust-owned ride state is stationary.
    pub fn arm_settings_writes(
        &self,
        state: RideOperatingState,
        monotonic_ms: MobileMonotonicMillisDto,
    ) -> bool {
        let speed_mm_per_second = self
            .lock_inner()
            .current_snapshot()
            .speed
            .map(|speed| speed.value);
        self.lock_inner().arm_settings_writes(
            mobile_ride_operating_state_dto(state),
            speed_mm_per_second,
            monotonic_ms.milliseconds,
        )
    }

    /// Returns the Rust-owned headlight write lifecycle state.
    pub fn headlight_state(&self) -> MobileLightSettingStateDto {
        self.lock_settings().headlight()
    }

    /// Returns the Rust-owned Aero high-beam write lifecycle state.
    pub fn aero_high_beam_state(&self) -> MobileLightSettingStateDto {
        self.lock_settings().aero_high_beam()
    }

    /// Returns the Rust-owned Aero tilt-back speed lifecycle state.
    pub fn aero_tiltback_speed_state(&self) -> MobileAeroSpeedSettingStateDto {
        self.lock_settings().aero_tiltback_speed()
    }

    /// Returns the Rust-owned Aero PWM-warning lifecycle state.
    pub fn aero_pwm_percent_state(&self) -> MobileAeroPwmSettingStateDto {
        self.lock_settings().aero_pwm_percent()
    }

    /// Returns the Rust-owned Aero alarm-speed lifecycle state.
    pub fn aero_alarm_speed_state(&self) -> MobileAeroSpeedSettingStateDto {
        self.lock_settings().aero_alarm_speed()
    }

    /// Returns the Rust-owned Aero angle-adjustment lifecycle state.
    pub fn aero_angle_adjustment_state(&self) -> MobileAeroAngleAdjustmentStateDto {
        self.lock_settings().aero_angle_adjustment()
    }

    /// Returns the Rust-owned pedal-mode setting lifecycle state.
    pub fn pedal_mode_state(&self) -> MobilePedalModeSettingStateDto {
        self.lock_settings().pedal_mode()
    }

    /// Returns the Rust-owned roll-angle setting lifecycle state.
    pub fn roll_angle_state(&self) -> MobileRollAngleSettingStateDto {
        self.lock_settings().roll_angle()
    }

    /// Returns the Rust-owned speed-alarm setting lifecycle state.
    pub fn speed_alarm_mode_state(&self) -> MobileSpeedAlarmModeSettingStateDto {
        self.lock_settings().speed_alarm_mode()
    }

    /// Returns the Rust-owned acceleration-assist setting lifecycle state.
    pub fn acceleration_assist_state(&self) -> MobileAccelerationAssistSettingStateDto {
        self.lock_settings().acceleration_assist()
    }

    /// Returns the Rust-owned taillight setting lifecycle state.
    pub fn taillight_state(&self) -> MobileLightSettingStateDto {
        self.lock_settings().taillight()
    }
}

impl FalconBenignControlSession {
    fn lock_inner(&self) -> MutexGuard<'_, ConcreteFalconBenignControlSession> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_settings(&self) -> MutexGuard<'_, MobileEucSettingTrackers> {
        self.settings.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Mobile-facing VESC board profile used to preserve geometry and pack facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct VescBoardProfile {
    /// Motor pole pairs used to convert electrical RPM to mechanical RPM.
    pub motor_pole_pairs: u8,

    /// Mechanical gear reduction denominator.
    pub gear_ratio_denominator: u8,

    /// Wheel circumference used for direct-drive speed calculations.
    pub wheel_circumference: Distance,

    /// VESC battery type used for voltage-derived pack level.
    pub battery_type: VescBatteryType,

    /// Number of series cells in the pack.
    pub battery_cells: u8,

    /// Number of parallel cells in each series group.
    pub battery_parallel_cells: u8,

    /// Physical cell model, independent of the generic voltage curve family.
    pub battery_cell_model: VescBatteryCellModel,

    /// Optional verified usable-capacity basis for charge estimation.
    pub charge_profile: Option<MobileChargeProfileDto>,

    /// Whether the controller reports battery current directly.
    pub reports_battery_current: bool,
}

impl From<VescBoardProfile> for CoreVescBoardProfile {
    fn from(profile: VescBoardProfile) -> Self {
        let battery_type = match profile.battery_type {
            VescBatteryType::LiIon => CoreVescBatteryType::LiIon,
            VescBatteryType::LiIron => CoreVescBatteryType::LiIron,
            VescBatteryType::LeadAcid => CoreVescBatteryType::LeadAcid,
            VescBatteryType::Other => CoreVescBatteryType::Other(0),
        };
        let mut core_profile = CoreVescBoardProfile::new(
            cutout_protocols::MotorPolePairs::new(profile.motor_pole_pairs),
            cutout_protocols::GearRatioDenominator::new(profile.gear_ratio_denominator),
            cutout_core::Distance::from_millimetres(profile.wheel_circumference.value),
        )
        .with_vesc_battery_type(battery_type, SeriesCount::new(profile.battery_cells));
        if profile.reports_battery_current {
            core_profile = core_profile.with_reported_battery_current();
        }
        core_profile
    }
}

/// VESC battery type from motor setup config.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum VescBatteryType {
    /// Li-ion 3.0-4.2 V pack type.
    LiIon,

    /// `LiFePO4` / lithium iron 2.6-3.6 V pack type.
    LiIron,

    /// Lead-acid 2.1-2.36 V cell model.
    LeadAcid,

    /// A battery type not modeled by libcutout yet.
    Other,
}

/// Cell identity retained for device-specific VESC pack configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum VescBatteryCellModel {
    /// Cell model is not known.
    Unknown,

    /// Murata/Sony US18650VTC6.
    SonyVtc6,
}

/// Mobile-facing wrapper for a generic VESC read-only session.
#[derive(Debug, uniffi::Object)]
pub struct VescReadOnlySession {
    inner: Mutex<CoreVescReadOnlySession>,
}

#[uniffi::export]
impl VescReadOnlySession {
    /// Creates a generic VESC read-only session.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(CoreVescReadOnlySession::new()),
        })
    }

    /// Creates a VESC read-only session with explicit board geometry and pack facts.
    #[uniffi::constructor]
    #[must_use]
    pub fn with_board_profile(board_profile: VescBoardProfile) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(CoreVescReadOnlySession::with_board_profile(
                board_profile.into(),
            )),
        })
    }

    /// Drives one input and returns owned outputs plus any stable error DTO.
    pub fn ingest_checked(&self, input: MobileSessionInputDto) -> MobileSessionStepResultDto {
        let input = SessionInputDto::from(input);
        MobileSessionStepResultDto::from(self.lock_inner().ingest_checked(&input))
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    pub fn drain_outputs(&self) -> Vec<MobileSessionOutputDto> {
        self.lock_inner()
            .drain_outputs()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    pub fn current_snapshot(&self) -> MobileTelemetrySnapshotDto {
        self.lock_inner().current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    pub fn diagnostics(&self) -> MobileParserDiagnosticsDto {
        self.lock_inner().diagnostics().into()
    }
}

impl VescReadOnlySession {
    fn lock_inner(&self) -> MutexGuard<'_, CoreVescReadOnlySession> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{PevcapCapture, PevcapEncoding};
    use cutout_protocols::{
        BEGODE_DATA_CHANNEL, BEGODE_SERVICE_CHANNEL, VESC_COMM_CUSTOM_APP_DATA, VESC_NOTIFY_CHANNEL,
    };

    #[test]
    fn mobile_ride_map_limits_match_rust_bounds() {
        let limits = mobile_ride_map_limits();

        assert_eq!(limits.live_tail_point_limit, 4_096);
        assert_eq!(limits.history_preview_point_limit, 16_384);
        assert_eq!(limits.history_page_limit, 50);
        assert_eq!(limits.history_context_max_routes, 8);
        assert_eq!(limits.history_context_per_route_budget, 512);
        assert_eq!(limits.history_context_total_point_budget, 4_096);
        assert_eq!(limits.history_recent_window_milliseconds, 2_592_000_000);
    }

    static RIDE_DATABASE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn drain_location_writes(state: &MobileRideMapCore) -> Vec<MobileRideMapCoreDecisionDto> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let decisions = state.poll_location_writes();
            if !decisions.is_empty() {
                return decisions;
            }
            assert!(
                Instant::now() < deadline,
                "queued location write did not complete in time"
            );
            thread::yield_now();
        }
    }

    fn discovery_service(uuid: CoreBluetoothServiceUuid) -> DiscoveryServiceUuid {
        DiscoveryServiceUuid {
            bytes: uuid.as_bytes().to_vec(),
        }
    }
    fn synthetic_veteran_frame_with_model_id(model_id: u16) -> [u8; 42] {
        let mut bytes = [0_u8; 42];
        bytes[0..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        bytes[28..30].copy_from_slice(&(model_id * 1_000).to_be_bytes());
        bytes
    }

    #[test]
    fn unknown_mobile_telemetry_state_is_not_downgraded_to_gps_only() {
        assert_eq!(
            map_ride_telemetry_state(MobileRideMapCoreTelemetryStateDto::Unknown),
            Err("unsupported telemetry state")
        );
    }

    #[test]
    fn mobile_ride_lifecycle_keeps_identity_through_background_and_reconnect() {
        let handle = CutoutSessionStateHandle::new();
        let started = handle
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "vesc-1".to_owned(),
            })
            .expect("Rust should create a valid ride identity");
        let identity = started
            .snapshot
            .identity
            .clone()
            .expect("a started ride has an identity");

        assert_eq!(
            started.effect,
            MobileRideSessionEffectDto::StartActivity {
                identity: identity.clone(),
            }
        );

        let active = handle
            .reduce_ride_session(MobileRideSessionInputDto::ActivityStarted {
                identity: identity.clone(),
                activity_id: "activity-1".to_owned(),
            })
            .expect("the generated identity should round-trip");
        assert_eq!(active.snapshot.phase, MobileRideSessionPhaseDto::Active);

        let backgrounded = handle
            .reduce_ride_session(MobileRideSessionInputDto::AppBackgrounded)
            .expect("backgrounding has no fallible input");
        assert_eq!(
            backgrounded.effect,
            MobileRideSessionEffectDto::RequestCaptureFlush {
                identity: identity.clone(),
            }
        );

        handle
            .reduce_ride_session(MobileRideSessionInputDto::TelemetryObserved { at_ms: 10 })
            .expect("telemetry time is already typed");
        let disconnected = handle
            .reduce_ride_session(MobileRideSessionInputDto::BluetoothDisconnected { at_ms: 12 })
            .expect("disconnect time is already typed");
        assert_eq!(
            disconnected.snapshot.phase,
            MobileRideSessionPhaseDto::Reconnecting
        );
        assert_eq!(disconnected.snapshot.identity, Some(identity.clone()));

        let reconnected = handle
            .reduce_ride_session(MobileRideSessionInputDto::BluetoothConnected)
            .expect("reconnect has no fallible input");
        assert_eq!(
            reconnected.snapshot.phase,
            MobileRideSessionPhaseDto::Active
        );
        assert_eq!(reconnected.snapshot.identity, Some(identity));
        assert_eq!(reconnected.effect, MobileRideSessionEffectDto::None);
    }

    #[test]
    fn mobile_ride_lifecycle_rejects_invalid_callback_identity_without_mutating_state() {
        let handle = CutoutSessionStateHandle::new();
        let started = handle
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "vesc-1".to_owned(),
            })
            .expect("Rust should create a valid ride identity");
        let expected = started.snapshot;

        let result = handle.reduce_ride_session(MobileRideSessionInputDto::ActivityStarted {
            identity: MobileRideSessionIdentityDto {
                platform_identifier: "vesc-1".to_owned(),
                session_id: "not-a-uuid".to_owned(),
            },
            activity_id: "late-activity".to_owned(),
        });

        assert_eq!(
            result,
            Err(MobileRideSessionInputError::InvalidSessionIdentifier)
        );
        assert_eq!(handle.ride_session_snapshot(), expected);
    }

    #[test]
    fn mobile_ride_lifecycle_preserves_reconnect_exhaustion_reason() {
        let handle = CutoutSessionStateHandle::new();
        let started = handle
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "vesc-1".to_owned(),
            })
            .expect("Rust should create a valid ride identity");
        let identity = started.snapshot.identity.expect("started ride identity");
        handle
            .reduce_ride_session(MobileRideSessionInputDto::ActivityStarted {
                identity: identity.clone(),
                activity_id: "activity-1".to_owned(),
            })
            .expect("the generated identity should round-trip");
        handle
            .reduce_ride_session(MobileRideSessionInputDto::BluetoothDisconnected { at_ms: 12 })
            .expect("disconnect time is already typed");

        let ending = handle
            .reduce_ride_session(MobileRideSessionInputDto::ReconnectExhausted)
            .expect("reconnect exhaustion has no fallible input");

        assert_eq!(
            ending.snapshot.phase,
            MobileRideSessionPhaseDto::Ending {
                reason: MobileRideSessionEndReasonDto::ReconnectExhausted,
            }
        );
        assert_eq!(
            ending.effect,
            MobileRideSessionEffectDto::EndActivity {
                identity,
                reason: MobileRideSessionEndReasonDto::ReconnectExhausted,
            }
        );
    }

    #[test]
    fn mobile_ride_lifecycle_preserves_explicit_session_failure_reasons() {
        for (input, reason) in [
            (
                MobileRideSessionInputDto::AppReset,
                MobileRideSessionEndReasonDto::AppReset,
            ),
            (
                MobileRideSessionInputDto::UnrecoverableSessionFailure,
                MobileRideSessionEndReasonDto::UnrecoverableSessionFailure,
            ),
        ] {
            let handle = CutoutSessionStateHandle::new();
            let started = handle
                .reduce_ride_session(MobileRideSessionInputDto::Start {
                    platform_identifier: "vesc-1".to_owned(),
                })
                .expect("Rust should create a valid ride identity");
            let identity = started.snapshot.identity.expect("started ride identity");
            handle
                .reduce_ride_session(MobileRideSessionInputDto::ActivityStarted {
                    identity: identity.clone(),
                    activity_id: "activity-1".to_owned(),
                })
                .expect("the generated identity should round-trip");

            let ending = handle
                .reduce_ride_session(input)
                .expect("typed terminal events have no fallible payload");

            assert_eq!(
                ending.snapshot.phase,
                MobileRideSessionPhaseDto::Ending { reason }
            );
            assert_eq!(
                ending.effect,
                MobileRideSessionEffectDto::EndActivity { identity, reason }
            );
        }
    }

    #[test]
    fn mobile_ride_marker_recovers_the_same_identity_for_the_restored_platform() {
        let source = CutoutSessionStateHandle::new();
        let started = source
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "vesc-1".to_owned(),
            })
            .expect("Rust should create a valid ride identity");
        let identity = started.snapshot.identity.expect("started ride identity");
        let marker = source
            .export_ride_session_marker()
            .expect("active marker should encode")
            .expect("active rides are restorable");

        let restored = CutoutSessionStateHandle::new();
        let recovered = restored
            .recover_ride_session_marker(marker, Some("vesc-1".to_owned()))
            .expect("valid marker should recover");

        assert_eq!(recovered.snapshot.identity, Some(identity.clone()));
        assert_eq!(
            recovered.snapshot.phase,
            MobileRideSessionPhaseDto::Starting
        );
        assert_eq!(
            recovered.effect,
            MobileRideSessionEffectDto::StartActivity { identity }
        );
        assert_eq!(restored.ride_session_snapshot(), recovered.snapshot);
    }

    #[test]
    fn mobile_ride_marker_ends_as_app_reset_without_a_restored_platform() {
        let source = CutoutSessionStateHandle::new();
        let started = source
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "vesc-1".to_owned(),
            })
            .expect("Rust should create a valid ride identity");
        let identity = started.snapshot.identity.expect("started ride identity");
        let marker = source
            .export_ride_session_marker()
            .expect("active marker should encode")
            .expect("active rides are restorable");

        let restored = CutoutSessionStateHandle::new();
        let recovered = restored
            .recover_ride_session_marker(marker, None)
            .expect("valid marker should reconcile");

        assert_eq!(
            recovered.snapshot.phase,
            MobileRideSessionPhaseDto::Ending {
                reason: MobileRideSessionEndReasonDto::AppReset,
            }
        );
        assert_eq!(
            recovered.effect,
            MobileRideSessionEffectDto::EndActivity {
                identity,
                reason: MobileRideSessionEndReasonDto::AppReset,
            }
        );
    }

    #[test]
    fn mobile_ride_marker_can_be_compared_with_a_restored_platform_without_leaving_rust() {
        let source = CutoutSessionStateHandle::new();
        source
            .reduce_ride_session(MobileRideSessionInputDto::Start {
                platform_identifier: "vesc-1".to_owned(),
            })
            .expect("Rust should create a valid ride identity");
        let marker = source
            .export_ride_session_marker()
            .expect("active marker should encode")
            .expect("active rides are restorable");

        assert!(
            source
                .ride_session_marker_matches_platform_identifier(
                    marker.clone(),
                    "vesc-1".to_owned()
                )
                .expect("valid marker should compare")
        );
        assert!(
            !source
                .ride_session_marker_matches_platform_identifier(marker, "aero-2".to_owned())
                .expect("valid marker should compare")
        );
    }

    #[test]
    fn invalid_mobile_ride_marker_does_not_mutate_session_state() {
        let handle = CutoutSessionStateHandle::new();
        let before = handle.ride_session_snapshot();

        assert_eq!(
            handle.recover_ride_session_marker(vec![0xff], Some("vesc-1".to_owned())),
            Err(MobileRideSessionMarkerError::InvalidEncoding)
        );
        assert_eq!(handle.ride_session_snapshot(), before);
    }

    #[test]
    fn mobile_ride_lifecycle_exposes_the_rust_owned_freshness_window() {
        let snapshot = CutoutSessionStateHandle::new().ride_session_snapshot();

        assert_eq!(snapshot.stale_after_ms, 2_000);
    }

    #[test]
    fn mobile_discovery_candidate_preserves_ios_local_id_without_mac() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-aero".to_owned(),
            Some("NOSFET Aero".to_owned()),
            vec![discovery_service(CoreBluetoothServiceUuid::EUC_SERIAL_FFE0)],
        );

        assert_eq!(candidate.platform_identifier, "ios-local-aero");
        assert_eq!(candidate.display_name, "NOSFET Aero");
        assert_eq!(candidate.product_category, "Electric unicycle");
        assert_eq!(candidate.evidence, "FFE0/FFE1 transport hint");
        assert_eq!(candidate.detail, "Read-only protocol probe recommended");
        assert!(candidate.is_picker_candidate);
        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::ProbeRecommended
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Read-only protocol probe recommended".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_routes_reported_falcon_model_to_session() {
        let candidate = mobile_discovery_candidate_from_begode_identity_probe(
            "ios-local-falcon".to_owned(),
            "GotWay_002441".to_owned(),
            MobileBegodeIdentityProbeDto {
                reported_model: Some("Falcon".to_owned()),
                reported_code_name: Some("GW-FALCON".to_owned()),
                reported_imu: Some("MPU6500".to_owned()),
                reported_firmware_version: Some("1.0.0".to_owned()),
                reported_serial: Some("012345".to_owned()),
                nominal_voltage_hint_mv: Some(100_800),
                missing_probe_response: None,
                malformed_probe_response: None,
            },
        );

        assert_eq!(candidate.platform_identifier, "ios-local-falcon");
        assert_eq!(candidate.display_name, "GotWay_002441");
        assert_eq!(candidate.product_category, "Electric unicycle");
        assert_eq!(
            candidate.evidence,
            "model=Falcon, code=GW-FALCON, imu=MPU6500, firmware=1.0.0, serial=012345, voltage_hint=100800mV"
        );
        assert_eq!(
            candidate.detail,
            "Begode/Falcon confirmed by reported model Falcon, code GW-FALCON, imu MPU6500, firmware 1.0.0, serial 012345, voltage hint 100800mV"
        );
        assert!(candidate.is_picker_candidate);
        assert_eq!(candidate.support, DiscoveryCandidateSupport::Supported);
        assert_eq!(
            candidate.connection_route,
            Some(DiscoveryConnectionRoute::ElectricUnicycle)
        );
        assert_eq!(
            candidate.electric_unicycle_model,
            Some(DiscoveryElectricUnicycleModel::Falcon)
        );
        assert_eq!(candidate.disabled_reason, None);
    }

    #[test]
    fn mobile_discovery_candidate_marks_non_falcon_begode_model_conflicting() {
        let candidate = mobile_discovery_candidate_from_begode_identity_probe(
            "ios-local-master".to_owned(),
            "GotWay_002441".to_owned(),
            MobileBegodeIdentityProbeDto {
                reported_model: Some("Master".to_owned()),
                reported_code_name: Some("GW-MASTER".to_owned()),
                reported_imu: None,
                reported_firmware_version: None,
                reported_serial: None,
                nominal_voltage_hint_mv: None,
                missing_probe_response: None,
                malformed_probe_response: None,
            },
        );

        assert_eq!(candidate.support, DiscoveryCandidateSupport::Conflicting);
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Conflicting identity evidence".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_does_not_confirm_from_code_substring_only() {
        let candidate = mobile_discovery_candidate_from_begode_identity_probe(
            "ios-local-falcon".to_owned(),
            "GotWay_002441".to_owned(),
            MobileBegodeIdentityProbeDto {
                reported_model: None,
                reported_code_name: Some("GW-FALCON".to_owned()),
                reported_imu: None,
                reported_firmware_version: None,
                reported_serial: None,
                nominal_voltage_hint_mv: None,
                missing_probe_response: None,
                malformed_probe_response: None,
            },
        );

        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Unresolved Begode code banner".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_keeps_missing_begode_probe_recordable() {
        let candidate = mobile_discovery_candidate_from_begode_identity_probe(
            "ios-local-falcon".to_owned(),
            "GotWay_002441".to_owned(),
            MobileBegodeIdentityProbeDto {
                reported_model: None,
                reported_code_name: None,
                reported_imu: None,
                reported_firmware_version: None,
                reported_serial: None,
                nominal_voltage_hint_mv: None,
                missing_probe_response: Some(MobilePendingProbeDto::BegodeName),
                malformed_probe_response: None,
            },
        );

        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Missing Begode probe response".to_owned())
        );
        assert_eq!(
            candidate.evidence,
            "missing_probe_response=BegodeName".to_owned()
        );
    }

    #[test]
    fn mobile_discovery_candidate_keeps_malformed_begode_probe_recordable() {
        let candidate = mobile_discovery_candidate_from_begode_identity_probe(
            "ios-local-falcon".to_owned(),
            "GotWay_002441".to_owned(),
            MobileBegodeIdentityProbeDto {
                reported_model: None,
                reported_code_name: None,
                reported_imu: None,
                reported_firmware_version: None,
                reported_serial: None,
                nominal_voltage_hint_mv: None,
                missing_probe_response: None,
                malformed_probe_response: Some(MobilePendingProbeDto::BegodeName),
            },
        );

        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Malformed Begode probe response".to_owned())
        );
        assert_eq!(
            candidate.evidence,
            "malformed_probe_response=BegodeName".to_owned()
        );
    }

    #[test]
    fn mobile_discovery_candidate_keeps_family_only_begode_probe_recordable() {
        let candidate = mobile_discovery_candidate_from_begode_identity_probe(
            "ios-local-begode".to_owned(),
            "GotWay_002441".to_owned(),
            MobileBegodeIdentityProbeDto {
                reported_model: None,
                reported_code_name: None,
                reported_imu: None,
                reported_firmware_version: None,
                reported_serial: None,
                nominal_voltage_hint_mv: None,
                missing_probe_response: None,
                malformed_probe_response: None,
            },
        );

        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Begode model not confirmed".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_recommends_protocol_probe_for_generic_ffe0() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-begode".to_owned(),
            Some("GotWay_002441".to_owned()),
            vec![discovery_service(CoreBluetoothServiceUuid::EUC_SERIAL_FFE0)],
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::ProbeRecommended
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Read-only protocol probe recommended".to_owned())
        );
        assert_eq!(candidate.detail, "Read-only protocol probe recommended");
    }

    #[test]
    fn mobile_discovery_candidate_ignores_nf_name_until_protocol_identity() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-aero".to_owned(),
            Some("NF2557".to_owned()),
            vec![discovery_service(CoreBluetoothServiceUuid::EUC_SERIAL_FFE0)],
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::ProbeRecommended
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Read-only protocol probe recommended".to_owned())
        );
        assert_eq!(candidate.detail, "Read-only protocol probe recommended");
    }

    #[test]
    fn mobile_device_detection_session_preserves_raw_advertisement_bytes() {
        let session = CutoutSessionStateHandle::new();

        let resolution = session.observe_advertisement(Some(vec![b'N', b'F', 0xff]));

        assert_eq!(resolution.protocol_family, None);
        assert_eq!(resolution.advertised_name, Some(vec![b'N', b'F', 0xff]));
        assert_eq!(resolution.model_banner, None);
    }

    #[test]
    fn mobile_cutout_session_state_retains_discovery_observations() {
        let session = CutoutSessionStateHandle::new();

        let snapshot = session.observe_discovery(DiscoveryObservation {
            platform_identifier: "ios-local-falcon".to_owned(),
            advertised_name: Some(b"Begode Falcon".to_vec()),
            advertised_service_uuids: vec![discovery_service(
                CoreBluetoothServiceUuid::EUC_SERIAL_FFE0,
            )],
            manufacturer_data: vec![DiscoveryManufacturerDataSummary {
                company_identifier: 0x004c,
                payload_len: 6,
            }],
            rssi_dbm: Some(-48),
        });

        assert_eq!(snapshot.observations.len(), 1);
        assert_eq!(
            snapshot.observations[0].advertised_name_text,
            Some("Begode Falcon".to_owned())
        );
        assert_eq!(snapshot.picker_candidates.len(), 1);
        assert_eq!(snapshot.picker_candidates[0].electric_unicycle_model, None);

        let selected = session.select_discovered_platform("ios-local-falcon".to_owned());
        assert_eq!(
            selected.selected_platform_identifier,
            Some("ios-local-falcon".to_owned())
        );
    }

    #[test]
    fn mobile_device_detection_session_preserves_begode_model_banner_bytes() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_begode_name_probe();

        let resolution = session.observe_notification(b"NAME=Falcon".to_vec());

        assert_eq!(resolution.model_banner, Some(b"Falcon".to_vec()));
    }

    #[test]
    fn mobile_identification_probe_returns_bounded_typed_writes_and_tracks_them() {
        let session = CutoutSessionStateHandle::new();

        let outcome = session.begin_identification_probe_at(1_000);

        assert_eq!(
            outcome,
            MobileIdentificationProbeOutcomeDto::Writes {
                writes: vec![
                    MobileIdentificationProbeWriteDto::begode(b"N"),
                    MobileIdentificationProbeWriteDto::begode(b"V"),
                    MobileIdentificationProbeWriteDto::begode(b"M"),
                ],
            }
        );
        assert_eq!(session.next_begode_probe_expiry(2_000), Some(3_001));
    }

    #[test]
    fn mobile_identification_probe_rejects_duplicate_without_resetting_deadline() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.begin_identification_probe_at(1_000);

        let duplicate = session.begin_identification_probe_at(1_500);

        assert_eq!(
            duplicate,
            MobileIdentificationProbeOutcomeDto::AlreadyPending
        );
        assert_eq!(session.next_begode_probe_expiry(2_000), Some(3_001));
    }

    #[test]
    fn mobile_identification_probe_runs_for_selected_ffe0_candidate() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_discovery(DiscoveryObservation {
            platform_identifier: "ios-local-aero".to_owned(),
            advertised_name: Some(b"NF2557".to_vec()),
            advertised_service_uuids: vec![discovery_service(
                CoreBluetoothServiceUuid::EUC_SERIAL_FFE0,
            )],
            manufacturer_data: vec![],
            rssi_dbm: Some(-48),
        });
        let _ = session.select_discovered_platform("ios-local-aero".to_owned());

        assert!(matches!(
            session.begin_identification_probe_at(1_000),
            MobileIdentificationProbeOutcomeDto::Writes { .. }
        ));
    }

    #[test]
    fn mobile_identification_probe_returns_writes_for_selected_probe_candidate() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_discovery(DiscoveryObservation {
            platform_identifier: "ios-local-unknown-euc".to_owned(),
            advertised_name: Some(b"Unknown EUC".to_vec()),
            advertised_service_uuids: vec![discovery_service(
                CoreBluetoothServiceUuid::EUC_SERIAL_FFE0,
            )],
            manufacturer_data: vec![],
            rssi_dbm: Some(-48),
        });
        let _ = session.select_discovered_platform("ios-local-unknown-euc".to_owned());

        assert!(matches!(
            session.begin_identification_probe_at(1_000),
            MobileIdentificationProbeOutcomeDto::Writes { .. }
        ));
    }

    #[test]
    fn mobile_identification_probe_reports_unsupported_for_selected_vesc() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_discovery(DiscoveryObservation {
            platform_identifier: "ios-local-vesc".to_owned(),
            advertised_name: Some(b"Little FOCer".to_vec()),
            advertised_service_uuids: vec![discovery_service(
                CoreBluetoothServiceUuid::VESC_NORDIC_UART,
            )],
            manufacturer_data: vec![],
            rssi_dbm: Some(-48),
        });
        let _ = session.select_discovered_platform("ios-local-vesc".to_owned());

        assert_eq!(
            session.begin_identification_probe_at(1_000),
            MobileIdentificationProbeOutcomeDto::Unsupported
        );
    }

    #[test]
    fn mobile_device_detection_session_exposes_fragmented_begode_family_frame() {
        let session = CutoutSessionStateHandle::new();
        let frame = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");

        let partial = session.observe_notification(frame[..20].to_vec());
        let resolution = session.observe_notification(frame[20..].to_vec());

        assert_eq!(partial.protocol_family, None);
        assert_eq!(
            resolution.protocol_family,
            Some(MobileProtocolFamilyDto::BegodeGotway)
        );
    }

    #[test]
    fn mobile_device_detection_resolution_projects_veteran_model_id() {
        let session = CutoutSessionStateHandle::new();
        let veteran_frame = synthetic_veteran_frame_with_model_id(43);

        let resolution = session.observe_notification(veteran_frame.to_vec());
        let candidate = mobile_discovery_candidate_from_detection_resolution(
            "ios-local-aero".to_owned(),
            "NF2557".to_owned(),
            resolution,
        );

        assert_eq!(candidate.evidence, "Veteran protocol model id");
        assert_eq!(candidate.detail, "NOSFET Aero confirmed by model id 43");
        assert_eq!(candidate.support, DiscoveryCandidateSupport::Supported);
        assert_eq!(
            candidate.connection_route,
            Some(DiscoveryConnectionRoute::ElectricUnicycle)
        );
        assert_eq!(
            candidate.electric_unicycle_model,
            Some(DiscoveryElectricUnicycleModel::Aero)
        );
    }

    #[test]
    fn mobile_device_detection_resolution_keeps_veteran_family_only_recordable() {
        let candidate = mobile_discovery_candidate_from_detection_resolution(
            "ios-local-veteran-family".to_owned(),
            "Veteran stream".to_owned(),
            DeviceDetectionResolutionRecord {
                protocol_family: Some(MobileProtocolFamilyDto::VeteranLeaperkimNosfet),
                protocol_conflict: false,
                veteran_protocol_model_id: None,
                advertised_name: None,
                model_banner: None,
                firmware_banner: None,
                imu_banner: None,
                missing_probe_response: None,
                malformed_probe_response: None,
            },
        );

        assert_eq!(candidate.product_category, "Electric unicycle");
        assert_eq!(candidate.evidence, "Veteran/NOSFET protocol family");
        assert_eq!(candidate.detail, "Veteran/NOSFET model not confirmed");
        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Veteran/NOSFET model not confirmed".to_owned())
        );
    }

    #[test]
    fn mobile_device_detection_resolution_routes_vesc_family_provisionally() {
        let candidate = mobile_discovery_candidate_from_detection_resolution(
            "ios-local-vesc-family".to_owned(),
            "VESC stream".to_owned(),
            DeviceDetectionResolutionRecord {
                protocol_family: Some(MobileProtocolFamilyDto::Vesc),
                protocol_conflict: false,
                veteran_protocol_model_id: None,
                advertised_name: None,
                model_banner: None,
                firmware_banner: None,
                imu_banner: None,
                missing_probe_response: None,
                malformed_probe_response: None,
            },
        );

        assert_eq!(candidate.product_category, "VESC Onewheel");
        assert_eq!(candidate.evidence, "VESC protocol family");
        assert_eq!(candidate.detail, "VESC read-only route");
        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::ProvisionalRoute
        );
        assert_eq!(
            candidate.connection_route,
            Some(DiscoveryConnectionRoute::VescOnewheel)
        );
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(candidate.disabled_reason, None);
    }

    #[test]
    fn mobile_device_detection_session_projects_mixed_family_conflict() {
        let session = CutoutSessionStateHandle::new();
        let veteran_frame = synthetic_veteran_frame_with_model_id(43);
        let begode_frame = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
        let _ = session.observe_notification(veteran_frame.to_vec());

        let resolution = session.observe_notification(begode_frame.to_vec());
        let candidate = mobile_discovery_candidate_from_begode_detection_resolution(
            "ios-local-conflict".to_owned(),
            "Conflicting wheel".to_owned(),
            resolution,
        );

        assert_eq!(candidate.support, DiscoveryCandidateSupport::Conflicting);
        assert_eq!(
            candidate.recommended_action,
            DiscoveryCandidateAction::Review
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Conflicting identity evidence".to_owned())
        );
    }

    #[test]
    fn mobile_device_detection_session_preserves_begode_firmware_banner_bytes() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_begode_firmware_probe();

        let resolution = session.observe_notification(b"GW FALCON 1.0".to_vec());

        assert_eq!(resolution.firmware_banner, Some(b"GW FALCON 1.0".to_vec()));
    }

    #[test]
    fn mobile_device_detection_session_preserves_begode_imu_banner_bytes() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_begode_imu_probe();

        let resolution = session.observe_notification(b"MPU6500".to_vec());

        assert_eq!(resolution.imu_banner, Some(b"MPU6500".to_vec()));
    }

    #[test]
    fn mobile_device_detection_session_exposes_missing_begode_probe_response() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_begode_name_probe();

        let resolution = session.observe_begode_name_probe_timeout();

        assert_eq!(
            resolution.missing_probe_response,
            Some(MobilePendingProbeDto::BegodeName)
        );
        assert_eq!(resolution.model_banner, None);
    }

    #[test]
    fn mobile_probe_correlation_expires_only_unanswered_probes() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_begode_name_probe_at(1_000);
        let _ = session.observe_begode_firmware_probe_at(1_001);
        let _ = session.observe_begode_imu_probe_at(1_002);
        let _ = session.observe_notification(b"NAME=Falcon".to_vec());

        let expired = session.expire_begode_probe_responses(3_003, 2_000);

        assert_eq!(
            expired,
            vec![
                MobilePendingProbeDto::BegodeFirmware,
                MobilePendingProbeDto::BegodeImu
            ]
        );
        assert_eq!(session.next_begode_probe_expiry(2_000), None);
    }

    #[test]
    fn mobile_device_detection_session_exposes_malformed_begode_probe_response() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_begode_name_probe();

        let resolution = session.observe_notification(b"NAME=Falcon\0".to_vec());

        assert_eq!(
            resolution.malformed_probe_response,
            Some(MobilePendingProbeDto::BegodeName)
        );
        assert_eq!(resolution.missing_probe_response, None);
        assert_eq!(resolution.model_banner, Some(b"Falcon\0".to_vec()));
    }

    #[test]
    fn mobile_device_detection_session_preserves_malformed_banner_until_valid_probe_response() {
        let session = CutoutSessionStateHandle::new();
        let gatt = vec![MobileGattFingerprintDto {
            service: BEGODE_SERVICE_CHANNEL.as_bytes().to_vec(),
            characteristic: BEGODE_DATA_CHANNEL.as_bytes().to_vec(),
            roles: vec![
                MobileGattRoleDto::WriteWithoutResponse,
                MobileGattRoleDto::Notify,
            ],
            verification: MobileVerificationStatusDto::HardwareVerified,
        }];
        let _ = session.observe_gatt(gatt.clone());
        let _ = session.observe_begode_name_probe();

        let malformed = session.observe_notification(b"NAME=Falcon\0".to_vec());
        let refreshed = session.observe_gatt(gatt);
        let _ = session.observe_begode_name_probe();
        let valid = session.observe_notification(b"NAME=Falcon".to_vec());

        assert_eq!(malformed.model_banner, Some(b"Falcon\0".to_vec()));
        assert_eq!(refreshed.model_banner, Some(b"Falcon\0".to_vec()));
        assert_eq!(valid.model_banner, Some(b"Falcon".to_vec()));
    }

    #[test]
    fn mobile_discovery_candidate_projects_begode_detection_resolution() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_begode_name_probe();
        let resolution = session.observe_begode_name_probe_timeout();

        let candidate = mobile_discovery_candidate_from_begode_detection_resolution(
            "ios-local-falcon".to_owned(),
            "GotWay_002441".to_owned(),
            resolution,
        );

        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(
            candidate.recommended_action,
            DiscoveryCandidateAction::Record
        );
        assert_eq!(candidate.section, DiscoveryCandidateSection::RecordOnly);
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Missing Begode probe response".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_rejects_empty_detection_resolution() {
        let session = CutoutSessionStateHandle::new();
        let resolution = session.resolution();

        let candidate = mobile_discovery_candidate_from_detection_resolution(
            "ios-local-empty".to_owned(),
            "Unknown peripheral".to_owned(),
            resolution,
        );

        assert!(!candidate.is_picker_candidate);
        assert_eq!(candidate.support, DiscoveryCandidateSupport::RejectedNoise);
        assert_eq!(
            candidate.recommended_action,
            DiscoveryCandidateAction::Record
        );
        assert_eq!(candidate.section, DiscoveryCandidateSection::RecordOnly);
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
    }

    #[test]
    fn mobile_discovery_candidate_projects_malformed_begode_detection_resolution() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_begode_name_probe();
        let resolution = session.observe_notification(b"NAME=Falcon\0".to_vec());

        let candidate = mobile_discovery_candidate_from_begode_detection_resolution(
            "ios-local-falcon-malformed".to_owned(),
            "GotWay_002441".to_owned(),
            resolution,
        );

        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(
            candidate.recommended_action,
            DiscoveryCandidateAction::Record
        );
        assert_eq!(candidate.section, DiscoveryCandidateSection::RecordOnly);
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Malformed Begode probe response".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_projects_missing_probe_over_stale_model_banner() {
        let session = CutoutSessionStateHandle::new();
        let _ = session.observe_begode_name_probe();
        let _ = session.observe_notification(b"NAME=Falcon".to_vec());
        let _ = session.observe_begode_name_probe();
        let resolution = session.observe_begode_name_probe_timeout();

        let candidate = mobile_discovery_candidate_from_begode_detection_resolution(
            "ios-local-falcon-missing".to_owned(),
            "GotWay_002441".to_owned(),
            resolution,
        );

        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(
            candidate.recommended_action,
            DiscoveryCandidateAction::Record
        );
        assert_eq!(candidate.section, DiscoveryCandidateSection::RecordOnly);
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Missing Begode probe response".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_routes_veteran_protocol_model_id_to_aero() {
        let candidate = mobile_discovery_candidate_from_veteran_protocol_identity(
            "ios-local-aero".to_owned(),
            "NF2557".to_owned(),
            43,
        );

        assert_eq!(candidate.platform_identifier, "ios-local-aero");
        assert_eq!(candidate.display_name, "NF2557");
        assert_eq!(candidate.product_category, "Electric unicycle");
        assert_eq!(candidate.evidence, "Veteran protocol model id");
        assert_eq!(candidate.detail, "NOSFET Aero confirmed by model id 43");
        assert!(candidate.is_picker_candidate);
        assert_eq!(candidate.support, DiscoveryCandidateSupport::Supported);
        assert_eq!(
            candidate.connection_route,
            Some(DiscoveryConnectionRoute::ElectricUnicycle)
        );
        assert_eq!(
            candidate.electric_unicycle_model,
            Some(DiscoveryElectricUnicycleModel::Aero)
        );
        assert_eq!(candidate.disabled_reason, None);
    }

    #[test]
    fn mobile_discovery_candidate_keeps_unknown_veteran_model_id_unrouteable() {
        let candidate = mobile_discovery_candidate_from_veteran_protocol_identity(
            "ios-local-veteran".to_owned(),
            "Veteran stream".to_owned(),
            99,
        );

        assert_eq!(candidate.product_category, "Electric unicycle");
        assert_eq!(candidate.evidence, "Veteran protocol model id");
        assert_eq!(candidate.detail, "Unknown Veteran/NOSFET model id 99");
        assert!(candidate.is_picker_candidate);
        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::UnknownRecordable
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason.as_deref(),
            Some("Unknown Veteran/NOSFET model id 99")
        );
    }

    fn bms_snapshot_fixture() -> MobileBmsSnapshotDto {
        MobileBmsSnapshotDto {
            availability: MobileReadbackAvailabilityDto::Available,
            topology: MobileBmsTopologyDto {
                layout_label: "20S4P split pack".to_owned(),
                series_group_count: Some(20),
                parallel_count: Some(4),
                pack_count: 2,
                bms_count: 2,
                confidence: MobileBmsTopologyConfidenceDto::Verified,
            },
            page_selector: None,
            page_tag: None,
            page_kind: None,
            page_verification: None,
            energy_percent: Some(BatteryLevelReading {
                value: BatteryLevel { value: 72 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            voltage: Some(VoltageReading {
                value: Voltage { value: 81_600 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            current: None,
            bms_pack_current_0: None,
            bms_pack_current_1: None,
            cell_delta: Some(VoltageDeltaReading {
                value: VoltageDelta { value: 18 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            lowest_group_index: Some(17),
            highest_temperature: Some(TemperatureReading {
                value: Temperature { value: 37_800 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            temperatures: Vec::new(),
            highest_temperature_label: Some("right pack".to_owned()),
            balancing_summary: Some("idle • top groups only".to_owned()),
            balancing_detail: Some("3 groups bleeding: 03, 11, 19".to_owned()),
            fault_summary: Some("no active faults".to_owned()),
            fault_detail: Some("last: under-voltage warning · 3 days ago".to_owned()),
            groups: vec![MobileBmsGroupSnapshotDto {
                index: 17,
                label: Some("group 17".to_owned()),
                voltage: Some(VoltageReading {
                    value: Voltage { value: 4_071 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                temperature: Some(TemperatureReading {
                    value: Temperature { value: 34_900 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                resistance: Some(Resistance { value: 21 }),
                is_balancing: Some(true),
                alert_level: MobileBmsAlertLevelDto::Warning,
                detail: Some("drops first during acceleration".to_owned()),
            }],
            faults: vec![MobileBmsFaultDto {
                code: "0x0040".to_owned(),
                label: "needs decoder".to_owned(),
                alert_level: MobileBmsAlertLevelDto::Critical,
            }],
            capture_action_title: Some("record unsupported pack".to_owned()),
            capture_action_state: Some("disabled for launch".to_owned()),
        }
    }

    #[test]
    fn mobile_bms_snapshot_dto_preserves_topology_and_group_detail() {
        let snapshot = bms_snapshot_fixture();

        assert_eq!(snapshot.topology.series_group_count, Some(20));
        assert_eq!(snapshot.topology.pack_count, 2);
        assert_eq!(snapshot.topology.bms_count, 2);
        assert_eq!(
            snapshot.topology.confidence,
            MobileBmsTopologyConfidenceDto::Verified
        );
        assert_eq!(snapshot.lowest_group_index, Some(17));
        assert_eq!(
            snapshot
                .groups
                .first()
                .and_then(|group| group.label.as_deref()),
            Some("group 17")
        );
        assert_eq!(
            snapshot.groups.first().and_then(|group| group.is_balancing),
            Some(true)
        );
        assert_eq!(
            snapshot
                .groups
                .first()
                .and_then(|group| group.resistance)
                .map(|resistance| resistance.value),
            Some(21)
        );
        assert_eq!(
            snapshot.faults.first().map(|fault| fault.code.as_str()),
            Some("0x0040")
        );
        assert_eq!(
            snapshot.capture_action_title.as_deref(),
            Some("record unsupported pack")
        );
    }

    #[test]
    fn mobile_settings_readback_preserves_present_fields_and_metadata() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(0x0102, -17),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::HardwareVerified,
            }),
            None,
            Some(SettingsEntry {
                field: RawFieldValue::new(0x0203, 42),
                source: ValueSource::Estimated,
                quality: ValueQuality::Inferred,
                verification: VerificationStatus::Inferred,
            }),
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.availability,
            MobileReadbackAvailabilityDto::Available
        );
        assert_eq!(
            mobile.entries,
            vec![
                MobileSettingsEntryDto {
                    field: MobileRawFieldValueDto {
                        id: 0x0102,
                        value: -17,
                    },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                },
                MobileSettingsEntryDto {
                    field: MobileRawFieldValueDto {
                        id: 0x0203,
                        value: 42,
                    },
                    source: MobileValueSourceDto::Estimated,
                    quality: MobileValueQualityDto::Inferred,
                    verification: MobileVerificationStatusDto::Inferred,
                },
            ]
        );
        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Available,
                beep_margin: None,
                tiltback: None,
                pedal_mode: None,
                roll_angle: None,
                speed_alarm_mode: None,
                light_state: None,
                auto_shutdown_seconds: None,
                charge_mode: None,
            }
        );
    }

    #[test]
    fn mobile_vesc_debug_snapshot_preserves_read_only_guardrail_state() {
        let snapshot = MobileVescDebugSnapshotDto {
            profile_title: "Profile: Street stable".to_owned(),
            transport_detail: "VESC Express · FW 6.x · UART bridge".to_owned(),
            duty_cycle: Some(DutyCycle { permille: 820 }),
            max_seen_duty_cycle: Some(DutyCycle { permille: 870 }),
            pack_voltage: Some(reported_voltage(75_400)),
            battery_current_limit: Some(reported_battery_current(45_000)),
            motor_current_limit: Some(reported_phase_current(90_000)),
            last_fault: Some("FAULT_CODE_NONE".to_owned()),
            input_app: Some("ADC + balance".to_owned()),
            can_status: Some("single controller".to_owned()),
            logging: Some("local CSV armed".to_owned()),
            write_guardrail: MobileVescWriteGuardrailDto::PolicyRefusal,
        };

        assert_eq!(snapshot.duty_cycle.map(|duty| duty.permille), Some(820));
        assert_eq!(
            snapshot.max_seen_duty_cycle.map(|duty| duty.permille),
            Some(870)
        );
        assert_eq!(
            snapshot.pack_voltage.map(|reading| reading.value.value),
            Some(75_400)
        );
        assert_eq!(
            snapshot
                .battery_current_limit
                .map(|reading| reading.value.value),
            Some(45_000)
        );
        assert_eq!(
            snapshot
                .motor_current_limit
                .map(|reading| reading.value.value),
            Some(90_000)
        );
        assert_eq!(
            snapshot.write_guardrail,
            MobileVescWriteGuardrailDto::PolicyRefusal
        );
    }

    #[test]
    fn mobile_settings_readback_projects_known_euc_garage_settings() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_SPEED_ALERT_DECI_KMH, 116),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceAndHardwareVerified,
            }),
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, 420),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::HardwareVerified,
            }),
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_PEDALS_MODE, 1_920),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Available,
                beep_margin: Some(SpeedReading {
                    value: Speed { value: 3_222 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::SourceAndHardwareVerified,
                }),
                tiltback: Some(SpeedReading {
                    value: Speed { value: 11_666 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                pedal_mode: Some(MobilePedalModeDto {
                    raw_mode: Some(1_920),
                    mode: None,
                }),
                roll_angle: None,
                speed_alarm_mode: None,
                light_state: None,
                auto_shutdown_seconds: None,
                charge_mode: None,
            }
        );
    }

    #[test]
    fn mobile_settings_readback_projects_veteran_auto_shutdown_and_charge_mode() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS, 900),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::HardwareVerified,
            }),
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_CHARGE_MODE, 1),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::HardwareVerified,
            }),
            None,
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(mobile.euc_garage.auto_shutdown_seconds, Some(900));
        assert_eq!(
            mobile.euc_garage.charge_mode.map(|reading| reading.value),
            Some(MobileChargeModeDto::Charging)
        );
    }

    #[test]
    fn mobile_settings_readback_decodes_documented_veteran_pedal_mode() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_PEDALS_MODE, 1),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
            None,
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage.pedal_mode,
            Some(MobilePedalModeDto {
                raw_mode: Some(1),
                mode: Some(MobilePedalModeKindDto::Medium),
            })
        );
    }

    #[test]
    fn mobile_settings_readback_projects_begode_tiltback_fallback() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(BEGODE_FIELD_TILTBACK_SPEED_KMH, 50),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
            None,
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Available,
                beep_margin: None,
                tiltback: Some(SpeedReading {
                    value: Speed { value: 13_888 },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::SourceVerified,
                }),
                pedal_mode: None,
                roll_angle: None,
                speed_alarm_mode: None,
                light_state: None,
                auto_shutdown_seconds: None,
                charge_mode: None,
            }
        );
    }

    #[test]
    fn mobile_settings_readback_decodes_documented_begode_pedal_mode() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(BEGODE_FIELD_SETTINGS_BITS, 0x4000),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
            None,
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage.pedal_mode,
            Some(MobilePedalModeDto {
                raw_mode: Some(2),
                mode: Some(MobilePedalModeKindDto::Hard),
            })
        );
    }

    #[test]
    fn mobile_settings_readback_decodes_documented_begode_roll_angle() {
        let settings_bits = 2_i64 << 7;
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(BEGODE_FIELD_SETTINGS_BITS, settings_bits),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
            None,
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage.roll_angle,
            Some(MobileRollAngleDto {
                raw_angle: Some(2),
                angle: Some(MobileRollAngleKindDto::High),
            })
        );
    }

    #[test]
    fn mobile_settings_readback_decodes_documented_begode_speed_alarm_mode() {
        let settings_bits = 1_i64 << 10;
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(BEGODE_FIELD_SETTINGS_BITS, settings_bits),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
            None,
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage.speed_alarm_mode,
            Some(MobileSpeedAlarmModeDto {
                raw_mode: Some(1),
                mode: Some(MobileSpeedAlarmModeKindDto::StageOneOnly),
            })
        );
    }

    #[test]
    fn mobile_settings_readback_projects_begode_light_state() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(BEGODE_FIELD_LED_AND_LIGHT_MODE, 0x0301),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
            None,
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(mobile.euc_garage.light_state, Some(MobileLightStateDto::On));

        let strobe = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(BEGODE_FIELD_LED_AND_LIGHT_MODE, 0x0302),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
            None,
            None,
        ]);

        assert_eq!(
            MobileSettingsReadbackDto::from(strobe)
                .euc_garage
                .light_state,
            None
        );
    }

    #[test]
    fn mobile_settings_readback_rejects_unrepresentable_euc_garage_speeds() {
        let readback = SettingsReadback::available([
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_SPEED_ALERT_DECI_KMH, -1),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceAndHardwareVerified,
            }),
            Some(SettingsEntry {
                field: RawFieldValue::new(VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, i64::MAX),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::HardwareVerified,
            }),
            Some(SettingsEntry {
                field: RawFieldValue::new(BEGODE_FIELD_TILTBACK_SPEED_KMH, i64::MAX),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            }),
            None,
        ]);

        let mobile = MobileSettingsReadbackDto::from(readback);

        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Available,
                beep_margin: None,
                tiltback: None,
                pedal_mode: None,
                roll_angle: None,
                speed_alarm_mode: None,
                light_state: None,
                auto_shutdown_seconds: None,
                charge_mode: None,
            }
        );
        assert_eq!(mobile.entries.len(), 3);
    }

    #[test]
    fn mobile_settings_readback_preserves_unsupported_availability() {
        let mobile = MobileSettingsReadbackDto::from(SettingsReadback::unsupported());

        assert_eq!(
            mobile,
            MobileSettingsReadbackDto {
                availability: MobileReadbackAvailabilityDto::Unsupported,
                euc_garage: MobileEucGarageSettingsDto {
                    availability: MobileReadbackAvailabilityDto::Unsupported,
                    beep_margin: None,
                    tiltback: None,
                    pedal_mode: None,
                    roll_angle: None,
                    speed_alarm_mode: None,
                    light_state: None,
                    auto_shutdown_seconds: None,
                    charge_mode: None,
                },
                entries: Vec::new(),
            }
        );
    }

    #[test]
    fn mobile_settings_readback_dto_strips_entries_when_not_available() {
        let mobile = MobileSettingsReadbackDto::from(SettingsReadbackDto {
            availability: SettingsReadbackAvailabilityDto::Unsupported,
            entries: vec![SettingsEntryDto {
                field: RawFieldValueDto {
                    id: VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
                    value: 420,
                },
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }],
        });

        assert_eq!(
            mobile.availability,
            MobileReadbackAvailabilityDto::Unsupported
        );
        assert!(mobile.entries.is_empty());
        assert_eq!(
            mobile.euc_garage,
            MobileEucGarageSettingsDto {
                availability: MobileReadbackAvailabilityDto::Unsupported,
                beep_margin: None,
                tiltback: None,
                pedal_mode: None,
                roll_angle: None,
                speed_alarm_mode: None,
                light_state: None,
                auto_shutdown_seconds: None,
                charge_mode: None,
            }
        );
    }

    #[test]
    fn mobile_fault_history_preserves_unknown_fault_code_and_since_distance() {
        let mobile = MobileFaultHistoryReadbackDto::from(FaultHistoryReadbackDto {
            availability: FaultHistoryAvailabilityDto::Available,
            last_fault: Some(FaultHistoryEntryDto {
                code: FaultCodeDto {
                    raw: RawFieldValueDto {
                        id: 0x0040,
                        value: 1,
                    },
                },
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }),
            since_distance: Some(DistanceReadingDto {
                value: 61_456_941,
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }),
        });

        assert_eq!(
            mobile.availability,
            MobileReadbackAvailabilityDto::Available
        );
        assert_eq!(
            mobile.last_fault.expect("fault"),
            MobileFaultHistoryEntryDto {
                code: MobileFaultCodeDto {
                    raw: MobileRawFieldValueDto {
                        id: 0x0040,
                        value: 1,
                    },
                },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            }
        );
        assert_eq!(
            mobile.since_distance.expect("distance").value,
            Distance { value: 61_456_941 }
        );
    }

    #[test]
    fn mobile_fault_history_dto_strips_payload_when_not_available() {
        let mobile = MobileFaultHistoryReadbackDto::from(FaultHistoryReadbackDto {
            availability: FaultHistoryAvailabilityDto::Unavailable,
            last_fault: Some(FaultHistoryEntryDto {
                code: FaultCodeDto {
                    raw: RawFieldValueDto {
                        id: 0x0040,
                        value: 1,
                    },
                },
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }),
            since_distance: Some(DistanceReadingDto {
                value: 61_456_941,
                source: ValueSourceDto::Reported,
                quality: ValueQualityDto::Known,
                verification: VerificationStatusDto::HardwareVerified,
            }),
        });

        assert_eq!(
            mobile.availability,
            MobileReadbackAvailabilityDto::Unavailable
        );
        assert_eq!(mobile.last_fault, None);
        assert_eq!(mobile.since_distance, None);
    }

    #[test]
    fn generic_firmware_output_does_not_invent_veteran_protocol_model_id() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestFirmwareInfo,
            payload: ReadOnlyOutputPayload::Firmware(cutout_core::FirmwareInfoDto {
                protocol_version: None,
                firmware_major: Some(cutout_core::VersionComponentDto {
                    value: 43,
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                firmware_minor: None,
                firmware_patch: None,
                build_id: None,
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::Event);
        assert_eq!(mobile.veteran_protocol_model_id, None);
    }

    #[test]
    fn aero_firmware_output_carries_veteran_protocol_model_id() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestFirmwareInfo,
            payload: ReadOnlyOutputPayload::Firmware(cutout_core::FirmwareInfoDto {
                protocol_version: None,
                firmware_major: Some(cutout_core::VersionComponentDto {
                    value: 43,
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                firmware_minor: None,
                firmware_patch: None,
                build_id: None,
            }),
        });

        let mobile = mobile_aero_session_output(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::Event);
        assert_eq!(mobile.veteran_protocol_model_id, Some(43));
    }

    #[test]
    fn mobile_session_output_preserves_settings_readback_event() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestSettings,
            payload: ReadOnlyOutputPayload::Settings(SettingsReadbackDto {
                availability: SettingsReadbackAvailabilityDto::Available,
                entries: vec![SettingsEntryDto {
                    field: RawFieldValueDto {
                        id: 0x0102,
                        value: -17,
                    },
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }],
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::SettingsReadback);
        assert!(mobile.channel.is_empty());
        assert!(mobile.bytes.is_empty());
        assert_eq!(mobile.ingest, None);
        assert_eq!(
            mobile.settings_readback,
            Some(MobileSettingsReadbackDto {
                availability: MobileReadbackAvailabilityDto::Available,
                euc_garage: MobileEucGarageSettingsDto {
                    availability: MobileReadbackAvailabilityDto::Available,
                    beep_margin: None,
                    tiltback: None,
                    pedal_mode: None,
                    roll_angle: None,
                    speed_alarm_mode: None,
                    light_state: None,
                    auto_shutdown_seconds: None,
                    charge_mode: None,
                },
                entries: vec![MobileSettingsEntryDto {
                    field: MobileRawFieldValueDto {
                        id: 0x0102,
                        value: -17,
                    },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }],
            })
        );
        assert_eq!(mobile.fault_history_readback, None);
        assert_eq!(mobile.bms_snapshot, None);
    }

    #[test]
    fn mobile_session_output_preserves_fault_history_readback_event() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestFaultHistory,
            payload: ReadOnlyOutputPayload::FaultHistory(FaultHistoryReadbackDto {
                availability: FaultHistoryAvailabilityDto::Available,
                last_fault: Some(FaultHistoryEntryDto {
                    code: FaultCodeDto {
                        raw: RawFieldValueDto {
                            id: 0x0040,
                            value: 1,
                        },
                    },
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                since_distance: None,
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(
            mobile.kind,
            MobileSessionOutputKindDto::FaultHistoryReadback
        );
        assert!(mobile.channel.is_empty());
        assert!(mobile.bytes.is_empty());
        assert_eq!(mobile.ingest, None);
        assert_eq!(mobile.settings_readback, None);
        assert_eq!(mobile.bms_snapshot, None);
        assert_eq!(
            mobile.fault_history_readback,
            Some(MobileFaultHistoryReadbackDto {
                availability: MobileReadbackAvailabilityDto::Available,
                last_fault: Some(MobileFaultHistoryEntryDto {
                    code: MobileFaultCodeDto {
                        raw: MobileRawFieldValueDto {
                            id: 0x0040,
                            value: 1,
                        },
                    },
                    source: MobileValueSourceDto::Reported,
                    quality: MobileValueQualityDto::Known,
                    verification: MobileVerificationStatusDto::HardwareVerified,
                }),
                since_distance: None,
            })
        );
    }

    #[test]
    fn mobile_command_mapping_keeps_readback_commands_distinct() {
        assert_eq!(
            mobile_command_from_command_kind(CommandKindDto::RequestDiagnostics),
            Some(MobileCommandDto::RequestDiagnostics)
        );
        assert_eq!(
            mobile_command_from_command_kind(CommandKindDto::RequestFaultHistory),
            Some(MobileCommandDto::RequestFaultHistory)
        );
        assert_eq!(
            mobile_command_from_command_kind(CommandKindDto::RequestSettings),
            Some(MobileCommandDto::RequestSettings)
        );
        assert_eq!(
            mobile_command_from_command_kind(CommandKindDto::SetRawMotorCurrent),
            None
        );
    }

    fn battery_readback_output_fixture() -> SessionOutputDto {
        let voltage = |value| VoltageReadingDto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::HardwareVerified,
        };
        let current = |value| BatteryCurrentReadingDto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::HardwareVerified,
        };
        let temperature = |value| TemperatureReadingDto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::HardwareVerified,
        };
        let level = |value| BatteryLevelReadingDto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::HardwareVerified,
        };
        SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestBatteryInfo,
            payload: ReadOnlyOutputPayload::Battery(BatteryReadbackDto {
                availability: BatteryReadbackAvailabilityDto::Available,
                page: Some(BatteryInfoDto {
                    page: cutout_core::BmsStatusPage {
                        id: cutout_core::BmsStatusPageId {
                            namespace: None,
                            selector: 3,
                        },
                        kind: BatteryPageKindDto::Temperature,
                        verification: VerificationStatusDto::HardwareVerified,
                    },
                    voltage: Some(voltage(81_600)),
                    current: Some(current(-1_250)),
                    bms_pack_current_0: Some(current(-1_100)),
                    bms_pack_current_1: Some(current(-150)),
                    level_reported: Some(level(72)),
                    level_estimated: None,
                    temperature: Some(temperature(31_000)),
                    temperatures: vec![None, Some(temperature(37_800)), Some(temperature(35_200))],
                    cell_voltages: vec![voltage(3_633), voltage(3_626), voltage(3_634)],
                    raw_state: None,
                }),
            }),
        })
    }

    #[test]
    fn mobile_session_output_maps_battery_readback_to_bms_snapshot() {
        let mobile = MobileSessionOutputDto::from(battery_readback_output_fixture());

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::BmsSnapshot);
        assert!(mobile.channel.is_empty());
        assert!(mobile.bytes.is_empty());
        assert_eq!(mobile.ingest, None);
        assert_eq!(mobile.settings_readback, None);
        assert_eq!(mobile.fault_history_readback, None);
        let snapshot = mobile.bms_snapshot.expect("BMS snapshot");
        assert_eq!(
            snapshot.availability,
            MobileReadbackAvailabilityDto::Available
        );
        assert_eq!(snapshot.topology.layout_label, "3 observed BMS groups");
        assert_eq!(snapshot.topology.series_group_count, None);
        assert_eq!(snapshot.topology.parallel_count, None);
        assert_eq!(snapshot.topology.pack_count, 1);
        assert_eq!(snapshot.topology.bms_count, 1);
        assert_eq!(
            snapshot.topology.confidence,
            MobileBmsTopologyConfidenceDto::Unverified
        );
        assert_eq!(snapshot.groups.len(), 3);
        assert_eq!(snapshot.groups[0].index, 1);
        assert_eq!(snapshot.groups[0].label.as_deref(), Some("group 1"));
        assert_eq!(
            snapshot.groups[0]
                .voltage
                .as_ref()
                .map(|voltage| voltage.value),
            Some(Voltage { value: 3_633 })
        );
        assert_eq!(snapshot.lowest_group_index, Some(2));
        assert_eq!(
            snapshot.cell_delta.expect("cell delta").value,
            VoltageDelta { value: 8 }
        );
        assert_eq!(
            snapshot.energy_percent.expect("reported level").value,
            BatteryLevel { value: 72 }
        );
        assert_eq!(
            snapshot.voltage.expect("pack voltage").value,
            Voltage { value: 81_600 }
        );
        assert_eq!(
            snapshot.current.expect("pack current").value,
            BatteryCurrent { value: -1_250 }
        );
        assert_eq!(snapshot.page_selector, Some(3));
        assert_eq!(snapshot.page_kind.as_deref(), Some("temperature"));
        assert_eq!(
            snapshot.page_verification,
            Some(MobileVerificationStatusDto::HardwareVerified)
        );
        assert_eq!(
            snapshot
                .bms_pack_current_0
                .expect("first BMS pack current")
                .value,
            BatteryCurrent { value: -1_100 }
        );
        assert_eq!(
            snapshot
                .bms_pack_current_1
                .expect("second BMS pack current")
                .value,
            BatteryCurrent { value: -150 }
        );
        assert_eq!(
            snapshot
                .highest_temperature
                .expect("highest temperature")
                .value,
            Temperature { value: 37_800 }
        );
        assert_eq!(
            snapshot
                .temperatures
                .iter()
                .map(|temperature| temperature.value)
                .collect::<Vec<_>>(),
            vec![Temperature { value: 37_800 }, Temperature { value: 35_200 }]
        );
    }

    #[test]
    fn mobile_session_output_preserves_unsupported_battery_readback() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestBatteryInfo,
            payload: ReadOnlyOutputPayload::Battery(BatteryReadbackDto {
                availability: BatteryReadbackAvailabilityDto::Unsupported,
                page: None,
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::BmsSnapshot);
        let snapshot = mobile.bms_snapshot.expect("BMS snapshot");
        assert_eq!(
            snapshot.availability,
            MobileReadbackAvailabilityDto::Unsupported
        );
        assert_eq!(snapshot.energy_percent, None);
        assert_eq!(snapshot.voltage, None);
        assert_eq!(snapshot.current, None);
        assert!(snapshot.groups.is_empty());
    }

    #[test]
    fn mobile_battery_readback_dto_strips_page_when_not_available() {
        let output = SessionOutputDto::ReadOnly(cutout_core::ReadOnlyOutput {
            command_kind: CommandKindDto::RequestBatteryInfo,
            payload: ReadOnlyOutputPayload::Battery(BatteryReadbackDto {
                availability: BatteryReadbackAvailabilityDto::Unsupported,
                page: Some(BatteryInfoDto {
                    page: cutout_core::BmsStatusPage {
                        id: cutout_core::BmsStatusPageId {
                            namespace: None,
                            selector: 3,
                        },
                        kind: BatteryPageKindDto::Temperature,
                        verification: VerificationStatusDto::HardwareVerified,
                    },
                    voltage: Some(voltage_reading(81_600)),
                    current: Some(battery_current_reading(-1_250)),
                    bms_pack_current_0: None,
                    bms_pack_current_1: None,
                    level_reported: Some(BatteryLevelReadingDto {
                        value: 72,
                        source: ValueSourceDto::Reported,
                        quality: ValueQualityDto::Known,
                        verification: VerificationStatusDto::HardwareVerified,
                    }),
                    level_estimated: None,
                    temperature: Some(temperature_reading(31_000)),
                    temperatures: vec![Some(temperature_reading(37_800))],
                    cell_voltages: vec![voltage_reading(3_633)],
                    raw_state: None,
                }),
            }),
        });

        let mobile = MobileSessionOutputDto::from(output);

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::BmsSnapshot);
        let snapshot = mobile.bms_snapshot.expect("BMS snapshot");
        assert_eq!(
            snapshot.availability,
            MobileReadbackAvailabilityDto::Unsupported
        );
        assert_eq!(snapshot.energy_percent, None);
        assert_eq!(snapshot.voltage, None);
        assert_eq!(snapshot.current, None);
        assert_eq!(snapshot.cell_delta, None);
        assert_eq!(snapshot.highest_temperature, None);
        assert!(snapshot.groups.is_empty());
    }

    #[test]
    fn bms_group_projection_skips_unrepresentable_group_indices() {
        let mut cell_voltages = vec![voltage_reading(3_600); usize::from(u8::MAX) + 1];
        cell_voltages[usize::from(u8::MAX)] = voltage_reading(3_500);
        let page_identity = BmsPageIdentity::from_tag_and_selector(None, 0);

        let groups = bms_groups_from_cell_voltages(&cell_voltages, page_identity);

        assert_eq!(groups.len(), usize::from(u8::MAX));
        assert_eq!(
            groups.last().map(|group| group.index),
            Some(BmsGroupIndex::MAX.as_mobile_dto())
        );
        assert_eq!(
            lowest_cell_voltage_group_index(&cell_voltages, page_identity),
            None
        );
    }

    #[test]
    fn begode_cell_pages_project_to_global_group_indices() {
        let cell_voltages = vec![voltage_reading(3_600), voltage_reading(3_590)];
        let page_identity = BmsPageIdentity::from_tag_and_selector(Some(ProtocolTag::new(0x03)), 2);
        let groups = bms_groups_from_cell_voltages(&cell_voltages, page_identity);

        assert_eq!(page_identity.first_group_index().as_mobile_dto(), 49);
        assert_eq!(
            groups.iter().map(|group| group.index).collect::<Vec<_>>(),
            vec![49, 50]
        );
        assert_eq!(
            lowest_cell_voltage_group_index(&cell_voltages, page_identity),
            Some(50)
        );
    }

    #[test]
    fn begode_second_bank_overflow_falls_back_to_bank_base() {
        let page_identity =
            BmsPageIdentity::from_tag_and_selector(Some(ProtocolTag::new(0x03)), u8::MAX);

        assert_eq!(page_identity.first_group_index().as_mobile_dto(), 33);
    }

    #[test]
    fn mobile_discovery_candidate_recommends_probe_for_unknown_ffe0() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-unknown-euc".to_owned(),
            Some("EUC-unknown".to_owned()),
            vec![discovery_service(CoreBluetoothServiceUuid::EUC_SERIAL_FFE0)],
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(candidate.product_category, "Electric unicycle");
        assert_eq!(candidate.evidence, "FFE0/FFE1 transport hint");
        assert_eq!(candidate.detail, "Read-only protocol probe recommended");
        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::ProbeRecommended
        );
        assert_eq!(
            candidate.recommended_action,
            DiscoveryCandidateAction::Probe
        );
        assert_eq!(candidate.section, DiscoveryCandidateSection::ProbeFirst);
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Read-only protocol probe recommended".to_owned())
        );
    }

    #[test]
    fn mobile_discovery_candidate_routes_vesc_advertisement_provisionally() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-unknown".to_owned(),
            Some("Little FOCer".to_owned()),
            vec![discovery_service(
                CoreBluetoothServiceUuid::VESC_NORDIC_UART,
            )],
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(candidate.product_category, "VESC Onewheel");
        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::ProvisionalRoute
        );
        assert_eq!(
            candidate.connection_route,
            Some(DiscoveryConnectionRoute::VescOnewheel)
        );
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(candidate.disabled_reason, None);
    }

    #[test]
    fn mobile_discovery_candidate_hides_unrelated_bluetooth() {
        let candidate = mobile_discovery_candidate_from_advertisement(
            "ios-local-keyboard".to_owned(),
            Some("Keyboard".to_owned()),
            vec![],
        );

        assert!(!candidate.is_picker_candidate);
        assert_eq!(candidate.support, DiscoveryCandidateSupport::RejectedNoise);
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
    }

    #[test]
    fn mobile_manual_discovery_candidate_is_typed_placeholder() {
        let candidate = mobile_manual_discovery_candidate();

        assert!(candidate.is_picker_candidate);
        assert_eq!(candidate.display_name, "Manual add / record unknown device");
        assert_eq!(
            candidate.support,
            DiscoveryCandidateSupport::ManualPlaceholder
        );
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Capture flow later".to_owned())
        );
    }

    #[test]
    fn mobile_ambiguous_discovery_candidate_requires_confirmation_without_route() {
        let candidate = mobile_ambiguous_discovery_candidate(
            "ios-local-begode".to_owned(),
            "GotWay_002441".to_owned(),
            "Falcon or Falcon variant".to_owned(),
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(candidate.support, DiscoveryCandidateSupport::Ambiguous);
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Needs user confirmation".to_owned())
        );
    }

    #[test]
    fn mobile_conflicting_discovery_candidate_stays_unrouteable() {
        let candidate = mobile_conflicting_discovery_candidate(
            "ios-local-conflict".to_owned(),
            "Conflicting wheel".to_owned(),
            "Veteran frame conflicts with Begode banner".to_owned(),
        );

        assert!(candidate.is_picker_candidate);
        assert_eq!(candidate.support, DiscoveryCandidateSupport::Conflicting);
        assert_eq!(candidate.connection_route, None);
        assert_eq!(candidate.electric_unicycle_model, None);
        assert_eq!(
            candidate.disabled_reason,
            Some("Conflicting identity evidence".to_owned())
        );
    }

    #[test]
    fn core_discovery_candidate_resolution_states_project_to_mobile_rows() {
        for (core_support, mobile_support, disabled_reason) in [
            (
                CoreDiscoveryCandidateSupport::Ambiguous,
                DiscoveryCandidateSupport::Ambiguous,
                "Needs user confirmation",
            ),
            (
                CoreDiscoveryCandidateSupport::Conflicting,
                DiscoveryCandidateSupport::Conflicting,
                "Conflicting identity evidence",
            ),
            (
                CoreDiscoveryCandidateSupport::RejectedNoise,
                DiscoveryCandidateSupport::RejectedNoise,
                "Rejected noise",
            ),
            (
                CoreDiscoveryCandidateSupport::ManualPlaceholder,
                DiscoveryCandidateSupport::ManualPlaceholder,
                "Capture flow later",
            ),
        ] {
            let candidate = DiscoveryCandidate::from(DiscoveryCandidateSnapshot {
                platform_identifier: "ios-local-row".to_owned(),
                display_name: "Candidate".to_owned(),
                product_category: "Electric unicycle".to_owned(),
                evidence: "resolver evidence".to_owned(),
                detail: "resolver detail".to_owned(),
                support: core_support,
                connection_route: None,
                electric_unicycle_model: None,
            });

            assert_eq!(candidate.support, mobile_support);
            assert_eq!(candidate.connection_route, None);
            assert_eq!(candidate.electric_unicycle_model, None);
            assert_eq!(candidate.disabled_reason, Some(disabled_reason.to_owned()));
        }
    }

    #[test]
    fn power_flow_direction_uses_motion_and_charge_context_for_negative_current() {
        assert_eq!(
            power_flow_from_signed_current(
                battery_current_reading(2_000),
                RideOperatingState::Riding
            ),
            PowerFlowDirection::Discharge
        );
        assert_eq!(
            power_flow_from_signed_current(battery_current_reading(0), RideOperatingState::Riding),
            PowerFlowDirection::Zero
        );
        assert_eq!(
            power_flow_from_signed_current(
                battery_current_reading(-2_000),
                RideOperatingState::Unknown
            ),
            PowerFlowDirection::NegativeUnknown
        );
        assert_eq!(
            power_flow_from_signed_current(
                battery_current_reading(-2_000),
                RideOperatingState::Parked
            ),
            PowerFlowDirection::NegativeUnknown
        );
        assert_eq!(
            power_flow_from_signed_current(
                battery_current_reading(-2_000),
                RideOperatingState::Standing
            ),
            PowerFlowDirection::NegativeUnknown
        );
        assert_eq!(
            power_flow_from_signed_current(
                battery_current_reading(-2_000),
                RideOperatingState::Riding
            ),
            PowerFlowDirection::Regeneration
        );
        assert_eq!(
            power_flow_from_signed_current(
                battery_current_reading(-2_000),
                RideOperatingState::Charging
            ),
            PowerFlowDirection::Charging
        );
    }

    #[test]
    fn ride_operating_state_uses_charge_mode_before_speed() {
        assert_eq!(
            ride_operating_state(None, None, None),
            RideOperatingState::Unknown
        );
        assert_eq!(
            ride_operating_state(None, None, Some(speed_reading(0))),
            RideOperatingState::Standing
        );
        assert_eq!(
            ride_operating_state(None, None, Some(speed_reading(1_000))),
            RideOperatingState::Riding
        );
        assert_eq!(
            ride_operating_state(None, None, Some(speed_reading(-1_000))),
            RideOperatingState::Riding
        );
        assert_eq!(
            ride_operating_state(
                None,
                Some(ChargeModeReadingDto {
                    value: ChargeModeDto::Charging,
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                Some(speed_reading(0)),
            ),
            RideOperatingState::Charging
        );
    }

    #[test]
    fn ride_operating_state_prefers_explicit_protocol_state() {
        assert_eq!(
            ride_operating_state(
                Some(RideOperatingStateDto::Parked),
                Some(ChargeModeReadingDto {
                    value: ChargeModeDto::NotCharging,
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                Some(speed_reading(1_000)),
            ),
            RideOperatingState::Parked
        );
    }

    #[test]
    fn ride_operating_state_uses_fallbacks_when_explicit_state_is_unknown() {
        assert_eq!(
            ride_operating_state(
                Some(RideOperatingStateDto::Unknown),
                None,
                Some(speed_reading(1_000))
            ),
            RideOperatingState::Riding
        );
        assert_eq!(
            ride_operating_state(
                Some(RideOperatingStateDto::Unknown),
                Some(ChargeModeReadingDto {
                    value: ChargeModeDto::Charging,
                    source: ValueSourceDto::Reported,
                    quality: ValueQualityDto::Known,
                    verification: VerificationStatusDto::HardwareVerified,
                }),
                Some(speed_reading(1_000))
            ),
            RideOperatingState::Charging
        );
    }

    const fn ms(value: u64) -> MobileMonotonicMillisDto {
        MobileMonotonicMillisDto {
            milliseconds: value,
        }
    }

    const fn wc(value: u64) -> MobileWallClockUnixMillisDto {
        MobileWallClockUnixMillisDto {
            milliseconds: value,
        }
    }

    const fn speed_reading(value: i32) -> SpeedReadingDto {
        SpeedReadingDto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::SourceVerified,
        }
    }

    const fn battery_current_reading(value: i32) -> BatteryCurrentReadingDto {
        BatteryCurrentReadingDto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::SourceVerified,
        }
    }

    const fn voltage_reading(value: i32) -> VoltageReadingDto {
        VoltageReadingDto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::SourceVerified,
        }
    }

    const fn temperature_reading(value: i32) -> TemperatureReadingDto {
        TemperatureReadingDto {
            value,
            source: ValueSourceDto::Reported,
            quality: ValueQualityDto::Known,
            verification: VerificationStatusDto::SourceVerified,
        }
    }

    const fn reported_voltage(value: i32) -> VoltageReading {
        VoltageReading {
            value: Voltage { value },
            source: MobileValueSourceDto::Reported,
            quality: MobileValueQualityDto::Known,
            verification: MobileVerificationStatusDto::SourceVerified,
        }
    }

    const fn reported_battery_current(value: i32) -> BatteryCurrentReading {
        BatteryCurrentReading {
            value: BatteryCurrent { value },
            source: MobileValueSourceDto::Reported,
            quality: MobileValueQualityDto::Known,
            verification: MobileVerificationStatusDto::SourceVerified,
        }
    }

    const fn reported_phase_current(value: i32) -> PhaseCurrentReading {
        PhaseCurrentReading {
            value: PhaseCurrent { value },
            source: MobileValueSourceDto::Reported,
            quality: MobileValueQualityDto::Known,
            verification: MobileVerificationStatusDto::SourceVerified,
        }
    }

    const fn notification_len(value: usize) -> NotificationByteLenDto {
        NotificationByteLenDto { bytes: value }
    }

    const fn mobile_notification_len(value: u64) -> MobileNotificationByteLenDto {
        MobileNotificationByteLenDto { bytes: value }
    }

    const fn body_len(value: usize) -> PayloadBodyLenDto {
        PayloadBodyLenDto { bytes: value }
    }

    const fn mobile_body_len(value: u64) -> MobilePayloadBodyLenDto {
        MobilePayloadBodyLenDto { bytes: value }
    }

    const fn frame_len(value: usize) -> ParserFrameLenDto {
        ParserFrameLenDto { bytes: value }
    }

    const fn mobile_frame_len(value: u64) -> MobileParserFrameLenDto {
        MobileParserFrameLenDto { bytes: value }
    }

    const fn event_count(value: usize) -> SemanticEventCountDto {
        SemanticEventCountDto { count: value }
    }

    const fn mobile_event_count(value: u64) -> MobileSemanticEventCountDto {
        MobileSemanticEventCountDto { count: value }
    }

    const fn mobile_diag_count(value: u64) -> MobileParserDiagnosticCountDto {
        MobileParserDiagnosticCountDto { count: value }
    }

    const fn mobile_write_len(value: u16) -> MobileTransportWriteLimitDto {
        MobileTransportWriteLimitDto { bytes: value }
    }

    const LIVE_VESC_VALUES_CHUNK_0: &[u8] = &hex_literal::hex!("024a");
    const LIVE_VESC_VALUES_CHUNK_1: &[u8] =
        &hex_literal::hex!("04010b00ea000000000000000000000000000000");
    const LIVE_VESC_VALUES_CHUNK_2: &[u8] =
        &hex_literal::hex!("00000000000000026b0000000000000000000000");
    const LIVE_VESC_VALUES_CHUNK_3: &[u8] =
        &hex_literal::hex!("0000000000fffffffe00000004000036ee861700");
    const LIVE_VESC_VALUES_CHUNK_4: &[u8] = &hex_literal::hex!("000000000000000007ffffffec00");
    const LIVE_VESC_VALUES_CHUNK_5: &[u8] = &hex_literal::hex!("e3be03");

    fn custom_app_frame(app_data: &[u8]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(app_data.len() + 1);
        payload.push(VESC_COMM_CUSTOM_APP_DATA);
        payload.extend_from_slice(app_data);
        let crc = crc16_xmodem(&payload);

        let mut frame = Vec::with_capacity(payload.len() + 5);
        frame.push(2);
        frame.push(u8::try_from(payload.len()).expect("test frame fits short VESC length"));
        frame.extend_from_slice(&payload);
        frame.extend_from_slice(&crc.to_be_bytes());
        frame.push(3);
        frame
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

    fn refloat_realtime_ids_frame() -> Vec<u8> {
        let mut payload = vec![101, 32, 4];
        for id in ["motor.speed", "imu.pitch", "footpad.adc1", "footpad.adc2"] {
            payload.push(u8::try_from(id.len()).expect("fixture id length fits"));
            payload.extend_from_slice(id.as_bytes());
        }
        payload.push(0);
        custom_app_frame(&payload)
    }

    fn refloat_realtime_data_frame(
        flags_and_footpad: u8,
        stop_and_sat: u8,
        beep_reason: u8,
    ) -> Vec<u8> {
        let mut payload = vec![101, 31, 0x4, 0];
        payload.extend_from_slice(&42_u32.to_be_bytes());
        payload.extend_from_slice(&[0x13, flags_and_footpad, stop_and_sat, beep_reason]);
        for half in [
            0x3c00_u16, // 1.0 m/s
            0x4400_u16, // 4 degrees pitch
            0x3d00_u16, // 1.25 adc1
            0x3b00_u16, // 0.875 adc2
        ] {
            payload.extend_from_slice(&half.to_be_bytes());
        }
        payload.extend_from_slice(&0_u32.to_be_bytes());
        payload.extend_from_slice(&0_u32.to_be_bytes());
        payload.push(0);
        custom_app_frame(&payload)
    }

    fn vesc_notification(session: &VescReadOnlySession, monotonic_ms: u64, bytes: &[u8]) {
        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Notification,
            monotonic_ms: ms(monotonic_ms),
            max_write_len: None,
            channel: VESC_NOTIFY_CHANNEL.as_bytes().to_vec(),
            bytes: bytes.to_vec(),
            command: None,
        });
        assert_eq!(result.error, None);
    }

    fn notification_fixture() -> NotificationEvidenceDto {
        NotificationEvidenceDto {
            family: ProtocolFamilyDto::VeteranLeaperkimNosfet,
            channel: [0x7a; 16],
            len: notification_len(17),
            monotonic_ms: MonotonicMillisDto { milliseconds: 42 },
        }
    }

    fn ignored_notification_fixture() -> IgnoredNotificationEvidenceDto {
        IgnoredNotificationEvidenceDto {
            family: None,
            channel: [0x7b; 16],
            len: notification_len(19),
            monotonic_ms: MonotonicMillisDto { milliseconds: 43 },
            retained_payload: vec![0xde, 0xad, 0xbe, 0xef],
        }
    }

    fn mobile_ingest(output: NotificationIngestOutcomeDto) -> MobileNotificationIngestOutcomeDto {
        let mobile = MobileSessionOutputDto::from(SessionOutputDto::NotificationIngest(output));

        assert_eq!(mobile.kind, MobileSessionOutputKindDto::NotificationIngest);
        assert!(mobile.channel.is_empty());
        assert!(mobile.bytes.is_empty());
        mobile.ingest.expect("ingest output carries typed outcome")
    }

    fn assert_ignored_notification_outcome(ignored: &MobileNotificationIngestOutcomeDto) {
        assert_eq!(
            ignored.kind,
            MobileNotificationIngestOutcomeKindDto::Ignored
        );
        assert_eq!(ignored.notification, None);
        assert_eq!(
            ignored.ignored_reason,
            Some(MobileIgnoredNotificationReasonDto::WrongChannel)
        );
        let evidence = ignored
            .ignored
            .as_ref()
            .expect("ignored outcome carries ignored evidence");
        assert_eq!(evidence.family, None);
        assert_eq!(evidence.retained_payload, vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(ignored.event_count, None);
    }

    #[test]
    fn mobile_notification_ingest_dto_preserves_each_typed_outcome_class() {
        let semantic = mobile_ingest(NotificationIngestOutcomeDto::SemanticEvents {
            notification: notification_fixture(),
            event_count: event_count(3),
        });
        assert_eq!(
            semantic.kind,
            MobileNotificationIngestOutcomeKindDto::SemanticEvents
        );
        let semantic_notification = semantic
            .notification
            .as_ref()
            .expect("semantic outcome carries accepted notification evidence");
        assert_eq!(
            semantic_notification.family,
            MobileProtocolFamilyDto::VeteranLeaperkimNosfet
        );
        assert_eq!(semantic_notification.channel, vec![0x7a; 16]);
        assert_eq!(semantic_notification.len, mobile_notification_len(17));
        assert_eq!(semantic.event_count, Some(mobile_event_count(3)));
        assert_eq!(semantic.parser_error, None);

        let buffered = mobile_ingest(NotificationIngestOutcomeDto::BufferedFragment(
            notification_fixture(),
        ));
        assert_eq!(
            buffered.kind,
            MobileNotificationIngestOutcomeKindDto::BufferedFragment
        );
        assert_eq!(buffered.event_count, None);

        let diagnostic = mobile_ingest(NotificationIngestOutcomeDto::ParserDiagnostic {
            notification: notification_fixture(),
            error: ParserErrorDto::OversizedFrame {
                claimed: frame_len(257),
                max: frame_len(256),
            },
        });
        assert_eq!(
            diagnostic.parser_error,
            Some(MobileParserErrorDto {
                kind: MobileParserErrorKindDto::OversizedFrame,
                claimed: Some(mobile_frame_len(257)),
                max: Some(mobile_frame_len(256)),
                elapsed_ms: None,
                timeout_ms: None,
            })
        );

        let reserved = mobile_ingest(NotificationIngestOutcomeDto::KnownReserved {
            notification: notification_fixture(),
            payload: ReservedPayloadEvidenceDto {
                selector: Some(8),
                tag: Some(0x5a5c),
                body_len: body_len(84),
                retained_payload: vec![0x08, 0xaa],
                verification: VerificationStatusDto::SourceVerified,
            },
        });
        assert_eq!(
            reserved.reserved,
            Some(MobileReservedPayloadEvidenceDto {
                selector: Some(8),
                tag: Some(0x5a5c),
                body_len: mobile_body_len(84),
                retained_payload: vec![0x08, 0xaa],
                verification: MobileVerificationStatusDto::SourceVerified,
            })
        );

        let gap = mobile_ingest(NotificationIngestOutcomeDto::ParserGap {
            notification: notification_fixture(),
            gap: ParserGapEvidenceDto {
                selector: Some(9),
                tag: None,
                body_len: body_len(11),
                retained_payload: vec![0x09, 0xbb],
            },
        });
        assert_eq!(
            gap.gap,
            Some(MobileParserGapEvidenceDto {
                selector: Some(9),
                tag: None,
                body_len: mobile_body_len(11),
                retained_payload: vec![0x09, 0xbb],
            })
        );

        let ignored = mobile_ingest(NotificationIngestOutcomeDto::Ignored {
            evidence: ignored_notification_fixture(),
            reason: IgnoredNotificationReasonDto::WrongChannel,
        });
        assert_ignored_notification_outcome(&ignored);
    }

    #[test]
    fn aero_wrapper_constructs_and_exposes_diagnostics() {
        let session = AeroBenignControlSession::new();

        assert_eq!(session.diagnostics().malformed_frames, mobile_diag_count(0));
    }

    #[test]
    fn falcon_wrapper_surfaces_unsupported_command_error() {
        let session = FalconBenignControlSession::new().expect("default profile should construct");

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SoundHorn),
        });

        assert!(matches!(
            result.error,
            Some(MobileSessionStepErrorDto {
                kind: MobileSessionStepErrorKindDto::CommandRefused,
                reason: Some(MobileControlRefusalReasonDto::UnsupportedCommand),
                ..
            })
        ));
    }

    #[test]
    fn aero_wrapper_writes_documented_pedal_mode_after_stationary_arm() {
        let session = AeroBenignControlSession::new();
        assert!(session.arm_settings_writes(RideOperatingState::Parked, ms(0)));

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetPedalMode(MobilePedalModeKindDto::Hard)),
        });

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| {
            output.kind == MobileSessionOutputKindDto::Write && output.bytes == b"SETh"
        }));
    }

    #[test]
    fn aero_wrapper_tracks_tiltback_write_until_readback() {
        let session = AeroBenignControlSession::new();
        assert!(session.arm_settings_writes(RideOperatingState::Parked, ms(0)));

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetAeroTiltbackSpeed(
                MobileAeroSpeedSettingDto {
                    kilometres_per_hour: 21,
                },
            )),
        });

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| {
            output.kind == MobileSessionOutputKindDto::Write && output.bytes.starts_with(b"LdAp")
        }));
        assert_eq!(
            session.aero_tiltback_speed_state().requested,
            Some(MobileAeroSpeedSettingDto {
                kilometres_per_hour: 21,
            })
        );
    }

    #[test]
    fn aero_wrapper_tracks_high_beam_write() {
        let session = AeroBenignControlSession::new();
        assert!(session.arm_settings_writes(RideOperatingState::Parked, ms(0)));

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetAeroHighBeam(MobileLightStateDto::On)),
        });

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| {
            output.kind == MobileSessionOutputKindDto::Write && output.bytes.starts_with(b"LkAp")
        }));
        assert_eq!(
            session.aero_high_beam_state().requested,
            Some(MobileLightStateDto::On)
        );
    }

    #[test]
    fn aero_speed_tracker_confirms_tiltback_from_typed_readback() {
        let mut trackers = MobileEucSettingTrackers::default();
        let input = MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(10),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetAeroTiltbackSpeed(
                MobileAeroSpeedSettingDto {
                    kilometres_per_hour: 21,
                },
            )),
        };
        let result = MobileSessionStepResultDto {
            outputs: vec![MobileSessionOutputDto {
                kind: MobileSessionOutputKindDto::SettingsReadback,
                channel: Vec::new(),
                bytes: Vec::new(),
                ingest: None,
                settings_readback: Some(MobileSettingsReadbackDto::from(
                    SettingsReadback::available([
                        Some(SettingsEntry {
                            field: RawFieldValue::new(VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, 210),
                            source: ValueSource::Reported,
                            quality: ValueQuality::Known,
                            verification: VerificationStatus::HardwareVerified,
                        }),
                        None,
                        None,
                        None,
                    ]),
                )),
                fault_history_readback: None,
                bms_snapshot: None,
                raw_telemetry: None,
                veteran_protocol_model_id: Some(43),
            }],
            error: None,
        };

        trackers.observe_step(&input, &result);

        let state = trackers.aero_tiltback_speed();
        assert_eq!(state.kind, MobileSettingStateKindDto::Confirmed);
        assert_eq!(
            state.current,
            Some(MobileAeroSpeedSettingDto {
                kilometres_per_hour: 21,
            })
        );
        assert_eq!(state.confirmed_at_ms, Some(10));
    }

    #[test]
    fn aero_wrapper_tracks_pwm_and_angle_writes() {
        let session = AeroBenignControlSession::new();
        assert!(session.arm_settings_writes(RideOperatingState::Parked, ms(0)));

        for command in [
            MobileCommandDto::SetAeroPwmPercent(MobileAeroPwmPercentDto { percent: 64 }),
            MobileCommandDto::SetAeroAngleAdjustment(MobileAeroAngleAdjustmentDto {
                tenths_of_degree: -36,
            }),
        ] {
            let result = session.ingest_checked(MobileSessionInputDto {
                kind: MobileSessionInputKindDto::Command,
                monotonic_ms: ms(0),
                max_write_len: None,
                channel: Vec::new(),
                bytes: Vec::new(),
                command: Some(command),
            });
            assert_eq!(result.error, None);
            assert!(
                result
                    .outputs
                    .iter()
                    .any(|output| output.kind == MobileSessionOutputKindDto::Write)
            );
        }

        assert_eq!(
            session.aero_pwm_percent_state().requested,
            Some(MobileAeroPwmPercentDto { percent: 64 })
        );
        assert_eq!(
            session.aero_angle_adjustment_state().requested,
            Some(MobileAeroAngleAdjustmentDto {
                tenths_of_degree: -36,
            })
        );
    }

    #[test]
    fn falcon_wrapper_writes_documented_roll_angle_after_stationary_arm() {
        let session = FalconBenignControlSession::new().expect("default profile should construct");
        assert!(session.arm_settings_writes(RideOperatingState::Parked, ms(0)));

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetRollAngle(MobileRollAngleKindDto::High)),
        });

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| {
            output.kind == MobileSessionOutputKindDto::Write && output.bytes == b"<"
        }));
        assert_eq!(
            session.roll_angle_state().requested,
            Some(MobileRollAngleKindDto::High)
        );
    }

    #[test]
    fn falcon_wrapper_writes_documented_speed_alarm_mode_after_stationary_arm() {
        let session = FalconBenignControlSession::new().expect("default profile should construct");
        assert!(session.arm_settings_writes(RideOperatingState::Parked, ms(0)));

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetSpeedAlarmMode(
                MobileSpeedAlarmModeKindDto::StageOneOnly,
            )),
        });

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| {
            output.kind == MobileSessionOutputKindDto::Write && output.bytes == b"u"
        }));
        assert_eq!(
            session.speed_alarm_mode_state().requested,
            Some(MobileSpeedAlarmModeKindDto::StageOneOnly)
        );
    }

    #[test]
    fn falcon_wrapper_schedules_typed_w_setting_sequence() {
        let session = FalconBenignControlSession::new().expect("default profile should construct");
        assert!(session.arm_settings_writes(RideOperatingState::Parked, ms(0)));

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetBegodeMaxSpeed(
                MobileBegodeMaxSpeedDto {
                    kilometres_per_hour: 30,
                },
            )),
        });
        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| {
            output.kind == MobileSessionOutputKindDto::Write && output.bytes == b"W"
        }));

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Tick,
            monotonic_ms: ms(100),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: None,
        });
        assert!(result.outputs.iter().any(|output| {
            output.kind == MobileSessionOutputKindDto::Write && output.bytes == b"Y"
        }));
    }

    #[test]
    fn falcon_wrapper_refuses_ambiguous_speed_alarm_off_command() {
        let session = FalconBenignControlSession::new().expect("default profile should construct");
        assert!(session.arm_settings_writes(RideOperatingState::Parked, ms(0)));

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetSpeedAlarmMode(
                MobileSpeedAlarmModeKindDto::Off,
            )),
        });

        assert!(matches!(
            result.error,
            Some(MobileSessionStepErrorDto {
                kind: MobileSessionStepErrorKindDto::CommandRefused,
                reason: Some(MobileControlRefusalReasonDto::UnsupportedCommand),
                ..
            })
        ));
        assert!(
            result
                .outputs
                .iter()
                .all(|output| output.kind != MobileSessionOutputKindDto::Write)
        );
    }

    #[test]
    fn mobile_wrapper_refuses_unverified_acceleration_assist_and_taillight_without_writes() {
        let session = AeroBenignControlSession::new();
        let commands = [
            MobileCommandDto::SetAccelerationAssist(MobileAccelerationAssistStateDto::Enabled),
            MobileCommandDto::SetTaillight(MobileLightStateDto::On),
        ];

        for command in commands {
            let result = session.ingest_checked(MobileSessionInputDto {
                kind: MobileSessionInputKindDto::Command,
                monotonic_ms: ms(0),
                max_write_len: None,
                channel: Vec::new(),
                bytes: Vec::new(),
                command: Some(command),
            });

            assert_eq!(
                result.error,
                Some(MobileSessionStepErrorDto {
                    kind: MobileSessionStepErrorKindDto::CommandRefused,
                    command: Some(command),
                    reason: Some(MobileControlRefusalReasonDto::UnsupportedCommand),
                })
            );
            assert!(
                result
                    .outputs
                    .iter()
                    .all(|output| output.kind != MobileSessionOutputKindDto::Write)
            );
        }
    }

    #[test]
    fn aero_wrapper_reports_typed_acceleration_assist_refusal_state() {
        let session = AeroBenignControlSession::new();

        let _ = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(7),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetAccelerationAssist(
                MobileAccelerationAssistStateDto::Enabled,
            )),
        });

        let state = session.acceleration_assist_state();
        assert_eq!(state.kind, MobileSettingStateKindDto::Refused);
        assert_eq!(
            state.requested,
            Some(MobileAccelerationAssistStateDto::Enabled)
        );
        assert_eq!(
            state.refusal_reason,
            Some(MobileControlRefusalReasonDto::UnsupportedCommand)
        );
    }

    #[test]
    fn aero_wrapper_reports_typed_taillight_refusal_state() {
        let session = AeroBenignControlSession::new();

        let _ = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(7),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetTaillight(MobileLightStateDto::On)),
        });

        let state = session.taillight_state();
        assert_eq!(state.kind, MobileSettingStateKindDto::Refused);
        assert_eq!(state.requested, Some(MobileLightStateDto::On));
        assert_eq!(
            state.refusal_reason,
            Some(MobileControlRefusalReasonDto::UnsupportedCommand)
        );
    }

    #[test]
    fn aero_wrapper_reports_missing_arm_for_pedal_mode() {
        let session = AeroBenignControlSession::new();

        let _ = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(7),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetPedalMode(MobilePedalModeKindDto::Hard)),
        });

        let state = session.pedal_mode_state();
        assert_eq!(state.kind, MobileSettingStateKindDto::Refused);
        assert_eq!(state.requested, Some(MobilePedalModeKindDto::Hard));
        assert_eq!(
            state.refusal_reason,
            Some(MobileControlRefusalReasonDto::MissingArm)
        );
    }

    #[test]
    fn aero_and_falcon_wrappers_write_lights_after_stationary_arm() {
        let aero = AeroBenignControlSession::new();
        let falcon = FalconBenignControlSession::new().expect("default profile should construct");
        assert!(aero.arm_settings_writes(RideOperatingState::Parked, ms(0)));
        assert!(falcon.arm_settings_writes(RideOperatingState::Parked, ms(0)));

        let command_input = |state| MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(0),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetLights(state)),
        };

        let aero_result = aero.ingest_checked(command_input(MobileLightStateDto::On));
        let falcon_result = falcon.ingest_checked(command_input(MobileLightStateDto::Off));

        assert_eq!(aero_result.error, None);
        assert!(aero_result.outputs.iter().any(|output| {
            output.kind == MobileSessionOutputKindDto::Write && output.bytes == b"SetLightON"
        }));
        assert_eq!(falcon_result.error, None);
        assert!(falcon_result.outputs.iter().any(|output| {
            output.kind == MobileSessionOutputKindDto::Write && output.bytes == b"E"
        }));
    }

    #[test]
    fn mobile_light_state_tracks_accepted_write_until_matching_readback() {
        let session = AeroBenignControlSession::new();

        assert_eq!(
            session.headlight_state(),
            MobileLightSettingStateDto::unknown()
        );

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(10),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetLights(MobileLightStateDto::On)),
        });
        assert_eq!(result.error, None);
        assert_eq!(
            session.headlight_state().kind,
            MobileSettingStateKindDto::Pending
        );

        let state = session.headlight_state();
        assert_eq!(state.current, None);
        assert_eq!(state.requested, Some(MobileLightStateDto::On));
        assert_eq!(state.submitted_at_ms, Some(10));
    }

    #[test]
    fn mobile_light_state_times_out_on_tick() {
        let session = AeroBenignControlSession::new();

        let _ = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: ms(10),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: Some(MobileCommandDto::SetLights(MobileLightStateDto::On)),
        });
        assert_eq!(
            session.headlight_state().kind,
            MobileSettingStateKindDto::Pending
        );

        let _ = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Tick,
            monotonic_ms: ms(2_010),
            max_write_len: None,
            channel: Vec::new(),
            bytes: Vec::new(),
            command: None,
        });

        let state = session.headlight_state();
        assert_eq!(state.kind, MobileSettingStateKindDto::TimedOut);
        assert_eq!(state.requested, Some(MobileLightStateDto::On));
    }

    #[test]
    fn mobile_pedal_state_times_out_on_tick() {
        let mut trackers = MobileEucSettingTrackers::default();
        let accepted = MobileSessionStepResultDto {
            outputs: Vec::new(),
            error: None,
        };

        trackers.observe_step(
            &MobileSessionInputDto {
                kind: MobileSessionInputKindDto::Command,
                monotonic_ms: ms(10),
                max_write_len: None,
                channel: Vec::new(),
                bytes: Vec::new(),
                command: Some(MobileCommandDto::SetPedalMode(MobilePedalModeKindDto::Hard)),
            },
            &accepted,
        );
        assert_eq!(
            trackers.pedal_mode().kind,
            MobileSettingStateKindDto::Pending
        );

        trackers.observe_step(
            &MobileSessionInputDto {
                kind: MobileSessionInputKindDto::Tick,
                monotonic_ms: ms(2_010),
                max_write_len: None,
                channel: Vec::new(),
                bytes: Vec::new(),
                command: None,
            },
            &accepted,
        );

        let state = trackers.pedal_mode();
        assert_eq!(state.kind, MobileSettingStateKindDto::TimedOut);
        assert_eq!(state.requested, Some(MobilePedalModeKindDto::Hard));
    }

    #[test]
    fn euc_settings_capabilities_expose_guarded_write_support() {
        let aero = AeroBenignControlSession::new();
        let falcon = FalconBenignControlSession::new().expect("default profile should construct");

        assert_eq!(
            aero.settings_capabilities().headlight,
            MobileSettingWriteSupportDto::Supported
        );
        assert_eq!(
            falcon.settings_capabilities().headlight,
            MobileSettingWriteSupportDto::Supported
        );
        assert_eq!(
            aero.settings_capabilities().taillight,
            MobileSettingWriteSupportDto::Unsupported
        );
        assert_eq!(
            aero.settings_capabilities().pedal_mode,
            MobileSettingWriteSupportDto::Supported
        );
        assert_eq!(
            falcon.settings_capabilities().pedal_mode,
            MobileSettingWriteSupportDto::Supported
        );
        assert_eq!(
            aero.settings_capabilities().acceleration_assist,
            MobileSettingWriteSupportDto::Unsupported
        );
        assert_eq!(
            mobile_euc_settings_capabilities(DiscoveryElectricUnicycleModel::Aero),
            aero.settings_capabilities()
        );
        assert_eq!(
            mobile_euc_settings_capabilities(DiscoveryElectricUnicycleModel::Falcon),
            falcon.settings_capabilities()
        );
    }

    #[test]
    fn falcon_wrapper_rejects_unsupported_profile() {
        let result = FalconBenignControlSession::with_profile(MobileFalconProfileDto::Unsupported);

        assert!(matches!(
            result,
            Err(MobileSessionConstructorError::UnsupportedFalconProfile)
        ));
    }

    #[test]
    fn aero_wrapper_accepts_owned_notification_input() {
        let session = AeroBenignControlSession::new();
        let link_result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::LinkUp,
            monotonic_ms: ms(1),
            max_write_len: Some(mobile_write_len(185)),
            channel: Vec::new(),
            bytes: Vec::new(),
            command: None,
        });
        let channel = link_result
            .outputs
            .iter()
            .find(|output| output.kind == MobileSessionOutputKindDto::Subscribe)
            .expect("link-up should subscribe")
            .channel
            .clone();

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Notification,
            monotonic_ms: ms(2),
            max_write_len: None,
            channel,
            bytes: hex_literal::hex!(
                "dc5a5c532a7c000000000000ab41001700000cff\
                 000000000226021ca8f607801afa000080c80000\
                 808080808080022880803080800e310e310e2f0e\
                 2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e\
                 310e2e9e05e3ad"
            )
            .to_vec(),
            command: None,
        });

        assert_eq!(result.error, None);
        let ingest = result
            .outputs
            .iter()
            .find_map(|output| output.ingest.as_ref())
            .expect("notification should emit typed ingest outcome");
        assert_eq!(
            ingest.kind,
            MobileNotificationIngestOutcomeKindDto::SemanticEvents
        );
        let notification = ingest
            .notification
            .as_ref()
            .expect("semantic ingest carries accepted notification evidence");
        assert_eq!(
            notification.family,
            MobileProtocolFamilyDto::VeteranLeaperkimNosfet
        );
        assert_eq!(notification.len, mobile_notification_len(87));
        assert_eq!(notification.monotonic_ms, ms(2));
        assert_eq!(ingest.event_count, Some(mobile_event_count(5)));
        assert_eq!(ingest.parser_error, None);
        assert_eq!(ingest.reserved, None);
        assert_eq!(ingest.gap, None);
        assert!(result.outputs.iter().all(|output| output.bytes.is_empty()));
        let snapshot = session.current_snapshot();
        assert_eq!(
            snapshot.voltage,
            Some(VoltageReading {
                value: Voltage { value: 108_760 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            })
        );
        assert_eq!(snapshot.operating_state, RideOperatingState::Standing);
        assert!(matches!(
            snapshot.battery_current,
            Some(BatteryCurrentReading {
                value: BatteryCurrent { .. },
                ..
            })
        ));
        assert!(matches!(
            snapshot.power,
            Some(PowerReading {
                value: Power { .. },
                ..
            })
        ));
        assert!(snapshot.power_flow.is_some());
        assert!(snapshot.controller_temperature.is_some() || snapshot.motor_temperature.is_some());
        assert!(snapshot.pwm.is_some());
        assert!(snapshot.battery_level_estimated.is_some());
    }

    #[test]
    fn vesc_wrapper_current_snapshot_keeps_refloat_fields_after_values_frames() {
        let session = VescReadOnlySession::new();
        let link_result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::LinkUp,
            monotonic_ms: ms(1),
            max_write_len: Some(mobile_write_len(185)),
            channel: Vec::new(),
            bytes: Vec::new(),
            command: None,
        });
        assert_eq!(link_result.error, None);

        vesc_notification(&session, 2, &refloat_realtime_ids_frame());
        vesc_notification(&session, 3, &refloat_realtime_data_frame(0xc1, 0, 6));

        let refloat_snapshot = session.current_snapshot();
        assert_eq!(
            refloat_snapshot.vesc_warning,
            Some(MobileVescRideWarningDto::Wheelslip)
        );
        assert_eq!(
            refloat_snapshot.vesc_operating_mode,
            Some(MobileVescRideOperatingModeDto::Handtest)
        );
        assert_eq!(
            refloat_snapshot.vesc_stop_reason,
            Some(MobileVescRideStopReasonDto::None)
        );
        assert_eq!(
            refloat_snapshot.pitch,
            Some(AngleReading {
                value: Angle { value: 4_000 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::HardwareVerified,
            })
        );
        assert_eq!(
            refloat_snapshot.footpad,
            Some(MobileFootpadTelemetryDto {
                state: 3,
                contact_state: Some(MobileFootpadContactState::Both),
                adc1_milliunits: Some(1_250),
                adc2_milliunits: Some(875),
            })
        );

        vesc_notification(&session, 4, &refloat_realtime_data_frame(0xc0, 1, 6));
        let stopped_snapshot = session.current_snapshot();
        assert_eq!(
            stopped_snapshot.vesc_warning,
            Some(MobileVescRideWarningDto::None)
        );
        assert_eq!(
            stopped_snapshot.vesc_stop_reason,
            Some(MobileVescRideStopReasonDto::Pitch)
        );

        for (index, chunk) in [
            LIVE_VESC_VALUES_CHUNK_0,
            LIVE_VESC_VALUES_CHUNK_1,
            LIVE_VESC_VALUES_CHUNK_2,
            LIVE_VESC_VALUES_CHUNK_3,
            LIVE_VESC_VALUES_CHUNK_4,
            LIVE_VESC_VALUES_CHUNK_5,
        ]
        .into_iter()
        .enumerate()
        {
            vesc_notification(
                &session,
                u64::try_from(index).expect("fixture index fits") + 5,
                chunk,
            );
        }

        let snapshot = session.current_snapshot();
        assert!(snapshot.voltage.is_some());
        assert!(snapshot.controller_temperature.is_some());
        assert!(snapshot.motor_temperature.is_some());
        assert_eq!(snapshot.pitch, refloat_snapshot.pitch);
        assert_eq!(snapshot.footpad, refloat_snapshot.footpad);
    }

    #[test]
    fn vesc_wrapper_with_board_profile_uses_geometry_and_pack_facts() {
        let session = VescReadOnlySession::with_board_profile(VescBoardProfile {
            motor_pole_pairs: 15,
            gear_ratio_denominator: 1,
            wheel_circumference: Distance { value: 2_100 },
            battery_type: VescBatteryType::LiIon,
            battery_cells: 20,
            battery_parallel_cells: 1,
            battery_cell_model: VescBatteryCellModel::Unknown,
            charge_profile: None,
            reports_battery_current: true,
        });

        let link_result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::LinkUp,
            monotonic_ms: ms(1),
            max_write_len: Some(mobile_write_len(185)),
            channel: Vec::new(),
            bytes: Vec::new(),
            command: None,
        });
        assert_eq!(link_result.error, None);

        for (index, chunk) in [
            LIVE_VESC_VALUES_CHUNK_0,
            LIVE_VESC_VALUES_CHUNK_1,
            LIVE_VESC_VALUES_CHUNK_2,
            LIVE_VESC_VALUES_CHUNK_3,
            LIVE_VESC_VALUES_CHUNK_4,
            LIVE_VESC_VALUES_CHUNK_5,
        ]
        .into_iter()
        .enumerate()
        {
            vesc_notification(
                &session,
                u64::try_from(index).expect("fixture index fits") + 2,
                chunk,
            );
        }

        let snapshot = session.current_snapshot();
        assert!(snapshot.speed.is_some());
        assert!(snapshot.battery_level_estimated.is_some());
        assert!(snapshot.battery_current.is_some());
    }

    #[test]
    fn vesc_board_profile_carries_device_specific_pack_and_charge_basis() {
        let estimator = MobileChargeEstimator::new();
        let board_profile = VescBoardProfile {
            motor_pole_pairs: 15,
            gear_ratio_denominator: 1,
            wheel_circumference: Distance { value: 2_100 },
            battery_type: VescBatteryType::LiIon,
            battery_cells: 15,
            battery_parallel_cells: 2,
            battery_cell_model: VescBatteryCellModel::SonyVtc6,
            charge_profile: Some(MobileChargeProfileDto {
                session_id: 1,
                profile_id: 15_002,
                capacity_milliamp_hours: 6_000,
                capacity_source: MobileChargeCapacitySourceDto::ProtocolProfile,
                verification: MobileVerificationStatusDto::SourceVerified,
                charge_flow_verification: MobileVerificationStatusDto::HardwareVerified,
            }),
            reports_battery_current: true,
        };

        estimator.configure_vesc_board_profile(board_profile);
        let state = estimator.update(MobileChargeEstimateInputDto {
            at: MobileMonotonicMillisDto { milliseconds: 0 },
            snapshot: charge_estimator_snapshot(0, PowerFlowDirection::Charging),
            freshness: MobileDurationDto {
                milliseconds: 30_000,
            },
        });

        assert_eq!(
            state.kind,
            MobileChargeEstimateStateKindDto::CollectingSamples
        );
    }

    fn capture_phone_location_fixture() -> MobilePhoneLocationSampleDto {
        MobilePhoneLocationSampleDto {
            wall_clock_unix_ms: 1_700_000_000_008,
            latitude_degrees: 39.739_235_8,
            longitude_degrees: -104.990_251,
            altitude_meters: 1_609.344,
            horizontal_accuracy_meters: Some(0.8),
            vertical_accuracy_meters: Some(1.2),
            speed_meters_per_second: Some(4.470_400_25),
            speed_accuracy_meters_per_second: Some(0.25),
            course_degrees: Some(271.5),
            course_accuracy_degrees: Some(3.0),
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "This integration fixture spells out the complete mobile capture contract."
    )]
    #[test]
    fn mobile_capture_builder_exports_cli_readable_jsonl() {
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-writer-{}-{}.jsonl",
            std::process::id(),
            thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_file(&path);
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );

        assert!(builder.start_writer(path.to_string_lossy().into_owned()));
        let service = vec![
            0x00, 0x00, 0xff, 0xe0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
            0x34, 0xfb,
        ];
        let characteristic = vec![
            0x00, 0x00, 0xff, 0xe1, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
            0x34, 0xfb,
        ];
        assert!(builder.add_advertised_service(service.clone()));
        assert!(builder.add_gatt_fingerprint(MobileGattFingerprintDto {
            service,
            characteristic,
            roles: vec![MobileGattRoleDto::Notify],
            verification: MobileVerificationStatusDto::SourceVerified,
        }));
        assert!(builder.set_resolved_identity(MobileResolvedIdentityDto {
            protocol_family: Some(MobileProtocolFamilyDto::Vesc),
            model: Some(MobileVerifiedStringDto {
                value: "VESC Refloat".into(),
                verification: MobileVerificationStatusDto::Inferred,
            }),
            firmware: None,
        }));
        assert!(builder.record_location_sample(
            ms(7),
            capture_phone_location_fixture(),
            Some(false),
            Some(true)
        ));
        let mut invalid_location = capture_phone_location_fixture();
        invalid_location.latitude_degrees = f64::NAN;
        assert!(!builder.record_location_sample(ms(7), invalid_location, None, None));
        assert!(builder.record_notification_with_context(
            ms(8),
            vec![0; 16],
            vec![1; 16],
            vec![0xaa, 0xbb],
            Some(MobileRawTelemetryReadbackDto {
                fields: vec![MobileRawFieldValueDto {
                    id: 0x8001,
                    value: 989
                }],
                float_fields: vec![MobileRawFloatFieldValueDto {
                    id: 0x8100,
                    value_bits: 0x3f80_0001,
                }],
            }),
            Some(capture_phone_location_fixture()),
        ));
        assert!(builder.record_link_down(ms(9)));
        assert!(builder.add_annotation("route=vesc".into()));
        assert!(builder.flush_writer());
        assert!(builder.finish_writer());
        let status = builder.writer_status();
        assert_eq!(status.queued_messages, 0);
        assert!(status.bytes_written > 0);
        assert!(!status.failed);

        let bytes = fs::read(&path).expect("stream writer output exists");
        let capture = PevcapCapture::decode(&bytes, PevcapEncoding::Jsonl)
            .expect("stream writer output is PEVCAP");
        assert_eq!(capture.records.len(), 2);
        assert_eq!(capture.locations.len(), 1);
        assert_eq!(capture.locations[0].receipt_monotonic_ms, ms(7).into_core());
        assert_eq!(capture.locations[0].simulated, Some(false));
        assert_eq!(capture.locations[0].produced_by_accessory, Some(true));
        let notification = &capture.records[0];
        let telemetry = notification
            .telemetry
            .as_ref()
            .expect("raw telemetry is correlated");
        assert_eq!(telemetry.fields[0].value, 989);
        assert_eq!(telemetry.float_fields[0].value_bits, 0x3f80_0001);
        let location = notification.phone_location.expect("location is correlated");
        assert_eq!(
            location.latitude_degrees.to_bits(),
            39.739_235_8_f64.to_bits()
        );
        assert_eq!(
            location.speed_meters_per_second.unwrap().to_bits(),
            4.470_400_25_f64.to_bits()
        );
        assert_eq!(capture.header.advertised_services.len(), 1);
        assert_eq!(capture.header.gatt_fingerprints.len(), 1);
        let identity = capture
            .header
            .resolved_identity
            .as_ref()
            .expect("late resolved identity is retained");
        assert_eq!(identity.protocol_family, Some(ProtocolFamily::Vesc));
        assert_eq!(
            identity.model.as_ref().map(|model| model.value.as_str()),
            Some("VESC Refloat")
        );
        assert!(
            capture
                .header
                .annotations
                .iter()
                .any(|annotation| annotation == "route=vesc")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_capture_writer_reports_start_failure() {
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-writer-missing-{}-{}/capture.jsonl",
            std::process::id(),
            thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_dir_all(path.parent().expect("missing capture parent"));
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );

        assert!(!builder.start_writer(path.to_string_lossy().into_owned()));
        let status = builder.writer_status();
        assert!(status.failed);
        assert!(status.last_error.is_some());
    }

    #[test]
    fn mobile_capture_writer_reports_failure_after_a_successful_append() {
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-writer-late-failure-{}-{}.jsonl",
            std::process::id(),
            thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&path);
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );

        assert!(builder.start_writer(path.to_string_lossy().into_owned()));
        assert!(builder.record_link_up(ms(1), None));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = builder.writer_status();
            if status.bytes_written > 0 {
                break;
            }
            assert!(!status.failed, "writer failed before fixture mutation");
            assert!(Instant::now() < deadline, "writer did not append in time");
            thread::yield_now();
        }
        fs::remove_file(&path).expect("open capture pathname is removable");
        fs::create_dir(&path).expect("directory replaces capture pathname");

        assert!(builder.add_annotation("force=header-rewrite".into()));
        assert!(!builder.finish_writer());
        let status = builder.writer_status();
        assert!(status.failed);
        assert!(status.last_error.is_some());
        fs::remove_dir(path).expect("failure fixture directory is removable");
    }

    #[test]
    fn capture_writer_queue_overrun_is_nonblocking_and_instrumented() {
        let (sender, _receiver) = sync_channel(0);
        let state = Arc::new(CaptureWriterState::default());
        let writer = CaptureWriter {
            sender,
            records: Arc::new(CaptureRecordPool::new(1)),
            state: Arc::clone(&state),
            join: None,
        };

        let (reply, _result) = sync_channel(0);
        assert!(!writer.try_send(CaptureWriterMessage::Flush(reply)));
        let status = state.status();
        assert_eq!(status.queued_messages, 0);
        assert_eq!(status.peak_queued_messages, 0);
        assert_eq!(status.dropped_messages, 1);
        assert!(status.failed);
        assert_eq!(
            status.last_error.as_deref(),
            Some("capture writer queue is full")
        );
    }

    #[test]
    fn capture_writer_status_retains_peak_accepted_queue_depth() {
        let (sender, _receiver) = sync_channel(1);
        let state = Arc::new(CaptureWriterState::default());
        let writer = CaptureWriter {
            sender,
            records: Arc::new(CaptureRecordPool::new(1)),
            state: Arc::clone(&state),
            join: None,
        };

        let (reply, _result) = sync_channel(0);
        assert!(writer.try_send(CaptureWriterMessage::Flush(reply)));
        let status = state.status();
        assert_eq!(status.queued_messages, 1);
        assert_eq!(status.peak_queued_messages, 1);
        assert_eq!(status.dropped_messages, 0);
    }

    #[test]
    fn capture_writer_coalesces_late_metadata_into_one_replacement() {
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-writer-late-metadata-{}-{}.jsonl",
            std::process::id(),
            thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_file(&path);
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );
        assert!(builder.start_writer(path.to_string_lossy().into_owned()));

        for index in 0..1_024 {
            while builder.writer_status().queued_messages >= 64 {
                thread::yield_now();
            }
            assert!(builder.record_notification(
                ms(index),
                vec![0xe1; 16],
                vec![0xe0; 16],
                vec![0xaa; 160],
            ));
        }
        for index in 0..cutout_core::PEVCAP_MAX_ANNOTATIONS {
            assert!(builder.add_annotation(format!("synthetic=late-{index}")));
        }
        assert!(builder.finish_writer());
        let status = builder.writer_status();
        assert!(
            status.physical_bytes_written < status.bytes_written * 3,
            "late metadata physically wrote {} bytes for {} record bytes",
            status.physical_bytes_written,
            status.bytes_written,
        );

        let capture = PevcapCapture::decode(
            &fs::read(&path).expect("capture is readable"),
            PevcapEncoding::Jsonl,
        )
        .expect("capture is valid PEVCAP");
        assert!(
            capture
                .header
                .annotations
                .iter()
                .any(|annotation| annotation == "synthetic=late-7")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn capture_writer_flush_returns_after_metadata_is_durable() {
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-writer-durable-flush-{}-{}.jsonl",
            std::process::id(),
            thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_file(&path);
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );
        assert!(builder.start_writer(path.to_string_lossy().into_owned()));
        assert!(builder.record_link_up(ms(1), None));
        assert!(builder.add_annotation("durability=background".into()));

        assert!(builder.flush_writer());
        assert_eq!(builder.writer_status().queued_messages, 0);
        let capture = PevcapCapture::decode(
            &fs::read(&path).expect("flushed capture is readable"),
            PevcapEncoding::Jsonl,
        )
        .expect("flushed capture is durable PEVCAP");
        assert_eq!(capture.records.len(), 1);
        assert!(
            capture
                .header
                .annotations
                .iter()
                .any(|annotation| annotation == "durability=background")
        );

        assert!(builder.finish_writer());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn capture_writer_handles_thirty_and_sixty_minute_synthetic_rides() {
        const NOTIFICATIONS_PER_SECOND: u64 = 10;

        fn run(minutes: u64) -> (u64, u64, u64, Duration) {
            let path = std::env::temp_dir().join(format!(
                "cutout-mobile-writer-{minutes}m-{}-{}.jsonl",
                std::process::id(),
                thread::current().name().unwrap_or("test")
            ));
            let _ = fs::remove_file(&path);
            let builder = MobilePevcapCaptureBuilder::new(
                wc(1_700_000_000_000),
                "ios-corebluetooth".into(),
                None,
            );
            assert!(builder.start_writer(path.to_string_lossy().into_owned()));

            let record_count = minutes * 60 * NOTIFICATIONS_PER_SECOND;
            let characteristic = vec![0xe1; 16];
            let service = vec![0xe0; 16];
            let payload = vec![0xaa; 160];
            let started = Instant::now();
            for index in 0..record_count {
                while builder.writer_status().queued_messages >= 64 {
                    thread::yield_now();
                }
                assert!(builder.record_notification(
                    ms(index * 1_000 / NOTIFICATIONS_PER_SECOND),
                    characteristic.clone(),
                    service.clone(),
                    payload.clone(),
                ));
                if index == record_count / 2 {
                    assert!(builder.add_annotation("synthetic=midpoint".into()));
                }
            }
            assert!(builder.finish_writer());
            let elapsed = started.elapsed();
            let status = builder.writer_status();
            let capture_size = fs::metadata(&path).expect("capture exists").len();
            let capture = PevcapCapture::decode(
                &fs::read(&path).expect("capture is readable"),
                PevcapEncoding::Jsonl,
            )
            .expect("capture is valid PEVCAP");

            assert_eq!(capture.records.len() as u64, record_count);
            assert_eq!(status.queued_messages, 0);
            assert!(status.peak_queued_messages <= CAPTURE_WRITER_QUEUE_CAPACITY as u64);
            assert_eq!(status.dropped_messages, 0);
            assert!(!status.failed);
            assert!(status.physical_bytes_written > capture_size);
            eprintln!(
                "capture_writer minutes={minutes} records={record_count} elapsed_ms={} capture_bytes={capture_size} physical_bytes={} peak_queue={}",
                elapsed.as_millis(),
                status.physical_bytes_written,
                status.peak_queued_messages,
            );
            let _ = fs::remove_file(path);
            (
                capture_size,
                status.physical_bytes_written,
                status.peak_queued_messages,
                elapsed,
            )
        }

        let thirty = run(30);
        let sixty = run(60);
        assert!(sixty.0 >= thirty.0 * 19 / 10 && sixty.0 <= thirty.0 * 21 / 10);
        assert!(sixty.1 >= thirty.1 * 19 / 10 && sixty.1 <= thirty.1 * 21 / 10);
    }

    fn charge_profile(
        charge_flow_verification: MobileVerificationStatusDto,
    ) -> MobileChargeProfileDto {
        MobileChargeProfileDto {
            session_id: 1,
            profile_id: 7,
            capacity_milliamp_hours: 10_000,
            capacity_source: MobileChargeCapacitySourceDto::HardwareMeasured,
            verification: MobileVerificationStatusDto::HardwareVerified,
            charge_flow_verification,
        }
    }

    fn charge_estimator_snapshot(
        at: u64,
        power_flow: PowerFlowDirection,
    ) -> MobileTelemetrySnapshotDto {
        MobileTelemetrySnapshotDto {
            at_ms: Some(MobileMonotonicMillisDto { milliseconds: at }),
            speed: None,
            operating_state: RideOperatingState::Charging,
            vesc_operating_mode: None,
            vesc_warning: None,
            vesc_stop_reason: None,
            voltage: Some(VoltageReading {
                value: Voltage { value: 95_000 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::SourceAndHardwareVerified,
            }),
            battery_current: Some(BatteryCurrentReading {
                value: BatteryCurrent { value: -2_000 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::SourceAndHardwareVerified,
            }),
            charge_mode: Some(MobileChargeModeReadingDto {
                value: MobileChargeModeDto::Charging,
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::SourceAndHardwareVerified,
            }),
            motor_current: None,
            power: None,
            power_flow: Some(power_flow),
            voltage_sag: None,
            controller_temperature: None,
            motor_temperature: None,
            battery_temperature: None,
            pwm: None,
            distance: None,
            trip_distance: None,
            limp_home_range: None,
            pitch: None,
            balance_angle: None,
            roll: None,
            footpad: None,
            battery_level_reported: Some(BatteryLevelReading {
                value: BatteryLevel { value: 50 },
                source: MobileValueSourceDto::Reported,
                quality: MobileValueQualityDto::Known,
                verification: MobileVerificationStatusDto::SourceAndHardwareVerified,
            }),
            battery_level_estimated: None,
        }
    }

    fn sag_estimator_snapshot(
        at: u64,
        power_flow: PowerFlowDirection,
    ) -> MobileTelemetrySnapshotDto {
        let mut snapshot = charge_estimator_snapshot(at, power_flow);
        snapshot.operating_state = RideOperatingState::Riding;
        snapshot
            .charge_mode
            .as_mut()
            .expect("fixture charge mode")
            .value = MobileChargeModeDto::NotCharging;
        snapshot
    }

    #[test]
    fn charge_estimator_ffi_projects_estimates() {
        let estimator = MobileChargeEstimator::new();
        estimator.configure_profile(charge_profile(
            MobileVerificationStatusDto::HardwareVerified,
        ));
        let update = |at, power_flow| {
            estimator.update(MobileChargeEstimateInputDto {
                at: MobileMonotonicMillisDto { milliseconds: at },
                snapshot: charge_estimator_snapshot(at, power_flow),
                freshness: MobileDurationDto {
                    milliseconds: 60_000,
                },
            })
        };

        assert_eq!(
            update(0, PowerFlowDirection::Charging).kind,
            MobileChargeEstimateStateKindDto::CollectingSamples
        );
        assert_eq!(
            update(15_000, PowerFlowDirection::Charging).kind,
            MobileChargeEstimateStateKindDto::CollectingSamples
        );
        let result = update(30_000, PowerFlowDirection::Charging);
        let estimate = result.estimate.expect("stable samples produce an estimate");
        assert_eq!(result.kind, MobileChargeEstimateStateKindDto::Available);
        assert_eq!(estimate.kind, MobileEstimateKindDto::AtPresentCurrent);
        assert_eq!(estimate.voltage_sag, None);

        let contradictory = update(45_000, PowerFlowDirection::Discharge);
        assert_eq!(
            contradictory.unavailable_reason,
            Some(MobileChargeEstimateUnavailableReasonDto::ContradictoryInputs)
        );

        estimator.configure_profile(charge_profile(MobileVerificationStatusDto::Unverified));
        let gated = update(0, PowerFlowDirection::Charging);
        assert_eq!(
            gated.unavailable_reason,
            Some(MobileChargeEstimateUnavailableReasonDto::CurrentDirectionUnverified)
        );

        estimator.configure_nosfet_aero_30s2p_samsung_50s_profile();
        let default_profile = update(0, PowerFlowDirection::Charging);
        assert_eq!(
            default_profile.unavailable_reason,
            Some(MobileChargeEstimateUnavailableReasonDto::CurrentDirectionUnverified)
        );
    }

    #[test]
    fn voltage_sag_ffi_learns_from_observed_load_steps() {
        let estimator = MobileChargeEstimator::new();
        let update = |at, voltage, current| {
            let mut snapshot = sag_estimator_snapshot(at, PowerFlowDirection::Discharge);
            snapshot
                .voltage
                .as_mut()
                .expect("fixture voltage")
                .value
                .value = voltage;
            snapshot
                .battery_current
                .as_mut()
                .expect("fixture current")
                .value
                .value = current;
            estimator.update(MobileChargeEstimateInputDto {
                at: MobileMonotonicMillisDto { milliseconds: at },
                snapshot,
                freshness: MobileDurationDto {
                    milliseconds: 30_000,
                },
            })
        };

        assert_eq!(update(0, 100_000, 0).voltage_sag, None);
        let sag = update(1_000, 99_000, 10_000)
            .voltage_sag
            .expect("the observed load step should produce sag");
        assert_eq!(sag.delta_millivolts, -1_000);
        assert_eq!(sag.load_current.value.value, 10_000);
        assert_eq!(sag.effective_resistance_milliohms, 100);
        assert_eq!(sag.observations, 1);
        assert_eq!(sag.confidence, MobileEstimateConfidenceDto::Low);
    }

    #[test]
    fn voltage_sag_ffi_exports_and_restores_a_per_device_model() {
        let estimator = MobileChargeEstimator::new();
        let update = |estimator: &MobileChargeEstimator, at, voltage, current| {
            let mut snapshot = sag_estimator_snapshot(at, PowerFlowDirection::Discharge);
            snapshot
                .voltage
                .as_mut()
                .expect("fixture voltage")
                .value
                .value = voltage;
            snapshot
                .battery_current
                .as_mut()
                .expect("fixture current")
                .value
                .value = current;
            estimator.update(MobileChargeEstimateInputDto {
                at: MobileMonotonicMillisDto { milliseconds: at },
                snapshot,
                freshness: MobileDurationDto {
                    milliseconds: 30_000,
                },
            })
        };
        let _ = update(&estimator, 0, 100_000, 0);
        let _ = update(&estimator, 1_000, 99_000, 10_000);
        let model = estimator
            .voltage_sag_model()
            .expect("learned resistance should be exportable");
        assert_eq!(model.schema_version, 1);
        assert_eq!(model.effective_resistance_milliohms, 100);
        assert_eq!(model.observations, 1);

        let restored = MobileChargeEstimator::new();
        assert!(restored.restore_voltage_sag_model(model));
        assert_eq!(
            update(&restored, 60_000, 99_000, 10_000)
                .voltage_sag
                .expect("restored model should project sag")
                .delta_millivolts,
            -1_000
        );
        restored.clear_voltage_sag_model();
        assert_eq!(restored.voltage_sag_model(), None);
    }

    #[test]
    fn voltage_sag_ffi_does_not_project_non_discharging_current() {
        let model = MobileVoltageSagModelDto {
            schema_version: 1,
            effective_resistance_milliohms: 100,
            observations: 8,
            hardware_verified: true,
        };
        let charging = charge_estimator_snapshot(1_000, PowerFlowDirection::Charging);
        let regeneration = sag_estimator_snapshot(1_000, PowerFlowDirection::Regeneration);

        for (flow, mut snapshot) in [("charging", charging), ("regeneration", regeneration)] {
            snapshot
                .battery_current
                .as_mut()
                .expect("fixture current")
                .value
                .value = 10_000;
            let estimator = MobileChargeEstimator::new();
            assert!(estimator.restore_voltage_sag_model(model));

            let state = estimator.update(MobileChargeEstimateInputDto {
                at: MobileMonotonicMillisDto {
                    milliseconds: 1_000,
                },
                snapshot,
                freshness: MobileDurationDto {
                    milliseconds: 30_000,
                },
            });

            assert_eq!(state.voltage_sag, None, "{flow} must not produce sag");
            assert_eq!(estimator.voltage_sag_model(), Some(model));
        }
    }

    #[test]
    fn voltage_sag_model_is_cleared_for_an_incompatible_pack_replacement() {
        let estimator = MobileChargeEstimator::new();
        let mut profile = charge_profile(MobileVerificationStatusDto::HardwareVerified);
        estimator.configure_profile(profile);
        assert!(
            estimator.restore_voltage_sag_model(MobileVoltageSagModelDto {
                schema_version: 1,
                effective_resistance_milliohms: 100,
                observations: 8,
                hardware_verified: true,
            })
        );

        profile.capacity_milliamp_hours = 5_000;
        estimator.configure_profile(profile);

        assert_eq!(estimator.voltage_sag_model(), None);
    }

    #[test]
    fn charge_estimator_ffi_does_not_infer_sag_from_curve_estimated_soc() {
        let estimator = MobileChargeEstimator::new();
        estimator.configure_profile(charge_profile(
            MobileVerificationStatusDto::HardwareVerified,
        ));
        let mut snapshot = charge_estimator_snapshot(0, PowerFlowDirection::Charging);
        snapshot.voltage = Some(VoltageReading {
            value: Voltage { value: 108_000 },
            source: MobileValueSourceDto::Reported,
            quality: MobileValueQualityDto::Known,
            verification: MobileVerificationStatusDto::SourceAndHardwareVerified,
        });
        snapshot.battery_level_reported = None;
        snapshot.battery_level_estimated = Some(BatteryLevelReading {
            value: BatteryLevel { value: 50 },
            source: MobileValueSourceDto::Estimated,
            quality: MobileValueQualityDto::Inferred,
            verification: MobileVerificationStatusDto::SourceVerified,
        });
        snapshot.voltage_sag = None;

        let update = |at| {
            let mut snapshot = snapshot.clone();
            snapshot.at_ms = Some(MobileMonotonicMillisDto { milliseconds: at });
            estimator.update(MobileChargeEstimateInputDto {
                at: MobileMonotonicMillisDto { milliseconds: at },
                snapshot,
                freshness: MobileDurationDto {
                    milliseconds: 60_000,
                },
            })
        };

        let _ = update(0);
        let _ = update(15_000);
        let result = update(30_000);
        let estimate = result.estimate.expect("stable samples produce an estimate");
        assert_eq!(estimate.voltage_sag, None);
    }
    #[test]
    fn spatial_pages_round_trip_mobile_cursors_and_antimeridian_bounds() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-spatial-{}-{}.sqlite3",
            std::process::id(),
            thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("mobile database opens");
        database
            .create_map_point(
                "east".into(),
                MobileMapCoordinateDto {
                    latitude_degrees: 0.0,
                    longitude_degrees: 179.5,
                },
            )
            .expect("east point is stored");
        database
            .create_map_point(
                "west".into(),
                MobileMapCoordinateDto {
                    latitude_degrees: 0.0,
                    longitude_degrees: -179.5,
                },
            )
            .expect("west point is stored");

        let bounds = MobileGeoBoundsDto {
            minimum_latitude_degrees: -1.0,
            maximum_latitude_degrees: 1.0,
            minimum_longitude_degrees: 179.0,
            maximum_longitude_degrees: -179.0,
        };
        let first = database
            .map_points_in_bounds(bounds, None, 1)
            .expect("first page is returned");
        assert_eq!(first.points.len(), 1);
        let second = database
            .map_points_in_bounds(bounds, first.next_cursor, 1)
            .expect("cursor returns the second page");
        assert_eq!(second.points.len(), 1);
        assert_ne!(first.points[0].id, second.points[0].id);
        assert!(second.next_cursor.is_none());

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_pevcap_requires_preview_confirmation_and_reports_capture_only() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let database_path =
            std::env::temp_dir().join(format!("cutout-mobile-pevcap-{}.sqlite3", Uuid::new_v4()));
        let artifact_path =
            std::env::temp_dir().join(format!("cutout-mobile-pevcap-{}.jsonl", Uuid::new_v4()));
        let header = PevcapHeader::new(
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "darwin",
            None,
            &[],
            &[],
            None,
            None,
            "test",
            [0; 32],
            &[],
        )
        .unwrap();
        let capture = PevcapCapture::new(
            header,
            vec![PevcapRecord::link_up(MonotonicTimestamp::new(1), None)],
        );
        fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();
        let database = open_ride_database(database_path.to_string_lossy().into_owned()).unwrap();

        let preview = database
            .preflight_pevcap(
                artifact_path.to_string_lossy().into_owned(),
                MobilePevcapEncodingDto::Jsonl,
            )
            .unwrap();
        assert_eq!(preview.outcome, MobilePevcapImportOutcomeDto::CaptureOnly);
        assert_eq!(
            preview.warnings,
            vec![MobilePevcapImportWarningDto::NoRouteLocations]
        );
        let receipt = database
            .confirm_pevcap_import(preview, 1_700_000_000_000)
            .unwrap();
        assert_eq!(receipt.ride_id, None);
        assert_eq!(receipt.outcome, MobilePevcapImportOutcomeDto::CaptureOnly);
        let managed_path = PathBuf::from(&receipt.managed_artifact_path);
        assert!(managed_path.exists());

        database.shutdown().unwrap();
        let _ = fs::remove_file(&managed_path);
        let _ = fs::remove_dir(managed_path.parent().unwrap());
        let _ = fs::remove_file(database_path);
        let _ = fs::remove_file(artifact_path);
    }

    #[test]
    fn mobile_pevcap_import_preserves_import_boundary_reason_in_route_page() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let database_path = std::env::temp_dir().join(format!(
            "cutout-mobile-pevcap-route-{}.sqlite3",
            Uuid::new_v4()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "cutout-mobile-pevcap-route-{}.jsonl",
            Uuid::new_v4()
        ));
        let builder = MobilePevcapCaptureBuilder::new(
            wc(1_700_000_000_000),
            "ios-corebluetooth".into(),
            None,
        );
        assert!(builder.start_writer(artifact_path.to_string_lossy().into_owned()));
        assert!(builder.record_location_sample(
            ms(7),
            capture_phone_location_fixture(),
            Some(false),
            Some(true),
        ));
        assert!(builder.finish_writer());

        let database = open_ride_database(database_path.to_string_lossy().into_owned()).unwrap();
        let preview = database
            .preflight_pevcap(
                artifact_path.to_string_lossy().into_owned(),
                MobilePevcapEncodingDto::Jsonl,
            )
            .unwrap();
        assert_eq!(
            preview.outcome,
            MobilePevcapImportOutcomeDto::RideAndCapture
        );
        assert_eq!(preview.location_count, 1);
        let receipt = database
            .confirm_pevcap_import(preview, 1_700_000_000_100)
            .unwrap();
        let ride_id = receipt.ride_id.expect("route import creates a ride");
        let page = database.route_points(ride_id, None, 10).unwrap();
        assert_eq!(page.points.len(), 1);
        assert_eq!(
            page.points[0].start_reason,
            MobileRideSegmentStartReasonDto::ImportBoundary
        );

        let managed_path = PathBuf::from(&receipt.managed_artifact_path);
        database.shutdown().unwrap();
        let _ = fs::remove_file(&managed_path);
        let _ = fs::remove_dir(managed_path.parent().unwrap());
        let _ = fs::remove_file(database_path);
        let _ = fs::remove_file(artifact_path);
    }

    #[test]
    fn mobile_ride_map_core_owns_lifecycle_and_vehicle_association() {
        let state = MobileRideMapCore::new();
        assert_eq!(
            state
                .start_gps_only(1_000, Some("pev-1".to_owned()))
                .expect("map recording starts")
                .state,
            MobileRideLifecycleStateDto::Active
        );
        assert_eq!(
            state
                .observe_vehicle_connection("pev-1".to_owned(), 1_001)
                .expect("connection observation succeeds"),
            MobileRideMapCoreAssociationDto::Associated
        );
        state
            .ingest_location(1_001, 1_700_000_000_001, 40.0, -105.0, 3.0)
            .expect("first segment point is accepted");
        state.pause(2_000).expect("recording pauses");
        state.resume(3_000).expect("recording resumes");
        let resumed = state
            .ingest_location(3_002, 1_700_000_003_002, 40.001, -105.0, 3.0)
            .expect("resumed segment point is accepted");
        assert_eq!(
            resumed,
            MobileRideMapCoreDecisionDto::Accepted {
                point: MobileRideMapCorePointDto {
                    sequence: 1,
                    segment_id: 1,
                    start_reason: MobileRideSegmentStartReasonDto::Resume,
                    latitude_degrees: 40.001,
                    longitude_degrees: -105.0,
                    wall_clock_unix_ms: 1_700_000_003_002,
                    monotonic_ms: 3_002,
                    horizontal_accuracy_meters: Some(3.0),
                    telemetry_state: MobileRideMapCoreTelemetryStateDto::AssociatedNoTelemetry,
                },
                segment_started: true,
            }
        );
        assert_eq!(
            state.stop(4_000).expect("recording stops").state,
            MobileRideLifecycleStateDto::Stopped
        );
        assert_eq!(
            state
                .resume(5_000)
                .expect_err("stopped rides cannot resume"),
            MobileRideMapCoreErrorDto::InvalidTransition
        );
    }

    #[test]
    fn mobile_ride_map_core_associates_a_vehicle_found_during_a_gps_only_ride() {
        let state = MobileRideMapCore::new();
        state
            .start_gps_only(1_000, None)
            .expect("GPS-only recording starts");
        assert_eq!(
            state
                .observe_vehicle_connection("pev-found-later".to_owned(), 1_001)
                .expect("late PEV connection is observed"),
            MobileRideMapCoreAssociationDto::Associated
        );
        assert_eq!(
            state
                .current_snapshot(1_001)
                .and_then(|snapshot| snapshot.associated_vehicle),
            Some("pev-found-later".to_owned())
        );
    }

    #[test]
    fn mobile_ride_map_snapshot_ticks_without_a_new_location() {
        let state = MobileRideMapCore::new();
        state.start_gps_only(1_000, None).expect("recording starts");
        assert_eq!(
            state
                .current_snapshot(6_000)
                .unwrap()
                .summary
                .duration_milliseconds,
            5_000
        );
    }

    #[test]
    fn mobile_ride_map_core_polls_pending_location_to_durable_acceptance() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-pending-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        state.start_gps_only(1_000, None).expect("recording starts");

        let pending = state
            .ingest_location(1_001, 1_700_000_000_001, 40.0, -105.0, 3.0)
            .expect("location queues without waiting");
        assert!(matches!(
            pending,
            MobileRideMapCoreDecisionDto::Pending { .. }
        ));

        let accepted = (0..10_000).find_map(|_| {
            state
                .poll_location_writes()
                .into_iter()
                .find(|decision| matches!(decision, MobileRideMapCoreDecisionDto::Accepted { .. }))
                .or_else(|| {
                    thread::yield_now();
                    None
                })
        });
        assert!(
            accepted.is_some(),
            "queued location should eventually settle"
        );
        assert_eq!(
            state.current_snapshot(1_002).unwrap().summary.point_count,
            1
        );

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_settles_pending_location_after_stop() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-terminal-pending-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        let started = state.start_gps_only(1_000, None).expect("recording starts");

        assert!(matches!(
            state
                .ingest_location(1_001, 1_700_000_000_001, 40.0, -105.0, 3.0)
                .expect("location queues without waiting"),
            MobileRideMapCoreDecisionDto::Pending { .. }
        ));
        state.stop(1_002).expect("recording stops");
        state.save().expect("recording saves");

        let settled = drain_location_writes(&state);
        assert!(settled.iter().any(|decision| matches!(
            decision,
            MobileRideMapCoreDecisionDto::Accepted {
                point: MobileRideMapCorePointDto { sequence: 0, .. },
                ..
            }
        )));
        let inner = state.inner.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(inner.recorder.summary().point_count().as_u64(), 1);
        drop(inner);
        assert_eq!(
            database
                .summary(MobileRideIdDto {
                    value: started.ride_id,
                })
                .expect("durable summary loads")
                .point_count,
            1
        );

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_bounds_pending_location_backpressure() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-backpressure-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        state.start_gps_only(1_000, None).expect("recording starts");

        for index in 0..MAX_PENDING_LOCATION_WRITES {
            let monotonic_ms = 1_001 + index as u64 * 1_000;
            let decision = state
                .ingest_location(
                    monotonic_ms,
                    1_700_000_000_000 + monotonic_ms,
                    40.0 + f64::from(u32::try_from(index).expect("bounded index")) * 0.00001,
                    -105.0,
                    3.0,
                )
                .expect("location remains nonblocking under queue load");
            assert!(matches!(
                decision,
                MobileRideMapCoreDecisionDto::Pending { .. }
            ));
        }
        let started = Instant::now();
        let decision = state
            .ingest_location(
                1_001 + MAX_PENDING_LOCATION_WRITES as u64 * 1_000,
                1_700_000_000_000 + 1_001 + MAX_PENDING_LOCATION_WRITES as u64 * 1_000,
                40.00065,
                -105.0,
                3.0,
            )
            .expect("queue saturation is reported as a decision");
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(matches!(
            decision,
            MobileRideMapCoreDecisionDto::StorageError { .. }
        ));

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_starts_and_associates_on_vehicle_connection() {
        let state = MobileRideMapCore::new();
        let snapshot = state
            .ensure_recording_for_vehicle("pev-1".to_owned(), 1_000)
            .expect("connection starts the live map ride");
        assert_eq!(snapshot.state, MobileRideLifecycleStateDto::Active);
        assert_eq!(snapshot.associated_vehicle, Some("pev-1".to_owned()));
        assert_eq!(snapshot.summary.point_count, 0);
        state.stop(3_000).expect("the live map ride stops");
        let restarted = state
            .ensure_recording_for_vehicle("pev-1".to_owned(), 2_000)
            .expect("a later connection starts a fresh live map ride");
        assert_eq!(restarted.state, MobileRideLifecycleStateDto::Active);
        assert_ne!(restarted.ride_id, snapshot.ride_id);
    }

    #[test]
    fn mobile_ride_map_core_rejects_invalid_start_without_an_orphan_ride() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-start-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        let invalid_candidate = "x".repeat(513);

        assert!(matches!(
            state.start_gps_only(1_000, Some(invalid_candidate)),
            Err(MobileRideMapCoreErrorDto::Storage(_))
        ));
        assert!(database.list_rides(None, 10).unwrap().rides.is_empty());

        state
            .start_gps_only(1_000, Some("pev-1".to_owned()))
            .expect("valid ride starts after rejected metadata");
        let rides = database.list_rides(None, 10).unwrap().rides;
        assert_eq!(rides.len(), 1);
        assert_eq!(rides[0].state, MobileRideLifecycleStateDto::Active);

        database.shutdown().unwrap();
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_keeps_the_first_confirmed_association() {
        let state = MobileRideMapCore::new();
        state
            .start_gps_only(1_000, Some("pev-1".to_owned()))
            .expect("recording starts");
        assert_eq!(
            state
                .observe_vehicle_connection("pev-1".to_owned(), 1_001)
                .expect("matching connection is observed"),
            MobileRideMapCoreAssociationDto::Associated
        );
        assert_eq!(
            state
                .observe_vehicle_connection("pev-2".to_owned(), 1_002)
                .expect("different connection is observed"),
            MobileRideMapCoreAssociationDto::IdentityMismatch
        );
        assert_eq!(
            state
                .current_snapshot(1_002)
                .and_then(|snapshot| snapshot.associated_vehicle),
            Some("pev-1".to_owned())
        );
    }

    #[test]
    fn mobile_ride_map_core_does_not_claim_association_when_storage_fails() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-association-error-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        state
            .start_gps_only(1_000, Some("pev-1".to_owned()))
            .expect("recording starts");
        database.shutdown().expect("database shuts down");

        assert!(matches!(
            state.observe_vehicle_connection("pev-1".to_owned(), 1_001),
            Err(MobileRideMapCoreErrorDto::Storage(_))
        ));
        assert_eq!(
            state
                .current_snapshot(1_001)
                .and_then(|snapshot| snapshot.associated_vehicle),
            None
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_failed_start_does_not_leave_an_orphan_ride() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-start-rollback-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());

        assert!(matches!(
            state.start_gps_only(1_000, Some("   ".to_owned())),
            Err(MobileRideMapCoreErrorDto::Storage(_))
        ));
        assert!(state.current_snapshot(1_000).is_none());
        assert!(
            database
                .list_rides(None, 10)
                .expect("history loads")
                .rides
                .is_empty(),
            "failed start must not publish an active or orphan ride"
        );

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_retains_point_telemetry_provenance_without_backfill() {
        let state = MobileRideMapCore::new();
        state
            .start_gps_only(1_000, Some("pev-1".to_owned()))
            .expect("recording starts");
        state
            .ingest_location(1_001, 1_700_000_001_001, 40.0, -105.0, 3.0)
            .expect("GPS-only point is accepted");
        state
            .observe_vehicle_connection("pev-1".to_owned(), 1_100)
            .expect("vehicle associates");
        let no_telemetry = state
            .ingest_location(1_200, 1_700_000_001_200, 40.0, -105.0, 3.0)
            .expect("associated point is accepted");
        assert!(matches!(
            no_telemetry,
            MobileRideMapCoreDecisionDto::Accepted {
                point: MobileRideMapCorePointDto {
                    telemetry_state: MobileRideMapCoreTelemetryStateDto::AssociatedNoTelemetry,
                    ..
                },
                ..
            }
        ));
        state
            .observe_telemetry(1_300)
            .expect("telemetry observation returns");
        let fresh = state
            .ingest_location(1_400, 1_700_000_001_400, 40.0, -105.0, 3.0)
            .expect("fresh associated point is accepted");
        assert!(matches!(
            fresh,
            MobileRideMapCoreDecisionDto::Accepted {
                point: MobileRideMapCorePointDto {
                    telemetry_state: MobileRideMapCoreTelemetryStateDto::AssociatedFresh,
                    ..
                },
                ..
            }
        ));
        let points = state.points_after(None, 10).unwrap();
        assert_eq!(
            points.points[0].telemetry_state,
            MobileRideMapCoreTelemetryStateDto::GpsOnly
        );
        assert_eq!(
            points.points[1].telemetry_state,
            MobileRideMapCoreTelemetryStateDto::AssociatedNoTelemetry
        );
    }

    #[test]
    fn mobile_ride_map_core_persists_route_and_pages_zero_sequence() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-core-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("map database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        state
            .start_gps_only(1_000, None)
            .expect("map recording starts");
        assert!(matches!(
            state
                .ingest_location(1_000, 1_700_000_000_000, 40.0, -105.0, 3.0)
                .expect("first location is queued"),
            MobileRideMapCoreDecisionDto::Pending { .. }
        ));
        let _ = drain_location_writes(&state);
        state.pause(2_000).expect("map recording pauses");
        state.resume(3_000).expect("map recording resumes");
        state
            .ingest_location(2_001, 1_700_000_002_001, 40.0, -104.999, 3.0)
            .expect("second location is accepted");
        let _ = drain_location_writes(&state);

        let snapshot = state
            .current_snapshot(2_001)
            .expect("active snapshot exists");
        assert_eq!(snapshot.summary.point_count, 2);
        let first = state.points_after(None, 1).unwrap();
        assert_eq!(first.points.len(), 1);
        assert_eq!(first.next_cursor, Some(0));
        assert!(first.has_more);
        let second = state.points_after(first.next_cursor, 1).unwrap();
        assert_eq!(second.points[0].sequence, 1);
        assert_eq!(second.points[0].segment_id, 1);
        assert_eq!(
            second.points[0].start_reason,
            MobileRideSegmentStartReasonDto::Resume
        );
        assert!(!second.has_more);

        let empty = state.points_after(None, 0).unwrap();
        assert!(empty.points.is_empty());
        assert!(!empty.has_more);
        assert!(empty.next_cursor.is_none());

        state.stop(4_000).expect("map recording stops");
        state.save().expect("map recording saves");
        assert!(state.current_snapshot(4_000).is_none());
        let rides = database.list_rides(None, 1).expect("saved ride lists");
        assert!(rides.rides[0].created_at_milliseconds >= 100_000_000_000);
        database.shutdown().expect("map database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_history_summary_exposes_rust_derived_average_speed() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-average-speed-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        state
            .start_gps_only(1_000, None)
            .expect("map recording starts");
        state
            .ingest_location(1_000, 1_700_000_000_000, 40.0, -105.0, 3.0)
            .expect("first location queues");
        let _ = drain_location_writes(&state);
        state
            .ingest_location(2_000, 1_700_000_001_000, 40.0, -104.99999, 3.0)
            .expect("second location queues");
        let _ = drain_location_writes(&state);
        state.stop(3_000).expect("map recording stops");
        state.save().expect("map recording saves");

        let rides = database.list_rides(None, 1).expect("saved ride lists");
        assert_eq!(rides.rides.len(), 1);
        assert!(
            rides.rides[0]
                .summary
                .average_speed_millimetres_per_second
                .is_some()
        );

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_persists_gap_segment_without_distance() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-gap-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        state
            .start_gps_only(1_000, None)
            .expect("map recording starts");
        assert!(matches!(
            state
                .ingest_location(1_000, 1_700_000_000_000, 40.0, -105.0, 3.0)
                .expect("first location is queued"),
            MobileRideMapCoreDecisionDto::Pending { .. }
        ));
        let _ = drain_location_writes(&state);
        let second = state
            .ingest_location(40_000, 1_700_000_039_000, 40.001, -105.0, 3.0)
            .expect("post-gap location is accepted");
        assert!(matches!(
            second,
            MobileRideMapCoreDecisionDto::Pending { .. }
        ));
        let settled = drain_location_writes(&state);
        assert!(
            settled
                .iter()
                .any(|decision| matches!(decision, MobileRideMapCoreDecisionDto::Accepted { .. })),
            "second location did not settle: {settled:?}"
        );
        let snapshot = state
            .current_snapshot(40_000)
            .expect("active snapshot exists");
        assert_eq!(snapshot.segment_count, 2);
        assert!(snapshot.summary.distance_meters.abs() < f64::EPSILON);
        let points = state.points_after(None, 10).expect("route points load");
        assert_eq!(
            points
                .points
                .iter()
                .map(|point| point.segment_id)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        let durable_summary = database
            .summary(MobileRideIdDto {
                value: snapshot.ride_id,
            })
            .expect("durable summary loads");
        assert_eq!(durable_summary.distance_millimetres, 0);
        assert_eq!(durable_summary.average_speed_millimetres_per_second, None);

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_persists_resume_after_a_long_pause() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-long-resume-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        state
            .start_gps_only(1_000, None)
            .expect("map recording starts");
        state
            .ingest_location(1_001, 1_700_000_000_001, 40.0, -105.0, 3.0)
            .expect("first location queues");
        while drain_location_writes(&state).is_empty() {
            thread::yield_now();
        }

        state.pause_at(2_000).expect("ride pauses");
        let resumed_at = 2_000 + ride_maps::MAX_GAP_MILLISECONDS + 1;
        state.resume_at(resumed_at).expect("ride resumes");
        state
            .ingest_location(
                resumed_at + 1,
                1_700_000_000_000 + resumed_at + 1,
                40.001,
                -105.0,
                3.0,
            )
            .expect("resumed location queues");
        while drain_location_writes(&state).is_empty() {
            thread::yield_now();
        }

        let points = state.points_after(None, 10).expect("route points load");
        assert_eq!(points.points.len(), 2);
        assert_eq!(points.points[1].segment_id, 1);
        assert_eq!(
            points.points[1].start_reason,
            MobileRideSegmentStartReasonDto::Resume
        );

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_preserves_a_durable_duplicate_admission() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-admission-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        let snapshot = state
            .start_gps_only(1_000, None)
            .expect("map recording starts");
        let location = MobileRideLocationDto {
            latitude_degrees: 40.0,
            longitude_degrees: -105.0,
            monotonic_milliseconds: 1_001,
            wall_clock_unix_milliseconds: 1_700_000_000_001,
            horizontal_accuracy_millimetres: Some(3_000),
            source: MobileRideSourceDto::Live,
        };
        assert_eq!(
            database
                .append_location(
                    MobileRideIdDto {
                        value: snapshot.ride_id,
                    },
                    location,
                )
                .expect("external durable append succeeds"),
            MobileRideLocationAdmissionDto::Accepted
        );
        assert!(matches!(
            state
                .ingest_location(1_001, 1_700_000_000_001, 40.0, -105.0, 3.0)
                .expect("duplicate admission is queued"),
            MobileRideMapCoreDecisionDto::Pending { .. }
        ));
        let settled = drain_location_writes(&state);
        assert!(settled.iter().any(|decision| matches!(
            decision,
            MobileRideMapCoreDecisionDto::Ignored {
                reason: MobileRideMapDecisionReasonDto::DuplicateLocation,
            }
        )));
        assert!(matches!(
            state
                .ingest_location(2_500, 1_700_000_002_500, 40.001, -105.0, 3.0)
                .expect("post-duplicate location is queued"),
            MobileRideMapCoreDecisionDto::Pending { .. }
        ));
        let settled = drain_location_writes(&state);
        assert!(settled.iter().any(|decision| matches!(
            decision,
            MobileRideMapCoreDecisionDto::Accepted {
                point: MobileRideMapCorePointDto { sequence: 1, .. },
                ..
            }
        )));

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_preserves_missing_accuracy_in_durable_points() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-missing-accuracy-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let state = MobileRideMapCore::with_database(database.clone());
        let snapshot = state
            .start_gps_only(1_000, None)
            .expect("map recording starts");
        database
            .append_location(
                MobileRideIdDto {
                    value: snapshot.ride_id,
                },
                MobileRideLocationDto {
                    latitude_degrees: 40.0,
                    longitude_degrees: -105.0,
                    monotonic_milliseconds: 1_001,
                    wall_clock_unix_milliseconds: 1_700_000_000_001,
                    horizontal_accuracy_millimetres: None,
                    source: MobileRideSourceDto::Live,
                },
            )
            .expect("external durable append succeeds");

        let points = state.points_after(None, 10).expect("route points load");
        assert_eq!(points.points.len(), 1);
        assert_eq!(points.points[0].horizontal_accuracy_meters, None);

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_restores_an_interrupted_route_after_reopen() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-recovery-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        {
            let database =
                open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
            let state = MobileRideMapCore::with_database(database.clone());
            state
                .start_gps_only(1_000, Some("pev-1".to_owned()))
                .expect("map recording starts");
            for index in 0..(ride_maps::MAX_LIVE_ROUTE_POINTS as u64 + 2) {
                state
                    .ingest_location(
                        1_000 + index * 1_000,
                        1_700_000_000_000 + index * 1_000,
                        40.0 + (f64::from(u32::try_from(index).expect("bounded")) * 0.000_01),
                        -105.0,
                        3.0,
                    )
                    .expect("route point is accepted");
                let _ = drain_location_writes(&state);
            }
            database.shutdown().expect("database shuts down");
        }

        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database reopens");
        let state = MobileRideMapCore::with_database(database.clone());
        let snapshot = state
            .current_snapshot(5_100_000)
            .expect("interrupted ride restores");
        assert_eq!(snapshot.state, MobileRideLifecycleStateDto::Interrupted);
        assert_eq!(
            snapshot.summary.point_count,
            ride_maps::MAX_LIVE_ROUTE_POINTS as u64 + 2
        );
        let restored = state.inner.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            restored.recorder.points().len(),
            ride_maps::MAX_LIVE_ROUTE_POINTS
        );
        assert_eq!(restored.recorder.first_point_sequence().as_u64(), 2);
        assert_eq!(
            restored
                .recorder
                .points()
                .last()
                .unwrap()
                .sample()
                .monotonic_milliseconds()
                .as_u64(),
            1_000 + 4_097 * 1_000
        );
        drop(restored);
        let rides = database.list_rides(None, 10).expect("recovered ride lists");
        assert_eq!(rides.rides.len(), 1);
        assert_eq!(rides.rides[0].candidate_vehicle.as_deref(), Some("pev-1"));
        assert_eq!(rides.rides[0].duration_milliseconds, 4_097_000);

        database.shutdown().expect("reopened database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_restores_recoverable_ride_beyond_history_page() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-recovery-page-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        let recoverable = database
            .create_ride(MobileRideSourceDto::Live, 1_000)
            .expect("recoverable ride creates");
        database
            .transition(recoverable.clone(), MobileRideEventDto::Start, 1_000)
            .expect("recoverable ride starts");
        for timestamp in 2_000..=2_050 {
            let ride = database
                .create_ride(MobileRideSourceDto::Live, timestamp)
                .expect("newer ride creates");
            database
                .transition(ride.clone(), MobileRideEventDto::Start, timestamp)
                .expect("newer ride starts");
            database
                .transition(ride.clone(), MobileRideEventDto::Stop, timestamp + 1)
                .expect("newer ride stops");
            database
                .transition(ride, MobileRideEventDto::Save, timestamp + 2)
                .expect("newer ride saves");
        }
        database.shutdown().expect("database shuts down");

        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database reopens");
        let state = MobileRideMapCore::with_database(database.clone());
        assert!(
            state.current_snapshot(2_051).is_some(),
            "restore failed: {:?}",
            state.initialization_error()
        );
        assert_eq!(
            state
                .current_snapshot(2_051)
                .map(|snapshot| snapshot.ride_id),
            Some(recoverable.value)
        );

        database.shutdown().expect("reopened database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_ride_map_core_reports_storage_failure_during_restore() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-recovery-error-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
        database.shutdown().expect("database shuts down");

        let state = MobileRideMapCore::with_database(database);
        assert!(matches!(
            state.initialization_error(),
            Some(MobileRideMapCoreErrorDto::Storage(_))
        ));

        let _ = fs::remove_file(path);
    }
    #[test]
    fn mobile_ride_map_core_restores_pause_excluded_duration_after_reopen() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-map-pause-recovery-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        {
            let database =
                open_ride_database(path.to_string_lossy().into_owned()).expect("database opens");
            let state = MobileRideMapCore::with_database(database.clone());
            state
                .start_gps_only(1_000, None)
                .expect("map recording starts");
            state.pause_at(5_000).expect("recording pauses");
            state.resume_at(7_000).expect("recording resumes");
            let pending = state
                .ingest_location(8_000, 1_700_000_008_000, 40.0, -105.0, 3.0)
                .expect("route point queues");
            assert!(matches!(
                pending,
                MobileRideMapCoreDecisionDto::Pending { .. }
            ));
            let decisions = drain_location_writes(&state);
            assert!(
                decisions.iter().any(|decision| matches!(
                    decision,
                    MobileRideMapCoreDecisionDto::Accepted { .. }
                )),
                "{decisions:?}"
            );
            let live_rides = database.list_rides(None, 10).expect("live ride lists");
            assert_eq!(live_rides.rides[0].duration_milliseconds, 5_000);
            database.shutdown().expect("database shuts down");
        }

        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("database reopens");
        let state = MobileRideMapCore::with_database(database.clone());
        let snapshot = state
            .current_snapshot(8_000)
            .expect("interrupted ride restores");
        assert_eq!(snapshot.state, MobileRideLifecycleStateDto::Interrupted);
        assert_eq!(snapshot.summary.duration_milliseconds, 5_000);
        let rides = database.list_rides(None, 10).expect("recovered ride lists");
        assert_eq!(rides.rides[0].duration_milliseconds, 5_000);
        assert_eq!(rides.rides[0].paused_duration_milliseconds, 2_000);

        database.shutdown().expect("reopened database shuts down");
        let _ = fs::remove_file(path);
    }
    #[test]
    fn location_batch_preserves_source_order_and_receipt_anchor() {
        let state = MobileRideMapCore::new();
        state
            .start_gps_only(9_000, None)
            .expect("GPS-only recording starts");

        let sample = |wall_clock_unix_ms, latitude_degrees| MobilePhoneLocationSampleDto {
            wall_clock_unix_ms,
            latitude_degrees,
            longitude_degrees: -105.0,
            altitude_meters: 1_600.0,
            horizontal_accuracy_meters: Some(3.0),
            vertical_accuracy_meters: None,
            speed_meters_per_second: None,
            speed_accuracy_meters_per_second: None,
            course_degrees: None,
            course_accuracy_degrees: None,
        };
        let decisions = state
            .ingest_location_batch(
                10_000,
                1_700_000_001_000,
                vec![
                    sample(1_700_000_000_000, 40.0),
                    sample(1_700_000_001_000, 40.00001),
                    // Future source timestamps are dropped before admission.
                    sample(1_700_000_002_000, 40.00002),
                ],
            )
            .expect("batch admission succeeds");

        assert_eq!(decisions.len(), 2);
        assert!(matches!(
            decisions[0],
            MobileRideMapCoreDecisionDto::Accepted { .. }
        ));
        assert!(matches!(
            decisions[1],
            MobileRideMapCoreDecisionDto::Accepted { .. }
        ));
        let accepted_points = decisions
            .iter()
            .map(|decision| match decision {
                MobileRideMapCoreDecisionDto::Accepted { point, .. } => point,
                _ => panic!("expected accepted decision"),
            })
            .collect::<Vec<_>>();
        assert_eq!(accepted_points[0].wall_clock_unix_ms, 1_700_000_000_000);
        assert_eq!(accepted_points[0].monotonic_ms, 9_000);
        assert_eq!(accepted_points[1].wall_clock_unix_ms, 1_700_000_001_000);
        assert_eq!(accepted_points[1].monotonic_ms, 10_000);
    }

    #[test]
    fn location_batch_without_active_ride_is_ignored() {
        let state = MobileRideMapCore::new();
        let decisions = state
            .ingest_location_batch(
                10_000,
                1_700_000_000_000,
                vec![MobilePhoneLocationSampleDto {
                    wall_clock_unix_ms: 1_700_000_000_000,
                    latitude_degrees: 40.0,
                    longitude_degrees: -105.0,
                    altitude_meters: 1_600.0,
                    horizontal_accuracy_meters: Some(3.0),
                    vertical_accuracy_meters: None,
                    speed_meters_per_second: None,
                    speed_accuracy_meters_per_second: None,
                    course_degrees: None,
                    course_accuracy_degrees: None,
                }],
            )
            .expect("no-active-ride batches are ignored");

        assert!(decisions.is_empty());
    }
    #[test]
    fn spatial_pages_round_trip_mobile_cursors_and_antimeridian_bounds() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-mobile-spatial-{}-{}.sqlite3",
            std::process::id(),
            thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_file(&path);
        let database =
            open_ride_database(path.to_string_lossy().into_owned()).expect("mobile database opens");
        database
            .create_map_point(
                "east".into(),
                MobileMapCoordinateDto {
                    latitude_degrees: 0.0,
                    longitude_degrees: 179.5,
                },
            )
            .expect("east point is stored");
        database
            .create_map_point(
                "west".into(),
                MobileMapCoordinateDto {
                    latitude_degrees: 0.0,
                    longitude_degrees: -179.5,
                },
            )
            .expect("west point is stored");

        let bounds = MobileGeoBoundsDto {
            minimum_latitude_degrees: -1.0,
            maximum_latitude_degrees: 1.0,
            minimum_longitude_degrees: 179.0,
            maximum_longitude_degrees: -179.0,
        };
        let first = database
            .map_points_in_bounds(bounds, None, 1)
            .expect("first page is returned");
        assert_eq!(first.points.len(), 1);
        let second = database
            .map_points_in_bounds(bounds, first.next_cursor, 1)
            .expect("cursor returns the second page");
        assert_eq!(second.points.len(), 1);
        assert_ne!(first.points[0].id, second.points[0].id);
        assert!(second.next_cursor.is_none());

        database.shutdown().expect("database shuts down");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mobile_pevcap_requires_preview_confirmation_and_reports_capture_only() {
        let _guard = RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let database_path =
            std::env::temp_dir().join(format!("cutout-mobile-pevcap-{}.sqlite3", Uuid::new_v4()));
        let artifact_path =
            std::env::temp_dir().join(format!("cutout-mobile-pevcap-{}.jsonl", Uuid::new_v4()));
        let header = PevcapHeader::new(
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "darwin",
            None,
            &[],
            &[],
            None,
            None,
            "test",
            [0; 32],
            &[],
        )
        .unwrap();
        let capture = PevcapCapture::new(
            header,
            vec![PevcapRecord::link_up(MonotonicTimestamp::new(1), None)],
        );
        fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();
        let database = open_ride_database(database_path.to_string_lossy().into_owned()).unwrap();

        let preview = database
            .preflight_pevcap(
                artifact_path.to_string_lossy().into_owned(),
                MobilePevcapEncodingDto::Jsonl,
            )
            .unwrap();
        assert_eq!(preview.outcome, MobilePevcapImportOutcomeDto::CaptureOnly);
        assert_eq!(
            preview.warnings,
            vec![MobilePevcapImportWarningDto::NoRouteLocations]
        );
        let receipt = database
            .confirm_pevcap_import(preview, 1_700_000_000_000)
            .unwrap();
        assert_eq!(receipt.ride_id, None);
        assert_eq!(receipt.outcome, MobilePevcapImportOutcomeDto::CaptureOnly);
        let managed_path = PathBuf::from(&receipt.managed_artifact_path);
        assert!(managed_path.exists());

        database.shutdown().unwrap();
        let _ = fs::remove_file(&managed_path);
        let _ = fs::remove_dir(managed_path.parent().unwrap());
        let _ = fs::remove_file(database_path);
        let _ = fs::remove_file(artifact_path);
    }
}
