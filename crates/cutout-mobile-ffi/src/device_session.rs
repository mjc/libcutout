//! Generic device-session projection; protocol selection stays below the FFI boundary.

use cutout_protocols::{DeviceConnectionSnapshot, DeviceConnectionStep, VehicleKind};

use crate::{
    CutoutSessionStateHandle, DeviceDetectionEvent, DeviceDetectionResolutionRecord,
    MobileConnectionAttemptSnapshotDto, MobileConnectionAttemptTokenDto, MobileGattFingerprintDto,
    MobileParserDiagnosticsDto, MobileProtocolFamilyDto, MobileSessionInputDto,
    MobileSessionStepResultDto, MobileTelemetrySnapshotDto,
};

/// Native drivetrain form; Rust owns parsing and conversion.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileVescDrivetrainDraft {
    /// Wheel diameter in millimetres.
    pub wheel_diameter_mm: String,
    /// Positive whole motor pole-pair count.
    pub motor_pole_pairs: String,
    /// Motor revolutions per wheel revolution.
    pub motor_to_wheel_ratio: String,
}
impl From<cutout_core::VescDrivetrain> for MobileVescDrivetrainDraft {
    fn from(value: cutout_core::VescDrivetrain) -> Self {
        Self {
            wheel_diameter_mm: value.diameter_text(),
            motor_pole_pairs: value.pole_pairs_text(),
            motor_to_wheel_ratio: value.reduction_text(),
        }
    }
}
impl From<cutout_core::VescDrivetrainProfile> for MobileVescDrivetrainDraft {
    fn from(value: cutout_core::VescDrivetrainProfile) -> Self {
        value.value().into()
    }
}

/// Origin of a local calculation profile, independent of hardware verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileVescDrivetrainSourceDto {
    /// The rider explicitly supplied local geometry.
    UserConfigured,
}

/// Immutable effective geometry associated with one connection's telemetry.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record, serde::Serialize)]
pub struct MobileVescDrivetrainContextDto {
    /// Attempt that produced this context; revisions are scoped to its device.
    pub connection_attempt: MobileConnectionAttemptTokenDto,
    /// Committed, nonzero per-device profile revision.
    pub revision: u64,
    /// User input is not a controller readback or a capability grant.
    pub source: MobileVescDrivetrainSourceDto,
    /// Entered wheel diameter in micrometres.
    pub wheel_diameter_micrometres: u32,
    /// Effective circumference used by the decoder, in millimetres.
    pub wheel_circumference_mm: u64,
    /// Motor pole pairs, distinct from pole count.
    pub motor_pole_pairs: u8,
    /// Motor revolutions per wheel revolution, in thousandths.
    pub motor_to_wheel_ratio_milli: u32,
}

impl MobileVescDrivetrainContextDto {
    fn from_profile(
        profile: cutout_core::VescDrivetrainProfile,
        token: &cutout_core::ConnectionAttemptToken,
    ) -> Self {
        let value = profile.value();
        let geometry = value.geometry();
        Self {
            connection_attempt: token.into(),
            revision: profile.revision().get(),
            source: match profile.source() {
                cutout_core::VescDrivetrainSource::UserConfigured => {
                    MobileVescDrivetrainSourceDto::UserConfigured
                }
            },
            wheel_diameter_micrometres: value.diameter_micrometres(),
            wheel_circumference_mm: geometry.wheel_circumference.as_millimetres(),
            motor_pole_pairs: geometry.motor_pole_pairs.get(),
            motor_to_wheel_ratio_milli: geometry.gear_ratio_denominator.as_milli(),
        }
    }
}
/// Failure of a local geometry operation; no controller write is performed.
#[derive(Debug, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileVescDrivetrainError {
    /// Invalid diameter text or representability.
    #[error("invalid wheel diameter")]
    InvalidWheelDiameter,
    /// Pole pairs must be a whole number from 1 through 255.
    #[error("invalid motor pole pairs")]
    InvalidMotorPolePairs,
    /// Invalid positive thousandths reduction.
    #[error("invalid reduction")]
    InvalidReduction,
    /// Attempt was replaced or disconnected.
    #[error("connection unavailable")]
    ConnectionUnavailable,
    /// Only standard VESC accepts user drivetrain configuration.
    #[error("unsupported device")]
    UnsupportedDevice,
    /// No durable database was configured.
    #[error("persistence unavailable")]
    PersistenceUnavailable,
    /// Database did not commit or load the requested geometry.
    #[error("drivetrain persistence failed: {error}")]
    Persistence {
        /// Storage failure.
        error: crate::MobileRideDatabaseError,
    },
}

impl From<cutout_core::VescDrivetrainError> for MobileVescDrivetrainError {
    fn from(value: cutout_core::VescDrivetrainError) -> Self {
        match value {
            cutout_core::VescDrivetrainError::WheelDiameter => Self::InvalidWheelDiameter,
            cutout_core::VescDrivetrainError::MotorPolePairs => Self::InvalidMotorPolePairs,
            cutout_core::VescDrivetrainError::Reduction => Self::InvalidReduction,
        }
    }
}

/// Native layout category, independent from the controller protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVehicleKindDto {
    /// Physical vehicle kind has not been established.
    Unknown,
    /// Electric unicycle.
    ElectricUnicycle,
    /// Single-wheel self-balancing board.
    Onewheel,
    /// Scooter.
    Scooter,
    /// Electric skateboard.
    Eskate,
    /// Electric bicycle.
    Ebike,
    /// Electric motorcycle.
    Emoto,
}

impl From<VehicleKind> for MobileVehicleKindDto {
    fn from(value: VehicleKind) -> Self {
        match value {
            VehicleKind::Unknown => Self::Unknown,
            VehicleKind::ElectricUnicycle => Self::ElectricUnicycle,
            VehicleKind::Onewheel => Self::Onewheel,
            VehicleKind::Scooter => Self::Scooter,
            VehicleKind::Eskate => Self::Eskate,
            VehicleKind::Ebike => Self::Ebike,
            VehicleKind::Emoto => Self::Emoto,
        }
    }
}

impl From<MobileVehicleKindDto> for VehicleKind {
    fn from(value: MobileVehicleKindDto) -> Self {
        match value {
            MobileVehicleKindDto::Unknown => Self::Unknown,
            MobileVehicleKindDto::ElectricUnicycle => Self::ElectricUnicycle,
            MobileVehicleKindDto::Onewheel => Self::Onewheel,
            MobileVehicleKindDto::Scooter => Self::Scooter,
            MobileVehicleKindDto::Eskate => Self::Eskate,
            MobileVehicleKindDto::Ebike => Self::Ebike,
            MobileVehicleKindDto::Emoto => Self::Emoto,
        }
    }
}

/// Evidence behind the device's physical type; this does not identify its model.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVehicleClassificationProvenanceDto {
    /// Physical type is unknown.
    Unknown,
    /// Validated protocol semantics establish the type.
    Protocol,
    /// User selection saved for this device.
    UserProvided,
}

/// Rust-owned physical-type admission for the native Ride presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVehicleClassificationRequirementDto {
    /// Application discovery is in progress.
    Pending,
    /// This device requires a user choice before showing Ride.
    UserSelectionRequired,
    /// The physical vehicle presentation is resolved.
    Resolved,
}

/// Result of the bounded VESC application discovery, independent from vehicle type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVescApplicationDiscoveryDto {
    /// The bounded application query is still in progress.
    Pending,
    /// A validated Refloat response established the application.
    Refloat,
    /// Non-Refloat INFO or no response within a fully submitted query's response interval.
    Absent,
}

/// Typed native submission admission carried with one application query write.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileApplicationQueryWriteDto {
    /// Query identity scoped by the containing connection attempt.
    pub query_id: u64,
    /// Exclusive deadline for actual native submission.
    pub submit_before_ms: u64,
}

impl From<cutout_core::ApplicationQueryWrite> for MobileApplicationQueryWriteDto {
    fn from(value: cutout_core::ApplicationQueryWrite) -> Self {
        Self {
            query_id: value.id.get(),
            submit_before_ms: value.submit_before.get(),
        }
    }
}

/// Native effect receipt for an indexed application-query fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileApplicationQueryTransportStatusDto {
    /// Queued without native submission.
    Queued,
    /// Submitted to the native peripheral API.
    Submitted,
    /// Rejected before native submission.
    Rejected,
    /// Cancelled before native submission.
    Cancelled,
}

impl From<MobileApplicationQueryTransportStatusDto>
    for cutout_core::ApplicationQueryTransportStatus
{
    fn from(value: MobileApplicationQueryTransportStatusDto) -> Self {
        match value {
            MobileApplicationQueryTransportStatusDto::Queued => Self::Queued,
            MobileApplicationQueryTransportStatusDto::Submitted => Self::Submitted,
            MobileApplicationQueryTransportStatusDto::Rejected => Self::Rejected,
            MobileApplicationQueryTransportStatusDto::Cancelled => Self::Cancelled,
        }
    }
}

/// Query delivery evidence independent from installed-application evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVescApplicationQueryStateDto {
    /// No query has been emitted.
    NotRequested,
    /// Waiting for native submission within a finite admission interval.
    Queued {
        /// Attempt-local query identity.
        query_id: u64,
        /// Exclusive native submission deadline.
        submit_before_ms: u64,
    },
    /// All fragments submitted; waiting for a validated response.
    AwaitingResponse {
        /// Attempt-local query identity.
        query_id: u64,
        /// Native submission time of the final fragment.
        submitted_at_ms: u64,
        /// Response expiration boundary.
        response_deadline_ms: u64,
    },
    /// No bytes were submitted; reconnect is required to restart discovery.
    KnownUnsent {
        /// Retired query identity.
        query_id: u64,
    },
    /// Some bytes may have reached the controller; reconnect is required.
    PartialUncertain {
        /// Retired query identity.
        query_id: u64,
    },
    /// A fully submitted query's response interval elapsed; its tombstone is retained.
    ResponseExpired {
        /// Submitted query identity retained until link reset.
        query_id: u64,
    },
    /// A validated response settled the current fully submitted query.
    Resolved,
}

