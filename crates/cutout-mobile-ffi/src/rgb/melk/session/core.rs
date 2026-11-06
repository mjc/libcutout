//! Thread-safe `UniFFI` handle for the MELK session reducer.

use std::sync::{Arc, Mutex, PoisonError};

use cutout_core::{
    LightingBrightness, LightingPowerState, MelkControl, RgbColor, RgbLightingCommand,
};
use cutout_protocols::MelkLightingProfile;

use super::super::commands::plan_schedule;
use super::{
    contract::{
        MobileMelkLightingCharacteristicEvidenceDto, MobileMelkLightingSessionActionDto,
        MobileMelkLightingSessionCandidateDto, MobileMelkLightingSessionEventDto,
        MobileMelkLightingSessionSnapshotDto, MobileMelkLightingSessionStateDto,
        MobileMelkLightingTimerDto,
    },
    reducer::SessionReducer,
};
use crate::{
    MobileMelkClockDto, MobileMelkLightingError, MobileMelkLightingRestoreStateDto,
    MobileMelkLightingWriteDto, MobileMelkLightingWriteModeDto, MobileMelkScheduleDto,
    mobile_melk_control, mobile_melk_transport_action,
};
use cutout_application::lighting::melk::{self as application_melk, session as application};

/// Rust-owned MELK session exposed to the thin `CoreBluetooth` wrapper.
#[derive(Debug, uniffi::Object)]
pub struct MobileMelkLightingSessionCore {
    inner: Mutex<SessionReducer>,
}

#[uniffi::export]
impl MobileMelkLightingSessionCore {
    /// Creates an idle session core.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(SessionReducer::default()),
        })
    }

    /// Starts the session with an optional remembered platform identifier.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns optional String values at the FFI boundary."
    )]
    pub fn start(&self, preferred_platform_identifier: Option<String>) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .start(preferred_platform_identifier.as_deref());
    }

    /// Stops the session and prevents future reconnects.
    pub fn stop(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stop();
    }

    /// Submits one `CoreBluetooth` event.
    pub fn handle(&self, event: MobileMelkLightingSessionEventDto) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .handle(event.into());
    }

    /// Selects a first-pairing candidate.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns String values at the FFI boundary."
    )]
    pub fn select_candidate(&self, platform_identifier: String) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .select(&platform_identifier);
    }

    /// Enqueues a power command if the verified profile is ready.
    pub fn set_power(&self, on: bool) -> bool {
        self.command(mobile_melk_transport_action(
            MelkLightingProfile::write_action(RgbLightingCommand::SetPower(if on {
                LightingPowerState::On
            } else {
                LightingPowerState::Off
            })),
        ))
    }

    /// Enqueues a solid-color command if the verified profile is ready.
    pub fn set_solid_color(&self, red: u8, green: u8, blue: u8) -> bool {
        self.command(mobile_melk_transport_action(
            MelkLightingProfile::write_action(RgbLightingCommand::SetSolidColor(RgbColor::new(
                red, green, blue,
            ))),
        ))
    }

    /// Enqueues a brightness command if the value is valid and the profile is ready.
    ///
    /// # Errors
    ///
    /// Returns [`MobileMelkLightingError::InvalidBrightness`] when `percentage` exceeds 100.
    pub fn set_brightness(&self, percentage: u8) -> Result<bool, MobileMelkLightingError> {
        let brightness = LightingBrightness::try_from_percent(percentage)
            .map_err(|_| MobileMelkLightingError::InvalidBrightness)?;
        let write = mobile_melk_transport_action(MelkLightingProfile::write_action(
            RgbLightingCommand::SetBrightness(brightness),
        ));
        Ok(self.command(write))
    }

    /// Enqueues an effect-speed command.
    pub fn set_effect_speed(&self, speed: u8) -> bool {
        self.command(mobile_melk_control(MelkControl::Speed(speed)))
    }

    /// Enqueues a complete restore state.
    ///
    /// # Errors
    ///
    /// Returns a validation error when the restore state is invalid.
    pub fn apply_state(
        &self,
        state: MobileMelkLightingRestoreStateDto,
    ) -> Result<bool, MobileMelkLightingError> {
        LightingBrightness::try_from_percent(state.brightness)
            .map_err(|_| MobileMelkLightingError::InvalidBrightness)?;
        let state = state
            .try_into()
            .map_err(|_| MobileMelkLightingError::InvalidPlayback)?;
        let writes = MelkLightingProfile::plan_state(state)
            .into_iter()
            .map(mobile_melk_transport_action)
            .collect::<Vec<_>>();
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if !inner.is_ready()
            || writes
                .iter()
                .any(|write| write.mode != MobileMelkLightingWriteModeDto::WithoutResponse)
        {
            return Ok(false);
        }
        Ok(inner.queue_writes(writes.into_iter().map(Into::into)))
    }

    /// Enqueues a controller-local schedule after synchronizing its local clock.
    ///
    /// # Errors
    ///
    /// Returns a validation error when the schedule or clock is invalid.
    pub fn set_schedule(
        &self,
        schedule: MobileMelkScheduleDto,
        clock: MobileMelkClockDto,
    ) -> Result<bool, MobileMelkLightingError> {
        let writes = plan_schedule(schedule, clock)?;
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if !inner.is_ready() {
            return Ok(false);
        }
        Ok(inner.queue_writes(writes.into_iter().map(Into::into)))
    }

    /// Marks the latest requested command as physically confirmed.
    pub fn mark_last_command_confirmed(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .mark_last_command_confirmed()
    }

    /// Marks the latest requested command as explicitly unconfirmed.
    pub fn mark_last_command_unconfirmed(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .mark_last_command_unconfirmed();
    }

    /// Returns the current Rust-owned snapshot.
    #[must_use]
    pub fn snapshot(&self) -> MobileMelkLightingSessionSnapshotDto {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .snapshot()
            .into()
    }

    /// Drains requested platform operations.
    pub fn drain_actions(&self) -> Vec<MobileMelkLightingSessionActionDto> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_actions()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Drains bounded diagnostic records.
    pub fn drain_records(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_records()
    }

    /// Drains first-pairing candidates.
    pub fn drain_candidates(&self) -> Vec<MobileMelkLightingSessionCandidateDto> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_candidates()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Drains candidate identities removed by Rust's bounded scan policy.
    pub fn drain_candidate_removals(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_candidate_removals()
    }

    /// Drains raw FFF4 notifications for the app-level observer.
    pub fn drain_notifications(&self) -> Vec<Vec<u8>> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_notifications()
    }

    /// Drains one pending user write when `CoreBluetooth` reports available capacity.
    pub fn flush_writes(&self, can_send: bool) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_writes(can_send);
    }
}

