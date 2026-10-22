//! Connection, detector and protocol-session ownership shared by native clients.

mod actions;
pub use actions::{DeviceActionSubmissionError, DeviceActionsSnapshot};
mod settings;
pub use settings::{DeviceSettingRequestError, DeviceSettingsSnapshot};
mod phone_drive;

use cutout_core::{
    ConnectionAttemptSnapshot, ConnectionAttemptToken, ConnectionReadiness, CutoutSessionState,
    DeviceEvent, DeviceSettingsState, GattFingerprint, MonotonicTimestamp, ParserDiagnosticsDto,
    ProtocolFamily, ReadOnlyResponse, SessionInputDto, SessionOutput, TelemetrySnapshotDto,
};

use crate::{
    DeviceDetectionEvent, DeviceDetectionResolution, DeviceDetectionSession, DeviceSession,
    DeviceSessionIdentity, DeviceSessionStep,
};

/// Connection admission and exact identity from one owner observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceConnectionSnapshot {
    /// Current attempt and transport availability.
    pub connection: ConnectionAttemptSnapshot,
    /// Identity of the admitted decoder, if present.
    pub identity: Option<DeviceSessionIdentity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ValidationGrant {
    generation: u64,
}

/// Prepared local geometry bound to the current standard VESC attempt.
#[derive(Debug)]
pub struct VescDrivetrainSelection {
    token: ConnectionAttemptToken,
    drivetrain: cutout_core::VescDrivetrain,
    target: cutout_core::VescDrivetrainTarget,
    previous_profile: Option<cutout_core::VescDrivetrainProfile>,
}
impl VescDrivetrainSelection {
    /// Device receiving this saved local configuration.
    #[must_use]
    pub fn platform_identifier(&self) -> &str {
        self.token.platform_identifier()
    }
    /// Explicit controller association, separate from physical type or identity proof.
    #[must_use]
    pub const fn target(&self) -> cutout_core::VescDrivetrainTarget {
        self.target
    }
    /// Checked local geometry.
    #[must_use]
    pub const fn drivetrain(&self) -> cutout_core::VescDrivetrain {
        self.drivetrain
    }
}

/// A current, non-Refloat VESC attempt's validated physical-type choice.
///
/// Persistence may consume its device identity before the same owner applies the choice.
#[derive(Debug)]
pub struct VehicleKindSelection {
    token: ConnectionAttemptToken,
    classification: cutout_core::VehicleClassification,
}

impl VehicleKindSelection {
    /// Platform-local device identity bound to this selection.
    #[must_use]
    pub fn platform_identifier(&self) -> &str {
        self.token.platform_identifier()
    }

    /// User-provided physical type admitted for this device.
    #[must_use]
    pub const fn kind(&self) -> cutout_core::VehicleKind {
        self.classification.kind()
    }
}

impl ValidationGrant {
    fn for_token(token: &ConnectionAttemptToken) -> Self {
        Self {
            generation: token.generation(),
        }
    }

    fn matches(self, token: &ConnectionAttemptToken) -> bool {
        self.generation == token.generation()
    }
}

/// Parser boundaries for one input, independent of its semantic output count.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NotificationFramingEvidence {
    /// Unsupported framing, a partial frame, or a non-notification input.
    #[default]
    Unclassified,
    /// The decoder retained no partial bytes before or after this notification.
    EmptyBoundaries,
}

/// One decoded input paired with its producing connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceConnectionStep {
    /// Parser state sampled before and after this exact input.
    pub notification_framing: NotificationFramingEvidence,
    /// Connection and identity that produced this result.
    pub session: DeviceConnectionSnapshot,
    /// Protocol outputs and typed error.
    pub result: DeviceSessionStep,
    /// Latest normalized telemetry.
    pub telemetry: TelemetrySnapshotDto,
    /// Effective user geometry retained atomically with this step's telemetry.
    pub vesc_drivetrain_context: Option<cutout_core::VescDrivetrainProfile>,
    /// Configured profiles retained with this exact step; CAN entries are not effective telemetry.
    pub configured_vesc_drivetrains: Vec<cutout_core::VescDrivetrainProfile>,
    /// Accumulated parser diagnostics.
    pub diagnostics: ParserDiagnosticsDto,
}

/// Capture metadata reconciliation for an exact connection attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureIdentityUpdate {
    /// The current attempt has no verified protocol; bind ownership without publishing proof.
    Unresolved,
    /// The current verified decoder established this identity.
    Resolved(cutout_core::PevcapResolvedIdentity),
    /// The attempt's previously verified protocol was contradicted by later wire evidence.
    Invalidated,
}

/// Authoritative device connection, backed by the existing state and protocol owners.
#[derive(Debug, Default)]
pub struct DeviceConnectionSession {
    /// Shared durable state, including ride, identity and settings slices.
    state: CutoutSessionState,
    /// Incremental detector retaining validated wire evidence.
    detector: DeviceDetectionSession,
    /// Prior wire evidence for picker presentation, never used to admit a new attempt.
    detected_devices: std::collections::BTreeMap<String, DeviceDetectionResolution>,
    /// Protocol-selected decoder, never selected by native model dispatch.
    device: Option<DeviceSession>,
    last_input_at: MonotonicTimestamp,
    vesc_board_profile: Option<crate::VescBoardProfile>,
    user_vesc_drivetrains: std::collections::BTreeMap<
        cutout_core::VescDrivetrainTarget,
        cutout_core::VescDrivetrainProfile,
    >,
    user_vehicle_classification: Option<cutout_core::VehicleClassification>,
    validation_grant: Option<ValidationGrant>,
    invalidated_capture_attempt: Option<ConnectionAttemptToken>,
    setting_operations:
        std::collections::BTreeMap<cutout_core::SettingId, settings::SettingTransportOperation>,
}

impl DeviceConnectionSession {
    /// Validates a query fragment immediately before its actual native submission.
    #[must_use]
    pub fn application_query_transport_is_current(
        &self,
        token: &ConnectionAttemptToken,
        query_id: u64,
        at: MonotonicTimestamp,
    ) -> bool {
        self.state.connection.is_verified(token)
            && self.device.as_ref().is_some_and(|device| {
                device.application_query_is_current(
                    cutout_core::ApplicationQueryId::new(query_id),
                    at,
                )
            })
    }

    /// Records one indexed native fragment receipt for exactly one current attempt.
    pub fn mark_application_query_transport(
        &mut self,
        token: &ConnectionAttemptToken,
        query_id: u64,
        index: u16,
        count: u16,
        status: cutout_core::ApplicationQueryTransportStatus,
        at: MonotonicTimestamp,
    ) -> bool {
        if !self.state.connection.is_verified(token) {
            return false;
        }
        self.device.as_mut().is_some_and(|device| {
            device.record_application_query_transport(
                cutout_core::ApplicationQueryId::new(query_id),
                index,
                count,
                status,
                at,
            )
        })
    }

    /// Advances only application-query deadlines, without scheduling protocol polling writes.
    pub fn tick_application_query_transport(
        &mut self,
        token: &ConnectionAttemptToken,
        at: MonotonicTimestamp,
    ) -> bool {
        if !self.state.connection.is_verified(token) {
            return false;
        }
        if let Some(device) = self.device.as_mut() {
            device.advance_application_query_deadlines(at);
        }
        true
    }

    /// Borrows the shared session state without exposing connection internals.
    #[must_use]
    pub fn session_state(&self) -> &CutoutSessionState {
        &self.state
    }

    /// Mutably borrows shared session state for FFI-owned projections.
    pub fn session_state_mut(&mut self) -> &mut CutoutSessionState {
        &mut self.state
    }

    /// Borrows the retained detector evidence.
    #[must_use]
    pub fn detector(&self) -> &DeviceDetectionSession {
        &self.detector
    }

    /// Retains per-peripheral detection independently from refreshed advertisements.
    /// A new connection must still establish its own wire identity before admission.
    #[must_use]
    pub fn discovery_resolution(
        &self,
        platform_identifier: &str,
    ) -> Option<DeviceDetectionResolution> {
        if self
            .state
            .connection
            .snapshot()
            .token
            .as_ref()
            .is_some_and(|token| token.platform_identifier() == platform_identifier)
        {
            let resolution = self.detector.resolution(&self.state);
            if resolution.protocol != crate::ProtocolFamilyState::Unknown
                || resolution.model_banner.is_some()
            {
                return Some(resolution);
            }
        }
        self.detected_devices.get(platform_identifier).cloned()
    }

    fn retain_detection(&mut self) {
        if let Some(token) = self.state.connection.snapshot().token.as_ref()
            && let Some(resolution) = self.discovery_resolution(token.platform_identifier())
        {
            self.detected_devices
                .insert(token.platform_identifier().to_owned(), resolution);
        }
    }

    /// Replaces detector evidence when starting a new identification pass.
    pub fn reset_detector(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.state.connection.is_current(token) {
            return false;
        }
        self.detector = DeviceDetectionSession::default();
        true
    }

    /// Resets standalone detector evidence before connection admission.
    pub fn reset_detector_unscoped(&mut self) -> bool {
        if self.state.connection.snapshot().token.is_some() {
            return false;
        }
        self.detector = DeviceDetectionSession::default();
        true
    }

    /// Starts protocol-owned identification probes against the current session state.
    pub fn begin_identification_probes(
        &mut self,
        token: &ConnectionAttemptToken,
        at: MonotonicTimestamp,
    ) -> Option<crate::IdentificationProbePlan> {
        if !self.state.connection.is_current(token) {
            return None;
        }
        Some(
            self.detector
                .begin_identification_probes(&mut self.state, at),
        )
    }

