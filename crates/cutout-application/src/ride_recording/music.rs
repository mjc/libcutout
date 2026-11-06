//! Music history operations sharing the ride recording owner and durable worker.

use super::{RecordingState, RideRecordingApplication, RideRecordingError, require_ready};
use cutout_core::{MonotonicTimestamp, WallClockUnixTimestamp};
use cutout_music::{
    MusicHistoryPolicy, MusicPlaybackPosition, MusicPlaybackState, MusicRideEvent,
    MusicRideEventKind, MusicSnapshot, MusicTimelineOutcome, MusicValidationError,
};
use cutout_ride_maps::RideLifecycleState;
use libcutout_persistence::{
    MusicHistory, MusicTimelineRecordResult, PendingMusicEventWrite, PendingMusicHistoryRead,
    PendingMusicHistoryWrite, RideId, StorageError,
};
use std::sync::{Arc, Mutex, PoisonError};

/// Admission or persistence failure for a ride music operation.
#[derive(Debug, thiserror::Error)]
pub enum RideMusicError {
    /// The recording owner rejected the operation.
    #[error(transparent)]
    Recording(#[from] RideRecordingError),
    /// A durable database is required for this operation.
    #[error("music history storage is unavailable")]
    Unavailable,
    /// The durable worker rejected or failed the operation.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// The observation or event timing is invalid.
    #[error(transparent)]
    InvalidInput(#[from] MusicValidationError),
}

/// Classification of a callback against the current recording interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideMusicContext {
    RideNotOpen,
    OutOfOrder,
    Ready(super::RideRecordingToken),
}

/// Authoritative selected ride and terminal history proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideMusicTerminalContext {
    pub ride_id: Option<RideId>,
    pub closed: bool,
}

/// A provider observation on the current process clock.
#[derive(Clone, Debug)]
pub struct RideMusicObservation {
    pub snapshot: MusicSnapshot,
    pub kind: MusicRideEventKind,
    pub monotonic_at_ms: u64,
    pub wall_clock_at_ms: u64,
    pub clock_uncertainty_ms: u64,
}

/// A history event decision, including admission before a durable write exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideMusicEventOutcome {
    /// The selected ride is no longer actively recording.
    RideNotOpen,
    /// The shared domain timeline classified the event.
    Timeline(MusicTimelineOutcome),
}

/// Authoritative event result and optional durable sequence/policy receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideMusicEventResult {
    pub outcome: RideMusicEventOutcome,
    pub sequence: Option<u64>,
    pub effective_policy: Option<MusicHistoryPolicy>,
}

impl From<MusicTimelineRecordResult> for RideMusicEventResult {
    fn from(result: MusicTimelineRecordResult) -> Self {
        Self {
            outcome: RideMusicEventOutcome::Timeline(result.outcome),
            sequence: result.sequence,
            effective_policy: Some(result.effective_policy),
        }
    }
}

impl RideMusicEventResult {
    fn immediate(outcome: RideMusicEventOutcome) -> Self {
        Self {
            outcome,
            sequence: None,
            effective_policy: None,
        }
    }
}

/// An admitted event is either complete or queued on the existing durable worker.
#[derive(Debug)]
pub enum RideMusicEventAdmission {
    Completed(RideMusicEventResult),
    Pending(PendingMusicEventWrite),
}

/// A current-history request keeps absence separate from unavailable persistence.
#[derive(Debug)]
pub enum RideMusicHistoryRead {
    Absent,
    Unavailable,
    Pending(PendingMusicHistoryRead),
}

/// A durable policy/deletion receipt which owns its cache settlement.
#[derive(Debug)]
pub struct PendingRideMusicWrite {
    state: Arc<Mutex<RecordingState>>,
    database: libcutout_persistence::RideDatabase,
    ride_id: RideId,
    previous_policy: Option<MusicHistoryPolicy>,
    policy: Option<MusicHistoryPolicy>,
    cache_epoch: u64,
    deletes_history: bool,
    storage: PendingMusicHistoryWrite,
}