impl MobileMelkLightingSessionCore {
    fn command(&self, write: MobileMelkLightingWriteDto) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if !inner.is_ready() {
            return false;
        }
        inner.queue_write(write.into())
    }
}

impl From<MobileMelkLightingSessionEventDto> for application::LightingSessionEvent {
    fn from(event: MobileMelkLightingSessionEventDto) -> Self {
        use MobileMelkLightingSessionEventDto as Native;
        use application::LightingSessionEvent as App;
        match event {
            Native::Resume => App::Resume,
            Native::BluetoothState {
                powered_on,
                state_code,
            } => App::BluetoothState {
                powered_on,
                state_code,
            },
            Native::Discovered {
                name,
                platform_identifier,
                rssi,
            } => App::Discovered {
                name,
                platform_identifier,
                rssi,
            },
            Native::Restored {
                name,
                platform_identifier,
                connected,
                pending,
            } => App::Restored {
                name,
                platform_identifier,
                connected,
                pending,
            },
            Native::RestoreUnavailable => App::RestoreUnavailable,
            Native::Connected {
                name,
                platform_identifier,
            } => App::Connected {
                name,
                platform_identifier,
            },
            Native::ConnectFailed { reason } => App::ConnectFailed { reason },
            Native::ConnectTimeout => App::ConnectTimeout,
            Native::ServicesDiscovered {
                service_uuids,
                error,
            } => App::ServicesDiscovered {
                service_uuids: service_uuids.into_iter().map(Into::into).collect(),
                error,
            },
            Native::CharacteristicsDiscovered {
                name,
                service_uuid,
                characteristics,
                error,
            } => App::CharacteristicsDiscovered {
                name,
                service_uuid: service_uuid.into(),
                characteristics: characteristics.into_iter().map(Into::into).collect(),
                error,
            },
            Native::NotificationState {
                characteristic,
                ready,
                can_send,
                error,
            } => App::NotificationState {
                characteristic: characteristic.into(),
                ready,
                can_send,
                error,
            },
            Native::Notification {
                characteristic,
                bytes,
            } => App::Notification {
                characteristic: characteristic.into(),
                bytes,
            },
            Native::WriteReady { can_send } => App::WriteReady { can_send },
            Native::TimerFired { timer, can_send } => App::TimerFired {
                timer: timer.into(),
                can_send,
            },
            Native::Disconnected { reason, powered_on } => App::Disconnected { reason, powered_on },
        }
    }
}

impl From<MobileMelkLightingSessionStateDto> for application::LightingSessionState {
    fn from(state: MobileMelkLightingSessionStateDto) -> Self {
        use MobileMelkLightingSessionStateDto as Native;
        use application::LightingSessionState as App;
        match state {
            Native::Idle => App::Idle,
            Native::Scanning => App::Scanning,
            Native::Connecting => App::Connecting,
            Native::Retrying {
                attempt,
                delay_milliseconds,
            } => App::Retrying {
                attempt,
                delay_milliseconds,
            },
            Native::Discovering => App::Discovering,
            Native::Ready => App::Ready,
            Native::Disconnected => App::Disconnected,
            Native::Failed { reason } => App::Failed { reason },
        }
    }
}

