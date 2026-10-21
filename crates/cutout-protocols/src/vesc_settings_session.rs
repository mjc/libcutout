//! Source-pinned persistent configuration over the generic VESC session.

use cutout_core::{
    CommandKind, ControlRefusal, ControlRefusalReason, DeviceCommand, DeviceEvent,
    DeviceSettingValue, MonotonicTimestamp, ParserDiagnosticsDto, ReadOnlyResponse,
    RideOperatingState, RideOperatingStateDto, SafetyClass, SessionInputDto, SessionOutput,
    SettingId, SettingTransportStatus, SettingsEntry, SettingsReadback, StationarySettingsArm,
    StationarySettingsPolicy, TelemetrySnapshotDto, TransportAction, TransportOperationId,
    ValueQuality, ValueSource, VerificationStatus, WriteMode, WritePayload,
};

use crate::refloat_settings::RefloatSettings;
use crate::session::SettingWriteAuthorization;
use crate::vesc_configuration::VescConfiguration;
use crate::vesc_settings_image::{ConfigurationKind, SettingsImage};
use crate::{
    DeviceControlProfile, DeviceSessionStep, RefloatInfo, VESC_WRITE_CHANNEL, VescBoardProfile,
    VescReadOnlySession,
};

const VESC_SETTINGS_MODEL: &str = "Vesc7.0PersistentSettings";
const SETTINGS_EVIDENCE_MAX_AGE_MS: u64 = 2_000;
const SETTING_OPERATION_TIMEOUT_MS: u64 = 2_000;

#[derive(Clone, Copy, Debug)]
struct ConfigurationKindValues<T> {
    refloat: T,
    motor: T,
    app: T,
}

impl<T> ConfigurationKindValues<T> {
    const fn new(refloat: T, motor: T, app: T) -> Self {
        Self {
            refloat,
            motor,
            app,
        }
    }

    fn get(&self, kind: ConfigurationKind) -> &T {
        match kind {
            ConfigurationKind::Refloat => &self.refloat,
            ConfigurationKind::Motor => &self.motor,
            ConfigurationKind::App => &self.app,
        }
    }

    fn get_mut(&mut self, kind: ConfigurationKind) -> &mut T {
        match kind {
            ConfigurationKind::Refloat => &mut self.refloat,
            ConfigurationKind::Motor => &mut self.motor,
            ConfigurationKind::App => &mut self.app,
        }
    }
}

/// Decoder evidence required to select the pinned Refloat 1.3.0 catalog.
#[derive(Clone, Debug, Default)]
pub(crate) struct VescSettingsEvidence {
    pub(crate) package: Option<RefloatInfo>,
    pub(crate) config: Option<(RefloatSettings, MonotonicTimestamp)>,
    pub(crate) config_generation: u64,
    pub(crate) motor_config: Option<(VescConfiguration, MonotonicTimestamp)>,
    pub(crate) app_config: Option<(VescConfiguration, MonotonicTimestamp)>,
    pub(crate) motor_generation: u64,
    pub(crate) app_generation: u64,
    pub(crate) firmware: Option<(u8, u8)>,
    pub(crate) controller_id: Option<u8>,
}

impl VescSettingsEvidence {
    fn image(&self, kind: ConfigurationKind) -> Option<(SettingsImage, MonotonicTimestamp)> {
        match kind {
            ConfigurationKind::Refloat => self
                .config
                .clone()
                .map(|(image, at)| (SettingsImage::Refloat(image), at)),
            ConfigurationKind::Motor => self
                .motor_config
                .clone()
                .map(|(image, at)| (SettingsImage::Firmware(image), at)),
            ConfigurationKind::App => self
                .app_config
                .clone()
                .map(|(image, at)| (SettingsImage::Firmware(image), at)),
        }
    }