impl From<cutout_protocols::VescApplicationQueryState> for MobileVescApplicationQueryStateDto {
    fn from(value: cutout_protocols::VescApplicationQueryState) -> Self {
        use cutout_protocols::VescApplicationQueryState;
        match value {
            VescApplicationQueryState::NotRequested => Self::NotRequested,
            VescApplicationQueryState::Queued {
                query_id,
                submit_before_ms,
            } => Self::Queued {
                query_id,
                submit_before_ms,
            },
            VescApplicationQueryState::AwaitingResponse {
                query_id,
                submitted_at_ms,
                response_deadline_ms,
            } => Self::AwaitingResponse {
                query_id,
                submitted_at_ms,
                response_deadline_ms,
            },
            VescApplicationQueryState::KnownUnsent { query_id } => Self::KnownUnsent { query_id },
            VescApplicationQueryState::PartialUncertain { query_id } => {
                Self::PartialUncertain { query_id }
            }
            VescApplicationQueryState::ResponseExpired { query_id } => {
                Self::ResponseExpired { query_id }
            }
            VescApplicationQueryState::Resolved => Self::Resolved,
        }
    }
}

/// Failure to durably attach a physical type to the intended VESC connection.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileVehicleSelectionError {
    /// The attempt, physical type or application-discovery state does not permit a choice.
    #[error("connection does not permit vehicle type selection")]
    ConnectionUnavailable,
    /// This session has no existing database owner.
    #[error("device persistence is unavailable")]
    PersistenceUnavailable,
    /// The existing database rejected the update; classification remains unchanged.
    #[error("vehicle type persistence failed: {error}")]
    Persistence {
        /// Typed database failure.
        error: crate::MobileRideDatabaseError,
    },
}

impl From<cutout_protocols::VescApplicationDiscovery> for MobileVescApplicationDiscoveryDto {
    fn from(value: cutout_protocols::VescApplicationDiscovery) -> Self {
        match value {
            cutout_protocols::VescApplicationDiscovery::Pending => Self::Pending,
            cutout_protocols::VescApplicationDiscovery::Refloat => Self::Refloat,
            cutout_protocols::VescApplicationDiscovery::Absent => Self::Absent,
        }
    }
}

impl From<cutout_core::VehicleClassificationProvenance>
    for MobileVehicleClassificationProvenanceDto
{
    fn from(value: cutout_core::VehicleClassificationProvenance) -> Self {
        use cutout_core::VehicleClassificationProvenance;
        match value {
            VehicleClassificationProvenance::Unknown => Self::Unknown,
            VehicleClassificationProvenance::Protocol => Self::Protocol,
            VehicleClassificationProvenance::UserProvided => Self::UserProvided,
        }
    }
}

impl From<cutout_core::VehicleClassificationRequirement>
    for MobileVehicleClassificationRequirementDto
{
    fn from(value: cutout_core::VehicleClassificationRequirement) -> Self {
        use cutout_core::VehicleClassificationRequirement;
        match value {
            VehicleClassificationRequirement::Pending => Self::Pending,
            VehicleClassificationRequirement::UserSelectionRequired => Self::UserSelectionRequired,
            VehicleClassificationRequirement::Resolved => Self::Resolved,
        }
    }
}

/// Canonical physical types that may be selected for a specific device.
#[uniffi::export]
#[must_use]
pub fn user_selectable_vehicle_kinds() -> Vec<MobileVehicleKindDto> {
    VehicleKind::USER_SELECTABLE
        .into_iter()
        .map(Into::into)
        .collect()
}

/// Verified protocol and independently known model/kind.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceIdentityDto {
    /// Wire protocol established by the detector.
    pub protocol: MobileProtocolFamilyDto,
    /// Native presentation template.
    pub vehicle_kind: MobileVehicleKindDto,
    /// Evidence behind the physical type.
    pub classification_provenance: MobileVehicleClassificationProvenanceDto,
    /// Whether application discovery or a per-device user choice remains outstanding.
    pub classification_requirement: MobileVehicleClassificationRequirementDto,
    /// Application proof kept separate from the physical type and model.
    pub vesc_application: Option<MobileVescApplicationDiscoveryDto>,
    /// Native query progress, deadlines and failures for snapshot equality/publication.
    pub vesc_application_query: Option<MobileVescApplicationQueryStateDto>,
    /// Exact model label, absent when only the protocol is verified.
    pub model: Option<String>,
}

/// Identity and connection admission observed together.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceSessionSnapshotDto {
    /// Immutable attempt state.
    pub connection: MobileConnectionAttemptSnapshotDto,
    /// Verified identity, absent for pending and capture-only attempts.
    pub identity: Option<MobileDeviceIdentityDto>,
}

/// Capture admission evidence established by the Rust decoder and connection owner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureNotificationEvidenceDto {
    /// The notification must retain its complete observation evidence.
    #[default]
    Unclassified,
    /// One complete telemetry frame supplied fresh stationary wheel speed.
    StationaryTelemetry,
}

/// Decoded result paired with the exact connection that produced it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileDeviceSessionStepDto {
    /// Identity and admission captured under the same lock as decoding.
    pub session: MobileDeviceSessionSnapshotDto,
    /// Ordered protocol outputs and a typed failure, if any.
    pub result: MobileSessionStepResultDto,
    /// Current normalized telemetry.
    pub telemetry: MobileTelemetrySnapshotDto,
    /// Current parser diagnostics.
    pub diagnostics: MobileParserDiagnosticsDto,
    /// Rust-established admission evidence for this exact notification.
    pub capture_notification_evidence: MobileCaptureNotificationEvidenceDto,
}

impl From<DeviceConnectionSnapshot> for MobileDeviceSessionSnapshotDto {
    fn from(value: DeviceConnectionSnapshot) -> Self {
        MobileDeviceSessionSnapshotDto {
            connection: (&value.connection).into(),
            identity: value.identity.map(|identity| MobileDeviceIdentityDto {
                protocol: cutout_core::ProtocolFamilyDto::from(identity.protocol).into(),
                vehicle_kind: identity.vehicle_kind().into(),
                classification_provenance: identity.classification.provenance().into(),
                classification_requirement: identity.classification_requirement.into(),
                vesc_application: identity.vesc_application.map(Into::into),
                vesc_application_query: identity.vesc_application_query.map(Into::into),
                model: identity.model.map(|model| model.model.to_string()),
            }),
        }
    }
}

impl From<DeviceConnectionStep> for MobileDeviceSessionStepDto {
    fn from(value: DeviceConnectionStep) -> Self {
        let context = value
            .vesc_drivetrain_context
            .zip(value.session.connection.token.as_ref())
            .map(|(profile, token)| MobileVescDrivetrainContextDto::from_profile(profile, token));
        let mut telemetry: MobileTelemetrySnapshotDto = value.telemetry.into();
        telemetry.vesc_drivetrain_context = context;
        Self {
            session: value.session.into(),
            result: cutout_protocols::ConcreteSessionStepResultDto::from(value.result).into(),
            telemetry,
            diagnostics: value.diagnostics.into(),
            capture_notification_evidence: MobileCaptureNotificationEvidenceDto::Unclassified,
        }
    }
}

#[uniffi::export]
#[allow(clippy::needless_pass_by_value)]
impl CutoutSessionStateHandle {
    /// Revalidates an application-query fragment immediately before native submission.
    pub fn application_query_transport_is_current(
        &self,
        token: MobileConnectionAttemptTokenDto,
        query_id: u64,
        monotonic_ms: u64,
    ) -> bool {
        self.lock_inner().application_query_transport_is_current(
            &token.into(),
            query_id,
            cutout_core::MonotonicTimestamp::new(monotonic_ms),
        )
    }

    /// Records a native fragment receipt; only final complete submission starts response time.
    pub fn mark_application_query_transport(
        &self,
        token: MobileConnectionAttemptTokenDto,
        query_id: u64,
        fragment_index: u16,
        fragment_count: u16,
        status: MobileApplicationQueryTransportStatusDto,
        monotonic_ms: u64,
    ) -> bool {
        self.lock_inner().mark_application_query_transport(
            &token.into(),
            query_id,
            fragment_index,
            fragment_count,
            status.into(),
            cutout_core::MonotonicTimestamp::new(monotonic_ms),
        )
    }

    /// Advances discovery deadlines without producing native writes under backpressure.
    pub fn tick_application_query_transport(
        &self,
        token: MobileConnectionAttemptTokenDto,
        monotonic_ms: u64,
    ) -> bool {
        self.lock_inner().tick_application_query_transport(
            &token.into(),
            cutout_core::MonotonicTimestamp::new(monotonic_ms),
        )
    }