impl PendingRideMusicWrite {
    /// Polls without blocking; applies a deletion receipt only to its owning selected ride.
    pub fn try_result(&mut self) -> Option<Result<(), StorageError>> {
        let result = self.storage.try_result()?;
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let current =
            state.ride_id == Some(self.ride_id) && state.music_cache_epoch == self.cache_epoch;
        if !current {
            return Some(result);
        }
        if self.deletes_history {
            match &result {
                Ok(()) => state.music_history_policy = Some(MusicHistoryPolicy::Disabled),
                Err(_) => {
                    if let Some(policy) = self.previous_policy {
                        state.music_history_policy = Some(policy);
                    }
                }
            }
        } else if let (Ok(()), Some(policy)) = (&result, self.policy) {
            state.music_history_policy = Some(policy);
            state.music_history_available = true;
        } else if result.is_err()
            && let Ok(policy) = self.database.music_history_policy(self.ride_id)
        {
            state.music_history_policy = Some(policy);
            state.music_history_available = true;
        }
        Some(result)
    }
}

impl RideRecordingApplication {
    /// Validates recording identity and generation before an observation can claim its write.
    ///
    /// # Errors
    /// Returns a readiness or stale-recording error.
    pub fn validate_music_recording(
        &self,
        token: super::RideRecordingToken,
    ) -> Result<(), RideRecordingError> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        require_ready(&state)?;
        if state.ride_id != Some(token.ride_id)
            || state.generation != token.generation
            || state.recorder.state() != Some(RideLifecycleState::Active)
        {
            return Err(RideRecordingError::StaleCommand);
        }
        Ok(())
    }

    /// Resolves one provider callback against the selected ride and its active interval.
    ///
    /// # Errors
    /// Returns readiness or stale-ride errors before the callback is admitted.
    pub fn resolve_music_context(
        &self,
        expected: RideId,
        observed_at_ms: u64,
    ) -> Result<RideMusicContext, RideRecordingError> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        require_ready(&state)?;
        if state.ride_id != Some(expected) {
            return Err(RideRecordingError::StaleCommand);
        }
        if state.recorder.state() != Some(RideLifecycleState::Active) {
            return Ok(RideMusicContext::RideNotOpen);
        }
        let at = cutout_ride_maps::MonotonicMilliseconds::new(
            observed_at_ms.saturating_add(state.monotonic_epoch_offset_ms),
        );
        if !state.recorder.accepts_recording_observation_at(at) {
            return Ok(RideMusicContext::OutOfOrder);
        }
        Ok(RideMusicContext::Ready(super::RideRecordingToken {
            ride_id: expected,
            generation: state.generation,
        }))
    }

    /// Reads terminal history proof without inferring lifecycle in a binding adapter.
    #[must_use]
    pub fn music_history_terminal_context(&self) -> RideMusicTerminalContext {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let closed = match state.recorder.state() {
            Some(
                RideLifecycleState::Stopped
                | RideLifecycleState::Saved
                | RideLifecycleState::Discarded,
            ) => true,
            _ => false,
        };
        RideMusicTerminalContext {
            ride_id: state.ride_id,
            closed,
        }
    }

    /// Reads the authoritative policy where possible, retaining the last known choice on error.
    #[must_use]
    pub fn current_music_history_policy(&self) -> MusicHistoryPolicy {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(ride_id) = state.ride_id else {
            return MusicHistoryPolicy::Disabled;
        };
        if let Some(database) = self.database.as_ref()
            && let Ok(policy) = database.music_history_policy(ride_id)
        {
            state.music_history_policy = Some(policy);
        }
        state
            .music_history_policy
            .unwrap_or(MusicHistoryPolicy::Disabled)
    }

    /// Saves a policy for the current recordable ride and publishes only its durable receipt.
    ///
    /// # Errors
    /// Returns a recording, storage availability, or persistence error.
    pub fn set_music_history_policy(
        &self,
        policy: MusicHistoryPolicy,
    ) -> Result<(), RideMusicError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ride_id = recordable_ride(&state)?;
        self.music_database()?
            .save_music_history_policy(ride_id, policy)?;
        state.music_history_policy = Some(policy);
        state.music_history_available = true;
        reserve_music_cache(&mut state);
        Ok(())
    }

    /// Queues a policy change in the same database FIFO as later events and lifecycle commands.
    ///
    /// # Errors
    /// Returns a recording, storage availability, or queue admission error.
    pub fn begin_set_music_history_policy(
        &self,
        policy: MusicHistoryPolicy,
    ) -> Result<PendingRideMusicWrite, RideMusicError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ride_id = recordable_ride(&state)?;
        let storage = self
            .music_database()?
            .queue_save_music_history_policy(ride_id, policy)?;
        let cache_epoch = reserve_music_cache(&mut state);
        Ok(PendingRideMusicWrite {
            state: Arc::clone(&self.state),
            database: self.music_database()?.clone(),
            ride_id,
            previous_policy: None,
            policy: Some(policy),
            cache_epoch,
            deletes_history: false,
            storage,
        })
    }

    /// Queues deletion and fences the selected ride's cache only when that ride is deleted.
    ///
    /// # Errors
    /// Returns storage availability or durable queue admission errors.
    pub fn begin_delete_music_history(
        &self,
        ride_id: RideId,
    ) -> Result<PendingRideMusicWrite, RideMusicError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let selected = state.ride_id == Some(ride_id);
        let previous_policy = selected.then_some(
            state
                .music_history_policy
                .unwrap_or(MusicHistoryPolicy::Disabled),
        );
        let storage = self.music_database()?.queue_delete_music_history(ride_id)?;
        // An unrelated ride does not own the active cache. Reserving here would make a pending
        // policy receipt for the selected ride miss its epoch and leave the old policy published.
        let cache_epoch = if selected {
            reserve_music_cache(&mut state)
        } else {
            state.music_cache_epoch
        };
        if selected {
            state.music_history_policy = Some(MusicHistoryPolicy::Disabled);
        }
        Ok(PendingRideMusicWrite {
            state: Arc::clone(&self.state),
            database: self.music_database()?.clone(),
            ride_id,
            previous_policy,
            policy: None,
            cache_epoch,
            deletes_history: true,
            storage,
        })
    }

    /// Records one low-rate provider transition using the current durable privacy policy.
    ///
    /// # Errors
    /// Returns a recording, validation, availability, or durable write error.
    pub fn record_music_event(
        &self,
        observation: &RideMusicObservation,
    ) -> Result<RideMusicEventResult, RideMusicError> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let prepared = prepare_event(&state, observation)?;
        let (ride_id, snapshot, at) = match prepared {
            PreparedEvent::Completed(result) => return Ok(result),
            PreparedEvent::Ready {
                ride_id,
                snapshot,
                at,
            } => (ride_id, snapshot, at),
        };
        let database = self.music_database()?;
        let policy = database.music_history_policy(ride_id)?;
        let Some(event) = event_from(observation, &snapshot, at, policy)? else {
            return Ok(RideMusicEventResult::immediate(
                RideMusicEventOutcome::Timeline(MusicTimelineOutcome::Disabled),
            ));
        };
        Ok(database
            .record_music_event_with_sequence(ride_id, event)?
            .into())
    }

    /// Queues an event; the ordered durable worker applies the policy at commit time.
    ///
    /// # Errors
    /// Returns a recording, validation, availability, or queue admission error.
    pub fn begin_record_music_event(
        &self,
        observation: &RideMusicObservation,
    ) -> Result<RideMusicEventAdmission, RideMusicError> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let prepared = prepare_event(&state, observation)?;
        let (ride_id, snapshot, at) = match prepared {
            PreparedEvent::Completed(result) => {
                return Ok(RideMusicEventAdmission::Completed(result));
            }
            PreparedEvent::Ready {
                ride_id,
                snapshot,
                at,
            } => (ride_id, snapshot, at),
        };
        let database = self.music_database()?;
        let Some(event) = event_from(
            observation,
            &snapshot,
            at,
            MusicHistoryPolicy::HumanReadable,
        )?
        else {
            return Ok(RideMusicEventAdmission::Completed(
                RideMusicEventResult::immediate(RideMusicEventOutcome::Timeline(
                    MusicTimelineOutcome::Disabled,
                )),
            ));
        };
        Ok(RideMusicEventAdmission::Pending(
            database.queue_record_music_event(ride_id, event)?,
        ))
    }

    /// Queues history for the current selection, retaining absence and unavailable storage.
    ///
    /// # Errors
    /// Returns a durable queue admission error.
    pub fn begin_current_music_history(&self) -> Result<RideMusicHistoryRead, RideMusicError> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(ride_id) = state.ride_id else {
            return Ok(RideMusicHistoryRead::Absent);
        };
        if state.recorder.state() == Some(RideLifecycleState::Discarded) {
            return Ok(RideMusicHistoryRead::Absent);
        }
        let Some(database) = self.database.as_ref() else {
            return Ok(RideMusicHistoryRead::Unavailable);
        };
        Ok(RideMusicHistoryRead::Pending(
            database.queue_music_history(ride_id)?,
        ))
    }

    /// Reads current authoritative history and retains whether the latest restore succeeded.
    #[must_use]
    pub fn current_music_history(&self) -> Option<Result<MusicHistory, RideMusicError>> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ride_id = state.ride_id?;
        if state.recorder.state() == Some(RideLifecycleState::Discarded) {
            return None;
        }
        let result = self
            .music_database()
            .and_then(|database| database.music_history(ride_id).map_err(Into::into));
        state.music_history_available = result.is_ok();
        Some(result)
    }

    /// Queues a history read for a caller-selected stored ride.
    ///
    /// # Errors
    /// Returns availability or durable queue admission errors.
    pub fn begin_stored_music_history(
        &self,
        ride_id: RideId,
    ) -> Result<PendingMusicHistoryRead, RideMusicError> {
        Ok(self.music_database()?.queue_music_history(ride_id)?)
    }

    /// Reads history for a caller-selected stored ride.
    ///
    /// # Errors
    /// Returns availability or durable storage errors.
    pub fn stored_music_history(&self, ride_id: RideId) -> Result<MusicHistory, RideMusicError> {
        Ok(self.music_database()?.music_history(ride_id)?)
    }

    /// Deletes stored history without changing the local recording or selected policy cache.
    ///
    /// # Errors
    /// Returns availability or durable storage errors.
    pub fn delete_stored_music_history(&self, ride_id: RideId) -> Result<(), RideMusicError> {
        Ok(self.music_database()?.delete_music_history(ride_id)?)
    }

    /// Redacts stored history to opaque identifiers through the durable owner.
    ///
    /// # Errors
    /// Returns availability or durable storage errors.
    pub fn redact_stored_music_history(&self, ride_id: RideId) -> Result<(), RideMusicError> {
        Ok(self
            .music_database()?
            .save_music_history_policy(ride_id, MusicHistoryPolicy::OpaqueItem)?)
    }

    /// Deletes history for the selected ride and clears its cached policy after commit.
    ///
    /// # Errors
    /// Returns a missing-ride, availability, or durable storage error.
    pub fn delete_current_music_history(&self) -> Result<(), RideMusicError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ride_id = state.ride_id.ok_or(RideRecordingError::NoActiveRide)?;
        self.music_database()?.delete_music_history(ride_id)?;
        state.music_history_policy = Some(MusicHistoryPolicy::Disabled);
        reserve_music_cache(&mut state);
        Ok(())
    }

    fn music_database(&self) -> Result<&libcutout_persistence::RideDatabase, RideMusicError> {
        self.database.as_ref().ok_or(RideMusicError::Unavailable)
    }
}