    /// Begins probes for standalone discovery before connection admission.
    pub fn begin_identification_probes_unscoped(
        &mut self,
        at: MonotonicTimestamp,
    ) -> crate::IdentificationProbePlan {
        if self.state.connection.snapshot().token.is_some() {
            return crate::IdentificationProbePlan::Unsupported;
        }
        self.detector
            .begin_identification_probes(&mut self.state, at)
    }

    /// Expires protocol-owned probes and returns the missing probes.
    pub fn expire_pending_probes(
        &mut self,
        token: &ConnectionAttemptToken,
        now: MonotonicTimestamp,
        timeout: cutout_core::Duration,
    ) -> Vec<cutout_core::PendingProbe> {
        if !self.state.connection.is_current(token) {
            return Vec::new();
        }
        self.detector
            .expire_pending_probes(&mut self.state, now, timeout)
            .into_iter()
            .collect()
    }

    /// Marks all protocol-owned probes missing.
    pub fn mark_pending_probes_missing(
        &mut self,
        token: &ConnectionAttemptToken,
    ) -> Vec<cutout_core::PendingProbe> {
        if !self.state.connection.is_current(token) {
            return Vec::new();
        }
        self.detector
            .mark_pending_probes_missing(&mut self.state)
            .into_iter()
            .collect()
    }

    /// Observes detector evidence without exposing the detector's mutable state.
    pub fn observe_detection(
        &mut self,
        token: &ConnectionAttemptToken,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        self.observe_for_attempt(token, event)
    }

    /// Records an identification probe write at its monotonic start time.
    pub fn observe_probe_write_at(
        &mut self,
        token: &ConnectionAttemptToken,
        probe: cutout_core::PendingProbe,
        at: MonotonicTimestamp,
    ) -> Option<DeviceDetectionResolution> {
        if !self.state.connection.is_current(token) {
            return None;
        }
        Some(
            self.detector
                .observe_probe_write_at(&mut self.state, probe, at),
        )
    }

    /// Applies detector evidence for standalone discovery before an attempt exists.
    pub fn observe_detection_unscoped(
        &mut self,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        if self.state.connection.snapshot().token.is_some() {
            return None;
        }
        Some(self.detector.observe(&mut self.state, event))
    }

    /// Applies a standalone GATT observation before connection admission.
    pub fn observe_gatt_unscoped(
        &mut self,
        fingerprints: &[GattFingerprint],
    ) -> Option<DeviceDetectionResolution> {
        self.observe_detection_unscoped(DeviceDetectionEvent::Gatt { gatt: fingerprints })
    }

    /// Records a standalone probe write before connection admission.
    pub fn observe_probe_write_at_unscoped(
        &mut self,
        probe: cutout_core::PendingProbe,
        at: MonotonicTimestamp,
    ) -> Option<DeviceDetectionResolution> {
        if self.state.connection.snapshot().token.is_some() {
            return None;
        }
        Some(
            self.detector
                .observe_probe_write_at(&mut self.state, probe, at),
        )
    }

    /// Expires standalone probe evidence before connection admission.
    pub fn expire_pending_probes_unscoped(
        &mut self,
        now: MonotonicTimestamp,
        timeout: cutout_core::Duration,
    ) -> Vec<cutout_core::PendingProbe> {
        if self.state.connection.snapshot().token.is_some() {
            return Vec::new();
        }
        self.detector
            .expire_pending_probes(&mut self.state, now, timeout)
            .into_iter()
            .collect()
    }

    /// Marks standalone probe evidence missing before connection admission.
    pub fn mark_pending_probes_missing_unscoped(&mut self) -> Vec<cutout_core::PendingProbe> {
        if self.state.connection.snapshot().token.is_some() {
            return Vec::new();
        }
        self.detector
            .mark_pending_probes_missing(&mut self.state)
            .into_iter()
            .collect()
    }

    /// Replaces all device-scoped state before native work for the new attempt.
    pub fn begin_attempt(&mut self, platform_identifier: String, at: MonotonicTimestamp) {
        self.reset_for_attempt(&platform_identifier, at);
        self.state.connection.begin(platform_identifier, at);
    }

    /// Admits a Rust-approved retry and resets device-scoped state before native work.
    pub fn admit_retry(
        &mut self,
        retry: cutout_core::ConnectionRetryToken,
        at: MonotonicTimestamp,
    ) -> Option<ConnectionAttemptToken> {
        let token = self.state.connection.admit_retry(retry, at)?;
        self.reset_for_attempt(token.platform_identifier(), at);
        Some(token)
    }

    fn reset_for_attempt(&mut self, platform_identifier: &str, at: MonotonicTimestamp) {
        self.cancel_phone_drive(cutout_core::PhoneDriveCancelReason::ConnectionChanged);
        self.retain_detection();
        self.invalidated_capture_attempt = None;
        self.last_input_at = at;
        self.vesc_board_profile = None;
        self.user_vesc_drivetrains.clear();
        self.user_vehicle_classification = None;
        self.validation_grant = None;
        self.state.reset_device_identity();
        self.state
            .select_discovered_platform(platform_identifier.to_owned());
        self.state.settings = DeviceSettingsState::default();
        self.setting_operations.clear();
        self.state.actions.disconnect();
        self.detector = DeviceDetectionSession::default();
        self.device = None;
    }

    /// Prepares a local user geometry change for a verified standard VESC.
    #[must_use]
    pub fn prepare_vesc_drivetrain_selection(
        &self,
        token: &ConnectionAttemptToken,
        drivetrain: cutout_core::VescDrivetrain,
    ) -> Option<VescDrivetrainSelection> {
        self.prepare_vesc_controller_drivetrain_selection(
            token,
            cutout_core::VescDrivetrainTarget::LOCAL,
            drivetrain,
        )
    }

    /// Prepares controller-scoped user configuration without admitting CAN traffic or speed.
    #[must_use]
    pub fn prepare_vesc_controller_drivetrain_selection(
        &self,
        token: &ConnectionAttemptToken,
        target: cutout_core::VescDrivetrainTarget,
        drivetrain: cutout_core::VescDrivetrain,
    ) -> Option<VescDrivetrainSelection> {
        let identity = self.device.as_ref()?.identity();
        if !self.state.connection.is_verified(token)
            || identity.protocol != ProtocolFamily::Vesc
            || identity.vesc_application != Some(crate::VescApplicationDiscovery::Absent)
        {
            return None;
        }
        Some(VescDrivetrainSelection {
            token: token.clone(),
            drivetrain,
            target,
            previous_profile: self.profile_for_route(target),
        })
    }

