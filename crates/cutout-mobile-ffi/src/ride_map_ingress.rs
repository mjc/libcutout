use crate::{
    MAX_PENDING_LOCATION_WRITES, MobileBmsVoltageWriteOutcomeDto, MobilePhoneLocationSampleDto,
    MobileRideMapAdmissionState, MobileRideMapConnectionAdmission, MobileRideMapCore,
    MobileRideMapCoreErrorDto, MobileRideMapCoreOutcomeDto, MobileRideMapCoreSnapshotDto,
    MobileRideMapRecordingTokenDto, MobileRideMapSnapshotLocationAcquisitionDto,
    RideDatabaseHandle, map_ride_database_error,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex, PoisonError},
};

/// Capacity ownership acquired before native dispatch retains a location callback.
#[derive(Debug, Default)]
pub(crate) struct LocationCallbackCapacity {
    state: Mutex<LocationCallbackCapacityState>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct LocationCallbackCapacityState {
    next_id: u64,
    owned_samples: usize,
    order: VecDeque<u64>,
}

#[derive(Debug)]
struct LocationCallbackPermit {
    capacity: Arc<LocationCallbackCapacity>,
    id: u64,
    weight: usize,
}

impl LocationCallbackCapacity {
    fn acquire(
        self: &Arc<Self>,
        sample_count: usize,
    ) -> Result<LocationCallbackPermit, MobileRideMapCoreErrorDto> {
        let weight = sample_count.clamp(1, MAX_PENDING_LOCATION_WRITES);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while state.owned_samples + weight > MAX_PENDING_LOCATION_WRITES {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        let id = state.next_id;
        state.next_id = id.checked_add(1).ok_or_else(|| {
            MobileRideMapCoreErrorDto::Storage("native location callback IDs exhausted".into())
        })?;
        state.order.push_back(id);
        state.owned_samples += weight;
        Ok(LocationCallbackPermit {
            capacity: Arc::clone(self),
            id,
            weight,
        })
    }
}

impl LocationCallbackPermit {
    fn wait_turn(&self) {
        let mut state = self
            .capacity
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while state.order.front() != Some(&self.id) {
            state = self
                .capacity
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

impl Drop for LocationCallbackPermit {
    fn drop(&mut self) {
        let mut state = self
            .capacity
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.order.retain(|id| *id != self.id);
        state.owned_samples -= self.weight;
        self.capacity.changed.notify_all();
    }
}

#[derive(Debug)]
struct NativeLocationCallback {
    permit: LocationCallbackPermit,
    recording: Option<MobileRideMapRecordingTokenDto>,
    receipt_monotonic_ms: u64,
    receipt_wall_clock_unix_ms: u64,
    samples: Vec<MobilePhoneLocationSampleDto>,
    generation_at_receipt: u64,
    may_adopt_connection_start: bool,
}

/// One-shot ownership of an ordered native callback before it enters the platform executor.
#[derive(Debug, uniffi::Object)]
pub struct MobileRideMapLocationCallback {
    core: Arc<MobileRideMapCore>,
    recording: Mutex<Option<MobileRideMapRecordingTokenDto>>,
    callback: Mutex<Option<NativeLocationCallback>>,
}

#[uniffi::export]
impl MobileRideMapLocationCallback {
    /// Returns the captured recording identity, resolved after a queued first autostart.
    #[must_use]
    pub fn recording_token(&self) -> Option<MobileRideMapRecordingTokenDto> {
        self.recording
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Waits for predecessors and durably records every sample on a background executor.
    /// Capacity is released on success, typed failure, or abandonment of this receipt.
    ///
    /// # Errors
    /// Returns the storage/admission error, or a stale command for an already consumed receipt.
    pub fn finish(&self) -> Result<Vec<MobileRideMapCoreOutcomeDto>, MobileRideMapCoreErrorDto> {
        let callback = self
            .callback
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or(MobileRideMapCoreErrorDto::StaleRideCommand)?;
        callback.permit.wait_turn();
        let recording = callback.recording.or_else(|| {
            let state = self
                .core
                .inner
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if callback.may_adopt_connection_start
                && callback.generation_at_receipt.checked_add(1) == Some(state.generation)
                && state.last_connection_transition_generation.is_some()
                && state.last_connection_transition_ride_id == state.ride_id
            {
                state
                    .snapshot_for_outcome(callback.receipt_monotonic_ms)
                    .and_then(|snapshot| snapshot.recording_token)
            } else {
                None
            }
        });
        self.recording
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone_from(&recording);
        self.core.ingest_location_callback_with_outcomes(
            recording,
            callback.receipt_monotonic_ms,
            callback.receipt_wall_clock_unix_ms,
            callback.samples,
        )
    }
}

#[uniffi::export]
impl MobileRideMapCore {
    /// Transfers complete callback ownership into Rust before native dispatch.
    /// At the existing pending-location sample budget, the producer waits for prior callbacks;
    /// no sample is dropped. A callback larger than the budget owns the whole budget until it
    /// settles, preserving the complete native batch while excluding other queued callbacks.
    /// The recording identity and receipt anchors are captured before waiting, fencing replacements.
    ///
    /// # Errors
    /// Returns a state error before admission or a storage error if callback IDs are exhausted.
    pub fn admit_location_callback(
        self: &Arc<Self>,
        receipt_monotonic_ms: u64,
        receipt_wall_clock_unix_ms: u64,
        samples: Vec<MobilePhoneLocationSampleDto>,
    ) -> Result<Arc<MobileRideMapLocationCallback>, MobileRideMapCoreErrorDto> {
        let (recording, capacity, generation_at_receipt, may_adopt_connection_start) = {
            let state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(error) = state.restoration_error() {
                return Err(error);
            }
            (
                state
                    .snapshot_for_outcome(receipt_monotonic_ms)
                    .and_then(|snapshot| snapshot.recording_token),
                Arc::clone(&state.location_callback_capacity),
                state.generation,
                state.recorder.state().is_none(),
            )
        };
        let permit = capacity.acquire(samples.len())?;
        Ok(Arc::new(MobileRideMapLocationCallback {
            core: Arc::clone(self),
            recording: Mutex::new(recording.clone()),
            callback: Mutex::new(Some(NativeLocationCallback {
                permit,
                recording,
                receipt_monotonic_ms,
                receipt_wall_clock_unix_ms,
                samples,
                generation_at_receipt,
                may_adopt_connection_start,
            })),
        }))
    }
}

/// Weighted Rust capacity held by a native connection/BMS callback until it settles.
#[derive(Debug, uniffi::Object)]
pub struct MobileRideMapRecordingWorkPermit {
    permit: Mutex<Option<LocationCallbackPermit>>,
}

#[uniffi::export]
impl MobileRideMapRecordingWorkPermit {
    /// Releases ownership after processing; abandonment also releases it through Rust Drop.
    pub fn release(&self) {
        let _ = self
            .permit
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

#[uniffi::export]
impl MobileRideMapCore {
    /// Returns whether native acquisition effects or recovery presentation changed since publication.
    /// Clock/revision-only updates do not trigger repeated platform acquisition work.
    pub fn take_location_acquisition_change(&self) -> bool {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let acquisition =
            MobileRideMapSnapshotLocationAcquisitionDto::from(state.location_acquisition());
        if state.published_location_acquisition == Some(acquisition) {
            return false;
        }
        state.published_location_acquisition = Some(acquisition);
        true
    }

    /// Bounds platform callback ownership before native executor dispatch, without dropping data.
    /// Connection observations cost one slot; BMS batches cost their observation count. Shares
    /// the same budget with GPS, excluding a large callback while any other work is outstanding.
    ///
    /// # Errors
    /// Returns an error if callback IDs have been exhausted.
    pub fn reserve_recording_work(
        &self,
        observation_count: u64,
    ) -> Result<Arc<MobileRideMapRecordingWorkPermit>, MobileRideMapCoreErrorDto> {
        let capacity = Arc::clone(
            &self
                .inner
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .location_callback_capacity,
        );
        let permit = capacity.acquire(usize::try_from(observation_count).unwrap_or(usize::MAX))?;
        Ok(Arc::new(MobileRideMapRecordingWorkPermit {
            permit: Mutex::new(Some(permit)),
        }))
    }
}

#[uniffi::export]
impl MobileRideMapConnectionAdmission {
    /// Waits for the actual storage receipt on a background executor and settles it once.
    /// Repeated wait/poll calls observe the same terminal result without a polling timer.
    ///
    /// # Errors
    /// Returns the terminal storage or connection-admission error.
    pub fn wait(&self) -> Result<Option<MobileRideMapCoreSnapshotDto>, MobileRideMapCoreErrorDto> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let pending = match std::mem::replace(
            &mut *state,
            MobileRideMapAdmissionState::Completed(Err(
                MobileRideMapCoreErrorDto::AdmissionPending,
            )),
        ) {
            MobileRideMapAdmissionState::Pending(pending) => pending,
            MobileRideMapAdmissionState::Completed(result) => {
                *state = MobileRideMapAdmissionState::Completed(result.clone());
                return result;
            }
        };
        let result = self.core.finish_verified_connection_admission(pending);
        *state = MobileRideMapAdmissionState::Completed(result.clone());
        result
    }
}

#[uniffi::export]
impl RideDatabaseHandle {
    /// Waits for queued BMS receipts on a background executor; no idle poll is required.
    pub fn finish_bms_voltage_writes(&self) -> Vec<MobileBmsVoltageWriteOutcomeDto> {
        let mut pending = self
            .pending_bms_writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        pending
            .writes
            .drain(..)
            .map(|item| MobileBmsVoltageWriteOutcomeDto {
                request_id: item.request_id,
                error: item.write.wait_result().err().map(map_ride_database_error),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CoreMusicHistoryPolicy, MobileRideMapAdmissionPollDto, MobileRideMapCoreDecisionDto,
        MobileRideMapCoreInner, MobileRideMapLocationAuthorizationDto,
        MobileRideMapLocationEnvironmentDto, MobileRideMapRestorationState,
        MobileStoredBmsVoltageSampleDto, Voltage, open_ride_database,
    };
    use std::{sync::mpsc, time::Duration};
    use uuid::Uuid;

    fn sample(index: u64) -> MobilePhoneLocationSampleDto {
        MobilePhoneLocationSampleDto {
            wall_clock_unix_ms: 1_700_000_000_000 + index * 1_000,
            source_timestamp_unix_seconds: None,
            latitude_degrees: 40.0 + f64::from(u32::try_from(index).unwrap()) * 0.00002,
            longitude_degrees: -105.0,
            altitude_meters: 1_600.0,
            horizontal_accuracy_meters: Some(3.0),
            vertical_accuracy_meters: None,
            speed_meters_per_second: Some(2.0),
            speed_accuracy_meters_per_second: None,
            course_degrees: None,
            course_accuracy_degrees: None,
        }
    }

    #[test]
    fn native_location_receipts_preserve_all_material_samples_and_order() {
        let core = MobileRideMapCore::new();
        core.start_gps_only(1_000).unwrap();
        let callbacks: Vec<_> = (0..u64::try_from(MAX_PENDING_LOCATION_WRITES).unwrap())
            .map(|index| {
                core.admit_location_callback(
                    1_000 + index * 1_000,
                    sample(index).wall_clock_unix_ms,
                    vec![sample(index)],
                )
                .unwrap()
            })
            .collect();
        let (entered, observing) = mpsc::sync_channel(1);
        let (finished, outcome) = mpsc::sync_channel(1);
        let later = Arc::clone(&callbacks[1]);
        let worker = std::thread::spawn(move || {
            entered.send(()).unwrap();
            finished.send(later.finish()).unwrap();
        });
        observing.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            outcome.recv_timeout(Duration::from_millis(50)).is_err(),
            "later callbacks must not overtake an unsettled predecessor"
        );
        let mut outcomes = callbacks[0].finish().unwrap();
        outcomes.extend(
            outcome
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap(),
        );
        worker.join().unwrap();
        for callback in &callbacks[2..] {
            outcomes.extend(callback.finish().unwrap());
        }
        assert_eq!(outcomes.len(), MAX_PENDING_LOCATION_WRITES);
        for (index, outcome) in outcomes.into_iter().enumerate() {
            let index = u64::try_from(index).unwrap();
            let MobileRideMapCoreDecisionDto::Accepted { point } = outcome.decision else {
                panic!("material sample was lost");
            };
            assert_eq!(point.monotonic_ms, 1_000 + index * 1_000);
            assert_eq!(point.wall_clock_unix_ms, sample(index).wall_clock_unix_ms);
            assert_eq!(
                point.latitude_degrees.to_bits(),
                sample(index).latitude_degrees.to_bits()
            );
            assert_eq!(outcome.snapshot.summary.point_count, index + 1);
        }
    }

    #[test]
    fn native_location_failure_releases_capacity_and_fences_replacement() {
        let core = MobileRideMapCore::new();
        core.start_gps_only(1_000).unwrap();
        let failed = core
            .admit_location_callback(1_000, sample(0).wall_clock_unix_ms, vec![sample(0)])
            .unwrap();
        {
            let mut state = core.inner.lock().unwrap();
            state.initialization_error = Some(MobileRideMapCoreErrorDto::Storage(
                "injected storage failure".into(),
            ));
            state.restoration_state = MobileRideMapRestorationState::Failed;
        }
        assert_eq!(
            failed.finish(),
            Err(MobileRideMapCoreErrorDto::Storage(
                "injected storage failure".into()
            ))
        );
        {
            let mut state = core.inner.lock().unwrap();
            state.initialization_error = None;
            state.restoration_state = MobileRideMapRestorationState::Ready;
        }
        let abandoned = core
            .admit_location_callback(2_000, sample(1).wall_clock_unix_ms, vec![sample(1)])
            .unwrap();
        drop(abandoned);
        let callback = core
            .admit_location_callback(3_000, sample(2).wall_clock_unix_ms, vec![sample(2)])
            .unwrap();
        let old_identity = callback.recording_token().unwrap();
        core.stop(3_000).unwrap();
        core.save().unwrap();
        let replacement = core.start_gps_only(3_000).unwrap();
        assert_ne!(replacement.recording_token.as_ref(), Some(&old_identity));
        assert_eq!(callback.finish().unwrap(), []);
        let next = core
            .admit_location_callback(4_000, sample(3).wall_clock_unix_ms, vec![sample(3)])
            .unwrap();
        assert_eq!(next.finish().unwrap().len(), 1);
    }

    #[test]
    fn connection_admission_wait_settles_once_without_polling() {
        let core = MobileRideMapCore::new();
        core.start_gps_only(1_000).unwrap();
        let pending = core
            .begin_verified_connection_admission(
                "wheel",
                1_000,
                1,
                CoreMusicHistoryPolicy::Disabled,
            )
            .unwrap();
        let admission = MobileRideMapConnectionAdmission::new(Arc::clone(&core), pending);
        let settled = admission.wait().unwrap();
        assert!(settled.is_some());
        assert_eq!(admission.wait().unwrap(), settled);
        assert_eq!(
            admission.poll().unwrap(),
            MobileRideMapAdmissionPollDto::Completed { snapshot: settled }
        );
    }

    #[test]
    fn oversized_native_callback_retains_all_samples_and_owns_the_full_budget() {
        let core = MobileRideMapCore::new();
        core.start_gps_only(1_000).unwrap();
        let sample_count = u64::try_from(MAX_PENDING_LOCATION_WRITES * 2).unwrap();
        let callback = core
            .admit_location_callback(
                sample_count * 1_000,
                sample(sample_count - 1).wall_clock_unix_ms,
                (0..sample_count).map(sample).collect(),
            )
            .unwrap();
        let (entered, observing) = mpsc::sync_channel(1);
        let (admitted, result) = mpsc::sync_channel(1);
        let worker_core = Arc::clone(&core);
        let worker = std::thread::spawn(move || {
            entered.send(()).unwrap();
            admitted
                .send(worker_core.admit_location_callback(
                    200_000,
                    1_700_000_200_000,
                    vec![sample(200)],
                ))
                .unwrap();
        });
        observing.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(result.recv_timeout(Duration::from_millis(50)).is_err());
        assert_eq!(
            callback.finish().unwrap().len(),
            usize::try_from(sample_count).unwrap()
        );
        let next = result
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
        assert_eq!(next.finish().unwrap().len(), 1);
    }

    #[test]
    fn restore_retains_outstanding_native_callback_capacity() {
        let core = MobileRideMapCore::new();
        core.start_gps_only(1_000).unwrap();
        let old = core
            .admit_location_callback(
                64_000,
                sample(63).wall_clock_unix_ms,
                (0..64).map(sample).collect(),
            )
            .unwrap();
        core.inner.lock().unwrap().pending_restore_command_id = Some(1);
        core.complete_restore(1, MobileRideMapCoreInner::new(None), Ok(None))
            .unwrap();
        let (entered, observing) = mpsc::sync_channel(1);
        let (admitted, result) = mpsc::sync_channel(1);
        let worker_core = Arc::clone(&core);
        let worker = std::thread::spawn(move || {
            entered.send(()).unwrap();
            admitted
                .send(worker_core.admit_location_callback(
                    65_000,
                    sample(64).wall_clock_unix_ms,
                    vec![],
                ))
                .unwrap();
        });
        observing.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(result.recv_timeout(Duration::from_millis(50)).is_err());
        drop(old);
        assert_eq!(
            result
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap()
                .finish()
                .unwrap(),
            []
        );
        worker.join().unwrap();
    }

    #[test]
    fn location_acquisition_notifications_ignore_clock_and_revision_only_changes() {
        let core = MobileRideMapCore::new();
        assert!(core.take_location_acquisition_change());
        assert!(!core.take_location_acquisition_change());
        core.observe_location_environment(MobileRideMapLocationEnvironmentDto {
            authorization: MobileRideMapLocationAuthorizationDto::Always,
            services_enabled: true,
            temporarily_unavailable: false,
        });
        assert!(core.take_location_acquisition_change());
        core.start_gps_only(1_000).unwrap();
        assert!(core.take_location_acquisition_change());
        let callback = core
            .admit_location_callback(1_000, sample(0).wall_clock_unix_ms, vec![sample(0)])
            .unwrap();
        assert_eq!(callback.finish().unwrap().len(), 1);
        assert!(!core.take_location_acquisition_change());
        let _ = core.current_snapshot(2_000);
        assert!(!core.take_location_acquisition_change());
        core.pause(2_000).unwrap();
        assert!(core.take_location_acquisition_change());
        assert!(!core.take_location_acquisition_change());
    }

    #[test]
    fn callbacks_without_a_token_survive_the_first_queued_verified_autostart() {
        let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path =
            std::env::temp_dir().join(format!("cutout-gps-autostart-{}.sqlite3", Uuid::new_v4()));
        let database = open_ride_database(path.to_string_lossy().into_owned()).unwrap();
        database
            .remember_selected_device("wheel".into(), None, 1_000)
            .unwrap();
        let core = MobileRideMapCore::with_database(Arc::clone(&database));
        core.restore(1_000).unwrap();
        let connection_work = core.reserve_recording_work(1).unwrap();
        let before = core
            .admit_location_callback(1_000, sample(0).wall_clock_unix_ms, vec![sample(0)])
            .unwrap();
        assert!(before.recording_token().is_none());
        let pending = core
            .begin_verified_connection_admission(
                "wheel",
                1_000,
                7,
                CoreMusicHistoryPolicy::Disabled,
            )
            .unwrap();
        let during = core
            .admit_location_callback(2_000, sample(1).wall_clock_unix_ms, vec![sample(1)])
            .unwrap();
        assert!(during.recording_token().is_none());
        let admission = MobileRideMapConnectionAdmission::new(Arc::clone(&core), pending);
        let started = admission.wait().unwrap().unwrap();
        connection_work.release();
        let first = before.finish().unwrap();
        let second = during.finish().unwrap();
        for outcomes in [&first, &second] {
            let points: Vec<_> = outcomes
                .iter()
                .filter_map(|outcome| match outcome.decision {
                    MobileRideMapCoreDecisionDto::Accepted { point } => Some(point),
                    MobileRideMapCoreDecisionDto::StorageError { .. } => {
                        panic!("queued autostart callback failed: {outcomes:?}")
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(points.len(), 1, "{outcomes:?}");
        }
        assert_eq!(before.recording_token(), started.recording_token);
        assert_eq!(during.recording_token(), started.recording_token);
        assert_eq!(core.current_snapshot(2_000).unwrap().summary.point_count, 2);
        assert_eq!(core.points_after(None, 256).unwrap().points.len(), 2);
        database.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn callback_without_a_token_cannot_adopt_an_unrelated_manual_start() {
        let core = MobileRideMapCore::new();
        let callback = core
            .admit_location_callback(1_000, sample(0).wall_clock_unix_ms, vec![sample(0)])
            .unwrap();
        core.start_gps_only(1_000).unwrap();
        assert_eq!(callback.finish().unwrap(), []);
        assert_eq!(core.current_snapshot(1_000).unwrap().summary.point_count, 0);
    }

    #[test]
    fn connection_and_bms_work_share_the_lossless_native_capacity_budget() {
        let core = MobileRideMapCore::new();
        let bms = core
            .reserve_recording_work(u64::try_from(MAX_PENDING_LOCATION_WRITES).unwrap())
            .unwrap();
        let (entered, observing) = mpsc::sync_channel(1);
        let (admitted, result) = mpsc::sync_channel(1);
        let worker_core = Arc::clone(&core);
        let worker = std::thread::spawn(move || {
            entered.send(()).unwrap();
            admitted
                .send(worker_core.reserve_recording_work(1))
                .unwrap();
        });
        observing.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(result.recv_timeout(Duration::from_millis(50)).is_err());
        bms.release();
        bms.release();
        let connection = result
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
        drop(connection);
        assert_eq!(
            core.admit_location_callback(1_000, sample(0).wall_clock_unix_ms, vec![sample(0)])
                .unwrap()
                .finish()
                .unwrap(),
            []
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one real SQLite stall must preserve capacity ownership and all ordered receipts"
    )]
    fn stalled_sqlite_preserves_bounded_native_ownership_and_every_material_point() {
        let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path =
            std::env::temp_dir().join(format!("cutout-gps-stalled-{}.sqlite3", Uuid::new_v4()));
        let database = open_ride_database(path.to_string_lossy().into_owned()).unwrap();
        let core = MobileRideMapCore::with_database(Arc::clone(&database));
        core.restore(1_000).unwrap();
        core.start_gps_only(1_000).unwrap();
        let mut callbacks: Vec<_> = (0..u64::try_from(MAX_PENDING_LOCATION_WRITES).unwrap())
            .map(|index| {
                core.admit_location_callback(
                    1_000 + index * 1_000,
                    sample(index).wall_clock_unix_ms,
                    vec![sample(index)],
                )
                .unwrap()
            })
            .collect();
        let blocker = rusqlite::Connection::open(&path).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
        // A write-only command takes SQLite's busy wait; a route transaction that
        // upgrades an earlier read snapshot instead terminates with SQLITE_BUSY.
        // Stall the real worker ahead of GPS, without injecting a test-only API.
        let (settings_started, settings_entered) = mpsc::sync_channel(1);
        let (settings_written, settings_result) = mpsc::sync_channel(1);
        let settings_database = Arc::clone(&database);
        let settings_writer = std::thread::spawn(move || {
            settings_started.send(()).unwrap();
            settings_written
                .send(settings_database.inner.save_ride_autostart_enabled(true))
                .unwrap();
        });
        settings_entered
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert!(
            settings_result
                .recv_timeout(Duration::from_millis(50))
                .is_err()
        );
        let (entered, observing) = mpsc::sync_channel(1);
        let (admitted, result) = mpsc::sync_channel(1);
        let worker_core = Arc::clone(&core);
        let next_index = u64::try_from(MAX_PENDING_LOCATION_WRITES).unwrap();
        let producer = std::thread::spawn(move || {
            entered.send(()).unwrap();
            admitted
                .send(worker_core.admit_location_callback(
                    1_000 + next_index * 1_000,
                    sample(next_index).wall_clock_unix_ms,
                    vec![sample(next_index)],
                ))
                .unwrap();
        });
        observing.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(result.recv_timeout(Duration::from_millis(50)).is_err());
        let first = callbacks.remove(0);
        let (persisted, receipt) = mpsc::sync_channel(1);
        let writer = std::thread::spawn(move || persisted.send(first.finish()).unwrap());
        let stalled_receipt = receipt.recv_timeout(Duration::from_millis(50));
        assert!(
            stalled_receipt.is_err(),
            "SQLite write lock holds the first durable receipt: {stalled_receipt:?}"
        );
        assert!(
            result.try_recv().is_err(),
            "capacity remains owned until durability settles"
        );
        blocker.execute_batch("COMMIT").unwrap();
        settings_result
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        settings_writer.join().unwrap();
        let mut outcomes = receipt
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        writer.join().unwrap();
        let next = result
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        producer.join().unwrap();
        for callback in callbacks {
            outcomes.extend(callback.finish().unwrap());
        }
        outcomes.extend(next.finish().unwrap());
        let points: Vec<_> = outcomes
            .iter()
            .filter_map(|outcome| match outcome.decision {
                MobileRideMapCoreDecisionDto::Accepted { point } => Some(point),
                MobileRideMapCoreDecisionDto::StorageError { .. } => {
                    panic!("material callback was not durable: {outcome:?}")
                }
                _ => None,
            })
            .collect();
        assert_eq!(points.len(), MAX_PENDING_LOCATION_WRITES + 1);
        for (index, point) in points.iter().enumerate() {
            let index = u64::try_from(index).unwrap();
            assert_eq!(point.sequence, index);
            assert_eq!(point.wall_clock_unix_ms, sample(index).wall_clock_unix_ms);
        }
        assert_eq!(
            core.points_after(None, 256).unwrap().points.len(),
            MAX_PENDING_LOCATION_WRITES + 1
        );
        database.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn bms_receipt_wait_returns_correlated_results_once_without_polling() {
        let _guard = crate::tests::RIDE_DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path =
            std::env::temp_dir().join(format!("cutout-bms-receipt-{}.sqlite3", Uuid::new_v4()));
        let database = open_ride_database(path.to_string_lossy().into_owned()).unwrap();
        let request_id = database
            .queue_bms_voltage_samples(
                "wheel".into(),
                vec![MobileStoredBmsVoltageSampleDto {
                    session_identifier: "session".into(),
                    event_sequence: 1,
                    monotonic_milliseconds: 1_000,
                    wall_clock_milliseconds: 1_700_000_000_000,
                    observation_index: 0,
                    pack_index: Some(0),
                    pack_observation_index: Some(0),
                    voltage: Voltage { value: 4_193 },
                }],
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            database.finish_bms_voltage_writes(),
            vec![MobileBmsVoltageWriteOutcomeDto {
                request_id,
                error: None
            }]
        );
        assert_eq!(database.finish_bms_voltage_writes(), []);
        assert_eq!(database.poll_bms_voltage_writes(), []);
        database.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn native_location_admission_is_bounded_lossless_and_releases_on_failure() {
        let core = MobileRideMapCore::new();
        core.start_gps_only(1_000).unwrap();
        let mut callbacks = Vec::new();
        for index in 0..MAX_PENDING_LOCATION_WRITES {
            let index = u64::try_from(index).unwrap();
            callbacks.push(
                core.admit_location_callback(1_000 + index, 1_700_000_000_000 + index, vec![])
                    .unwrap(),
            );
        }
        let (entered, observing) = mpsc::sync_channel(1);
        let (admitted, result) = mpsc::sync_channel(1);
        let worker_core = Arc::clone(&core);
        let worker = std::thread::spawn(move || {
            entered.send(()).unwrap();
            admitted
                .send(worker_core.admit_location_callback(2_000, 1_700_000_001_000, vec![]))
                .unwrap();
        });
        observing.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            result.recv_timeout(Duration::from_millis(50)).is_err(),
            "full native ownership must apply backpressure instead of dropping callbacks"
        );
        assert_eq!(callbacks.remove(0).finish().unwrap(), []);
        let callback = result
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
        // Abandoned receipts release capacity without waiting for a timer or a worker.
        drop(callbacks);
        assert_eq!(callback.finish().unwrap(), []);
        assert_eq!(
            callback.finish(),
            Err(MobileRideMapCoreErrorDto::StaleRideCommand)
        );
        let callback = core
            .admit_location_callback(3_000, 1_700_000_002_000, vec![])
            .unwrap();
        assert_eq!(callback.finish().unwrap(), []);
    }
}