fn reserve_music_cache(state: &mut RecordingState) -> u64 {
    state.music_cache_epoch = state.music_cache_epoch.saturating_add(1);
    state.music_cache_epoch
}

fn recordable_ride(state: &RecordingState) -> Result<RideId, RideRecordingError> {
    require_ready(state)?;
    let ride_id = state.ride_id.ok_or(RideRecordingError::NoActiveRide)?;
    if !state
        .recorder
        .state()
        .is_some_and(RideLifecycleState::is_recording)
    {
        return Err(RideRecordingError::InvalidTransition);
    }
    Ok(ride_id)
}

enum PreparedEvent {
    Completed(RideMusicEventResult),
    Ready {
        ride_id: RideId,
        snapshot: MusicSnapshot,
        at: MonotonicTimestamp,
    },
}

fn prepare_event(
    state: &RecordingState,
    observation: &RideMusicObservation,
) -> Result<PreparedEvent, RideMusicError> {
    require_ready(state)?;
    let offset = state.monotonic_epoch_offset_ms;
    let at = MonotonicTimestamp::new(observation.monotonic_at_ms.saturating_add(offset));
    let original = &observation.snapshot;
    let snapshot = MusicSnapshot::new(
        original.provider(),
        original.session_id().as_str(),
        original.state(),
        original.item().cloned(),
        MusicPlaybackPosition::new(
            original.position_milliseconds(),
            original.duration_milliseconds(),
        )?,
        MonotonicTimestamp::new(
            original
                .observed_at()
                .as_milliseconds()
                .saturating_add(offset),
        ),
        original.capabilities(),
    )?;
    let ride_id = state.ride_id.ok_or(RideRecordingError::NoActiveRide)?;
    if state.recorder.state() != Some(RideLifecycleState::Active) {
        return Ok(PreparedEvent::Completed(RideMusicEventResult::immediate(
            RideMusicEventOutcome::RideNotOpen,
        )));
    }
    if snapshot.state() == MusicPlaybackState::Stale || snapshot.observed_at() > at {
        return Ok(PreparedEvent::Completed(RideMusicEventResult::immediate(
            RideMusicEventOutcome::Timeline(MusicTimelineOutcome::OutOfOrder),
        )));
    }
    Ok(PreparedEvent::Ready {
        ride_id,
        snapshot,
        at,
    })
}