    /// Applies durable geometry without sending controller configuration commands.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "consumes the prepared selection so it cannot be reused"
    )]
    pub fn apply_vesc_drivetrain_selection(
        &mut self,
        selection: VescDrivetrainSelection,
        profile: cutout_core::VescDrivetrainProfile,
    ) -> bool {
        if profile.value() != selection.drivetrain
            || profile.target() != selection.target
            || self
                .prepare_vesc_controller_drivetrain_selection(
                    &selection.token,
                    selection.target,
                    selection.drivetrain,
                )
                .is_none()
        {
            return false;
        }
        let current = self.profile_for_route(selection.target);
        if current == Some(profile) {
            return true;
        }
        // An older restore can populate an initially empty slot during persistence.
        // Only the same controller association can be superseded by this save.
        let supersedes_older_restore = selection.previous_profile.is_none()
            && current.is_some_and(|current| {
                current.target() == selection.target && current.revision() < profile.revision()
            });
        if (current != selection.previous_profile && !supersedes_older_restore)
            || current.is_some_and(|current| {
                current.target() == profile.target() && profile.revision() <= current.revision()
            })
        {
            return false;
        }
        self.install_user_drivetrain(profile);
        true
    }

    /// Current attempt's user geometry, independent of physical type.
    #[must_use]
    pub fn user_vesc_drivetrain(
        &self,
        token: &ConnectionAttemptToken,
    ) -> Option<cutout_core::VescDrivetrainProfile> {
        self.user_vesc_controller_drivetrain(token, cutout_core::VescDrivetrainTarget::LOCAL)
    }

    /// Configuration for this exact attempt and controller association, not hardware proof.
    #[must_use]
    pub fn user_vesc_controller_drivetrain(
        &self,
        token: &ConnectionAttemptToken,
        target: cutout_core::VescDrivetrainTarget,
    ) -> Option<cutout_core::VescDrivetrainProfile> {
        self.state
            .connection
            .is_current(token)
            .then(|| self.user_vesc_drivetrains.get(&target).copied())
            .flatten()
    }

    fn profile_for_route(
        &self,
        target: cutout_core::VescDrivetrainTarget,
    ) -> Option<cutout_core::VescDrivetrainProfile> {
        self.user_vesc_drivetrains
            .values()
            .find(|profile| profile.target().request_target() == target.request_target())
            .copied()
    }

    /// Restores saved facts unless an explicit edit or earlier restore already supplied geometry.
    pub fn restore_vesc_drivetrain(
        &mut self,
        token: &ConnectionAttemptToken,
        profile: cutout_core::VescDrivetrainProfile,
    ) -> bool {
        if profile.target() != cutout_core::VescDrivetrainTarget::LOCAL {
            return false;
        }
        self.restore_vesc_controller_drivetrain(token, profile)
    }

    /// Restores a known association without overwriting any edit on the same route.
    /// CAN restoration is explicit; it does not establish live controller applicability.
    pub fn restore_vesc_controller_drivetrain(
        &mut self,
        token: &ConnectionAttemptToken,
        profile: cutout_core::VescDrivetrainProfile,
    ) -> bool {
        if !self.state.connection.is_current(token)
            || self.profile_for_route(profile.target()).is_some()
        {
            return false;
        }
        match self.state.connection.snapshot().readiness {
            ConnectionReadiness::Pending | ConnectionReadiness::Verified => {}
            ConnectionReadiness::Disconnected
            | ConnectionReadiness::RecordOnly
            | ConnectionReadiness::Failed
            | ConnectionReadiness::Conflicted => return false,
        }
        if self.device.as_ref().is_some_and(|device| {
            let identity = device.identity();
            identity.protocol != ProtocolFamily::Vesc
                || identity.vesc_application == Some(crate::VescApplicationDiscovery::Refloat)
        }) {
            return false;
        }
        self.install_user_drivetrain(profile);
        true
    }

    fn install_user_drivetrain(&mut self, profile: cutout_core::VescDrivetrainProfile) {
        let target = profile.target();
        let previous = self.user_vesc_drivetrains.get(&target).copied();
        if previous != Some(profile) {
            self.cancel_phone_drive(cutout_core::PhoneDriveCancelReason::ConnectionChanged);
        }
        self.user_vesc_drivetrains
            .retain(|key, _| key.request_target() != target.request_target());
        self.user_vesc_drivetrains.insert(target, profile);
        if target == cutout_core::VescDrivetrainTarget::LOCAL
            && previous.map(cutout_core::VescDrivetrainProfile::value) != Some(profile.value())
            && let Some(device) = self.device.as_mut()
        {
            device.configure_user_drivetrain(profile.value());
        }
    }

    /// Immutable configured context for this verified Standard VESC attempt.
    /// Forwarded profiles remain configuration until CAN identity/correlation admits telemetry.
    #[must_use]
    pub fn configured_vesc_drivetrains(
        &self,
        token: &ConnectionAttemptToken,
    ) -> Vec<cutout_core::VescDrivetrainProfile> {
        let Some(identity) = self.device.as_ref().map(DeviceSession::identity) else {
            return Vec::new();
        };
        if !self.state.connection.is_verified(token)
            || identity.protocol != ProtocolFamily::Vesc
            || identity.vesc_application != Some(crate::VescApplicationDiscovery::Absent)
        {
            return Vec::new();
        }
        self.user_vesc_drivetrains.values().copied().collect()
    }

    fn effective_vesc_drivetrain_context(&self) -> Option<cutout_core::VescDrivetrainProfile> {
        let token = self.state.connection.snapshot().token.as_ref()?;
        let identity = self.device.as_ref()?.identity();
        (self.state.connection.is_verified(token)
            && identity.protocol == ProtocolFamily::Vesc
            && identity.vesc_application == Some(crate::VescApplicationDiscovery::Absent))
        .then(|| {
            self.user_vesc_drivetrains
                .get(&cutout_core::VescDrivetrainTarget::LOCAL)
                .copied()
        })
        .flatten()
    }

    /// Effective user geometry for this exact verified standard VESC attempt.
    ///
    /// Retained profiles are not effective during application discovery or after Refloat is
    /// identified. Stale callers cannot obtain the replacement attempt's context.
    #[must_use]
    pub fn vesc_drivetrain_context(
        &self,
        token: &ConnectionAttemptToken,
    ) -> Option<cutout_core::VescDrivetrainProfile> {
        if !self.state.connection.is_current(token) {
            return None;
        }
        self.effective_vesc_drivetrain_context()
    }

    /// Supplies saved controller geometry for this pending attempt without asserting identity.
    pub fn configure_vesc_board_profile(
        &mut self,
        token: &ConnectionAttemptToken,
        profile: crate::VescBoardProfile,
    ) -> bool {
        if !self.state.connection.is_current(token)
            || self.state.connection.snapshot().readiness != ConnectionReadiness::Pending
        {
            return false;
        }
        self.vesc_board_profile = Some(profile);
        true
    }

    /// Attaches a saved or newly selected physical type to this exact device attempt.
    ///
    /// The choice cannot supply protocol or model proof, authorize commands, or override Refloat.
    pub fn configure_vehicle_kind(
        &mut self,
        token: &ConnectionAttemptToken,
        kind: cutout_core::VehicleKind,
    ) -> bool {
        let Some(classification) = self.validate_vehicle_kind(token, kind) else {
            return false;
        };
        self.apply_vehicle_classification(classification)
    }

    /// Restores a saved association only before another choice has reached this attempt.
    ///
    /// A delayed storage result cannot replace a newer choice or cross a connection generation.
    pub fn restore_vehicle_kind(
        &mut self,
        token: &ConnectionAttemptToken,
        kind: cutout_core::VehicleKind,
    ) -> bool {
        if self.user_vehicle_classification.is_some() {
            return false;
        }
        self.configure_vehicle_kind(token, kind)
    }

    fn validate_vehicle_kind(
        &self,
        token: &ConnectionAttemptToken,
        kind: cutout_core::VehicleKind,
    ) -> Option<cutout_core::VehicleClassification> {
        if !self.state.connection.is_current(token) {
            return None;
        }
        match self.state.connection.snapshot().readiness {
            ConnectionReadiness::Pending | ConnectionReadiness::Verified => {}
            ConnectionReadiness::Disconnected
            | ConnectionReadiness::RecordOnly
            | ConnectionReadiness::Failed
            | ConnectionReadiness::Conflicted => return None,
        }
        if self
            .device
            .as_ref()
            .is_some_and(|device| device.identity().protocol != ProtocolFamily::Vesc)
        {
            return None;
        }
        cutout_core::VehicleClassification::user_provided(kind)
    }

    /// Validates a user action without changing the current classification.
    #[must_use]
    pub fn prepare_vehicle_kind_selection(
        &self,
        token: &ConnectionAttemptToken,
        kind: cutout_core::VehicleKind,
    ) -> Option<VehicleKindSelection> {
        let classification = self.validate_vehicle_kind(token, kind)?;
        let identity = self.device.as_ref()?.identity();
        if !self.state.connection.is_verified(token)
            || identity.vesc_application != Some(crate::VescApplicationDiscovery::Absent)
        {
            return None;
        }
        Some(VehicleKindSelection {
            token: token.clone(),
            classification,
        })
    }

    /// Applies a persisted choice only while its admitted attempt still owns the connection.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Consume the prepared selection so callers cannot reuse its admission."
    )]
    pub fn apply_vehicle_kind_selection(&mut self, selection: VehicleKindSelection) -> bool {
        if self
            .prepare_vehicle_kind_selection(&selection.token, selection.kind())
            .is_none()
        {
            return false;
        }
        self.apply_vehicle_classification(selection.classification)
    }

    fn apply_vehicle_classification(
        &mut self,
        classification: cutout_core::VehicleClassification,
    ) -> bool {
        if let Some(device) = self.device.as_mut()
            && !device.configure_vehicle_kind(classification.kind())
        {
            return false;
        }
        if self.user_vehicle_classification != Some(classification) {
            self.cancel_phone_drive(cutout_core::PhoneDriveCancelReason::ConnectionChanged);
        }
        self.user_vehicle_classification = Some(classification);
        true
    }

    /// Selects usable capacity only for the protocol established in this attempt.
    #[must_use]
    pub fn default_charge_profile(&self) -> Option<cutout_core::ChargeProfile> {
        let device = self.device.as_ref()?;
        match device.identity().protocol {
            ProtocolFamily::Vesc => self
                .vesc_board_profile
                .and_then(|profile| profile.charge_profile),
            _ => device.control_profile().default_charge_profile(),
        }
    }

    /// Ends the attempt before native cancellation or explicit selection replacement.
    pub fn disconnect(&mut self) {
        self.cancel_phone_drive(cutout_core::PhoneDriveCancelReason::ConnectionChanged);
        self.retain_detection();
        self.state.connection.disconnect();
        self.state.settings.disconnect();
        self.setting_operations.clear();
        self.state.actions.disconnect();
        self.device = None;
        self.validation_grant = None;
    }

    /// Records link availability and invalidates verified-session work after link loss.
    pub fn link_down(&mut self, token: &ConnectionAttemptToken) {
        if self.state.connection.is_current(token) {
            self.cancel_phone_drive(cutout_core::PhoneDriveCancelReason::ConnectionChanged);
        }
        self.state.connection.link_down(token);
        self.clear_failed_device();
    }

    /// Preserves pending detection as record-only; established-session failures are terminal.
    pub fn transport_failed(&mut self, token: &ConnectionAttemptToken) {
        if self.state.connection.is_current(token) {
            self.cancel_phone_drive(cutout_core::PhoneDriveCancelReason::ConnectionChanged);
        }
        self.state.connection.transport_failed(token);
        self.clear_failed_device();
    }

    /// Invalidates a verified decoder when a later notification proves it wrong.
    pub fn protocol_conflict(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.state.connection.conflict(token) {
            return false;
        }
        self.cancel_phone_drive(cutout_core::PhoneDriveCancelReason::ConnectionChanged);
        self.invalidated_capture_attempt = Some(token.clone());
        self.device = None;
        self.validation_grant = None;
        self.state.settings.disconnect();
        self.state.actions.disconnect();
        true
    }

    fn clear_failed_device(&mut self) {
        if self.state.connection.snapshot().readiness == ConnectionReadiness::Failed {
            self.device = None;
            self.state.settings.disconnect();
            self.state.actions.disconnect();
        }
    }

    /// Accepts detector evidence only from the attempt owning the native callback.
    #[must_use]
    pub fn observe_for_attempt(
        &mut self,
        token: &ConnectionAttemptToken,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        if !self.state.connection.is_current(token) {
            return None;
        }
        let resolution = self.detector.observe(&mut self.state, event);
        // Retain accepted evidence before a terminal transition retires its token.
        // Repeated telemetry with unchanged identity does not replace stored evidence.
        if resolution.protocol != crate::ProtocolFamilyState::Unknown
            || resolution.model_banner.is_some()
        {
            if let Some(previous) = self.detected_devices.get_mut(token.platform_identifier()) {
                if *previous != resolution {
                    previous.clone_from(&resolution);
                }
            } else {
                self.detected_devices
                    .insert(token.platform_identifier().to_owned(), resolution.clone());
            }
        }
        if resolution.protocol == crate::ProtocolFamilyState::Conflict
            && self.state.connection.snapshot().readiness == ConnectionReadiness::Verified
        {
            self.protocol_conflict(token);
        }
        Some(resolution)
    }

    /// Resolves readiness from retained wire evidence, respecting the whole-attempt deadline.
    pub fn resolve(
        &mut self,
        token: &ConnectionAttemptToken,
        identification_complete: bool,
        at: MonotonicTimestamp,
    ) {
        if !self.state.connection.is_current(token)
            || self.state.connection.snapshot().readiness != ConnectionReadiness::Pending
            || self.state.connection.expire(token, at)
        {
            return;
        }
        let resolution = self.detector.resolution(&self.state);
        // Native transport completion cannot retire an unanswered identity query.
        // An exact model may be admitted while optional firmware/IMU queries continue.
        if resolution.staged.model.is_none()
            && self
                .detector
                .next_probe_expiry(&self.state, cutout_core::Duration::from_milliseconds(0))
                .is_some()
        {
            return;
        }
        if let Some(mut device) =
            DeviceSession::from_detection_with_vesc_profile(&resolution, self.vesc_board_profile)
        {
            if let Some(drivetrain) = self
                .user_vesc_drivetrains
                .get(&cutout_core::VescDrivetrainTarget::LOCAL)
            {
                device.configure_user_drivetrain(drivetrain.value());
            }
            if let Some(classification) = self.user_vehicle_classification {
                let _ = device.configure_vehicle_kind(classification.kind());
            }
            let identity = device.identity();
            if (identification_complete
                || identity.model.is_some()
                || identity.protocol == ProtocolFamily::Vesc)
                && self.state.connection.finish_detection(token, true)
            {
                self.device = Some(device);
            }
        } else if identification_complete {
            self.state.connection.finish_detection(token, false);
        }
    }

    /// Decodes transport input and read-only commands for the current verified attempt.
    /// Device writes enter through validated semantic settings or action submission.
    #[must_use]
    pub fn ingest(
        &mut self,
        token: &ConnectionAttemptToken,
        input: &SessionInputDto,
    ) -> Option<DeviceConnectionStep> {
        if let SessionInputDto::Command(command) | SessionInputDto::CommandAt { command, .. } =
            input
            && cutout_core::DeviceCommand::from(*command).safety_class()
                != cutout_core::SafetyClass::ReadOnly
        {
            return None;
        }
        self.ingest_validated(token, input)
    }

    fn ingest_validated(
        &mut self,
        token: &ConnectionAttemptToken,
        input: &SessionInputDto,
    ) -> Option<DeviceConnectionStep> {
        if !self.state.connection.is_verified(token) {
            return None;
        }
        if let SessionInputDto::Notification { monotonic_ms, .. }
        | SessionInputDto::Tick { monotonic_ms }
        | SessionInputDto::CommandAt { monotonic_ms, .. }
        | SessionInputDto::LinkUp { monotonic_ms, .. } = input
        {
            self.last_input_at = self
                .last_input_at
                .max(MonotonicTimestamp::new(monotonic_ms.milliseconds));
            self.state.settings.tick(self.last_input_at);
        }
        let device = self.device.as_mut()?;
        if let SessionInputDto::CommandAt {
            command,
            monotonic_ms,
        } = input
        {
            let command = cutout_core::DeviceCommand::from(*command);
            let at = MonotonicTimestamp::new(monotonic_ms.milliseconds);
            if command.safety_class() == cutout_core::SafetyClass::StationaryOnly {
                let snapshot = device.current_snapshot();
                let operating_state = cutout_core::RideOperatingState::resolve(
                    snapshot.operating_state.map(Into::into),
                    snapshot.charge_mode.map(|mode| mode.value.into()),
                    snapshot
                        .speed
                        .map(|speed| cutout_core::Speed::from_millimetres_per_second(speed.value)),
                );
                let _ = device.arm_settings_writes(
                    operating_state.into(),
                    snapshot.speed.map(|speed| speed.value),
                    at.get(),
                );
            }
        }
        let identity_before = device.identity();
        let framing_before = device.notification_buffer_state();
        let result = device.ingest_typed(input);
        if let Some(operation) = device.take_failed_setting_operation() {
            self.state
                .settings
                .fail_operation(operation, self.last_input_at);
        }
        let notification_framing = match (input, framing_before, device.notification_buffer_state())
        {
            (
                SessionInputDto::Notification { .. },
                crate::NotificationBufferState::Empty,
                crate::NotificationBufferState::Empty,
            ) => NotificationFramingEvidence::EmptyBoundaries,
            _ => NotificationFramingEvidence::Unclassified,
        };
        let telemetry = device.current_snapshot();
        let diagnostics = device.diagnostics();
        if let SessionInputDto::Notification { monotonic_ms, .. } = input {
            let received_at = MonotonicTimestamp::new(monotonic_ms.milliseconds);
            let profile = device.control_profile();
            for output in &result.outputs {
                let SessionOutput::Event(DeviceEvent::ReadOnlyResponse(response)) = output else {
                    continue;
                };
                let ReadOnlyResponse::Settings(readback) = response else {
                    continue;
                };
                profile.apply_action_readback(&mut self.state.actions, *readback, received_at);
                for observation in profile.normalize_readback(*readback) {
                    if let Some(value) = observation.value {
                        self.state
                            .settings
                            .observe_measured(observation.id, value, received_at);
                    } else {
                        self.state
                            .settings
                            .invalidate_readback(observation.id, received_at);
                    }
                }
            }
        }
        let identity_after = device.identity();
        if identity_before.classification != identity_after.classification
            || identity_before.vesc_application != identity_after.vesc_application
            || if let SessionInputDto::LinkDown = input {
                true
            } else {
                false
            }
        {
            self.cancel_phone_drive(cutout_core::PhoneDriveCancelReason::ConnectionChanged);
        }
        Some(DeviceConnectionStep {
            notification_framing,
            session: self.snapshot(),
            result,
            telemetry,
            vesc_drivetrain_context: self.effective_vesc_drivetrain_context(),
            configured_vesc_drivetrains: self.configured_vesc_drivetrains(token),
            diagnostics,
        })
    }

    /// Reads bounded raw-page evidence with its current connection and model identity.
    #[must_use]
    pub fn raw_settings_snapshot(&self) -> crate::RawSettingsSnapshot {
        crate::RawSettingsSnapshot {
            session: self.snapshot(),
            pages: self
                .device
                .as_ref()
                .map_or_else(Vec::new, |device| device.raw_settings_pages().to_vec()),
        }
    }

    /// Reconciles capture identity for an exact current or invalidated attempt.
    /// Local vehicle classification and drivetrain geometry do not establish identity.
    /// Ordinary disconnect preserves historical proof; only contradictory evidence revokes it.
    #[must_use]
    pub fn capture_identity(
        &self,
        token: &ConnectionAttemptToken,
    ) -> Option<CaptureIdentityUpdate> {
        if self.invalidated_capture_attempt.as_ref() == Some(token) {
            return Some(CaptureIdentityUpdate::Invalidated);
        }
        if !self.state.connection.is_current(token) {
            return None;
        }
        if !self.state.connection.is_verified(token) {
            return Some(CaptureIdentityUpdate::Unresolved);
        }
        let identity = self.device.as_ref()?.identity();
        Some(CaptureIdentityUpdate::Resolved(
            cutout_core::PevcapResolvedIdentity {
                protocol_family: Some(identity.protocol),
                model: identity.model.map(|model| cutout_core::VerifiedValue {
                    value: format!("{} {}", model.manufacturer, model.model),
                    verification: cutout_core::VerificationStatus::HardwareVerified,
                }),
                firmware: None,
            },
        ))
    }

    /// Returns an immutable connection and identity snapshot.
    #[must_use]
    pub fn snapshot(&self) -> DeviceConnectionSnapshot {
        DeviceConnectionSnapshot {
            connection: self.state.connection.snapshot().clone(),
            identity: self.device.as_ref().map(DeviceSession::identity),
        }
    }

    /// Returns the selected decoder's current normalized evidence, including charging inference.
    #[must_use]
    pub fn telemetry_snapshot(&self) -> Option<TelemetrySnapshotDto> {
        self.device.as_ref().map(DeviceSession::current_snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_drive_generic_vesc_classification_never_qualifies_motor_authority() {
        use cutout_core::{
            PhoneControlMode, PhoneDriveContacts, PhoneDriveUnavailableReason, VehicleKind,
        };
        let mut owner = DeviceConnectionSession::default();
        assert_eq!(
            owner.phone_drive_unavailability(),
            PhoneDriveUnavailableReason::Disconnected
        );
        let token = begin(&mut owner, "phone-board");
        assert!(owner.configure_vehicle_kind(&token, VehicleKind::Eskate));
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        assert_eq!(
            owner.snapshot().identity.unwrap().vehicle_kind(),
            VehicleKind::Eskate
        );
        for mode in [
            PhoneControlMode::Disabled,
            PhoneControlMode::Buttons,
            PhoneControlMode::Area,
        ] {
            owner.set_phone_control_mode(mode);
            let generation = owner.phone_drive_input_snapshot().generation.unwrap();
            owner.observe_phone_drive_contacts(&token, generation, PhoneDriveContacts::Released);
            assert_eq!(
                owner.phone_drive_unavailability(),
                PhoneDriveUnavailableReason::ControllerUnqualified
            );
        }
    }

    #[test]
    fn phone_drive_contacts_cannot_change_transport_transcripts_or_survive_replacement() {
        use cutout_core::{
            PhoneControlMode, PhoneDriveAction, PhoneDriveCancelReason, PhoneDriveContactId,
            PhoneDriveContacts, PhoneDriveInputPhase, VehicleKind,
        };
        fn board() -> (DeviceConnectionSession, ConnectionAttemptToken) {
            let mut owner = DeviceConnectionSession::default();
            let token = begin(&mut owner, "phone-board");
            assert!(owner.configure_vehicle_kind(&token, VehicleKind::Eskate));
            identify_vesc(&mut owner, &token);
            let _ = drivetrain_tick(&mut owner, &token, 3_000);
            owner.set_phone_control_mode(PhoneControlMode::Buttons);
            (owner, token)
        }
        let (mut owner, first) = board();
        let (mut baseline, baseline_token) = board();
        let generation = owner.phone_drive_input_snapshot().generation.unwrap();
        owner.observe_phone_drive_contacts(&first, generation, PhoneDriveContacts::Released);
        owner.observe_phone_drive_contacts(
            &first,
            generation,
            PhoneDriveContacts::Single {
                contact: PhoneDriveContactId::new(1).unwrap(),
                region: Some(PhoneDriveAction::Throttle),
            },
        );
        assert_eq!(
            owner.phone_drive_input_snapshot().phase,
            PhoneDriveInputPhase::Pressed(PhoneDriveAction::Throttle)
        );
        for at in [3_001, 3_100, 4_000] {
            let observed = drivetrain_tick(&mut owner, &first, at);
            let unchanged = drivetrain_tick(&mut baseline, &baseline_token, at);
            assert_eq!(observed.result.outputs, unchanged.result.outputs);
            assert_eq!(observed.telemetry, unchanged.telemetry);
        }
        let replacement = begin(&mut owner, "phone-board");
        assert_eq!(
            owner.phone_drive_input_snapshot().phase,
            PhoneDriveInputPhase::Cancelled(PhoneDriveCancelReason::ConnectionChanged)
        );
        let replaced = owner.phone_drive_input_snapshot();
        owner.observe_phone_drive_contacts(&first, generation, PhoneDriveContacts::Released);
        owner.link_down(&first);
        assert_eq!(owner.phone_drive_input_snapshot(), replaced);
        assert_ne!(replacement, first);
    }

    #[test]
    fn phone_drive_disconnect_and_geometry_edits_retire_held_semantic_input() {
        use cutout_core::{
            PhoneControlMode, PhoneDriveAction, PhoneDriveCancelReason, PhoneDriveContactId,
            PhoneDriveContacts, PhoneDriveInputPhase, PhoneDriveUnavailableReason, VehicleKind,
        };
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "phone-board");
        assert!(owner.configure_vehicle_kind(&token, VehicleKind::Eskate));
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        owner.set_phone_control_mode(PhoneControlMode::Area);
        let generation = owner.phone_drive_input_snapshot().generation.unwrap();
        owner.observe_phone_drive_contacts(&token, generation, PhoneDriveContacts::Released);
        owner.observe_phone_drive_contacts(
            &token,
            generation,
            PhoneDriveContacts::Single {
                contact: PhoneDriveContactId::new(1).unwrap(),
                region: Some(PhoneDriveAction::Brake),
            },
        );
        let profile = drivetrain_profile("90", 1);
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, profile.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, profile));
        assert_eq!(
            owner.phone_drive_input_snapshot().phase,
            PhoneDriveInputPhase::Cancelled(PhoneDriveCancelReason::ConnectionChanged)
        );
        owner.disconnect();
        assert_eq!(
            owner.phone_drive_unavailability(),
            PhoneDriveUnavailableReason::Disconnected
        );
        assert_ne!(
            owner.phone_drive_input_snapshot().generation,
            Some(generation)
        );
    }

    const VESC_REPLY: &[u8] = &[
        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
        38, 208, 3,
    ];

    fn begin(owner: &mut DeviceConnectionSession, name: &str) -> ConnectionAttemptToken {
        owner.begin_attempt(name.into(), MonotonicTimestamp::new(0));
        let token = owner.snapshot().connection.token.unwrap();
        owner.state.connection.connected(&token);
        token
    }

    fn identify_vesc(owner: &mut DeviceConnectionSession, token: &ConnectionAttemptToken) {
        let _ = owner.observe_for_attempt(
            token,
            DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
        );
        owner.resolve(token, false, MonotonicTimestamp::new(1));
        let step = owner
            .ingest(
                token,
                &SessionInputDto::LinkUp {
                    monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                    max_write_len: None,
                },
            )
            .unwrap();
        let query =
            step.result
                .outputs
                .iter()
                .find_map(|output| match output {
                    cutout_core::SessionOutput::Transport(
                        cutout_core::TransportAction::Write { bytes, .. },
                    ) => bytes.application_query(),
                    _ => None,
                })
                .expect("startup owns the bounded application query");
        assert!(owner.mark_application_query_transport(
            token,
            query.id.get(),
            0,
            1,
            cutout_core::ApplicationQueryTransportStatus::Submitted,
            MonotonicTimestamp::new(1)
        ));
    }

    #[test]
    fn controller_drivetrain_changes_cannot_modify_local_speed_or_reuse_replaced_target() {
        use cutout_core::{VescControllerId, VescDrivetrainTarget};
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "scooter");
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        let local = drivetrain_profile("90", 1);
        assert!(owner.restore_vesc_drivetrain(&token, local));
        let measured = owner
            .ingest(
                &token,
                &SessionInputDto::Notification {
                    channel: crate::VESC_NOTIFY_CHANNEL.as_bytes(),
                    bytes: vec![2, 9, 50, 0, 0, 0, 128, 0, 0, 23, 112, 244, 198, 3],
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 4_000,
                    },
                },
            )
            .unwrap();
        assert!(measured.telemetry.speed.is_some());
        let can = VescDrivetrainTarget::can(VescControllerId::new(7), [1; 12]).unwrap();
        let replacement = VescDrivetrainTarget::can(VescControllerId::new(7), [2; 12]).unwrap();
        let geometry = cutout_core::VescDrivetrain::parse("250", "15", "2.5").unwrap();
        let profile = cutout_core::VescDrivetrainProfile::for_target(
            can,
            geometry,
            cutout_core::VescDrivetrainRevision::new(1).unwrap(),
        );
        let selection = owner
            .prepare_vesc_controller_drivetrain_selection(&token, can, geometry)
            .unwrap();
        assert!(!owner.apply_vesc_drivetrain_selection(
            selection,
            cutout_core::VescDrivetrainProfile::user_configured(
                geometry,
                cutout_core::VescDrivetrainRevision::new(1).unwrap()
            )
        ));
        let selection = owner
            .prepare_vesc_controller_drivetrain_selection(&token, can, geometry)
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, profile));
        let captured = drivetrain_tick(&mut owner, &token, 4_001);
        assert_eq!(captured.telemetry, measured.telemetry);
        assert_eq!(captured.vesc_drivetrain_context, Some(local));
        assert_eq!(captured.configured_vesc_drivetrains, vec![local, profile]);
        assert_eq!(
            owner.user_vesc_controller_drivetrain(&token, replacement),
            None
        );
        let edited = cutout_core::VescDrivetrainProfile::for_target(
            can,
            local.value(),
            cutout_core::VescDrivetrainRevision::new(2).unwrap(),
        );
        let selection = owner
            .prepare_vesc_controller_drivetrain_selection(&token, can, edited.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, edited));
        let after = drivetrain_tick(&mut owner, &token, 4_002);
        assert_eq!(after.telemetry, measured.telemetry);
        assert_eq!(captured.configured_vesc_drivetrains, vec![local, profile]);
        let stale = owner
            .prepare_vesc_controller_drivetrain_selection(&token, can, geometry)
            .unwrap();
        let next = begin(&mut owner, "scooter");
        assert!(!owner.apply_vesc_drivetrain_selection(stale, profile));
        assert_eq!(owner.user_vesc_controller_drivetrain(&next, can), None);
    }

    #[test]
    fn controller_drivetrain_newer_durable_save_wins_over_older_same_target_restore() {
        use cutout_core::{
            VescControllerId, VescDrivetrainProfile, VescDrivetrainRevision, VescDrivetrainTarget,
        };
        for target in [
            VescDrivetrainTarget::LOCAL,
            VescDrivetrainTarget::can(VescControllerId::new(7), [1; 12]).unwrap(),
        ] {
            let mut owner = DeviceConnectionSession::default();
            let token = begin(&mut owner, "scooter");
            identify_vesc(&mut owner, &token);
            let _ = drivetrain_tick(&mut owner, &token, 3_000);
            let older = VescDrivetrainProfile::for_target(
                target,
                cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap(),
                VescDrivetrainRevision::new(1).unwrap(),
            );
            let newer = VescDrivetrainProfile::for_target(
                target,
                cutout_core::VescDrivetrain::parse("100", "7", "2.5").unwrap(),
                VescDrivetrainRevision::new(2).unwrap(),
            );
            let pending = owner
                .prepare_vesc_controller_drivetrain_selection(&token, target, newer.value())
                .unwrap();
            assert!(owner.restore_vesc_controller_drivetrain(&token, older));
            assert!(owner.apply_vesc_drivetrain_selection(pending, newer));
            assert_eq!(
                owner.user_vesc_controller_drivetrain(&token, target),
                Some(newer)
            );
        }
    }

    #[test]
    fn controller_drivetrain_pending_save_cannot_override_a_restored_other_association() {
        use cutout_core::{
            VescControllerId, VescDrivetrainProfile, VescDrivetrainRevision, VescDrivetrainTarget,
        };
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "scooter");
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        let old_target = VescDrivetrainTarget::can(VescControllerId::new(7), [1; 12]).unwrap();
        let replacement = VescDrivetrainTarget::can(VescControllerId::new(7), [2; 12]).unwrap();
        let value = cutout_core::VescDrivetrain::parse("90", "7", "2.5").unwrap();
        let pending = owner
            .prepare_vesc_controller_drivetrain_selection(&token, old_target, value)
            .unwrap();
        let restored = VescDrivetrainProfile::for_target(
            replacement,
            value,
            VescDrivetrainRevision::new(1).unwrap(),
        );
        assert!(owner.restore_vesc_controller_drivetrain(&token, restored));
        let old_receipt = VescDrivetrainProfile::for_target(
            old_target,
            value,
            VescDrivetrainRevision::new(99).unwrap(),
        );
        assert!(!owner.apply_vesc_drivetrain_selection(pending, old_receipt));
        assert_eq!(owner.configured_vesc_drivetrains(&token), vec![restored]);
    }

    #[test]
    fn controller_drivetrain_replacement_fences_pending_edits_on_the_same_can_route() {
        use cutout_core::{
            VescControllerId, VescDrivetrainProfile, VescDrivetrainRevision, VescDrivetrainTarget,
        };
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "scooter");
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        let first = VescDrivetrainTarget::can(VescControllerId::new(7), [1; 12]).unwrap();
        let replacement = VescDrivetrainTarget::can(VescControllerId::new(7), [2; 12]).unwrap();
        let value = cutout_core::VescDrivetrain::parse("250", "15", "2.5").unwrap();
        let old = VescDrivetrainProfile::for_target(
            first,
            value,
            VescDrivetrainRevision::new(1).unwrap(),
        );
        assert!(owner.restore_vesc_controller_drivetrain(&token, old));
        let captured = drivetrain_tick(&mut owner, &token, 3_001);
        let stale = owner
            .prepare_vesc_controller_drivetrain_selection(&token, first, value)
            .unwrap();
        let replacement_selection = owner
            .prepare_vesc_controller_drivetrain_selection(&token, replacement, value)
            .unwrap();
        let new = VescDrivetrainProfile::for_target(
            replacement,
            value,
            VescDrivetrainRevision::new(1).unwrap(),
        );
        assert!(owner.apply_vesc_drivetrain_selection(replacement_selection, new));
        assert!(!owner.apply_vesc_drivetrain_selection(stale, old));
        assert!(!owner.restore_vesc_controller_drivetrain(&token, old));
        assert_eq!(owner.user_vesc_controller_drivetrain(&token, first), None);
        assert_eq!(owner.configured_vesc_drivetrains(&token), vec![new]);
        assert_eq!(captured.configured_vesc_drivetrains, vec![old]);
        assert_eq!(captured.vesc_drivetrain_context, None);
    }

    #[test]
    fn drivetrain_context_is_absent_without_profile_and_while_application_is_pending() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        let profile = drivetrain_profile("90", 4);
        assert!(owner.restore_vesc_drivetrain(&token, profile));
        identify_vesc(&mut owner, &token);
        assert_eq!(owner.vesc_drivetrain_context(&token), None);
        assert_eq!(
            drivetrain_tick(&mut owner, &token, 2).vesc_drivetrain_context,
            None
        );
        assert_eq!(
            drivetrain_tick(&mut owner, &token, 3_000).vesc_drivetrain_context,
            Some(profile)
        );
        assert_eq!(owner.vesc_drivetrain_context(&token), Some(profile));

        let replacement = begin(&mut owner, "B");
        assert_eq!(owner.vesc_drivetrain_context(&token), None);
        assert_eq!(owner.vesc_drivetrain_context(&replacement), None);
        identify_vesc(&mut owner, &replacement);
        assert_eq!(
            drivetrain_tick(&mut owner, &replacement, 3_000).vesc_drivetrain_context,
            None
        );
    }

    #[test]
    fn drivetrain_save_requires_matching_durable_value_and_advancing_revision() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        let first = drivetrain_profile("90", 4);
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, first.value())
            .unwrap();
        assert!(!owner.apply_vesc_drivetrain_selection(selection, drivetrain_profile("100", 4)));
        assert_eq!(owner.user_vesc_drivetrain(&token), None);
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, first.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, first));
        let conflicting = drivetrain_profile("100", 4);
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, conflicting.value())
            .unwrap();
        assert!(!owner.apply_vesc_drivetrain_selection(selection, conflicting));
        let lower = drivetrain_profile("90", 3);
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, lower.value())
            .unwrap();
        assert!(!owner.apply_vesc_drivetrain_selection(selection, lower));
        let original_step = drivetrain_tick(&mut owner, &token, 3_001);
        assert_eq!(original_step.vesc_drivetrain_context, Some(first));
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, first.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, first));
        let same_value = drivetrain_profile("90", 5);
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, same_value.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, same_value));
        assert_eq!(
            drivetrain_tick(&mut owner, &token, 3_002).vesc_drivetrain_context,
            Some(same_value)
        );
        let changed = drivetrain_profile("100", 6);
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, changed.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, changed));
        assert_eq!(
            drivetrain_tick(&mut owner, &token, 3_003).vesc_drivetrain_context,
            Some(changed)
        );
        assert_eq!(original_step.vesc_drivetrain_context, Some(first));
    }

    #[test]
    fn drivetrain_exact_durable_receipt_succeeds_after_identical_restore() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        let profile = drivetrain_profile("90", 1);
        let prepared = owner
            .prepare_vesc_drivetrain_selection(&token, profile.value())
            .unwrap();
        assert!(owner.restore_vesc_drivetrain(&token, profile));
        let measured = owner
            .ingest(
                &token,
                &SessionInputDto::Notification {
                    channel: crate::VESC_NOTIFY_CHANNEL.as_bytes(),
                    bytes: vec![2, 9, 50, 0, 0, 0, 128, 0, 0, 23, 112, 244, 198, 3],
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 4_000,
                    },
                },
            )
            .unwrap();
        assert!(measured.telemetry.speed.is_some());
        assert!(owner.apply_vesc_drivetrain_selection(prepared, profile));
        let unchanged = drivetrain_tick(&mut owner, &token, 4_001);
        assert_eq!(unchanged.telemetry, measured.telemetry);
        assert_eq!(unchanged.vesc_drivetrain_context, Some(profile));
    }

    #[test]
    fn drivetrain_unchanged_durable_profile_preserves_calculated_speed_observation() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        let profile = drivetrain_profile("90", 1);
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, profile.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, profile));
        let measured = owner
            .ingest(
                &token,
                &SessionInputDto::Notification {
                    channel: crate::VESC_NOTIFY_CHANNEL.as_bytes(),
                    bytes: vec![2, 9, 50, 0, 0, 0, 128, 0, 0, 23, 112, 244, 198, 3],
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 4_000,
                    },
                },
            )
            .unwrap();
        assert!(measured.telemetry.speed.is_some());
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, profile.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, profile));
        let unchanged = drivetrain_tick(&mut owner, &token, 4_001);
        assert_eq!(unchanged.telemetry, measured.telemetry);
        assert_eq!(unchanged.vesc_drivetrain_context, Some(profile));
    }

    #[test]
    fn drivetrain_stale_save_and_restore_cannot_replace_newer_context() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        let first = drivetrain_profile("90", 1);
        let old_selection = owner
            .prepare_vesc_drivetrain_selection(&token, first.value())
            .unwrap();
        let newer = drivetrain_profile("100", 2);
        let newer_selection = owner
            .prepare_vesc_drivetrain_selection(&token, newer.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(newer_selection, newer));
        assert!(!owner.apply_vesc_drivetrain_selection(old_selection, first));
        assert!(!owner.restore_vesc_drivetrain(&token, drivetrain_profile("90", 3)));
        assert_eq!(owner.user_vesc_drivetrain(&token), Some(newer));
        let retired_selection = owner
            .prepare_vesc_drivetrain_selection(&token, first.value())
            .unwrap();
        let replacement = begin(&mut owner, "A");
        assert!(
            !owner.apply_vesc_drivetrain_selection(retired_selection, drivetrain_profile("90", 4))
        );
        assert!(!owner.restore_vesc_drivetrain(&token, first));
        assert_eq!(owner.user_vesc_drivetrain(&replacement), None);
    }

    #[test]
    fn late_refloat_removes_effective_drivetrain_context_from_the_same_step() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        identify_vesc(&mut owner, &token);
        let _ = drivetrain_tick(&mut owner, &token, 3_000);
        let profile = drivetrain_profile("90", 1);
        let selection = owner
            .prepare_vesc_drivetrain_selection(&token, profile.value())
            .unwrap();
        assert!(owner.apply_vesc_drivetrain_selection(selection, profile));
        let standard = drivetrain_tick(&mut owner, &token, 3_001);
        assert_eq!(standard.vesc_drivetrain_context, Some(profile));
        let mut payload = [0_u8; 60];
        payload[..4].copy_from_slice(&[101, 0, 2, 0]);
        payload[4..11].copy_from_slice(b"Refloat");
        payload[24..27].copy_from_slice(&[1, 2, 3]);
        payload[51..55].copy_from_slice(&10_000_u32.to_be_bytes());
        let mut frame = arrayvec::ArrayVec::new();
        crate::refloat_codec::encode_custom_app_frame(&payload, &mut frame).unwrap();
        let refloat = owner
            .ingest(
                &token,
                &SessionInputDto::Notification {
                    channel: crate::VESC_NOTIFY_CHANNEL.as_bytes(),
                    bytes: frame.to_vec(),
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 3_002,
                    },
                },
            )
            .unwrap();
        assert_eq!(
            refloat.session.identity.unwrap().vesc_application,
            Some(crate::VescApplicationDiscovery::Refloat)
        );
        assert_eq!(refloat.vesc_drivetrain_context, None);
        assert_eq!(owner.vesc_drivetrain_context(&token), None);
        assert_eq!(
            drivetrain_tick(&mut owner, &token, 3_003).vesc_drivetrain_context,
            None
        );
        assert_eq!(standard.vesc_drivetrain_context, Some(profile));
    }

    fn drivetrain_profile(wheel: &str, revision: u64) -> cutout_core::VescDrivetrainProfile {
        cutout_core::VescDrivetrainProfile::user_configured(
            cutout_core::VescDrivetrain::parse(wheel, "7", "2.5").unwrap(),
            cutout_core::VescDrivetrainRevision::new(revision).unwrap(),
        )
    }

    fn drivetrain_tick(
        owner: &mut DeviceConnectionSession,
        token: &ConnectionAttemptToken,
        at: u64,
    ) -> DeviceConnectionStep {
        owner
            .ingest(
                token,
                &SessionInputDto::Tick {
                    monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: at },
                },
            )
            .unwrap()
    }

    #[test]
    fn vehicle_classification_restore_cannot_replace_a_newer_choice_or_attempt() {
        use cutout_core::VehicleKind;
        let mut owner = DeviceConnectionSession::default();
        let first = begin(&mut owner, "A");
        identify_vesc(&mut owner, &first);
        let _ = owner
            .ingest(
                &first,
                &SessionInputDto::Tick {
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 3_000,
                    },
                },
            )
            .unwrap();
        let choice = owner
            .prepare_vehicle_kind_selection(&first, VehicleKind::Eskate)
            .unwrap();
        assert!(owner.apply_vehicle_kind_selection(choice));
        assert!(!owner.restore_vehicle_kind(&first, VehicleKind::Ebike));
        assert_eq!(
            owner.snapshot().identity.unwrap().vehicle_kind(),
            VehicleKind::Eskate
        );
        let second = begin(&mut owner, "B");
        assert!(!owner.restore_vehicle_kind(&first, VehicleKind::Emoto));
        assert!(owner.restore_vehicle_kind(&second, VehicleKind::Scooter));
        assert!(!owner.restore_vehicle_kind(&second, VehicleKind::Ebike));
        identify_vesc(&mut owner, &second);
        let _ = owner
            .ingest(
                &second,
                &SessionInputDto::Tick {
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 3_000,
                    },
                },
            )
            .unwrap();
        assert_eq!(
            owner.snapshot().identity.unwrap().vehicle_kind(),
            VehicleKind::Scooter
        );
    }

    #[test]
    fn vehicle_classification_choice_is_attempt_scoped_and_waits_for_application_discovery() {
        use cutout_core::{
            VehicleClassificationProvenance, VehicleClassificationRequirement, VehicleKind,
        };
        let mut owner = DeviceConnectionSession::default();
        let first = begin(&mut owner, "A");
        assert!(owner.configure_vehicle_kind(&first, VehicleKind::Eskate));
        identify_vesc(&mut owner, &first);
        let pending = owner.snapshot().identity.unwrap();
        assert_eq!(
            pending.classification_requirement,
            VehicleClassificationRequirement::Pending
        );
        assert_eq!(pending.vehicle_kind(), VehicleKind::Unknown);
        let _ = owner
            .ingest(
                &first,
                &SessionInputDto::Tick {
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 3_000,
                    },
                },
            )
            .unwrap();
        let classified = owner.snapshot().identity.unwrap();
        assert_eq!(
            classified.classification_requirement,
            VehicleClassificationRequirement::Resolved
        );
        assert_eq!(classified.vehicle_kind(), VehicleKind::Eskate);
        assert_eq!(
            classified.classification.provenance(),
            VehicleClassificationProvenance::UserProvided
        );
        assert_eq!(classified.model, None);

        let replacement = begin(&mut owner, "B");
        assert!(!owner.configure_vehicle_kind(&first, VehicleKind::Emoto));
        identify_vesc(&mut owner, &replacement);
        let _ = owner
            .ingest(
                &replacement,
                &SessionInputDto::Tick {
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 3_000,
                    },
                },
            )
            .unwrap();
        let unclassified = owner.snapshot().identity.unwrap();
        assert_eq!(
            unclassified.classification_requirement,
            VehicleClassificationRequirement::UserSelectionRequired
        );
        assert_eq!(unclassified.vehicle_kind(), VehicleKind::Unknown);
        assert!(!owner.configure_vehicle_kind(&replacement, VehicleKind::Unknown));
        assert!(owner.configure_vehicle_kind(&replacement, VehicleKind::Scooter));
        assert_eq!(
            owner.snapshot().identity.unwrap().vehicle_kind(),
            VehicleKind::Scooter
        );
        assert_eq!(owner.snapshot().identity.unwrap().model, None);
        assert_eq!(
            owner
                .device
                .as_ref()
                .unwrap()
                .control_profile()
                .descriptors(true),
            [] as [crate::device_settings::SettingDescriptor; 0]
        );
    }

    #[test]
    fn vehicle_classification_validated_refloat_overrides_user_choice_without_model_proof() {
        use cutout_core::{
            VehicleClassificationProvenance, VehicleClassificationRequirement, VehicleKind,
        };
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        assert!(owner.configure_vehicle_kind(&token, VehicleKind::Ebike));
        identify_vesc(&mut owner, &token);
        let mut payload = [0_u8; 60];
        payload[..4].copy_from_slice(&[101, 0, 2, 0]);
        payload[4..11].copy_from_slice(b"Refloat");
        payload[24..27].copy_from_slice(&[1, 2, 3]);
        payload[51..55].copy_from_slice(&10_000_u32.to_be_bytes());
        let mut frame = arrayvec::ArrayVec::new();
        crate::refloat_codec::encode_custom_app_frame(&payload, &mut frame).unwrap();
        let input = |bytes| SessionInputDto::Notification {
            channel: crate::VESC_NOTIFY_CHANNEL.as_bytes(),
            bytes,
            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 2 },
        };
        let mut corrupt = frame.to_vec();
        let crc_index = corrupt.len() - 3;
        corrupt[crc_index] ^= 1;
        let _ = owner.ingest(&token, &input(corrupt)).unwrap();
        assert_eq!(
            owner
                .snapshot()
                .identity
                .unwrap()
                .classification_requirement,
            VehicleClassificationRequirement::Pending
        );
        let _ = owner.ingest(&token, &input(frame.to_vec())).unwrap();
        let observed = owner.snapshot().identity.unwrap();
        assert_eq!(observed.vehicle_kind(), VehicleKind::Onewheel);
        assert_eq!(
            observed.classification.provenance(),
            VehicleClassificationProvenance::Protocol
        );
        assert_eq!(
            observed.classification_requirement,
            VehicleClassificationRequirement::Resolved
        );
        assert_eq!(
            observed.vesc_application,
            Some(crate::VescApplicationDiscovery::Refloat)
        );
        assert_eq!(observed.model, None);
        assert!(owner.configure_vehicle_kind(&token, VehicleKind::Eskate));
        assert_eq!(
            owner.snapshot().identity.unwrap().vehicle_kind(),
            VehicleKind::Onewheel
        );
    }

    #[test]
    fn replacement_owns_selection_and_rejects_previous_wire_evidence() {
        let mut owner = DeviceConnectionSession::default();
        let previous = begin(&mut owner, "A");
        let current = begin(&mut owner, "B");
        assert_eq!(
            owner
                .state
                .discovery()
                .selected_platform_identifier
                .as_deref(),
            Some("B")
        );
        assert!(
            owner
                .observe_for_attempt(
                    &previous,
                    DeviceDetectionEvent::Notification { bytes: VESC_REPLY }
                )
                .is_none()
        );
        owner.resolve(&current, false, MonotonicTimestamp::new(1));
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::Pending
        );
        assert!(owner.snapshot().identity.is_none());
    }

    #[test]
    fn admitted_retry_replaces_attempt_and_resets_device_scoped_state() {
        let mut owner = DeviceConnectionSession::default();
        let previous = begin(&mut owner, "A");
        let cutout_core::ConnectionRetryDecision::Scheduled(retry) = owner
            .state
            .connection
            .request_retry(&previous, MonotonicTimestamp::new(100), 500)
        else {
            panic!("current attempt should receive a retry");
        };

        let current = owner
            .admit_retry(retry.token(), retry.deadline())
            .expect("retry admitted at its deadline");

        assert!(!owner.state.connection.is_current(&previous));
        assert!(owner.state.connection.is_current(&current));
        assert_eq!(current.platform_identifier(), "A");
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::Pending
        );
        assert!(owner.snapshot().identity.is_none());
        assert_eq!(
            owner
                .state
                .discovery()
                .selected_platform_identifier
                .as_deref(),
            Some("A")
        );
    }

    #[test]
    fn stale_attempt_cannot_mutate_detector_or_probe_state() {
        let mut owner = DeviceConnectionSession::default();
        let previous = begin(&mut owner, "A");
        let current = begin(&mut owner, "B");

        assert!(
            owner
                .begin_identification_probes(&previous, MonotonicTimestamp::new(1))
                .is_none()
        );
        assert!(
            owner
                .observe_detection(
                    &previous,
                    DeviceDetectionEvent::Notification { bytes: VESC_REPLY }
                )
                .is_none()
        );
        assert!(!owner.reset_detector(&previous));

        assert!(
            owner
                .begin_identification_probes(&current, MonotonicTimestamp::new(1))
                .is_some()
        );
    }

    #[test]
    fn standalone_probes_cannot_mutate_an_active_attempt() {
        let mut owner = DeviceConnectionSession::default();
        let _token = begin(&mut owner, "A");

        assert_eq!(
            owner.begin_identification_probes_unscoped(MonotonicTimestamp::new(1)),
            crate::IdentificationProbePlan::Unsupported
        );
    }

    #[test]
    fn terminal_record_only_keeps_evidence_but_cannot_construct_decoder() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        owner.transport_failed(&token);
        let evidence = owner.observe_for_attempt(
            &token,
            DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
        );
        assert!(evidence.is_some());
        owner.resolve(&token, true, MonotonicTimestamp::new(1));
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::RecordOnly
        );
        assert!(owner.snapshot().identity.is_none());
        assert!(owner.state.connection.is_current(&token));
    }

    #[test]
    fn saved_vesc_geometry_is_attempt_scoped_and_does_not_invent_vehicle_kind() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        let profile = crate::VescBoardProfile::new(
            crate::MotorPolePairs::new(1),
            crate::GearRatioDenominator::new(1),
            cutout_core::Distance::from_millimetres(60),
        );
        assert!(owner.configure_vesc_board_profile(&token, profile));
        let read = |owner: &mut DeviceConnectionSession, token: &ConnectionAttemptToken| {
            let _ = owner.observe_for_attempt(
                token,
                DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
            );
            owner.resolve(token, false, MonotonicTimestamp::new(1));
            let _ = owner.ingest(
                token,
                &SessionInputDto::LinkUp {
                    monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                    max_write_len: None,
                },
            );
            owner
                .ingest(
                    token,
                    &SessionInputDto::Notification {
                        channel: crate::VESC_NOTIFY_CHANNEL.as_bytes(),
                        bytes: vec![
                            2, 23, 50, 0, 2, 161, 138, 0, 0, 0, 0, 0, 4, 0, 0, 3, 221, 1, 119, 255,
                            255, 170, 43, 0, 20, 45, 58, 3,
                        ],
                        monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 2 },
                    },
                )
                .unwrap()
        };
        let first = read(&mut owner, &token);
        assert_eq!(first.telemetry.speed.unwrap().value, 989);
        assert_eq!(
            first.session.identity.unwrap().vehicle_kind(),
            crate::VehicleKind::Unknown
        );
        assert!(
            !owner.configure_vesc_board_profile(&token, profile),
            "live decoder configuration is immutable"
        );
        let replacement = begin(&mut owner, "B");
        assert!(!owner.configure_vesc_board_profile(&token, profile));
        assert!(read(&mut owner, &replacement).telemetry.speed.is_none());
    }

    #[test]
    fn verified_vesc_starts_protocol_owned_polling_after_subscription() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "VESC");
        let _ = owner.observe_for_attempt(
            &token,
            DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
        );
        owner.resolve(&token, false, MonotonicTimestamp::new(1));
        let step = owner
            .ingest(
                &token,
                &SessionInputDto::LinkUp {
                    monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                    max_write_len: None,
                },
            )
            .unwrap();
        let subscribe = step
            .result
            .outputs
            .iter()
            .position(|output| {
                matches!(
                    output,
                    SessionOutput::Transport(cutout_core::TransportAction::Subscribe { .. })
                )
            })
            .unwrap();
        let write = step
            .result
            .outputs
            .iter()
            .position(|output| {
                matches!(
                    output,
                    SessionOutput::Transport(cutout_core::TransportAction::Write { .. })
                )
            })
            .expect("generic link-up starts the existing VESC polling owner");
        assert!(
            subscribe < write,
            "native must enable notifications before initial requests"
        );
        assert_eq!(
            owner.next_wakeup_at(&token, true),
            Some(MonotonicTimestamp::new(101))
        );
        assert_eq!(
            owner.next_wakeup_at(&token, false),
            Some(MonotonicTimestamp::new(5_001))
        );
        let tick = owner
            .ingest(
                &token,
                &SessionInputDto::Tick {
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 2_001,
                    },
                },
            )
            .unwrap();
        assert!(tick.result.outputs.iter().any(|output| matches!(
            output,
            SessionOutput::Transport(cutout_core::TransportAction::Write { .. })
        )));
        owner.link_down(&token);
        assert_eq!(owner.next_wakeup_at(&token, true), None);
        assert!(
            owner
                .ingest(
                    &token,
                    &SessionInputDto::Tick {
                        monotonic_ms: cutout_core::MonotonicMillisDto {
                            milliseconds: 4_001
                        },
                    }
                )
                .is_none()
        );
    }

    #[test]
    fn verified_link_loss_retires_decoder_before_queued_input() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        let _ = owner.observe_for_attempt(
            &token,
            DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
        );
        owner.resolve(&token, false, MonotonicTimestamp::new(1));
        assert!(owner.state.connection.is_verified(&token));
        assert!(owner.snapshot().identity.is_some());
        owner.link_down(&token);
        assert!(!owner.state.connection.is_verified(&token));
        assert!(owner.snapshot().identity.is_none());
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::Failed
        );
    }

    #[test]
    fn verified_decoder_is_retired_when_later_evidence_conflicts() {
        let mut owner = DeviceConnectionSession::default();
        let token = connected_aero(&mut owner);
        assert!(owner.state.connection.is_verified(&token));

        let resolution = owner
            .observe_detection(
                &token,
                DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
            )
            .expect("current attempt evidence is retained");

        assert_eq!(resolution.protocol, crate::ProtocolFamilyState::Conflict);
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::Conflicted
        );
        assert!(owner.snapshot().identity.is_none());
        assert!(!owner.state.connection.is_current(&token));
        assert!(
            owner
                .ingest(
                    &token,
                    &SessionInputDto::Tick {
                        monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 2 },
                    }
                )
                .is_none()
        );
    }

    pub(super) fn connected_aero(owner: &mut DeviceConnectionSession) -> ConnectionAttemptToken {
        let token = begin(owner, "A");
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        let _ =
            owner.observe_for_attempt(&token, DeviceDetectionEvent::Notification { bytes: &frame });
        owner.resolve(&token, false, MonotonicTimestamp::new(1));
        let _ = owner.ingest(
            &token,
            &SessionInputDto::LinkUp {
                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                max_write_len: None,
            },
        );
        token
    }

    pub(super) fn aero_mode_telemetry(at: u64) -> SessionInputDto {
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        SessionInputDto::Notification {
            channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
            bytes: frame,
            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: at },
        }
    }

    fn pwm_duty_readback(pwm: u8, at: u64) -> SessionInputDto {
        let mut frame = vec![0x80; 58];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 54]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        frame[46] = 8;
        frame[53] = pwm;
        let checksum = crc32fast::hash(&frame[..54]);
        frame[54..].copy_from_slice(&checksum.to_be_bytes());
        SessionInputDto::Notification {
            channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
            bytes: frame,
            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: at },
        }
    }

    #[test]
    fn decoded_settings_update_only_the_current_connections_semantic_owner() {
        let mut owner = DeviceConnectionSession::default();
        let token = connected_aero(&mut owner);
        assert!(owner.ingest(&token, &pwm_duty_readback(80, 10)).is_some());
        let snapshots = owner.state.settings.snapshot(MonotonicTimestamp::new(15));
        let pwm = snapshots
            .iter()
            .find(|snapshot| snapshot.id == cutout_core::SettingId::PwmTiltback)
            .expect("decoded PWM reaches shared settings owner");
        assert_eq!(
            pwm.current.unwrap().value,
            cutout_core::DeviceSettingValue::Number(80)
        );
        assert_eq!(pwm.age, Some(cutout_core::Duration::from_milliseconds(5)));
        let _ = begin(&mut owner, "B");
        assert!(owner.ingest(&token, &pwm_duty_readback(30, 20)).is_none());
        assert_eq!(
            owner.state.settings.snapshot(MonotonicTimestamp::new(20)),
            [] as [cutout_core::DeviceSettingSnapshot; 0]
        );
    }

    #[test]
    fn explicit_unknown_settings_frame_clears_current_but_preserves_request() {
        let mut owner = DeviceConnectionSession::default();
        let token = connected_aero(&mut owner);
        let _ = owner.ingest(&token, &pwm_duty_readback(80, 10));
        assert!(
            owner
                .state
                .settings
                .snapshot(MonotonicTimestamp::new(10))
                .iter()
                .any(
                    |snapshot| snapshot.id == cutout_core::SettingId::PwmTiltback
                        && snapshot.current.is_some()
                )
        );
        owner.state.settings.submission(
            cutout_core::SettingId::PwmTiltback,
            cutout_core::DeviceSettingValue::Number(85),
            cutout_core::SettingSubmissionOutcome::Accepted,
            cutout_core::SettingCompletionStrategy::MatchingReadback,
            MonotonicTimestamp::new(11),
        );
        let _ = owner.ingest(&token, &pwm_duty_readback(128, 12));
        let snapshots = owner.state.settings.snapshot(MonotonicTimestamp::new(12));
        let pwm = snapshots
            .iter()
            .find(|snapshot| snapshot.id == cutout_core::SettingId::PwmTiltback)
            .unwrap();
        assert!(pwm.current.is_none());
        assert_eq!(
            pwm.requested,
            Some(cutout_core::DeviceSettingValue::Number(85))
        );
    }
}