    const fn generation(&self, kind: ConfigurationKind) -> u64 {
        match kind {
            ConfigurationKind::Refloat => self.config_generation,
            ConfigurationKind::Motor => self.motor_generation,
            ConfigurationKind::App => self.app_generation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingStage {
    PreflightQueued,
    AwaitingPreflight {
        submitted_at: MonotonicTimestamp,
    },
    MutationQueued,
    VerificationReadQueued {
        mutation_submitted_at: MonotonicTimestamp,
    },
    AwaitingVerification {
        mutation_submitted_at: MonotonicTimestamp,
        submitted_at: MonotonicTimestamp,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StandaloneConfigRead {
    Idle,
    AwaitingProcedureReply,
    Queued {
        operation_id: TransportOperationId,
        accepted_at: MonotonicTimestamp,
        receipt_at: MonotonicTimestamp,
    },
    Submitted {
        operation_id: TransportOperationId,
        accepted_at: MonotonicTimestamp,
        receipt_at: MonotonicTimestamp,
    },
}

#[derive(Clone, Debug)]
struct PendingSetting {
    id: SettingId,
    value: DeviceSettingValue,
    expires_at: MonotonicTimestamp,
    operation_id: Option<TransportOperationId>,
    authorization: Option<SettingWriteAuthorization>,
    controller_id: u8,
    baseline: SettingsImage,
    expected: Option<SettingsImage>,
    stage: PendingStage,
}

/// Private VESC settings owner; the underlying generic VESC protocol remains read-only.
#[derive(Debug)]
pub(crate) struct VescSettingsSession {
    read_only: VescReadOnlySession,
    arm: Option<StationarySettingsArm>,
    cancellation_generation: u64,
    pending: Option<PendingSetting>,
    failed_operation: Option<u64>,
    now: MonotonicTimestamp,
    last_speed: Option<(cutout_core::Speed, MonotonicTimestamp)>,
    last_operating_state: Option<(RideOperatingState, MonotonicTimestamp)>,
    last_operating_mode: Option<(cutout_core::RideOperatingMode, MonotonicTimestamp)>,
    last_config_generation: ConfigurationKindValues<u64>,
    initial_config_requested: ConfigurationKindValues<bool>,
    initial_config_retry_count: ConfigurationKindValues<u8>,
    standalone_kind: ConfigurationKind,
    firmware_inventory_requested: bool,
    next_config_read_id: Option<u64>,
    standalone_config_read: StandaloneConfigRead,
    identity_invalidated: bool,
}

impl VescSettingsSession {
    pub(crate) fn new(board_profile: Option<VescBoardProfile>) -> Self {
        Self {
            read_only: board_profile.map_or_else(
                VescReadOnlySession::new,
                VescReadOnlySession::with_board_profile,
            ),
            arm: None,
            cancellation_generation: 0,
            pending: None,
            failed_operation: None,
            now: MonotonicTimestamp::new(0),
            last_speed: None,
            last_operating_state: None,
            last_operating_mode: None,
            last_config_generation: ConfigurationKindValues::new(0, 0, 0),
            initial_config_requested: ConfigurationKindValues::new(false, false, false),
            initial_config_retry_count: ConfigurationKindValues::new(0, 0, 0),
            standalone_kind: ConfigurationKind::Refloat,
            firmware_inventory_requested: false,
            next_config_read_id: Some(1_u64 << 63),
            standalone_config_read: StandaloneConfigRead::Idle,
            identity_invalidated: false,
        }
    }

    pub(crate) fn current_snapshot(&self) -> TelemetrySnapshotDto {
        self.read_only.current_snapshot()
    }

    pub(crate) fn diagnostics(&self) -> ParserDiagnosticsDto {
        self.read_only.diagnostics()
    }

    pub(crate) fn next_tick_at(&self) -> Option<MonotonicTimestamp> {
        [
            self.read_only.next_tick_at(),
            self.pending.as_ref().map(|pending| pending.expires_at),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub(crate) fn application_discovery(&self) -> crate::VescApplicationDiscovery {
        if self.identity_invalidated {
            return crate::VescApplicationDiscovery::Pending;
        }
        match self.read_only.settings_evidence().package {
            None => crate::VescApplicationDiscovery::Pending,
            Some(package) if package.package_name.as_str() == "Refloat" => {
                crate::VescApplicationDiscovery::Refloat
            }
            Some(_) => crate::VescApplicationDiscovery::Absent,
        }
    }

    pub(crate) fn control_profile(&self) -> DeviceControlProfile {
        let evidence = self.read_only.settings_evidence();
        if self.identity_invalidated || !supported_firmware_facts(&evidence) {
            return DeviceControlProfile::default();
        }
        crate::device_settings::vesc_control_profile(
            supported_package_facts(&evidence) && evidence.config.is_some(),
            evidence.motor_config.is_some(),
            evidence.app_config.is_some(),
        )
        .with_firmware_configuration_write_evidence(
            if self.fresh_stationary_evidence_for(ConfigurationKind::Motor, self.now) {
                crate::FirmwareConfigurationWriteEvidence::Ready
            } else {
                crate::FirmwareConfigurationWriteEvidence::MissingDisengagedState
            },
        )
        .with_refloat_lighting_mode(
            evidence
                .config
                .as_ref()
                .map_or(crate::RefloatLightingMode::Unknown, |(config, _)| {
                    crate::refloat_settings::lighting_mode(config)
                }),
        )
    }

    pub(crate) fn settings_busy(&self) -> bool {
        self.pending.is_some() || self.standalone_config_read != StandaloneConfigRead::Idle
    }

    pub(crate) fn settings_read_requires_reconnect(&self, at: MonotonicTimestamp) -> bool {
        match self.standalone_config_read {
            StandaloneConfigRead::AwaitingProcedureReply => true,
            StandaloneConfigRead::Submitted { receipt_at, .. } => {
                at >= receipt_at
                    && at.saturating_duration_since(receipt_at)
                        >= cutout_core::Duration::from_milliseconds(SETTING_OPERATION_TIMEOUT_MS)
            }
            StandaloneConfigRead::Idle | StandaloneConfigRead::Queued { .. } => false,
        }
    }

    pub(crate) fn ingest_typed(&mut self, input: &SessionInputDto) -> DeviceSessionStep {
        let is_connection_edge = match input {
            SessionInputDto::LinkUp { .. } | SessionInputDto::LinkDown => true,
            _ => false,
        };
        if is_connection_edge {
            self.reset_connection_state();
        }
        if let Some(at) = input_timestamp(input) {
            self.now = self.now.max(at);
        }

        // Settings commands are sequenced by this wrapper and must not inherit
        // the read-only reactor's command-triggered telemetry polls.
        let protocol_input = match input {
            SessionInputDto::CommandAt { command, .. } => match command {
                cutout_core::DeviceCommandDto::SetSetting { .. }
                | cutout_core::DeviceCommandDto::RequestSettings => {
                    Some(SessionInputDto::Command(*command))
                }
                _ => None,
            },
            _ => None,
        };
        let previous = self.read_only.settings_evidence();
        let mut step = self
            .read_only
            .ingest_typed(protocol_input.as_ref().unwrap_or(input));
        let requested_motor_config = step.outputs.iter().any(is_automatic_motor_config_read);
        step.outputs
            .retain(|output| !is_automatic_motor_config_read(output));
        let current = self.read_only.settings_evidence();
        if !is_connection_edge && identity_changed(&previous, &current) {
            // Same-link identity changes make outstanding replies ambiguous.
            // Reconnect before admitting another configuration write.
            self.identity_invalidated = true;
            self.clear_arm_and_cancel();
            self.last_speed = None;
            self.last_operating_state = None;
            self.last_operating_mode = None;
        }
        self.observe_telemetry(&step.outputs);
        self.expire_pending();

        if let Some(command) = setting_command(input) {
            self.handle_setting_command(command, &mut step);
        } else if is_settings_request(input) {
            self.handle_settings_read_request(&mut step);
        }

        if let SessionInputDto::Notification { .. } = input {
            self.observe_config(&mut step);
        }
        if requested_motor_config {
            self.maybe_request_telemetry_motor_config(&mut step);
        }
        self.maybe_request_initial_config(&mut step);
        step
    }

    pub(crate) fn bind_setting_operation(
        &mut self,
        operation_id: TransportOperationId,
    ) -> (Option<SettingWriteAuthorization>, usize) {
        let Some(pending) = self.pending.as_mut() else {
            return (None, 0);
        };
        pending.operation_id = Some(operation_id);
        let Some(arm) = self.arm else {
            return (None, 0);
        };
        let authorization = SettingWriteAuthorization {
            arm,
            cancellation_generation: self.cancellation_generation,
        };
        pending.authorization = Some(authorization);
        (Some(authorization), 2)
    }

    pub(crate) fn record_setting_transport(
        &mut self,
        operation_id: u64,
        status: SettingTransportStatus,
        at: MonotonicTimestamp,
    ) {
        let matches = self
            .pending
            .as_ref()
            .and_then(|pending| pending.operation_id)
            .is_some_and(|current| current.get() == operation_id);
        if !matches {
            return;
        }
        match status {
            SettingTransportStatus::Submitted => {
                if let Some(pending) = self.pending.as_mut() {
                    pending.stage = match pending.stage {
                        PendingStage::PreflightQueued => {
                            PendingStage::AwaitingPreflight { submitted_at: at }
                        }
                        PendingStage::MutationQueued => PendingStage::VerificationReadQueued {
                            mutation_submitted_at: at,
                        },
                        PendingStage::VerificationReadQueued {
                            mutation_submitted_at,
                        } => PendingStage::AwaitingVerification {
                            mutation_submitted_at,
                            submitted_at: at,
                        },
                        stage @ (PendingStage::AwaitingPreflight { .. }
                        | PendingStage::AwaitingVerification { .. }) => stage,
                    };
                }
            }
            SettingTransportStatus::Rejected | SettingTransportStatus::Cancelled => {
                self.cancel_setting_operation(operation_id);
            }
            SettingTransportStatus::Accepted | SettingTransportStatus::Queued => {}
        }
    }

    pub(crate) fn setting_write_is_current(
        &self,
        authorization: SettingWriteAuthorization,
        at: MonotonicTimestamp,
    ) -> bool {
        let Some(pending) = self.pending.as_ref() else {
            return false;
        };
        if let PendingStage::VerificationReadQueued { .. } = pending.stage {
            return pending.expires_at > at
                && self.profile_is_supported()
                && self.read_only.settings_evidence().controller_id == Some(pending.controller_id);
        }
        let Some(current_arm) = self.arm else {
            return false;
        };
        pending.authorization.is_some_and(|current| {
            current.arm == authorization.arm
                && current.cancellation_generation == authorization.cancellation_generation
        }) && authorization.cancellation_generation == self.cancellation_generation
            && authorization.arm == current_arm
            && current_arm.is_valid_for(VESC_SETTINGS_MODEL, at)
            && pending.expires_at >= at
            && self.profile_is_supported()
            && self
                .control_profile()
                .command(pending.id, pending.value, true)
                .is_ok()
            && self
                .read_only
                .settings_evidence()
                .image(pending.baseline.kind())
                .is_some_and(|(image, _)| pending.baseline.matches(&image))
            && self.read_only.settings_evidence().controller_id == Some(pending.controller_id)
            && self.fresh_stationary_evidence_for(pending.baseline.kind(), at)
    }

    pub(crate) fn setting_authorization_expires_at(
        &self,
        authorization: SettingWriteAuthorization,
    ) -> Option<MonotonicTimestamp> {
        if authorization.cancellation_generation != self.cancellation_generation
            || !self.arm.is_some_and(|arm| arm == authorization.arm)
        {
            return None;
        }
        if self.pending.as_ref().is_some_and(|pending| {
            pending.authorization.is_some_and(|current| {
                current.arm == authorization.arm
                    && current.cancellation_generation == authorization.cancellation_generation
            }) && match pending.stage {
                PendingStage::VerificationReadQueued { .. } => true,
                PendingStage::PreflightQueued
                | PendingStage::AwaitingPreflight { .. }
                | PendingStage::MutationQueued
                | PendingStage::AwaitingVerification { .. } => false,
            }
        }) {
            return self.pending.as_ref().map(|pending| pending.expires_at);
        }
        let freshness_deadline = [
            self.last_speed.map(|(_, observed)| observed),
            self.last_operating_state.map(|(_, observed)| observed),
            self.last_operating_mode.map(|(_, observed)| observed),
        ]
        .into_iter()
        .flatten()
        .map(|observed| {
            observed.saturating_add_duration(cutout_core::Duration::from_milliseconds(
                SETTINGS_EVIDENCE_MAX_AGE_MS.saturating_add(1),
            ))
        })
        .min();
        Some(
            freshness_deadline.map_or(authorization.arm.expires_at_ms(), |deadline| {
                deadline.min(authorization.arm.expires_at_ms())
            }),
        )
    }

    pub(crate) fn arm_settings_writes(
        &mut self,
        requested_state: RideOperatingStateDto,
        requested_speed_mm_per_second: Option<i32>,
        monotonic_ms: u64,
    ) -> bool {
        let at = MonotonicTimestamp::new(monotonic_ms);
        let valid_state = match requested_state {
            RideOperatingStateDto::Parked | RideOperatingStateDto::Standing => true,
            _ => false,
        };
        let actual_state = self.fresh_operating_state(at);
        let actual_speed = self.fresh_speed(at);
        let speed_matches = actual_speed.is_some_and(|speed| {
            requested_speed_mm_per_second == Some(speed.as_millimetres_per_second())
                && !speed.is_moving()
        });
        let state_matches =
            actual_state.is_some_and(|state| RideOperatingStateDto::from(state) == requested_state);
        let mode_is_normal = self.fresh_normal_mode(at);
        let arm = (self.profile_is_supported()
            && valid_state
            && state_matches
            && speed_matches
            && mode_is_normal
            && self.read_only.settings_evidence().controller_id.is_some())
        .then(|| {
            StationarySettingsPolicy {
                model: VESC_SETTINGS_MODEL,
                arm_duration: cutout_core::Duration::from_milliseconds(
                    SETTINGS_EVIDENCE_MAX_AGE_MS,
                ),
            }
            .arm(RideOperatingState::from(requested_state), at)
        })
        .flatten();
        let Some(arm) = arm else {
            self.clear_arm_and_cancel();
            return false;
        };
        self.arm = Some(arm);
        true
    }

    pub(crate) fn cancel_setting_operation(&mut self, operation_id: u64) {
        if self
            .pending
            .as_ref()
            .and_then(|pending| pending.operation_id)
            .is_some_and(|current| current.get() == operation_id)
        {
            self.cancel_pending();
        }
    }

    pub(crate) fn setting_mutation_submitted_at(
        &self,
        operation_id: u64,
    ) -> Option<MonotonicTimestamp> {
        let pending = self.pending.as_ref()?;
        if pending
            .operation_id
            .is_none_or(|current| current.get() != operation_id)
        {
            return None;
        }
        match pending.stage {
            PendingStage::VerificationReadQueued {
                mutation_submitted_at,
            }
            | PendingStage::AwaitingVerification {
                mutation_submitted_at,
                ..
            } => Some(mutation_submitted_at),
            PendingStage::PreflightQueued
            | PendingStage::AwaitingPreflight { .. }
            | PendingStage::MutationQueued => None,
        }
    }

    pub(crate) fn take_failed_setting_operation(&mut self) -> Option<u64> {
        self.failed_operation.take()
    }

    fn handle_setting_command(&mut self, command: DeviceCommand, step: &mut DeviceSessionStep) {
        let DeviceCommand::SetSetting { id, value } = command else {
            return;
        };
        remove_refusal(step, CommandKind::SetSetting);
        let at = self.now;
        let kind = ConfigurationKind::for_setting(id);
        let refusal = if self.settings_busy() {
            Some(ControlRefusalReason::Busy)
        } else if !self.profile_is_supported()
            || self.control_profile().command(id, value, true).is_err()
        {
            Some(ControlRefusalReason::UnsupportedCommand)
        } else if self.arm.is_none() {
            Some(ControlRefusalReason::MissingArm)
        } else if !self
            .arm
            .is_some_and(|arm| arm.is_valid_for(VESC_SETTINGS_MODEL, at))
        {
            Some(ControlRefusalReason::ExpiredArm)
        } else if kind.is_none_or(|kind| !self.fresh_stationary_evidence_for(kind, at)) {
            Some(ControlRefusalReason::MissingArm)
        } else {
            None
        };
        if let Some(reason) = refusal {
            append_refusal(step, CommandKind::SetSetting, reason);
            return;
        }
        let evidence = self.read_only.settings_evidence();
        let Some((baseline, _observed_at)) = kind.and_then(|kind| evidence.image(kind)) else {
            append_refusal(
                step,
                CommandKind::SetSetting,
                ControlRefusalReason::UnsupportedCommand,
            );
            return;
        };
        let Some(controller_id) = evidence.controller_id else {
            append_refusal(
                step,
                CommandKind::SetSetting,
                ControlRefusalReason::UnsupportedCommand,
            );
            return;
        };
        let Some(bytes) = encode_settings_payload(baseline.read_command_payload()) else {
            append_refusal(
                step,
                CommandKind::SetSetting,
                ControlRefusalReason::UnsupportedCommand,
            );
            return;
        };
        self.pending = Some(PendingSetting {
            id,
            value,
            expires_at: at.saturating_add_duration(cutout_core::Duration::from_milliseconds(
                SETTING_OPERATION_TIMEOUT_MS,
            )),
            operation_id: None,
            authorization: None,
            controller_id,
            baseline,
            expected: None,
            stage: PendingStage::PreflightQueued,
        });
        step.outputs
            .push(SessionOutput::Transport(TransportAction::Write {
                channel: VESC_WRITE_CHANNEL,
                bytes,
                mode: WriteMode::WithoutResponse,
            }));
        step.error = None;
    }

    fn handle_settings_read_request(&mut self, step: &mut DeviceSessionStep) {
        remove_settings_response(step);
        remove_refusal(step, CommandKind::RequestSettings);
        if self.settings_busy() {
            append_refusal(
                step,
                CommandKind::RequestSettings,
                ControlRefusalReason::Busy,
            );
            return;
        }
        let evidence = self.read_only.settings_evidence();
        if !self.identity_invalidated && supported_firmware_facts(&evidence) {
            self.firmware_inventory_requested = true;
            self.initial_config_requested = ConfigurationKindValues::new(false, false, false);
            let kind = if supported_package_facts(&evidence) {
                ConfigurationKind::Refloat
            } else {
                ConfigurationKind::Motor
            };
            *self.initial_config_requested.get_mut(kind) = true;
            self.push_config_read(kind, step);
            step.error = None;
        } else {
            append_refusal(
                step,
                CommandKind::RequestSettings,
                ControlRefusalReason::UnsupportedCommand,
            );
        }
    }

    fn observe_telemetry(&mut self, outputs: &[SessionOutput]) {
        for output in outputs {
            let SessionOutput::Event(DeviceEvent::Telemetry(delta)) = output else {
                continue;
            };
            if let Some(speed) = delta.speed
                && self
                    .last_speed
                    .is_none_or(|(_, observed)| delta.at_ms >= observed)
            {
                self.last_speed = Some((speed.value, delta.at_ms));
            }
            if let Some(state) = delta.operating_state
                && self
                    .last_operating_state
                    .is_none_or(|(_, observed)| delta.at_ms >= observed)
            {
                self.last_operating_state = Some((state, delta.at_ms));
            }
            if let Some(mode) = delta.operating_mode
                && self
                    .last_operating_mode
                    .is_none_or(|(_, observed)| delta.at_ms >= observed)
            {
                self.last_operating_mode = Some((mode, delta.at_ms));
            }
        }
    }

    fn observe_config(&mut self, step: &mut DeviceSessionStep) {
        let evidence = self.read_only.settings_evidence();
        for kind in ConfigurationKind::ALL {
            if evidence.generation(kind) > *self.last_config_generation.get(kind) {
                *self.last_config_generation.get_mut(kind) = evidence.generation(kind);
                self.observe_config_image(kind, &evidence, step);
            }
        }
    }

    fn observe_config_image(
        &mut self,
        kind: ConfigurationKind,
        evidence: &VescSettingsEvidence,
        step: &mut DeviceSessionStep,
    ) {
        if self.standalone_kind == kind {
            self.standalone_config_read = StandaloneConfigRead::Idle;
        }
        let Some((config, observed_at)) = evidence.image(kind) else {
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.baseline.kind() == kind)
            {
                self.cancel_pending_after_read_response();
            }
            return;
        };
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.baseline.kind() != kind)
        {
            push_settings_readback(step, &config);
            return;
        }

        if self
            .pending
            .as_ref()
            .is_some_and(|pending| evidence.controller_id != Some(pending.controller_id))
        {
            self.cancel_pending_after_read_response();
            push_settings_readback(step, &config);
            return;
        }

        let Some(pending) = self.pending.clone() else {
            push_settings_readback(step, &config);
            return;
        };
        match pending.stage {
            PendingStage::PreflightQueued
            | PendingStage::MutationQueued
            | PendingStage::VerificationReadQueued { .. } => {}
            PendingStage::AwaitingPreflight { submitted_at } => {
                if observed_at < submitted_at {
                    return;
                }
                self.apply_preflight(&pending, &config, observed_at, step);
            }
            PendingStage::AwaitingVerification { submitted_at, .. } => {
                if observed_at < submitted_at {
                    return;
                }
                let verified = pending
                    .expected
                    .as_ref()
                    .is_some_and(|expected| expected.matches(&config));
                if verified {
                    push_settings_readback(step, &config);
                    self.finish_pending();
                } else {
                    self.cancel_pending_after_read_response();
                    push_settings_readback(step, &config);
                }
            }
        }
    }

    fn apply_preflight(
        &mut self,
        pending: &PendingSetting,
        config: &SettingsImage,
        observed_at: MonotonicTimestamp,
        step: &mut DeviceSessionStep,
    ) {
        if !pending
            .authorization
            .is_some_and(|authorization| self.setting_write_is_current(authorization, observed_at))
        {
            self.cancel_pending_after_read_response();
            push_settings_readback(step, config);
            return;
        }
        if !pending.baseline.matches(config) {
            self.cancel_pending_after_read_response();
            push_settings_readback(step, config);
            return;
        }
        let Some(expected) = config.changed(pending.id, pending.value) else {
            self.cancel_pending_after_read_response();
            push_settings_readback(step, config);
            return;
        };
        let Some(operation_id) = pending.operation_id else {
            self.cancel_pending_after_read_response();
            return;
        };
        let Some(write) = encode_settings_payload(&expected.write_command_payload()) else {
            self.cancel_pending_after_read_response();
            return;
        };
        let Some(read) = encode_settings_payload(config.read_command_payload()) else {
            self.cancel_pending_after_read_response();
            return;
        };
        let mut write = write;
        let mut read = read;
        write.set_operation_id(operation_id);
        read.set_operation_id(operation_id);
        if let Some(pending) = self.pending.as_mut() {
            pending.expected = Some(expected);
            pending.stage = PendingStage::MutationQueued;
        }
        step.outputs
            .push(SessionOutput::Transport(TransportAction::Write {
                channel: VESC_WRITE_CHANNEL,
                bytes: write,
                mode: WriteMode::WithoutResponse,
            }));
        step.outputs
            .push(SessionOutput::Transport(TransportAction::Write {
                channel: VESC_WRITE_CHANNEL,
                bytes: read,
                mode: WriteMode::WithoutResponse,
            }));
    }

    fn maybe_request_initial_config(&mut self, step: &mut DeviceSessionStep) {
        let evidence = self.read_only.settings_evidence();
        if !supported_firmware_facts(&evidence)
            || self.identity_invalidated
            || self.pending.is_some()
            || self.standalone_config_read != StandaloneConfigRead::Idle
        {
            return;
        }
        for kind in ConfigurationKind::ALL {
            let applicable = match kind {
                ConfigurationKind::Refloat => supported_package_facts(&evidence),
                ConfigurationKind::Motor | ConfigurationKind::App => true,
            };
            if applicable
                && !*self.initial_config_requested.get(kind)
                && (self.firmware_inventory_requested || evidence.image(kind).is_none())
            {
                *self.initial_config_requested.get_mut(kind) = true;
                self.push_config_read(kind, step);
                break;
            }
        }
    }

    fn maybe_request_telemetry_motor_config(&mut self, step: &mut DeviceSessionStep) {
        let evidence = self.read_only.settings_evidence();
        if self.identity_invalidated
            || self.pending.is_some()
            || self.standalone_config_read != StandaloneConfigRead::Idle
            || evidence.motor_config.is_some()
            || *self.initial_config_requested.get(ConfigurationKind::Motor)
        {
            return;
        }
        *self
            .initial_config_requested
            .get_mut(ConfigurationKind::Motor) = true;
        self.push_config_read(ConfigurationKind::Motor, step);
    }

    fn push_config_read(&mut self, kind: ConfigurationKind, step: &mut DeviceSessionStep) {
        let Some(bytes) = encode_settings_payload(kind.read_command()) else {
            return;
        };
        self.standalone_kind = kind;
        let Some(operation_id) = self.next_config_read_id.map(TransportOperationId::new) else {
            return;
        };
        self.next_config_read_id = operation_id.get().checked_add(1);
        let mut bytes = bytes;
        bytes.set_operation_id(operation_id);
        self.standalone_config_read = StandaloneConfigRead::Queued {
            operation_id,
            accepted_at: self.now,
            receipt_at: self.now,
        };
        step.outputs
            .push(SessionOutput::Transport(TransportAction::Write {
                channel: VESC_WRITE_CHANNEL,
                bytes,
                mode: WriteMode::WithoutResponse,
            }));
        // These configuration commands have no request nonce. An unanswered
        // submitted read retains its slot until a reply or reconnect.
    }

    pub(crate) fn setting_read_transport_is_current(
        &self,
        operation_id: u64,
        at: MonotonicTimestamp,
    ) -> bool {
        match self.standalone_config_read {
            StandaloneConfigRead::Queued {
                operation_id: current,
                accepted_at,
                receipt_at,
            } => {
                current.get() == operation_id
                    && at >= accepted_at
                    && at >= receipt_at
                    && self.read_kind_is_supported(self.standalone_kind)
            }
            StandaloneConfigRead::Idle
            | StandaloneConfigRead::AwaitingProcedureReply
            | StandaloneConfigRead::Submitted { .. } => false,
        }
    }

    pub(crate) fn record_setting_read_transport(
        &mut self,
        operation_id: u64,
        status: SettingTransportStatus,
        at: MonotonicTimestamp,
    ) -> bool {
        let StandaloneConfigRead::Queued {
            operation_id: current,
            accepted_at,
            receipt_at,
        } = self.standalone_config_read
        else {
            return false;
        };
        if current.get() != operation_id || at < accepted_at || at < receipt_at {
            return false;
        }
        match status {
            SettingTransportStatus::Accepted | SettingTransportStatus::Queued => {
                self.standalone_config_read = StandaloneConfigRead::Queued {
                    operation_id: current,
                    accepted_at,
                    receipt_at: at,
                };
                true
            }
            SettingTransportStatus::Submitted => {
                self.standalone_config_read = StandaloneConfigRead::Submitted {
                    operation_id: current,
                    accepted_at,
                    receipt_at: at,
                };
                true
            }
            SettingTransportStatus::Rejected | SettingTransportStatus::Cancelled => {
                self.standalone_config_read = StandaloneConfigRead::Idle;
                if *self.initial_config_retry_count.get(self.standalone_kind) == 0 {
                    *self
                        .initial_config_retry_count
                        .get_mut(self.standalone_kind) = 1;
                    *self.initial_config_requested.get_mut(self.standalone_kind) = false;
                }
                true
            }
        }
    }

    fn expire_pending(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| self.now >= pending.expires_at)
        {
            self.cancel_pending();
        }
    }

    fn cancel_pending(&mut self) {
        self.cancel_pending_retaining_submitted_read(true);
    }

    fn cancel_pending_after_read_response(&mut self) {
        self.cancel_pending_retaining_submitted_read(false);
    }

    fn cancel_pending_retaining_submitted_read(&mut self, retain_read: bool) {
        if let Some(pending) = self.pending.take() {
            if retain_read {
                match pending.stage {
                    PendingStage::AwaitingPreflight { .. }
                    | PendingStage::AwaitingVerification { .. } => {
                        self.standalone_kind = pending.baseline.kind();
                        self.standalone_config_read = StandaloneConfigRead::AwaitingProcedureReply;
                    }
                    PendingStage::PreflightQueued
                    | PendingStage::MutationQueued
                    | PendingStage::VerificationReadQueued { .. } => {}
                }
            }
            if let Some(operation_id) = pending.operation_id {
                self.failed_operation = Some(operation_id.get());
            }
        }
        self.cancellation_generation = self.cancellation_generation.saturating_add(1);
    }

    fn finish_pending(&mut self) {
        self.pending = None;
        self.cancellation_generation = self.cancellation_generation.saturating_add(1);
    }

    fn clear_arm_and_cancel(&mut self) {
        self.arm = None;
        self.cancel_pending();
    }

    fn reset_connection_state(&mut self) {
        self.clear_arm_and_cancel();
        self.last_speed = None;
        self.last_operating_state = None;
        self.last_operating_mode = None;
        self.last_config_generation = ConfigurationKindValues::new(0, 0, 0);
        self.initial_config_requested = ConfigurationKindValues::new(false, false, false);
        self.initial_config_retry_count = ConfigurationKindValues::new(0, 0, 0);
        self.firmware_inventory_requested = false;
        self.standalone_config_read = StandaloneConfigRead::Idle;
        self.identity_invalidated = false;
    }

    fn profile_is_supported(&self) -> bool {
        let evidence = self.read_only.settings_evidence();
        !self.identity_invalidated
            && supported_firmware_facts(&evidence)
            && ((supported_package_facts(&evidence) && evidence.config.is_some())
                || evidence.motor_config.is_some()
                || evidence.app_config.is_some())
    }

    fn read_kind_is_supported(&self, kind: ConfigurationKind) -> bool {
        let evidence = self.read_only.settings_evidence();
        !self.identity_invalidated
            && match kind {
                ConfigurationKind::Refloat => supported_package_facts(&evidence),
                // VESC's read-only decoder also probes motor geometry to calculate
                // telemetry on firmware versions without a writable settings schema.
                // This read never enables descriptors or writes; those still require
                // exact supported firmware facts in `control_profile`.
                ConfigurationKind::Motor => true,
                ConfigurationKind::App => supported_firmware_facts(&evidence),
            }
    }

    fn fresh_stationary_evidence_for(
        &self,
        kind: ConfigurationKind,
        at: MonotonicTimestamp,
    ) -> bool {
        self.fresh_stationary_evidence(at)
            && match kind {
                ConfigurationKind::Refloat => true,
                // Motor/App changes can reinitialize the drive or inputs. Zero speed while
                // balancing is insufficient; require the package to report disengaged/parked.
                ConfigurationKind::Motor | ConfigurationKind::App => {
                    self.fresh_operating_state(at) == Some(RideOperatingState::Parked)
                }
            }
    }

    fn fresh_speed(&self, at: MonotonicTimestamp) -> Option<cutout_core::Speed> {
        self.last_speed
            .and_then(|(speed, observed)| fresh(at, observed).then_some(speed))
    }

    fn fresh_operating_state(&self, at: MonotonicTimestamp) -> Option<RideOperatingState> {
        self.last_operating_state
            .and_then(|(state, observed)| fresh(at, observed).then_some(state))
    }

    fn fresh_stationary_evidence(&self, at: MonotonicTimestamp) -> bool {
        self.fresh_speed(at).is_some_and(|speed| !speed.is_moving())
            && match self.fresh_operating_state(at) {
                Some(RideOperatingState::Parked | RideOperatingState::Standing) => true,
                _ => false,
            }
            && self.fresh_normal_mode(at)
    }

    fn fresh_normal_mode(&self, at: MonotonicTimestamp) -> bool {
        self.last_operating_mode.is_some_and(|(mode, observed)| {
            mode == cutout_core::RideOperatingMode::Normal && fresh(at, observed)
        })
    }
}

fn identity_changed(previous: &VescSettingsEvidence, current: &VescSettingsEvidence) -> bool {
    previous
        .firmware
        .is_some_and(|value| current.firmware != Some(value))
        || previous
            .controller_id
            .is_some_and(|value| current.controller_id != Some(value))
        || previous
            .package
            .as_ref()
            .is_some_and(|value| current.package.as_ref() != Some(value))
}

fn supported_firmware_facts(evidence: &VescSettingsEvidence) -> bool {
    evidence.firmware == Some((7, 0)) && evidence.controller_id.is_some()
}

fn supported_package_facts(evidence: &VescSettingsEvidence) -> bool {
    let package_matches = evidence.package.as_ref().is_some_and(|package| {
        package.package_name.as_str() == "Refloat"
            && package.package_major == 1
            && package.package_minor == 3
            && package.package_patch == 0
            && package.package_version_suffix.is_empty()
    });
    package_matches && supported_firmware_facts(evidence)
}

fn push_settings_readback(step: &mut DeviceSessionStep, config: &SettingsImage) {
    let ids = config.ids();
    for batch in ids.chunks(SettingsReadback::capacity()) {
        let mut entries = [None; SettingsReadback::capacity()];
        for (slot, id) in entries.iter_mut().zip(batch.iter().copied()) {
            let Some(field) = config.field_id(id) else {
                continue;
            };
            let value = match (id, config.value(id)) {
                (_, Some(DeviceSettingValue::Number(value))) => i64::from(value),
                (_, Some(DeviceSettingValue::Boolean(value))) => i64::from(value),
                (_, Some(DeviceSettingValue::Choice(value))) => i64::from(value),
                (SettingId::PwmTiltback, Some(DeviceSettingValue::Disabled)) => 100,
                (SettingId::TiltbackSpeed, Some(DeviceSettingValue::Disabled)) => 0,
                _ => i64::MIN,
            };
            *slot = Some(SettingsEntry {
                field: cutout_core::RawFieldValue::new(field, value),
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            });
        }
        step.outputs
            .push(SessionOutput::Event(DeviceEvent::read_only_response(
                ReadOnlyResponse::Settings(SettingsReadback::available(entries)),
            )));
    }
}

fn is_automatic_motor_config_read(output: &SessionOutput) -> bool {
    let SessionOutput::Transport(TransportAction::Write { bytes, .. }) = output else {
        return false;
    };
    let bytes = bytes.as_slice();
    let command = match bytes.first() {
        Some(2) => bytes.get(2),
        Some(3) => bytes.get(3),
        _ => None,
    };
    command == Some(&14)
}

fn append_refusal(
    step: &mut DeviceSessionStep,
    command: CommandKind,
    reason: ControlRefusalReason,
) {
    let refusal = ControlRefusal {
        command,
        safety_class: if command == CommandKind::SetSetting {
            SafetyClass::StationaryOnly
        } else {
            SafetyClass::ReadOnly
        },
        reason,
    };
    step.outputs
        .push(SessionOutput::Event(DeviceEvent::ControlRefusal(refusal)));
    step.error = Some(refusal);
}

fn remove_refusal(step: &mut DeviceSessionStep, command: CommandKind) {
    step.outputs.retain(|output| match output {
        SessionOutput::Event(DeviceEvent::ControlRefusal(refusal)) => refusal.command != command,
        _ => true,
    });
    if step.error.is_some_and(|refusal| refusal.command == command) {
        step.error = None;
    }
}

fn remove_settings_response(step: &mut DeviceSessionStep) {
    step.outputs.retain(|output| match output {
        SessionOutput::Event(DeviceEvent::ReadOnlyResponse(ReadOnlyResponse::Settings(_))) => false,
        _ => true,
    });
}

fn setting_command(input: &SessionInputDto) -> Option<DeviceCommand> {
    let command = match input {
        SessionInputDto::Command(command) | SessionInputDto::CommandAt { command, .. } => {
            DeviceCommand::from(*command)
        }
        _ => return None,
    };
    match command {
        DeviceCommand::SetSetting { .. } => Some(command),
        _ => None,
    }
}

fn is_settings_request(input: &SessionInputDto) -> bool {
    match input {
        SessionInputDto::Command(cutout_core::DeviceCommandDto::RequestSettings)
        | SessionInputDto::CommandAt {
            command: cutout_core::DeviceCommandDto::RequestSettings,
            ..
        } => true,
        _ => false,
    }
}

fn input_timestamp(input: &SessionInputDto) -> Option<MonotonicTimestamp> {
    match input {
        SessionInputDto::LinkUp { monotonic_ms, .. }
        | SessionInputDto::Tick { monotonic_ms }
        | SessionInputDto::Notification { monotonic_ms, .. }
        | SessionInputDto::CommandAt { monotonic_ms, .. } => {
            Some(MonotonicTimestamp::new(monotonic_ms.milliseconds))
        }
        SessionInputDto::LinkDown | SessionInputDto::Command(_) => None,
    }
}

fn fresh(at: MonotonicTimestamp, observed: MonotonicTimestamp) -> bool {
    at >= observed
        && at.saturating_duration_since(observed)
            <= cutout_core::Duration::from_milliseconds(SETTINGS_EVIDENCE_MAX_AGE_MS)
}

fn encode_settings_payload(payload: &[u8]) -> Option<WritePayload> {
    crate::vesc_codec::encode_settings_payload(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inclusive_connected_motion_threshold_never_counts_as_stationary() {
        let threshold = cutout_core::CONNECTED_WHEEL_MOVEMENT_THRESHOLD.as_millimetres_per_second();
        for (speed, stationary) in [
            (threshold - 1, true),
            (threshold, false),
            (-threshold, false),
        ] {
            let mut session = VescSettingsSession::new(None);
            let at = MonotonicTimestamp::new(10);
            session.last_speed = Some((cutout_core::Speed::from_millimetres_per_second(speed), at));
            session.last_operating_state = Some((RideOperatingState::Parked, at));
            session.last_operating_mode = Some((cutout_core::RideOperatingMode::Normal, at));
            assert_eq!(
                session.fresh_stationary_evidence(at),
                stationary,
                "speed {speed}"
            );
        }
    }
}