    /// Observes bytes only for the attempt that owned the native callback.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports own byte buffers."
    )]
    pub fn observe_connection_notification(
        &self,
        token: MobileConnectionAttemptTokenDto,
        bytes: Vec<u8>,
    ) -> Option<DeviceDetectionResolutionRecord> {
        self.observe_connection_event(token, DeviceDetectionEvent::Notification { bytes: &bytes })
    }

    /// Feeds one complete GATT inventory to the authoritative detector.
    pub fn observe_connection_gatt(
        &self,
        token: MobileConnectionAttemptTokenDto,
        fingerprints: Vec<MobileGattFingerprintDto>,
    ) -> Option<DeviceDetectionResolutionRecord> {
        let fingerprints = fingerprints.into_iter().map(Into::into).collect::<Vec<_>>();
        self.observe_connection_event(
            token,
            DeviceDetectionEvent::Gatt {
                gatt: &fingerprints,
            },
        )
    }

    /// Retains the selected advertisement as a hint scoped to its attempt.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports own optional byte buffers."
    )]
    pub fn observe_connection_advertisement(
        &self,
        token: MobileConnectionAttemptTokenDto,
        name: Option<Vec<u8>>,
    ) -> Option<DeviceDetectionResolutionRecord> {
        self.observe_connection_event(
            token,
            DeviceDetectionEvent::Advertisement {
                name: name.as_deref(),
            },
        )
    }

    /// Saves local geometry and applies it only to the same standard VESC attempt.
    ///
    /// Future telemetry uses the saved geometry; historical distance is not reconstructed.
    ///
    /// # Errors
    /// Returns field validation, connection admission or durable storage failure.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI receives owned form strings"
    )]
    pub fn save_connection_vesc_drivetrain(
        &self,
        token: MobileConnectionAttemptTokenDto,
        draft: MobileVescDrivetrainDraft,
        updated_at_ms: u64,
    ) -> Result<MobileVescDrivetrainDraft, MobileVescDrivetrainError> {
        let value = cutout_core::VescDrivetrain::parse(
            &draft.wheel_diameter_mm,
            &draft.motor_pole_pairs,
            &draft.motor_to_wheel_ratio,
        )
        .map_err(MobileVescDrivetrainError::from)?;
        self.save_drivetrain_with_persistence(token, value, |selection| {
            self.diagnostic_database
                .as_ref()
                .ok_or(MobileVescDrivetrainError::PersistenceUnavailable)?
                .save_user_vesc_drivetrain(
                    selection.platform_identifier(),
                    selection.drivetrain(),
                    updated_at_ms,
                )
                .map_err(|error| MobileVescDrivetrainError::Persistence {
                    error: crate::map_ride_database_error(error),
                })
        })
    }

    /// Copies canonical telemetry for this exact selected connection without observing a sample.
    #[must_use]
    pub fn connection_telemetry_snapshot(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> Option<MobileTelemetrySnapshotDto> {
        let inner = self.lock_inner();
        let token = token.into();
        if inner.snapshot().connection.token.as_ref() != Some(&token) {
            return None;
        }
        let mut snapshot: MobileTelemetrySnapshotDto = inner.telemetry_snapshot()?.into();
        snapshot.vesc_drivetrain_context = inner
            .vesc_drivetrain_context(&token)
            .map(|profile| MobileVescDrivetrainContextDto::from_profile(profile, &token));
        Some(snapshot)
    }

    /// Returns this attempt's editable user geometry.
    #[must_use]
    pub fn connection_vesc_drivetrain(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> Option<MobileVescDrivetrainDraft> {
        self.lock_inner()
            .user_vesc_drivetrain(&token.into())
            .map(Into::into)
    }

    /// Loads saved local geometry without overwriting an edit made during the load.
    ///
    /// # Errors
    /// Returns stale connection, unsupported device or storage failure.
    pub fn restore_connection_vesc_drivetrain(
        &self,
        token: MobileConnectionAttemptTokenDto,
    ) -> Result<Option<MobileVescDrivetrainDraft>, MobileVescDrivetrainError> {
        self.restore_drivetrain_with_load(token, |identifier| {
            self.diagnostic_database
                .as_ref()
                .ok_or(MobileVescDrivetrainError::PersistenceUnavailable)?
                .user_vesc_drivetrain(identifier)
                .map_err(|error| MobileVescDrivetrainError::Persistence {
                    error: crate::map_ride_database_error(error),
                })
        })
    }

    /// Attaches saved controller geometry to the pending attempt without asserting identity.
    pub fn configure_connection_vesc_profile(
        &self,
        token: MobileConnectionAttemptTokenDto,
        profile: crate::VescBoardProfile,
    ) -> bool {
        self.lock_inner()
            .configure_vesc_board_profile(&token.into(), profile.into())
    }

    /// Associates a physical type with this attempt without asserting a model or authorizing writes.
    pub fn configure_connection_vehicle_kind(
        &self,
        token: MobileConnectionAttemptTokenDto,
        kind: MobileVehicleKindDto,
    ) -> bool {
        self.lock_inner()
            .configure_vehicle_kind(&token.into(), kind.into())
    }

    /// Applies a delayed saved association only if this attempt has no newer choice.
    pub fn restore_connection_vehicle_kind(
        &self,
        token: MobileConnectionAttemptTokenDto,
        kind: MobileVehicleKindDto,
    ) -> bool {
        self.lock_inner()
            .restore_vehicle_kind(&token.into(), kind.into())
    }

    /// Persists and applies a user choice for the captured connection attempt.
    ///
    /// # Errors
    /// Returns a typed refusal or persistence failure without changing classification.
    pub fn select_connection_vehicle_kind(
        &self,
        token: MobileConnectionAttemptTokenDto,
        kind: MobileVehicleKindDto,
        updated_at_ms: u64,
    ) -> Result<MobileDeviceSessionSnapshotDto, MobileVehicleSelectionError> {
        self.select_vehicle_kind_with_persistence(token, kind, |selection| {
            let database = self
                .diagnostic_database
                .as_ref()
                .ok_or(MobileVehicleSelectionError::PersistenceUnavailable)?;
            database
                .save_user_vehicle_kind(
                    selection.platform_identifier(),
                    selection.kind(),
                    updated_at_ms,
                )
                .map_err(|error| MobileVehicleSelectionError::Persistence {
                    error: crate::map_ride_database_error(error),
                })
        })
    }

    /// Returns the protocol-selected session without exposing a model constructor.
    #[must_use]
    pub fn device_session_snapshot(&self) -> MobileDeviceSessionSnapshotDto {
        self.lock_inner().snapshot().into()
    }

    /// Completes detection from retained Rust evidence; terminal attempts cannot promote.
    pub fn resolve_device_session(
        &self,
        token: MobileConnectionAttemptTokenDto,
        identification_complete: bool,
        now_ms: u64,
    ) -> MobileDeviceSessionSnapshotDto {
        let mut inner = self.lock_inner();
        inner.resolve(
            &token.into(),
            identification_complete,
            crate::MonotonicTimestamp::new(now_ms),
        );
        inner.snapshot().into()
    }

    /// Computes the complete Rust-owned admission candidate from retained evidence.
    /// Swift consumes this projection; it does not reconstruct route or model policy.
    pub fn connection_admission_candidate(
        &self,
        platform_identifier: String,
        display_name: String,
        allow_closest_match: bool,
    ) -> crate::DiscoveryCandidate {
        let inner = self.lock_inner();
        let resolution: DeviceDetectionResolutionRecord =
            inner.detector().resolution(inner.session_state()).into();
        if allow_closest_match {
            crate::mobile_discovery_candidate_from_closest_detection_resolution(
                platform_identifier,
                display_name,
                resolution,
            )
        } else {
            crate::mobile_discovery_candidate_from_detection_resolution(
                platform_identifier,
                display_name,
                resolution,
            )
        }
    }

    /// Decodes only for the currently verified attempt and pairs the resulting identity.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports an owned input DTO."
    )]
    pub fn ingest_device_session(
        &self,
        token: MobileConnectionAttemptTokenDto,
        input: MobileSessionInputDto,
    ) -> Option<MobileDeviceSessionStepDto> {
        let token = token.into();
        let core_input = input.clone().into();
        let step = self.lock_inner().ingest(&token, &core_input);
        if let Some(step) = &step {
            let telemetry = MobileTelemetrySnapshotDto::from(step.telemetry);
            self.apply_phone_alarm_step(&input, &telemetry, true);
        }
        step.map(|step| {
            let evidence = capture_notification_evidence(&input, &step);
            let mut projected = MobileDeviceSessionStepDto::from(step);
            projected.capture_notification_evidence = evidence;
            projected
        })
    }
}

impl CutoutSessionStateHandle {
    fn save_drivetrain_with_persistence(
        &self,
        token: MobileConnectionAttemptTokenDto,
        value: cutout_core::VescDrivetrain,
        persist: impl FnOnce(
            &cutout_protocols::VescDrivetrainSelection,
        )
            -> Result<cutout_core::VescDrivetrainProfile, MobileVescDrivetrainError>,
    ) -> Result<MobileVescDrivetrainDraft, MobileVescDrivetrainError> {
        // Reuse the durable device-selection ordering lock; snapshots never acquire it.
        let _selection = self
            .vehicle_selection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let token: cutout_core::ConnectionAttemptToken = token.into();
        let selection = {
            let inner = self.lock_inner();
            if inner.snapshot().connection.token.as_ref() != Some(&token) {
                return Err(MobileVescDrivetrainError::ConnectionUnavailable);
            }
            inner
                .prepare_vesc_drivetrain_selection(&token, value)
                .ok_or(MobileVescDrivetrainError::UnsupportedDevice)?
        };
        let profile = persist(&selection)?;
        if !self
            .lock_inner()
            .apply_vesc_drivetrain_selection(selection, profile)
        {
            return Err(MobileVescDrivetrainError::ConnectionUnavailable);
        }
        Ok(value.into())
    }

    fn restore_drivetrain_with_load(
        &self,
        token: MobileConnectionAttemptTokenDto,
        load: impl FnOnce(
            &str,
        ) -> Result<
            Option<cutout_core::VescDrivetrainProfile>,
            MobileVescDrivetrainError,
        >,
    ) -> Result<Option<MobileVescDrivetrainDraft>, MobileVescDrivetrainError> {
        let token: cutout_core::ConnectionAttemptToken = token.into();
        {
            let inner = self.lock_inner();
            if inner.snapshot().connection.token.as_ref() != Some(&token) {
                return Err(MobileVescDrivetrainError::ConnectionUnavailable);
            }
            if let Some(value) = inner.user_vesc_drivetrain(&token) {
                return Ok(Some(value.into()));
            }
        }
        let loaded = load(token.platform_identifier())?;
        let mut inner = self.lock_inner();
        if inner.snapshot().connection.token.as_ref() != Some(&token) {
            return Err(MobileVescDrivetrainError::ConnectionUnavailable);
        }
        if let Some(value) = loaded
            && !inner.restore_vesc_drivetrain(&token, value)
            && inner.user_vesc_drivetrain(&token).is_none()
        {
            return Err(MobileVescDrivetrainError::UnsupportedDevice);
        }
        Ok(inner.user_vesc_drivetrain(&token).map(Into::into))
    }

    fn select_vehicle_kind_with_persistence(
        &self,
        token: MobileConnectionAttemptTokenDto,
        kind: MobileVehicleKindDto,
        persist: impl FnOnce(
            &cutout_protocols::VehicleKindSelection,
        ) -> Result<(), MobileVehicleSelectionError>,
    ) -> Result<MobileDeviceSessionSnapshotDto, MobileVehicleSelectionError> {
        // Serialize durable selections without blocking telemetry or snapshot readers on storage.
        let _selection = self
            .vehicle_selection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let selection = self
            .lock_inner()
            .prepare_vehicle_kind_selection(&token.into(), kind.into())
            .ok_or(MobileVehicleSelectionError::ConnectionUnavailable)?;
        persist(&selection)?;
        let mut inner = self.lock_inner();
        if !inner.apply_vehicle_kind_selection(selection) {
            return Err(MobileVehicleSelectionError::ConnectionUnavailable);
        }
        Ok(inner.snapshot().into())
    }