impl From<application::LightingSessionState> for MobileMelkLightingSessionStateDto {
    fn from(state: application::LightingSessionState) -> Self {
        use MobileMelkLightingSessionStateDto as Native;
        use application::LightingSessionState as App;
        match state {
            App::Idle => Native::Idle,
            App::Scanning => Native::Scanning,
            App::Connecting => Native::Connecting,
            App::Retrying {
                attempt,
                delay_milliseconds,
            } => Native::Retrying {
                attempt,
                delay_milliseconds,
            },
            App::Discovering => Native::Discovering,
            App::Ready => Native::Ready,
            App::Disconnected => Native::Disconnected,
            App::Failed { reason } => Native::Failed { reason },
        }
    }
}

impl From<MobileMelkLightingTimerDto> for application::LightingTimer {
    fn from(timer: MobileMelkLightingTimerDto) -> Self {
        match timer {
            MobileMelkLightingTimerDto::ConnectionAttempt => Self::ConnectionAttempt,
            MobileMelkLightingTimerDto::Discovery => Self::Discovery,
            MobileMelkLightingTimerDto::Reconnect => Self::Reconnect,
            MobileMelkLightingTimerDto::Initialization => Self::Initialization,
            MobileMelkLightingTimerDto::WriteDrain => Self::WriteDrain,
        }
    }
}

impl From<application::LightingTimer> for MobileMelkLightingTimerDto {
    fn from(timer: application::LightingTimer) -> Self {
        match timer {
            application::LightingTimer::ConnectionAttempt => Self::ConnectionAttempt,
            application::LightingTimer::Discovery => Self::Discovery,
            application::LightingTimer::Reconnect => Self::Reconnect,
            application::LightingTimer::Initialization => Self::Initialization,
            application::LightingTimer::WriteDrain => Self::WriteDrain,
        }
    }
}

impl From<MobileMelkLightingCharacteristicEvidenceDto>
    for application::LightingCharacteristicEvidence
{
    fn from(value: MobileMelkLightingCharacteristicEvidenceDto) -> Self {
        Self {
            uuid: value.uuid.into(),
            write_without_response: value.write_without_response,
            notify_or_indicate: value.notify_or_indicate,
        }
    }
}

impl From<MobileMelkLightingWriteDto> for application_melk::LightingWrite {
    fn from(value: MobileMelkLightingWriteDto) -> Self {
        Self {
            characteristic: value.characteristic.into(),
            payload: value.payload,
            mode: match value.mode {
                MobileMelkLightingWriteModeDto::WithoutResponse => {
                    application_melk::LightingWriteMode::WithoutResponse
                }
            },
            confirmation_characteristic: value.confirmation_characteristic.into(),
            minimum_interval_ms: value.minimum_interval_ms,
        }
    }
}

impl From<application_melk::LightingWrite> for MobileMelkLightingWriteDto {
    fn from(value: application_melk::LightingWrite) -> Self {
        Self {
            characteristic: value.characteristic.into(),
            payload: value.payload,
            mode: MobileMelkLightingWriteModeDto::WithoutResponse,
            confirmation_characteristic: value.confirmation_characteristic.into(),
            minimum_interval_ms: value.minimum_interval_ms,
        }
    }
}

impl From<application::LightingSessionSnapshot> for MobileMelkLightingSessionSnapshotDto {
    fn from(value: application::LightingSessionSnapshot) -> Self {
        Self {
            state: value.state.into(),
            platform_identifier: value.platform_identifier,
            name: value.name,
            command_status: value.command_status,
            notification_ready: value.notification_ready,
        }
    }
}

impl From<application::LightingSessionCandidate> for MobileMelkLightingSessionCandidateDto {
    fn from(value: application::LightingSessionCandidate) -> Self {
        Self {
            platform_identifier: value.platform_identifier,
            name: value.name,
            rssi: value.rssi,
        }
    }
}

impl From<application::LightingSessionAction> for MobileMelkLightingSessionActionDto {
    fn from(action: application::LightingSessionAction) -> Self {
        use MobileMelkLightingSessionActionDto as Native;
        use application::LightingSessionAction as App;
        match action {
            App::RestartTransport => Native::RestartTransport,
            App::Scan => Native::Scan,
            App::StopScan => Native::StopScan,
            App::RestorePeripheral {
                platform_identifier,
            } => Native::RestorePeripheral {
                platform_identifier,
            },
            App::Connect {
                platform_identifier,
            } => Native::Connect {
                platform_identifier,
            },
            App::CancelConnect {
                platform_identifier,
            } => Native::CancelConnect {
                platform_identifier,
            },
            App::DiscoverServices {
                platform_identifier,
                service,
            } => Native::DiscoverServices {
                platform_identifier,
                service: service.into(),
            },
            App::DiscoverCharacteristics {
                platform_identifier,
                service,
            } => Native::DiscoverCharacteristics {
                platform_identifier,
                service: service.into(),
            },
            App::Subscribe {
                platform_identifier,
                characteristic,
            } => Native::Subscribe {
                platform_identifier,
                characteristic: characteristic.into(),
            },
            App::Write {
                platform_identifier,
                write,
            } => Native::Write {
                platform_identifier,
                write: write.into(),
            },
            App::ArmTimer {
                timer,
                delay_milliseconds,
            } => Native::ArmTimer {
                timer: timer.into(),
                delay_milliseconds,
            },
        }
    }
}
