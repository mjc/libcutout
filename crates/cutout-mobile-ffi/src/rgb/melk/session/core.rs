//! Thread-safe UniFFI handle for the MELK session reducer.

use std::sync::{Arc, Mutex, PoisonError};

use cutout_core::{
    LightingBrightness, LightingPowerState, MelkClock, MelkControl, MelkSchedule, RgbColor,
    RgbLightingCommand,
};
use cutout_protocols::MelkLightingProfile;

use super::{contract::*, reducer::SessionReducer};
use crate::{
    MobileMelkClockDto, MobileMelkLightingError, MobileMelkLightingRestoreStateDto,
    MobileMelkLightingWriteDto, MobileMelkLightingWriteModeDto, MobileMelkScheduleDto,
    mobile_melk_control, mobile_melk_transport_action,
};

/// Rust-owned MELK session exposed to the thin CoreBluetooth wrapper.
#[derive(Debug, uniffi::Object)]
pub struct MobileMelkLightingSessionCore {
    inner: Mutex<SessionReducer>,
}

#[uniffi::export]
impl MobileMelkLightingSessionCore {
    /// Creates an idle session core.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(SessionReducer::default()),
        })
    }

    /// Starts the session with an optional remembered platform identifier.
    pub fn start(&self, preferred_platform_identifier: Option<String>) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .start(preferred_platform_identifier);
    }

    /// Stops the session and prevents future reconnects.
    pub fn stop(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stop();
    }

    /// Submits one CoreBluetooth event.
    pub fn handle(&self, event: MobileMelkLightingSessionEventDto) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .handle(event);
    }

    /// Selects a first-pairing candidate.
    pub fn select_candidate(&self, platform_identifier: String) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .select(platform_identifier);
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
        Ok(inner.queue_writes(writes))
    }

    /// Enqueues a controller-local schedule after synchronizing its local clock.
    pub fn set_schedule(
        &self,
        schedule: MobileMelkScheduleDto,
        clock: MobileMelkClockDto,
    ) -> Result<bool, MobileMelkLightingError> {
        let schedule = MelkSchedule::new(
            if schedule.power_on {
                LightingPowerState::On
            } else {
                LightingPowerState::Off
            },
            schedule.hour,
            schedule.minute,
            schedule.days,
            schedule.enabled,
        )
        .map_err(|_| MobileMelkLightingError::InvalidSchedule)?;
        let clock = MelkClock::new(clock.hour, clock.minute, clock.second, clock.weekday)
            .map_err(|_| MobileMelkLightingError::InvalidClock)?;
        if !MelkLightingProfile::capabilities().schedules {
            return Err(MobileMelkLightingError::UnsupportedCapability);
        }
        let writes = [
            mobile_melk_transport_action(MelkLightingProfile::control_action(MelkControl::Clock(
                clock,
            ))),
            mobile_melk_transport_action(MelkLightingProfile::control_action(
                MelkControl::Schedule(schedule),
            )),
        ];
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if !inner.is_ready() {
            return Ok(false);
        }
        Ok(inner.queue_writes(writes))
    }

    /// Marks the latest requested command as physically confirmed.
    pub fn mark_last_command_confirmed(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .mark_last_command_confirmed();
    }

    /// Marks the latest requested command as explicitly unconfirmed.
    pub fn mark_last_command_unconfirmed(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .mark_last_command_unconfirmed();
    }

    /// Returns the current Rust-owned snapshot.
    pub fn snapshot(&self) -> MobileMelkLightingSessionSnapshotDto {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .snapshot()
    }

    /// Drains requested platform operations.
    pub fn drain_actions(&self) -> Vec<MobileMelkLightingSessionActionDto> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_actions()
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
    }

    /// Drains raw FFF4 notifications for the app-level observer.
    pub fn drain_notifications(&self) -> Vec<Vec<u8>> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_notifications()
    }

    /// Drains one pending user write when CoreBluetooth reports available capacity.
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
        inner.queue_write(write)
    }
}