    fn observe_connection_event(
        &self,
        token: MobileConnectionAttemptTokenDto,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolutionRecord> {
        self.lock_inner()
            .observe_for_attempt(&token.into(), event)
            .map(Into::into)
    }
}

fn capture_notification_evidence(
    input: &MobileSessionInputDto,
    step: &DeviceConnectionStep,
) -> MobileCaptureNotificationEvidenceDto {
    use cutout_core::{
        ConnectionReadiness, ConnectionTransportState, DeviceEvent, NotificationIngestOutcome,
        ProtocolFamily, ReadOnlyResponse, SessionOutput, Speed,
    };

    if input.kind != crate::MobileSessionInputKindDto::Notification
        || step.notification_framing
            != cutout_protocols::NotificationFramingEvidence::EmptyBoundaries
        || step.result.error.is_some()
        || step.session.connection.readiness != ConnectionReadiness::Verified
        || step.session.connection.transport != ConnectionTransportState::Connected
        || step.session.connection.token.is_none()
        || step
            .session
            .identity
            .is_none_or(|identity| identity.protocol != ProtocolFamily::VeteranLeaperkimNosfet)
        || input.channel.as_slice() != cutout_protocols::VETERAN_DATA_CHANNEL.as_bytes()
    {
        return MobileCaptureNotificationEvidenceDto::Unclassified;
    }
    // Only one self-contained Veteran telemetry frame can qualify. Completing a
    // prior fragment, concatenated frames, and trailing partial frames retain
    // every byte. Other adapters need their own complete-frame proof.
    let Ok(frame) = cutout_protocols::VeteranFrame::try_from_slice(&input.bytes) else {
        return MobileCaptureNotificationEvidenceDto::Unclassified;
    };
    if frame
        .as_slice()
        .get(46)
        .is_some_and(|selector| *selector != 0)
    {
        return MobileCaptureNotificationEvidenceDto::Unclassified;
    }
    let Some(speed) = step.telemetry.speed else {
        return MobileCaptureNotificationEvidenceDto::Unclassified;
    };
    if step
        .telemetry
        .speed_observed_at_ms
        .map(|at| at.milliseconds)
        != Some(input.monotonic_ms.milliseconds)
        || speed.source != cutout_core::ValueSourceDto::Reported
        || speed.quality != cutout_core::ValueQualityDto::Known
        || Speed::from_millimetres_per_second(speed.value).is_moving()
    {
        return MobileCaptureNotificationEvidenceDto::Unclassified;
    }
    let mut telemetry_events = 0;
    let mut complete_frames = 0;
    for output in &step.result.outputs {
        match output {
            SessionOutput::Event(DeviceEvent::Telemetry(delta))
                if delta.speed.is_some_and(|measured| {
                    measured.value.as_millimetres_per_second() == speed.value
                }) =>
            {
                telemetry_events += 1;
            }
            // These readbacks are decoded from this same telemetry frame; its
            // exact bytes remain part of the material comparison key.
            SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                ReadOnlyResponse::Firmware(_)
                | ReadOnlyResponse::Settings(_)
                | ReadOnlyResponse::RawTelemetry(_),
            )) => {}
            SessionOutput::NotificationIngest(NotificationIngestOutcome::SemanticEvents {
                ..
            }) => complete_frames += 1,
            _ => return MobileCaptureNotificationEvidenceDto::Unclassified,
        }
    }
    if telemetry_events == 1 && complete_frames == 1 {
        MobileCaptureNotificationEvidenceDto::StationaryTelemetry
    } else {
        MobileCaptureNotificationEvidenceDto::Unclassified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VESC_REPLY: &[u8] = &[
        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
        38, 208, 3,
    ];

    fn absent_vesc_application(
        handle: &CutoutSessionStateHandle,
        identifier: &str,
    ) -> MobileConnectionAttemptTokenDto {
        let token = handle
            .begin_connection_attempt(identifier.to_owned(), 0)
            .token
            .unwrap();
        standard_vesc_on_attempt(handle, token)
    }

    fn standard_vesc_on_attempt(
        handle: &CutoutSessionStateHandle,
        token: MobileConnectionAttemptTokenDto,
    ) -> MobileConnectionAttemptTokenDto {
        handle.connection_link_established(token.clone());
        handle.observe_connection_notification(token.clone(), VESC_REPLY.to_vec());
        handle.resolve_device_session(token.clone(), false, 1);
        for (kind, milliseconds) in [
            (crate::MobileSessionInputKindDto::LinkUp, 1),
            (crate::MobileSessionInputKindDto::Tick, 3_000),
        ] {
            let step = handle
                .ingest_device_session(
                    token.clone(),
                    MobileSessionInputDto {
                        kind,
                        monotonic_ms: crate::MobileMonotonicMillisDto { milliseconds },
                        max_write_len: None,
                        channel: Vec::new(),
                        bytes: Vec::new(),
                    },
                )
                .unwrap();
            if kind == crate::MobileSessionInputKindDto::LinkUp {
                let query = step
                    .result
                    .outputs
                    .into_iter()
                    .find_map(|output| output.application_query)
                    .expect("startup owns the bounded application query");
                assert!(handle.mark_application_query_transport(
                    token.clone(),
                    query.query_id,
                    0,
                    1,
                    MobileApplicationQueryTransportStatusDto::Submitted,
                    milliseconds
                ));
            }
        }
        assert_eq!(
            handle
                .device_session_snapshot()
                .identity
                .unwrap()
                .classification_requirement,
            MobileVehicleClassificationRequirementDto::UserSelectionRequired
        );
        token
    }

    fn drivetrain_erpm(
        handle: &CutoutSessionStateHandle,
        token: MobileConnectionAttemptTokenDto,
        reverse: bool,
        at: u64,
    ) -> MobileDeviceSessionStepDto {
        let bytes = if reverse {
            vec![2, 9, 50, 0, 0, 0, 128, 255, 255, 232, 144, 142, 215, 3]
        } else {
            vec![2, 9, 50, 0, 0, 0, 128, 0, 0, 23, 112, 244, 198, 3]
        };
        handle
            .ingest_device_session(
                token,
                MobileSessionInputDto {
                    kind: crate::MobileSessionInputKindDto::Notification,
                    monotonic_ms: crate::MobileMonotonicMillisDto { milliseconds: at },
                    max_write_len: None,
                    channel: cutout_protocols::VESC_NOTIFY_CHANNEL.as_bytes().to_vec(),
                    bytes,
                },
            )
            .unwrap()
    }

    fn drivetrain_draft() -> MobileVescDrivetrainDraft {
        MobileVescDrivetrainDraft {
            wheel_diameter_mm: "90".to_owned(),
            motor_pole_pairs: "7".to_owned(),
            motor_to_wheel_ratio: "2.5".to_owned(),
        }
    }

    fn saved_drivetrain(
        value: cutout_core::VescDrivetrain,
        revision: u64,
    ) -> cutout_core::VescDrivetrainProfile {
        cutout_core::VescDrivetrainProfile::user_configured(
            value,
            cutout_core::VescDrivetrainRevision::new(revision).unwrap(),
        )
    }

    #[test]
    fn captured_drivetrain_context_preserves_each_saved_revision() {
        let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-drivetrain-context-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::open_ride_database(path.to_string_lossy().into_owned()).unwrap();
        let handle = CutoutSessionStateHandle::with_database(database.clone());
        let token = absent_vesc_application(&handle, "controller-a");
        let unknown = drivetrain_erpm(&handle, token.clone(), false, 3_001);
        let unknown_json = serde_json::to_value(&unknown.telemetry).unwrap();
        assert!(unknown_json["vesc_drivetrain_context"].is_null());
        handle
            .save_connection_vesc_drivetrain(token.clone(), drivetrain_draft(), 1)
            .unwrap();
        let first = drivetrain_erpm(&handle, token.clone(), false, 3_002);
        let original_json = serde_json::to_value(&first.telemetry).unwrap();
        assert_eq!(original_json["vesc_drivetrain_context"]["revision"], 1);
        assert_eq!(
            original_json["vesc_drivetrain_context"]["source"],
            "user_configured"
        );
        assert_eq!(
            original_json["vesc_drivetrain_context"]["wheel_diameter_micrometres"],
            90_000
        );
        assert_eq!(
            original_json["vesc_drivetrain_context"]["wheel_circumference_mm"],
            283
        );
        assert_eq!(
            original_json["vesc_drivetrain_context"]["motor_pole_pairs"],
            7
        );
        assert_eq!(
            original_json["vesc_drivetrain_context"]["motor_to_wheel_ratio_milli"],
            2_500
        );
        assert_eq!(
            original_json["vesc_drivetrain_context"]["connection_attempt"]["platform_identifier"],
            "controller-a"
        );
        assert_eq!(
            original_json["vesc_drivetrain_context"]["connection_attempt"]["generation"],
            first.session.connection.token.as_ref().unwrap().generation
        );
        let before_noop = handle.connection_telemetry_snapshot(token.clone()).unwrap();
        handle
            .save_connection_vesc_drivetrain(
                token.clone(),
                MobileVescDrivetrainDraft {
                    wheel_diameter_mm: "90.000".to_owned(),
                    motor_to_wheel_ratio: "2.5000".to_owned(),
                    ..drivetrain_draft()
                },
                2,
            )
            .unwrap();
        assert_eq!(
            handle.connection_telemetry_snapshot(token.clone()).unwrap(),
            before_noop,
            "saving the same geometry must preserve its revision and observation timestamp"
        );
        handle
            .save_connection_vesc_drivetrain(
                token.clone(),
                MobileVescDrivetrainDraft {
                    wheel_diameter_mm: "100".to_owned(),
                    ..drivetrain_draft()
                },
                3,
            )
            .unwrap();
        let changed = drivetrain_erpm(&handle, token, false, 3_003);
        let changed_json = serde_json::to_value(&changed.telemetry).unwrap();
        assert_eq!(changed_json["vesc_drivetrain_context"]["revision"], 2);
        assert_eq!(
            changed_json["vesc_drivetrain_context"]["wheel_diameter_micrometres"],
            100_000
        );
        assert_eq!(original_json["vesc_drivetrain_context"]["revision"], 1);
        assert_eq!(first.telemetry.speed.unwrap().value.value, 1_617);
        assert_eq!(changed.telemetry.speed.unwrap().value.value, 1_794);
        assert_drivetrain_capture_roundtrip(
            &path,
            [unknown.telemetry, first.telemetry, changed.telemetry],
        );
        database.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
    }

    fn assert_drivetrain_capture_roundtrip(
        path: &std::path::Path,
        snapshots: [MobileTelemetrySnapshotDto; 3],
    ) {
        let capture_path = path.with_extension("jsonl");
        let builder = crate::MobilePevcapCaptureBuilder::new(
            crate::MobileWallClockUnixMillisDto {
                milliseconds: 1_700_000_000_000,
            },
            "ios-corebluetooth".to_owned(),
            None,
        );
        assert!(builder.set_capture_start_monotonic_ms(3_000));
        assert!(builder.start_writer(capture_path.to_string_lossy().into_owned()));
        for (at, snapshot) in [3_001, 3_002, 3_003].into_iter().zip(snapshots) {
            assert_eq!(
                builder.record_notification_with_context_and_semantic_telemetry(
                    crate::MobileMonotonicMillisDto { milliseconds: at },
                    cutout_protocols::VESC_NOTIFY_CHANNEL.as_bytes().to_vec(),
                    Vec::new(),
                    vec![2, 9, 50, 0, 0, 0, 128, 0, 0, 23, 112, 244, 198, 3],
                    None,
                    Some(snapshot),
                    None,
                ),
                crate::MobileCaptureWriteOutcomeDto::Accepted
            );
        }
        assert!(builder.finish_writer());
        let captured = cutout_core::PevcapCapture::decode(
            &std::fs::read(&capture_path).unwrap(),
            cutout_core::PevcapEncoding::Jsonl,
        )
        .unwrap();
        let binary = captured
            .encode(cutout_core::PevcapEncoding::Binary)
            .unwrap();
        let restored =
            cutout_core::PevcapCapture::decode(&binary, cutout_core::PevcapEncoding::Binary)
                .unwrap();
        for capture in [&captured, &restored] {
            assert_eq!(capture.records.len(), 3);
            for (record, (expected_revision, expected_speed)) in capture.records.iter().zip([
                (None, None),
                (Some(1), Some(1_617)),
                (Some(2), Some(1_794)),
            ]) {
                let semantic = record.semantic_telemetry.as_ref().unwrap();
                assert_eq!(
                    semantic.provenance,
                    cutout_core::PevcapTelemetryProvenance::LiveSession
                );
                assert_eq!(semantic.snapshot_schema_version, 2);
                assert_eq!(semantic.observed_at_ms, Some(record.monotonic_ms));
                let json: serde_json::Value =
                    serde_json::from_str(&semantic.snapshot_json).unwrap();
                assert_eq!(
                    json["vesc_drivetrain_context"]["revision"].as_u64(),
                    expected_revision
                );
                assert_eq!(json["speed"]["value"]["value"].as_i64(), expected_speed);
                assert!(
                    json["trip_distance"].is_null(),
                    "new geometry must not backfill distance"
                );
            }
        }
        let _ = std::fs::remove_file(capture_path);
    }

    #[test]
    fn user_drivetrain_save_applies_live_and_restores_only_same_device() {
        let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path =
            std::env::temp_dir().join(format!("cutout-drivetrain-{}.sqlite", uuid::Uuid::new_v4()));
        let database = crate::open_ride_database(path.to_string_lossy().into_owned()).unwrap();
        let handle = CutoutSessionStateHandle::with_database(database.clone());
        let token = absent_vesc_application(&handle, "controller-a");
        let identity = handle.device_session_snapshot().identity;
        let before = drivetrain_erpm(&handle, token.clone(), false, 3_001);
        assert_eq!(before.telemetry.speed, None);
        assert_eq!(
            handle
                .save_connection_vesc_drivetrain(token.clone(), drivetrain_draft(), 1)
                .unwrap(),
            drivetrain_draft()
        );
        assert_eq!(
            handle.connection_vesc_drivetrain(token.clone()),
            Some(drivetrain_draft())
        );
        assert_eq!(handle.device_session_snapshot().identity, identity);
        let forward = drivetrain_erpm(&handle, token.clone(), false, 3_002);
        assert_eq!(forward.telemetry.speed.unwrap().value.value, 1_617);
        assert_eq!(
            forward.telemetry.speed_observed_at_ms.unwrap().milliseconds,
            3_002
        );
        assert_eq!(forward.telemetry.distance, None);
        assert_eq!(forward.telemetry.trip_distance, None);
        let reverse = drivetrain_erpm(&handle, token.clone(), true, 3_003);
        assert_eq!(reverse.telemetry.speed.unwrap().value.value, -1_617);
        assert!(matches!(
            handle.save_connection_vesc_drivetrain(
                token.clone(),
                MobileVescDrivetrainDraft {
                    motor_to_wheel_ratio: "1".to_owned(),
                    ..drivetrain_draft()
                },
                u64::MAX
            ),
            Err(MobileVescDrivetrainError::Persistence { .. })
        ));
        assert_eq!(
            handle.connection_vesc_drivetrain(token.clone()),
            Some(drivetrain_draft())
        );
        let replacement = handle
            .begin_connection_attempt("controller-b".to_owned(), 4_000)
            .token
            .unwrap();
        assert_eq!(
            handle
                .restore_connection_vesc_drivetrain(replacement)
                .unwrap(),
            None
        );
        let reconnected = handle
            .begin_connection_attempt("controller-a".to_owned(), 0)
            .token
            .unwrap();
        assert_eq!(
            handle
                .restore_connection_vesc_drivetrain(reconnected.clone())
                .unwrap(),
            Some(drivetrain_draft())
        );
        assert_eq!(handle.connection_vesc_drivetrain(token), None);
        let reconnected = standard_vesc_on_attempt(&handle, reconnected);
        assert_eq!(
            drivetrain_erpm(&handle, reconnected, false, 7_001)
                .telemetry
                .speed
                .unwrap()
                .value
                .value,
            1_617
        );
        database.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn user_drivetrain_storage_does_not_lock_session_or_apply_to_replacement() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let geometry = cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap();
        let result = handle.save_drivetrain_with_persistence(token, geometry, |_| {
            assert!(
                handle.inner.try_lock().is_ok(),
                "storage must not hold session mutex"
            );
            handle.begin_connection_attempt("controller-b".to_owned(), 4_000);
            Ok(saved_drivetrain(geometry, 1))
        });
        assert_eq!(
            result,
            Err(MobileVescDrivetrainError::ConnectionUnavailable)
        );
        let replacement = handle.device_session_snapshot().connection.token.unwrap();
        assert_eq!(handle.connection_vesc_drivetrain(replacement), None);
    }

    #[test]
    fn user_drivetrain_save_accepts_its_receipt_installed_by_concurrent_restore() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let geometry = cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap();
        let profile = saved_drivetrain(geometry, 1);
        let result = handle.save_drivetrain_with_persistence(token.clone(), geometry, |_| {
            assert_eq!(
                handle.restore_drivetrain_with_load(token.clone(), |_| Ok(Some(profile)))?,
                Some(geometry.into())
            );
            Ok(profile)
        });
        assert_eq!(result, Ok(geometry.into()));
        assert_eq!(
            handle
                .connection_telemetry_snapshot(token)
                .unwrap()
                .vesc_drivetrain_context
                .unwrap()
                .revision,
            1
        );
    }

    #[test]
    fn user_drivetrain_newer_save_supersedes_an_older_restore_during_persistence() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let older = cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap();
        let edited = cutout_core::VescDrivetrain::parse("100", "7", "2.5").unwrap();
        let result = handle.save_drivetrain_with_persistence(token.clone(), edited, |_| {
            assert_eq!(
                handle.restore_drivetrain_with_load(token.clone(), |_| Ok(Some(
                    saved_drivetrain(older, 1)
                )))?,
                Some(older.into())
            );
            Ok(saved_drivetrain(edited, 2))
        });
        assert_eq!(result, Ok(edited.into()));
        let observed = drivetrain_erpm(&handle, token.clone(), false, 3_001);
        assert_eq!(observed.telemetry.speed.unwrap().value.value, 1_794);
        assert_eq!(
            handle
                .connection_telemetry_snapshot(token)
                .unwrap()
                .vesc_drivetrain_context
                .unwrap()
                .revision,
            2
        );
    }

    #[test]
    fn user_drivetrain_edit_invalidates_existing_speed_until_a_new_measurement() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let original = cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap();
        handle
            .save_drivetrain_with_persistence(token.clone(), original, |_| {
                Ok(saved_drivetrain(original, 1))
            })
            .unwrap();
        drivetrain_erpm(&handle, token.clone(), false, 3_001);
        handle
            .ingest_device_session(
                token.clone(),
                MobileSessionInputDto {
                    kind: crate::MobileSessionInputKindDto::Notification,
                    monotonic_ms: crate::MobileMonotonicMillisDto {
                        milliseconds: 3_001,
                    },
                    max_write_len: None,
                    channel: cutout_protocols::VESC_NOTIFY_CHANNEL.as_bytes().to_vec(),
                    bytes: vec![2, 11, 50, 0, 0, 1, 8, 0, 0, 4, 216, 1, 244, 17, 228, 3],
                },
            )
            .unwrap();
        let before = handle.connection_telemetry_snapshot(token.clone()).unwrap();
        assert_eq!(before.speed.unwrap().value.value, 1_617);
        assert_eq!(before.voltage.unwrap().value.value, 50_000);
        assert!(before.battery_current.is_some());
        let identity = handle.device_session_snapshot();
        let edited = cutout_core::VescDrivetrain::parse("100", "7", "2.5").unwrap();
        handle
            .save_drivetrain_with_persistence(token.clone(), edited, |_| {
                Ok(saved_drivetrain(edited, 2))
            })
            .unwrap();
        let cleared = handle.connection_telemetry_snapshot(token.clone()).unwrap();
        assert_eq!(cleared.speed, None);
        assert_eq!(cleared.speed_observed_at_ms, None);
        assert_eq!(cleared.at_ms, before.at_ms);
        assert_eq!(cleared.voltage, before.voltage);
        assert_eq!(cleared.battery_current, before.battery_current);
        assert_eq!(handle.device_session_snapshot(), identity);
        let next = drivetrain_erpm(&handle, token.clone(), false, 3_002);
        assert_eq!(next.telemetry.speed.unwrap().value.value, 1_794);
        assert_eq!(
            next.telemetry.speed_observed_at_ms.unwrap().milliseconds,
            3_002
        );
        handle.begin_connection_attempt("replacement".to_owned(), 4_000);
        assert_eq!(handle.connection_telemetry_snapshot(token), None);
    }

    #[test]
    fn user_drivetrain_new_edit_wins_over_delayed_restore() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let old = cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap();
        let new = cutout_core::VescDrivetrain::parse("100", "7", "2.5").unwrap();
        let restored = handle
            .restore_drivetrain_with_load(token.clone(), |_| {
                handle.save_drivetrain_with_persistence(token.clone(), new, |_| {
                    Ok(saved_drivetrain(new, 2))
                })?;
                Ok(Some(saved_drivetrain(old, 1)))
            })
            .unwrap();
        assert_eq!(restored, Some(new.into()));
        assert_eq!(handle.connection_vesc_drivetrain(token), Some(new.into()));
    }

    #[test]
    fn user_drivetrain_late_refloat_proof_invalidates_prior_local_speed() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let value = cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap();
        handle
            .save_drivetrain_with_persistence(token.clone(), value, |_| {
                Ok(saved_drivetrain(value, 1))
            })
            .unwrap();
        drivetrain_erpm(&handle, token.clone(), false, 3_001);
        let at = handle
            .connection_telemetry_snapshot(token.clone())
            .unwrap()
            .at_ms;
        let step = handle
            .ingest_device_session(
                token.clone(),
                MobileSessionInputDto {
                    kind: crate::MobileSessionInputKindDto::Notification,
                    monotonic_ms: crate::MobileMonotonicMillisDto {
                        milliseconds: 3_002,
                    },
                    max_write_len: None,
                    channel: cutout_protocols::VESC_NOTIFY_CHANNEL.as_bytes().to_vec(),
                    bytes: vec![
                        2, 61, 36, 101, 0, 2, 0, 82, 101, 102, 108, 111, 97, 116, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 232, 0, 0, 0, 0, 0, 89, 106, 3,
                    ],
                },
            )
            .unwrap();
        assert_eq!(step.telemetry.speed, None);
        assert_eq!(step.telemetry.speed_observed_at_ms, None);
        assert_eq!(step.telemetry.at_ms, at);
        assert_eq!(
            handle
                .connection_telemetry_snapshot(token.clone())
                .unwrap()
                .speed,
            None
        );
        assert_eq!(
            drivetrain_erpm(&handle, token.clone(), false, 3_003)
                .telemetry
                .speed,
            None,
            "Refloat ignores local user geometry"
        );
        for (at, bytes) in [
            (
                3_004,
                vec![
                    2, 17, 36, 101, 32, 1, 11, 109, 111, 116, 111, 114, 46, 115, 112, 101, 101,
                    100, 0, 201, 182, 3,
                ],
            ),
            (
                3_005,
                vec![
                    2, 24, 36, 101, 31, 4, 0, 0, 0, 0, 42, 19, 193, 0, 6, 60, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 51, 168, 3,
                ],
            ),
        ] {
            handle
                .ingest_device_session(
                    token.clone(),
                    MobileSessionInputDto {
                        kind: crate::MobileSessionInputKindDto::Notification,
                        monotonic_ms: crate::MobileMonotonicMillisDto { milliseconds: at },
                        max_write_len: None,
                        channel: cutout_protocols::VESC_NOTIFY_CHANNEL.as_bytes().to_vec(),
                        bytes,
                    },
                )
                .unwrap();
        }
        let reported = handle.connection_telemetry_snapshot(token).unwrap();
        assert_eq!(
            reported.speed.unwrap().source,
            crate::MobileValueSourceDto::Reported
        );
        assert_eq!(reported.speed.unwrap().value.value, 278);
        assert_eq!(reported.speed_observed_at_ms.unwrap().milliseconds, 3_005);
    }

    #[test]
    fn user_drivetrain_refloat_evidence_rejects_save_and_restore() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        handle
            .ingest_device_session(
                token.clone(),
                MobileSessionInputDto {
                    kind: crate::MobileSessionInputKindDto::Notification,
                    monotonic_ms: crate::MobileMonotonicMillisDto {
                        milliseconds: 3_001,
                    },
                    max_write_len: None,
                    channel: cutout_protocols::VESC_NOTIFY_CHANNEL.as_bytes().to_vec(),
                    bytes: vec![
                        2, 61, 36, 101, 0, 2, 0, 82, 101, 102, 108, 111, 97, 116, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 232, 0, 0, 0, 0, 0, 89, 106, 3,
                    ],
                },
            )
            .unwrap();
        assert_eq!(
            handle
                .device_session_snapshot()
                .identity
                .unwrap()
                .vesc_application,
            Some(MobileVescApplicationDiscoveryDto::Refloat)
        );
        let value = cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap();
        assert_eq!(
            handle.save_drivetrain_with_persistence(token.clone(), value, |_| panic!(
                "Refloat must never enter local save"
            )),
            Err(MobileVescDrivetrainError::UnsupportedDevice)
        );
        assert_eq!(
            handle.restore_drivetrain_with_load(token.clone(), |_| Ok(Some(saved_drivetrain(
                value, 1
            )))),
            Err(MobileVescDrivetrainError::UnsupportedDevice)
        );
        assert_eq!(handle.connection_vesc_drivetrain(token), None);
    }

    #[test]
    fn user_drivetrain_restore_completion_does_not_modify_a_replacement_attempt() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let value = cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap();
        let result = handle.restore_drivetrain_with_load(token, |_| {
            handle.begin_connection_attempt("controller-b".to_owned(), 4_000);
            Ok(Some(saved_drivetrain(value, 1)))
        });
        assert_eq!(
            result,
            Err(MobileVescDrivetrainError::ConnectionUnavailable)
        );
        let replacement = handle.device_session_snapshot().connection.token.unwrap();
        assert_eq!(handle.connection_vesc_drivetrain(replacement), None);
    }

    #[test]
    fn user_drivetrain_invalid_draft_never_enters_storage() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        assert_eq!(
            handle.save_connection_vesc_drivetrain(
                token.clone(),
                MobileVescDrivetrainDraft {
                    motor_to_wheel_ratio: "2.5001".to_owned(),
                    ..drivetrain_draft()
                },
                1
            ),
            Err(MobileVescDrivetrainError::InvalidReduction)
        );
        assert_eq!(
            handle.save_connection_vesc_drivetrain(token.clone(), drivetrain_draft(), 1),
            Err(MobileVescDrivetrainError::PersistenceUnavailable)
        );
        assert_eq!(handle.connection_vesc_drivetrain(token), None);
    }

    #[test]
    fn vehicle_selection_storage_wait_keeps_session_available_and_rejects_stale_completion() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let (entered, waiting) = std::sync::mpsc::sync_channel(0);
        let (release, resume) = std::sync::mpsc::sync_channel(0);
        let worker_handle = handle.clone();
        let worker = std::thread::spawn(move || {
            worker_handle.select_vehicle_kind_with_persistence(
                token,
                MobileVehicleKindDto::Eskate,
                |selection| {
                    assert_eq!(selection.platform_identifier(), "controller-a");
                    assert_eq!(selection.kind(), VehicleKind::Eskate);
                    entered.send(()).unwrap();
                    resume.recv().unwrap();
                    Ok(())
                },
            )
        });
        waiting
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let session_available = handle.inner.try_lock().is_ok();
        if session_available {
            handle.begin_connection_attempt("controller-b".to_owned(), 4_000);
        }
        release.send(()).unwrap();
        let result = worker.join().unwrap();
        assert!(
            session_available,
            "storage latency must not hold the session mutex used by synchronous snapshot readers"
        );
        assert_eq!(
            result,
            Err(MobileVehicleSelectionError::ConnectionUnavailable)
        );
        let current = handle.device_session_snapshot();
        assert_eq!(
            current.connection.token.unwrap().platform_identifier,
            "controller-b"
        );
        assert_eq!(current.identity, None);
    }

    #[test]
    fn concurrent_vehicle_selections_keep_durable_and_active_choices_in_order() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let durable = std::sync::Arc::new(std::sync::Mutex::new(VehicleKind::Unknown));
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, resume) = std::sync::mpsc::channel();
        let first_handle = handle.clone();
        let first_token = token.clone();
        let first_durable = durable.clone();
        let first = std::thread::spawn(move || {
            first_handle.select_vehicle_kind_with_persistence(
                first_token,
                MobileVehicleKindDto::Eskate,
                |selection| {
                    *first_durable.lock().unwrap() = selection.kind();
                    entered.send(()).unwrap();
                    resume.recv().unwrap();
                    Ok(())
                },
            )
        });
        waiting
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let (started, start) = std::sync::mpsc::channel();
        let (completed, completion) = std::sync::mpsc::channel();
        let second_handle = handle.clone();
        let second_durable = durable.clone();
        let second = std::thread::spawn(move || {
            started.send(()).unwrap();
            let result = second_handle.select_vehicle_kind_with_persistence(
                token,
                MobileVehicleKindDto::Scooter,
                |selection| {
                    *second_durable.lock().unwrap() = selection.kind();
                    Ok(())
                },
            );
            completed.send(()).unwrap();
            result
        });
        start
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        // An unserialized second save can finish before the first operation applies its older choice.
        let _ = completion.recv_timeout(std::time::Duration::from_millis(100));
        release.send(()).unwrap();
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
        let active = handle
            .device_session_snapshot()
            .identity
            .unwrap()
            .vehicle_kind;
        assert_eq!(active, MobileVehicleKindDto::Scooter);
        assert_eq!(VehicleKind::from(active), *durable.lock().unwrap());
    }

    #[test]
    fn vehicle_selection_without_persistence_leaves_classification_required() {
        let handle = CutoutSessionStateHandle::new();
        let token = absent_vesc_application(&handle, "controller-a");
        let before = handle.device_session_snapshot();
        assert_eq!(
            handle.select_connection_vehicle_kind(token, MobileVehicleKindDto::Eskate, 1),
            Err(MobileVehicleSelectionError::PersistenceUnavailable)
        );
        assert_eq!(handle.device_session_snapshot(), before);
    }

    #[test]
    fn vehicle_selection_commits_before_admission_and_rejects_stale_or_unknown_choices() {
        let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-vehicle-choice-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::open_ride_database(path.to_string_lossy().into_owned()).unwrap();
        let handle = CutoutSessionStateHandle::with_database(database.clone());
        let token = absent_vesc_application(&handle, "controller-a");
        let before = handle.device_session_snapshot();
        assert!(matches!(
            handle.select_connection_vehicle_kind(
                token.clone(),
                MobileVehicleKindDto::Eskate,
                u64::MAX
            ),
            Err(MobileVehicleSelectionError::Persistence { .. })
        ));
        assert_eq!(handle.device_session_snapshot(), before);
        assert_eq!(
            database
                .user_vehicle_kind("controller-a".to_owned())
                .unwrap(),
            None
        );
        assert_eq!(
            handle.select_connection_vehicle_kind(token.clone(), MobileVehicleKindDto::Unknown, 1),
            Err(MobileVehicleSelectionError::ConnectionUnavailable)
        );
        let selected = handle
            .select_connection_vehicle_kind(token.clone(), MobileVehicleKindDto::Eskate, 1)
            .unwrap();
        let identity = selected.identity.unwrap();
        assert_eq!(identity.vehicle_kind, MobileVehicleKindDto::Eskate);
        assert_eq!(
            identity.classification_requirement,
            MobileVehicleClassificationRequirementDto::Resolved
        );
        assert_eq!(
            identity.classification_provenance,
            MobileVehicleClassificationProvenanceDto::UserProvided
        );
        assert_eq!(identity.model, None);
        assert_eq!(
            database
                .user_vehicle_kind("controller-a".to_owned())
                .unwrap(),
            Some(MobileVehicleKindDto::Eskate)
        );
        handle.begin_connection_attempt("controller-b".to_owned(), 4_000);
        assert_eq!(
            handle.select_connection_vehicle_kind(token, MobileVehicleKindDto::Emoto, 2),
            Err(MobileVehicleSelectionError::ConnectionUnavailable)
        );
        assert_eq!(
            database
                .user_vehicle_kind("controller-a".to_owned())
                .unwrap(),
            Some(MobileVehicleKindDto::Eskate)
        );
        assert_eq!(
            database
                .user_vehicle_kind("controller-b".to_owned())
                .unwrap(),
            None
        );
        database.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
    }

    fn veteran_frame_with_model_id(model_id: u16) -> Vec<u8> {
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&(model_id * 1_000).to_be_bytes());
        frame
    }

    fn connected_capture_session() -> (
        std::sync::Arc<CutoutSessionStateHandle>,
        MobileConnectionAttemptTokenDto,
        Vec<u8>,
    ) {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("NF2557".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        let frame = veteran_frame_with_model_id(43);
        handle.observe_connection_notification(token.clone(), frame.clone());
        handle.resolve_device_session(token.clone(), true, 1);
        let linked = handle
            .ingest_device_session(
                token.clone(),
                MobileSessionInputDto {
                    kind: crate::MobileSessionInputKindDto::LinkUp,
                    monotonic_ms: crate::MobileMonotonicMillisDto { milliseconds: 1 },
                    max_write_len: None,
                    channel: Vec::new(),
                    bytes: Vec::new(),
                },
            )
            .expect("resolved decoder receives native link-up before notifications");
        assert!(linked.result.error.is_none());
        (handle, token, frame)
    }

    #[test]
    fn capture_admission_requires_current_complete_stationary_telemetry() {
        let (handle, token, frame) = connected_capture_session();
        let step = handle
            .ingest_device_session(
                token,
                MobileSessionInputDto {
                    kind: crate::MobileSessionInputKindDto::Notification,
                    monotonic_ms: crate::MobileMonotonicMillisDto { milliseconds: 100 },
                    max_write_len: None,
                    channel: cutout_protocols::VETERAN_DATA_CHANNEL.as_bytes().to_vec(),
                    bytes: frame,
                },
            )
            .expect("current notification is admitted");
        assert_eq!(
            step.capture_notification_evidence,
            MobileCaptureNotificationEvidenceDto::StationaryTelemetry,
        );
    }

    #[test]
    fn capture_admission_uses_signed_shared_threshold_on_each_current_frame() {
        let (handle, token, frame) = connected_capture_session();
        for (index, deci_kmh) in [-19_i16, -18, -17, 0, 17, 18, 19].into_iter().enumerate() {
            let mut bytes = frame.clone();
            bytes[6..8].copy_from_slice(&deci_kmh.to_be_bytes());
            let input = MobileSessionInputDto {
                kind: crate::MobileSessionInputKindDto::Notification,
                monotonic_ms: crate::MobileMonotonicMillisDto {
                    milliseconds: 100 + index as u64,
                },
                max_write_len: None,
                channel: cutout_protocols::VETERAN_DATA_CHANNEL.as_bytes().to_vec(),
                bytes,
            };
            let step = handle.ingest_device_session(token.clone(), input).unwrap();
            assert_eq!(
                step.telemetry.speed.as_ref().unwrap().value.value,
                cutout_core::Speed::from_deci_kmh(i32::from(deci_kmh)).as_millimetres_per_second()
            );
            let expected = if cutout_core::Speed::from_deci_kmh(i32::from(deci_kmh)).is_moving() {
                MobileCaptureNotificationEvidenceDto::Unclassified
            } else {
                MobileCaptureNotificationEvidenceDto::StationaryTelemetry
            };
            assert_eq!(
                step.capture_notification_evidence, expected,
                "speed {deci_kmh}"
            );
        }
    }

    #[test]
    fn capture_admission_rejects_stale_disconnected_or_ambiguous_evidence() {
        let (handle, token, frame) = connected_capture_session();
        let input = MobileSessionInputDto {
            kind: crate::MobileSessionInputKindDto::Notification,
            monotonic_ms: crate::MobileMonotonicMillisDto { milliseconds: 100 },
            max_write_len: None,
            channel: cutout_protocols::VETERAN_DATA_CHANNEL.as_bytes().to_vec(),
            bytes: frame,
        };
        let step = handle
            .lock_inner()
            .ingest(&token.into(), &input.clone().into())
            .unwrap();
        assert_eq!(
            step.notification_framing,
            cutout_protocols::NotificationFramingEvidence::EmptyBoundaries
        );
        assert_eq!(
            capture_notification_evidence(&input, &step),
            MobileCaptureNotificationEvidenceDto::StationaryTelemetry,
            "{step:#?}"
        );
        let mut stale = step.clone();
        stale.telemetry.speed_observed_at_ms =
            Some(cutout_core::MonotonicMillisDto { milliseconds: 99 });
        let mut missing = step.clone();
        missing.telemetry.speed = None;
        let mut inferred = step.clone();
        inferred.telemetry.speed.as_mut().unwrap().quality = cutout_core::ValueQualityDto::Inferred;
        let mut estimated = step.clone();
        estimated.telemetry.speed.as_mut().unwrap().source = cutout_core::ValueSourceDto::Estimated;
        let mut disconnected = step.clone();
        disconnected.session.connection.transport =
            cutout_core::ConnectionTransportState::Disconnected;
        let mut unverified = step.clone();
        unverified.session.connection.readiness = cutout_core::ConnectionReadiness::Pending;
        let mut no_token = step.clone();
        no_token.session.connection.token = None;
        let mut unknown_framing = step.clone();
        unknown_framing.notification_framing =
            cutout_protocols::NotificationFramingEvidence::Unclassified;
        let mut ambiguous = step.clone();
        ambiguous.result.outputs.extend(step.result.outputs.clone());
        for rejected in [
            stale,
            missing,
            inferred,
            estimated,
            disconnected,
            unverified,
            no_token,
            unknown_framing,
            ambiguous,
        ] {
            assert_eq!(
                capture_notification_evidence(&input, &rejected),
                MobileCaptureNotificationEvidenceDto::Unclassified
            );
        }
    }

    #[test]
    fn capture_admission_keeps_notifications_with_pending_parser_bytes() {
        let (handle, token, frame) = connected_capture_session();
        let notification = |at, bytes| MobileSessionInputDto {
            kind: crate::MobileSessionInputKindDto::Notification,
            monotonic_ms: crate::MobileMonotonicMillisDto { milliseconds: at },
            max_write_len: None,
            channel: cutout_protocols::VETERAN_DATA_CHANNEL.as_bytes().to_vec(),
            bytes,
        };
        let fragment = handle
            .ingest_device_session(token.clone(), notification(100, frame[..4].to_vec()))
            .unwrap();
        assert_eq!(
            fragment.capture_notification_evidence,
            MobileCaptureNotificationEvidenceDto::Unclassified
        );
        let after_fragment = handle
            .ingest_device_session(token.clone(), notification(101, frame.clone()))
            .unwrap();
        assert!(after_fragment.telemetry.speed.is_some());
        assert_eq!(
            after_fragment.capture_notification_evidence,
            MobileCaptureNotificationEvidenceDto::Unclassified
        );
        let mut frame_and_partial = frame.clone();
        frame_and_partial.extend_from_slice(&frame[..4]);
        let trailing = handle
            .ingest_device_session(token.clone(), notification(102, frame_and_partial))
            .unwrap();
        assert!(trailing.telemetry.speed.is_some());
        assert_eq!(
            trailing.capture_notification_evidence,
            MobileCaptureNotificationEvidenceDto::Unclassified
        );
        let after_trailing = handle
            .ingest_device_session(token, notification(103, frame))
            .unwrap();
        assert_eq!(
            after_trailing.capture_notification_evidence,
            MobileCaptureNotificationEvidenceDto::Unclassified
        );
    }

    #[test]
    fn falcon_detection_admits_reported_model_with_its_settings_profile() {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("falcon-a".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        handle.observe_connection_advertisement(token.clone(), Some(b"GotWay_002441".to_vec()));
        // Fragment boundaries from the physical Falcon's connection capture.
        let notifications: &[&[u8]] = &[
            &[
                90, 90, 90, 90, 85, 170, 0, 75, 0, 0, 3, 214, 0, 0, 0, 0, 19, 136, 0, 0,
            ],
            &[
                0, 0, 1, 3, 90, 90, 90, 90, 85, 170, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            &[
                0, 0, 0, 0, 0, 0, 3, 3, 90, 90, 90, 90, 85, 170, 0, 0, 0, 0, 73, 1,
            ],
            &[
                24, 223, 0, 45, 0, 0, 0, 0, 0, 18, 4, 24, 90, 90, 90, 90, 85, 170, 255, 219,
            ],
            &[
                0, 147, 0, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 24, 90, 90, 90, 90,
            ],
        ];
        for bytes in notifications {
            handle.observe_connection_notification(token.clone(), bytes.to_vec());
            let pending = handle.resolve_device_session(token.clone(), false, 1_500);
            assert_eq!(
                pending.connection.readiness,
                crate::MobileConnectionReadinessDto::Pending
            );
        }
        handle.observe_begode_name_probe_for_attempt_at(token.clone(), 1_500);
        let still_detecting = handle.resolve_device_session(token.clone(), true, 2_000);
        assert_eq!(
            still_detecting.connection.readiness,
            crate::MobileConnectionReadinessDto::Pending,
            "a native completion callback cannot bypass an outstanding model query"
        );
        handle.observe_connection_notification(token.clone(), b"NAME:Falcon\r\n".to_vec());
        let candidate =
            handle.connection_admission_candidate("falcon-a".into(), "GotWay_002441".into(), false);
        assert_eq!(
            candidate.support,
            crate::DiscoveryCandidateSupport::Supported
        );
        let resolved = handle.resolve_device_session(token, true, 2_333);
        assert_eq!(resolved.identity.unwrap().model.as_deref(), Some("Falcon"));
        assert!(handle.settings().default_charge_profile.is_some());
        assert_ne!(handle.settings().setting_descriptors.len(), 0);
    }

    #[test]
    fn discovery_retains_each_detected_falcon_across_disconnect_and_advertisement_refresh() {
        let handle = CutoutSessionStateHandle::new();
        for (id, name) in [("A", "GotWay_002441"), ("B", "GotWay_002442")] {
            let observation = crate::DiscoveryObservation {
                platform_identifier: id.into(),
                advertised_name: Some(name.as_bytes().to_vec()),
                advertised_service_uuids: vec![],
                manufacturer_data: vec![],
                rssi_dbm: Some(-60),
            };
            handle.observe_discovery(observation.clone());
            let token = handle.begin_connection_attempt(id.into(), 0).token.unwrap();
            handle.connection_link_established(token.clone());
            let mut frame = vec![0; 24];
            frame[..2].copy_from_slice(&[0x55, 0xaa]);
            frame[18..].copy_from_slice(&[0, 0x18, 0x5a, 0x5a, 0x5a, 0x5a]);
            handle.observe_connection_notification(token.clone(), frame);
            handle.observe_connection_notification(token, b"NAME:Falcon\r\n".to_vec());
            handle.disconnect_connection_attempt();
            handle.observe_discovery(observation);
        }
        let snapshot = handle.discovery_snapshot();
        assert_eq!(snapshot.picker_candidates.len(), 2);
        for (id, name) in [("A", "GotWay_002441"), ("B", "GotWay_002442")] {
            let row = snapshot
                .picker_candidates
                .iter()
                .find(|row| row.platform_identifier == id)
                .unwrap();
            assert_eq!(row.display_name, "Begode Falcon");
            assert!(row.detail.contains(name));
            assert_eq!(
                row.electric_unicycle_model,
                Some(crate::DiscoveryElectricUnicycleModel::Falcon)
            );
        }
        // Retained discovery evidence is not admission evidence for a new connection.
        let token = handle
            .begin_connection_attempt("A".into(), 1)
            .token
            .unwrap();
        let pending = handle.resolve_device_session(token, false, 2);
        assert!(pending.identity.is_none());
    }

    #[test]
    fn picker_selection_cannot_reassign_another_connections_detected_identity() {
        let handle = CutoutSessionStateHandle::new();
        for id in ["A", "B"] {
            handle.observe_discovery(crate::DiscoveryObservation {
                platform_identifier: id.into(),
                advertised_name: Some(id.as_bytes().to_vec()),
                advertised_service_uuids: vec![],
                manufacturer_data: vec![],
                rssi_dbm: None,
            });
        }
        let token = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.observe_connection_notification(token, b"NAME:Falcon\r\n".to_vec());
        handle.select_discovered_platform("B".into());
        let snapshot = handle.discovery_snapshot();
        assert_eq!(snapshot.picker_candidates.len(), 1);
        assert_eq!(snapshot.picker_candidates[0].platform_identifier, "A");
        handle.begin_connection_attempt("B".into(), 1);
        assert_eq!(
            handle.discovery_snapshot().picker_candidates[0].platform_identifier,
            "A"
        );
    }

    #[test]
    fn discovery_preserves_detection_when_transport_ends_or_conflicts() {
        for conflicting_reply in [None, Some(VESC_REPLY)] {
            let handle = CutoutSessionStateHandle::new();
            handle.observe_discovery(crate::DiscoveryObservation {
                platform_identifier: "A".into(),
                advertised_name: Some(b"GotWay_002441".to_vec()),
                advertised_service_uuids: vec![],
                manufacturer_data: vec![],
                rssi_dbm: None,
            });
            let token = handle
                .begin_connection_attempt("A".into(), 0)
                .token
                .unwrap();
            handle.connection_link_established(token.clone());
            let mut frame = vec![0; 24];
            frame[..2].copy_from_slice(&[0x55, 0xaa]);
            frame[18..].copy_from_slice(&[0, 0x18, 0x5a, 0x5a, 0x5a, 0x5a]);
            handle.observe_connection_notification(token.clone(), frame);
            handle.observe_connection_notification(token.clone(), b"NAME:Falcon\r\n".to_vec());
            handle.resolve_device_session(token.clone(), false, 1);
            let expected = if let Some(bytes) = conflicting_reply {
                handle.observe_connection_notification(token, bytes.to_vec());
                crate::DiscoveryCandidateSupport::Conflicting
            } else {
                handle.connection_link_down(token);
                crate::DiscoveryCandidateSupport::Supported
            };
            let candidates = handle.discovery_snapshot().picker_candidates;
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].support, expected);
            assert!(handle.device_session_snapshot().identity.is_none());
        }
    }

    #[test]
    fn expired_attempt_cannot_promote_from_late_protocol_reply() {
        let handle = CutoutSessionStateHandle::new();
        let attempt = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.expire_connection_attempt(attempt.clone(), 15_000);
        handle.observe_connection_notification(attempt.clone(), VESC_REPLY.to_vec());
        let snapshot = handle.resolve_device_session(attempt.clone(), true, 15_000);
        assert_eq!(
            snapshot.connection.readiness,
            crate::MobileConnectionReadinessDto::RecordOnly
        );
        assert!(snapshot.identity.is_none());
        assert!(!handle.verified_connection_attempt_is_current(attempt));
    }

    #[test]
    fn verified_session_identity_is_retired_when_same_peripheral_is_retried() {
        let handle = CutoutSessionStateHandle::new();
        let old = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(old.clone());
        handle.observe_connection_notification(old.clone(), VESC_REPLY.to_vec());
        let verified = handle.resolve_device_session(old.clone(), false, 0);
        assert_eq!(
            verified.identity.unwrap().vehicle_kind,
            MobileVehicleKindDto::Unknown
        );
        assert!(handle.verified_connection_attempt_is_current(old.clone()));
        let next = handle.begin_connection_attempt("A".into(), 1);
        assert!(handle.device_session_snapshot().identity.is_none());
        let late = handle.resolve_device_session(old, true, 1);
        assert_eq!(late.connection, next);
        assert!(late.identity.is_none());
    }

    #[test]
    fn old_native_notification_cannot_supply_replacement_protocol_identity() {
        let handle = CutoutSessionStateHandle::new();
        let old = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        let current = handle
            .begin_connection_attempt("B".into(), 1)
            .token
            .unwrap();
        assert!(
            handle
                .observe_connection_notification(old, VESC_REPLY.to_vec())
                .is_none()
        );
        let snapshot = handle.resolve_device_session(current, false, 1);
        assert!(snapshot.identity.is_none());
        assert_eq!(
            snapshot.connection.readiness,
            crate::MobileConnectionReadinessDto::Pending
        );
    }

    #[test]
    fn delayed_timer_delivery_does_not_extend_identification_deadline() {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.observe_connection_notification(token.clone(), VESC_REPLY.to_vec());
        let snapshot = handle.resolve_device_session(token, true, 15_000);
        assert_eq!(
            snapshot.connection.readiness,
            crate::MobileConnectionReadinessDto::RecordOnly
        );
        assert!(snapshot.identity.is_none());
    }

    #[test]
    fn supported_veteran_model_can_admit_a_pending_attempt_from_detection_evidence() {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("NF2557".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        handle.observe_connection_notification(token.clone(), veteran_frame_with_model_id(43));

        let candidate =
            handle.connection_admission_candidate("NF2557".into(), "NF2557".into(), false);
        assert_eq!(
            candidate.support,
            crate::DiscoveryCandidateSupport::Supported
        );
        assert_eq!(
            candidate.electric_unicycle_model,
            Some(crate::DiscoveryElectricUnicycleModel::Aero)
        );

        let snapshot = handle.resolve_device_session(token.clone(), true, 1);
        assert_eq!(
            snapshot.connection.readiness,
            crate::MobileConnectionReadinessDto::Verified
        );
        assert_eq!(
            snapshot.identity.unwrap().model.as_deref(),
            Some("NOSFET Aero")
        );
        assert!(handle.verified_connection_attempt_is_current(token));
    }
}