fn event_from(
    observation: &RideMusicObservation,
    snapshot: &MusicSnapshot,
    at: MonotonicTimestamp,
    policy: MusicHistoryPolicy,
) -> Result<Option<MusicRideEvent>, MusicValidationError> {
    MusicRideEvent::try_from_snapshot(
        snapshot,
        observation.kind,
        at,
        WallClockUnixTimestamp::new(observation.wall_clock_at_ms),
        observation.clock_uncertainty_ms,
        policy,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_history_restore_policy_repair_and_deletion_share_the_recording_owner() {
        let _guard = crate::ride_recording::tests::DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-app-ride-music-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = libcutout_persistence::RideDatabase::open(&path).expect("database opens");
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().expect("restore admitted"))
            .expect("restore completes");
        let started = application
            .finish_start(
                application
                    .begin_start(100, 1_700_000_000_000, None, None)
                    .expect("start admitted"),
            )
            .expect("start commits");
        application
            .set_music_history_policy(MusicHistoryPolicy::HumanReadable)
            .expect("history enabled");
        application
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .music_history_available = false;
        application
            .set_music_history_policy(MusicHistoryPolicy::OpaqueItem)
            .expect("durable policy repairs history availability");
        assert!(application.snapshot().music_history_available);
        assert_eq!(
            application.current_music_history_policy(),
            MusicHistoryPolicy::OpaqueItem
        );
        let mut deletion = application
            .begin_delete_music_history(started.ride_id.expect("ride exists"))
            .expect("deletion admitted");
        assert_eq!(
            application.snapshot().music_history_policy,
            Some(MusicHistoryPolicy::Disabled)
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(result) = deletion.try_result() {
                result.expect("deletion commits");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "music deletion did not settle"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            application.current_music_history_policy(),
            MusicHistoryPolicy::Disabled
        );
        assert_eq!(
            application
                .current_music_history()
                .expect("current ride")
                .expect("history readable")
                .status,
            libcutout_persistence::MusicHistoryStatus::Deleted
        );
        database.clone().shutdown().expect("database shuts down");
        drop(deletion);
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    fn poll_music_write(pending: &mut PendingRideMusicWrite) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(result) = pending.try_result() {
                result.expect("music write commits");
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "music write did not settle"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn queued_policy_receipt_publishes_the_cache_and_keeps_a_newer_change() {
        let _guard = crate::ride_recording::tests::DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-app-ride-music-policy-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = libcutout_persistence::RideDatabase::open(&path).expect("database opens");
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().expect("restore admitted"))
            .expect("restore completes");
        let started = application
            .finish_start(
                application
                    .begin_start(100, 1_700_000_000_000, None, None)
                    .expect("start admitted"),
            )
            .expect("start commits");
        let ride_id = started.ride_id.expect("ride exists");
        application
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .music_history_available = false;

        let mut queued = application
            .begin_set_music_history_policy(MusicHistoryPolicy::OpaqueItem)
            .expect("policy queues");
        poll_music_write(&mut queued);
        assert_eq!(
            application.snapshot().music_history_policy,
            Some(MusicHistoryPolicy::OpaqueItem)
        );
        assert!(application.snapshot().music_history_available);

        let mut older = application
            .begin_set_music_history_policy(MusicHistoryPolicy::HumanReadable)
            .expect("older policy queues");
        application
            .set_music_history_policy(MusicHistoryPolicy::Disabled)
            .expect("newer policy commits");
        poll_music_write(&mut older);
        assert_eq!(
            application.snapshot().music_history_policy,
            Some(MusicHistoryPolicy::Disabled)
        );

        let mut superseded = application
            .begin_set_music_history_policy(MusicHistoryPolicy::HumanReadable)
            .expect("superseded policy queues");
        let mut deletion = application
            .begin_delete_music_history(ride_id)
            .expect("deletion queues");
        assert_eq!(
            application.snapshot().music_history_policy,
            Some(MusicHistoryPolicy::Disabled)
        );
        poll_music_write(&mut superseded);
        assert_eq!(
            application.snapshot().music_history_policy,
            Some(MusicHistoryPolicy::Disabled),
            "an older policy receipt must not undo a deletion"
        );
        poll_music_write(&mut deletion);
        assert_eq!(
            application.snapshot().music_history_policy,
            Some(MusicHistoryPolicy::Disabled)
        );

        database.clone().shutdown().expect("database shuts down");
        drop(queued);
        drop(older);
        drop(superseded);
        drop(deletion);
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unrelated_history_deletion_keeps_a_pending_policy_receipt() {
        let _guard = crate::ride_recording::tests::DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-app-ride-music-unrelated-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = libcutout_persistence::RideDatabase::open(&path).expect("database opens");
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().expect("restore admitted"))
            .expect("restore completes");
        application
            .finish_start(
                application
                    .begin_start(100, 1_700_000_000_000, None, None)
                    .expect("start admitted"),
            )
            .expect("start commits");

        let mut queued = application
            .begin_set_music_history_policy(MusicHistoryPolicy::OpaqueItem)
            .expect("policy queues");
        let mut unrelated = application
            .begin_delete_music_history(RideId::new())
            .expect("other ride deletion queues");
        poll_music_write(&mut queued);
        assert_eq!(
            application.snapshot().music_history_policy,
            Some(MusicHistoryPolicy::OpaqueItem)
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if unrelated.try_result().is_some() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "unrelated deletion did not settle"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            application.snapshot().music_history_policy,
            Some(MusicHistoryPolicy::OpaqueItem)
        );

        database.clone().shutdown().expect("database shuts down");
        drop(queued);
        drop(unrelated);
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }
}
