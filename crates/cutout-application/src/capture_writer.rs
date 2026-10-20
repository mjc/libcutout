//! Application ownership for capture metadata and durable writer lifecycle.

use std::{
    path::PathBuf,
    sync::{Mutex, PoisonError},
};

use cutout_core::{
    CaptureAnnotationCapacityReached, CaptureAnnotations, CaptureObservationSource, CaptureOrigin,
    CaptureResearchProfile, CaptureResearchProfileSelection, CaptureSessionLabel,
    CaptureSettingObservationValue, CaptureSettingsObservation, GattChannel, GattFingerprint,
    MonotonicTimestamp, PevcapLocationSample, PevcapRecord, PevcapResolvedIdentity,
    TransportWriteLimit, WallClockUnixTimestamp,
};
use libcutout_persistence::{
    CaptureFlushOutcome, CaptureJsonlExport, CaptureMetadata, CaptureRecordAdmission,
    CaptureRecordingPolicy, CaptureSettingsObservationAdmission, CaptureWriteOutcome,
    CaptureWriter, CaptureWriterFinish, CaptureWriterMonitor, CaptureWriterStatus, RideDatabase,
    SavedCaptureArtifact,
};

#[derive(Debug, Default)]
enum WriterSlot {
    #[default]
    Ready,
    Recording(CaptureWriter),
    Finalizing,
    Complete(Result<CaptureWriterFinish, String>),
}

impl WriterSlot {
    fn as_ref(&self) -> Option<&CaptureWriter> {
        match self {
            Self::Recording(writer) => Some(writer),
            Self::Ready | Self::Finalizing | Self::Complete(_) => None,
        }
    }

    fn is_ready(&self) -> bool {
        match self {
            Self::Ready => true,
            Self::Recording(_) | Self::Finalizing | Self::Complete(_) => false,
        }
    }

    fn accepts_metadata(&self) -> bool {
        match self {
            Self::Ready | Self::Recording(_) => true,
            Self::Finalizing | Self::Complete(_) => false,
        }
    }

    fn is_recording(&self) -> bool {
        match self {
            Self::Recording(_) => true,
            Self::Ready | Self::Finalizing | Self::Complete(_) => false,
        }
    }
}

/// Application-owned capture writer state. Native callers provide typed observations and
/// execute returned DTO conversion while metadata and writer lifecycle stay in Rust.
#[derive(Debug)]
pub struct CaptureWriterService {
    wall_clock_start_unix_ms: WallClockUnixTimestamp,
    platform_id: String,
    write_limit: Option<TransportWriteLimit>,
    advertised_services: Mutex<Vec<GattChannel>>,
    gatt_fingerprints: Mutex<Vec<GattFingerprint>>,
    resolved_identity: Mutex<Option<PevcapResolvedIdentity>>,
    annotations: Mutex<CaptureAnnotations>,
    writer: Mutex<WriterSlot>,
    finish_lock: Mutex<()>,
    writer_monitor: Mutex<Option<CaptureWriterMonitor>>,
    database: Mutex<Option<RideDatabase>>,
    capture_start_monotonic_ms: Mutex<Option<u64>>,
    recording_policy: Mutex<CaptureRecordingPolicy>,
    settings_observations: Mutex<Vec<CaptureSettingsObservation>>,
    settings_save_lock: Mutex<()>,
}

/// Result state of a terminal writer operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureFinishState {
    /// No writer has started.
    NotStarted,
    /// Another caller owns finalization.
    Finalizing,
    /// The durable writer result, including failure.
    Complete(
        /// Durable result or its terminal error.
        Result<CaptureWriterFinish, String>,
    ),
}

/// Failure to convert a host timestamp to capture-relative time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureTimestampError {
    /// There is no active writer or immutable capture origin.
    NotRecording,
    /// The observation predates the capture origin.
    BeforeStart,
}

/// Rejection when a bounded annotation transition cannot be retained.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CaptureAnnotationError {
    /// No active recording accepts this change.
    #[error("capture writer is not recording")]
    NotRecording,
    /// Header capacity cannot retain the metadata transition.
    #[error("capture annotation capacity reached")]
    CapacityReached,
    /// Metadata admission lost evidence.
    #[error("capture writer admission was lost")]
    AdmissionLost { dropped_messages: u64 },
    /// The writer is closed or failed.
    #[error("capture writer failed")]
    WriterFailed,
}

/// Rejection while preparing or durably admitting a settings observation.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CaptureSettingsError {
    /// The bounded observation list is full.
    #[error("settings observation limit reached")]
    CapacityReached,
    /// No recording writer is active.
    #[error("capture writer is not recording")]
    NotRecording,
    /// The observation failed validation or writer admission.
    #[error("settings observation is invalid")]
    Invalid {
        /// The persistence validator's diagnostic text.
        reason: String,
    },
    /// The bounded queue rejected the observation.
    #[error("capture writer admission was lost")]
    AdmissionLost,
    /// The writer or its durable receipt failed.
    #[error("capture writer failed")]
    WriterFailed,
}

/// Native-independent input for one manually reported settings snapshot.
///
/// Capture identity, selection authority, and timeline timestamps are assigned by the
/// application service so callers cannot forge another capture's association.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureSettingsDraft {
    /// Reported profile context; its selection is always downgraded to Manual.
    pub profile: CaptureResearchProfile,
    /// Optional capture-relative time when the values were observed.
    pub observed_at_ms: Option<u64>,
    /// Where the values were observed.
    pub source: CaptureObservationSource,
    /// Independently reported setting values.
    pub values: Vec<CaptureSettingObservationValue>,
    /// Optional context for the snapshot.
    pub note: Option<String>,
}

/// Provenance supplied by a host when publishing a completed capture to history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureHistoryPublication {
    /// User or system action that produced the capture.
    pub origin: CaptureOrigin,
    /// Advertised device name captured when recording started.
    pub advertised_name: Option<String>,
    /// Publication time; absent or zero time is a failed publication.
    pub published_at_unix_ms: Option<WallClockUnixTimestamp>,
}

/// Durable finish result correlated with its optional history publication result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureCompletion {
    /// Terminal writer result.
    pub finish: CaptureFinishState,
    /// Whether provenance was retained; `None` means publication was not applicable.
    pub database_publication_succeeded: Option<bool>,
}

impl CaptureWriterService {
    /// Creates an idle owner for one capture header.
    #[must_use]
    pub fn new(
        wall_clock_start_unix_ms: WallClockUnixTimestamp,
        platform_id: String,
        write_limit: Option<TransportWriteLimit>,
    ) -> Self {
        Self {
            wall_clock_start_unix_ms,
            platform_id,
            write_limit,
            advertised_services: Mutex::new(Vec::new()),
            gatt_fingerprints: Mutex::new(Vec::new()),
            resolved_identity: Mutex::new(None),
            annotations: Mutex::new(CaptureAnnotations::default()),
            writer: Mutex::new(WriterSlot::Ready),
            finish_lock: Mutex::new(()),
            writer_monitor: Mutex::new(None),
            database: Mutex::new(None),
            capture_start_monotonic_ms: Mutex::new(None),
            recording_policy: Mutex::new(CaptureRecordingPolicy::EveryObservation),
            settings_observations: Mutex::new(Vec::new()),
            settings_save_lock: Mutex::new(()),
        }
    }

    /// Pins the absolute monotonic origin once, before writer startup.
    pub fn set_start_monotonic_ms(&self, milliseconds: u64) -> bool {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        if !writer.is_ready() {
            return false;
        }
        let mut started = self
            .capture_start_monotonic_ms
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if started.is_some() {
            return false;
        }
        *started = Some(milliseconds);
        true
    }

    /// Returns the immutable absolute origin selected for this capture, if present.
    #[must_use]
    pub fn start_monotonic_ms(&self) -> Option<u64> {
        *self
            .capture_start_monotonic_ms
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Attaches the process database while the writer is idle.
    pub fn set_database(&self, database: RideDatabase) -> bool {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        if !writer.is_ready() {
            return false;
        }
        *self.database.lock().unwrap_or_else(PoisonError::into_inner) = Some(database);
        true
    }

    /// Selects retention semantics and updates the matching startup annotation.
    pub fn set_origin(&self, origin: CaptureOrigin) -> bool {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        if !writer.is_ready() {
            return false;
        }
        let policy = if origin == CaptureOrigin::Automatic {
            CaptureRecordingPolicy::MaterialChanges
        } else {
            CaptureRecordingPolicy::EveryObservation
        };
        let mut annotations = self
            .annotations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut next = CaptureAnnotations::default();
        let entries = annotations
            .entries()
            .iter()
            .filter(|entry| !entry.starts_with("capture_recording_policy="))
            .cloned();
        if next.try_append(entries).is_err() {
            return false;
        }
        if policy == CaptureRecordingPolicy::MaterialChanges
            && next
                .try_append(["capture_recording_policy=material_changes".to_owned()])
                .is_err()
        {
            return false;
        }
        *annotations = next;
        *self
            .recording_policy
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = policy;
        true
    }

    /// Adds a service identifier unless the core metadata bound is already reached.
    pub fn add_advertised_service(&self, service: GattChannel) -> CaptureWriteOutcome {
        let mut services = self
            .advertised_services
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if services.len() < cutout_core::PEVCAP_MAX_ADVERTISED_SERVICES {
            services.push(service);
        }
        drop(services);
        self.update_metadata()
    }

    /// Adds a GATT fingerprint unless the core metadata bound is already reached.
    pub fn add_gatt_fingerprint(&self, fingerprint: GattFingerprint) -> CaptureWriteOutcome {
        let mut fingerprints = self
            .gatt_fingerprints
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if fingerprints.len() < cutout_core::PEVCAP_MAX_GATT_FINGERPRINTS {
            fingerprints.push(fingerprint);
        }
        drop(fingerprints);
        self.update_metadata()
    }

    /// Updates the verified protocol identity and publishes it to an active writer.
    pub fn set_resolved_identity(
        &self,
        identity: Option<PevcapResolvedIdentity>,
    ) -> CaptureWriteOutcome {
        *self
            .resolved_identity
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = identity;
        self.update_metadata()
    }

    /// Changes identity and its evidence annotations as one admitted metadata update.
    ///
    /// # Errors
    /// Returns an error if the writer is terminal or the annotation update cannot be admitted.
    pub fn update_resolved_identity(
        &self,
        identity: PevcapResolvedIdentity,
        evidence: Option<String>,
        detail: Option<String>,
    ) -> Result<(), CaptureAnnotationError> {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        if !writer.accepts_metadata() {
            return Err(CaptureAnnotationError::WriterFailed);
        }
        let mut metadata = self.metadata();
        let mut annotations = self
            .annotations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut candidate = annotations.clone();
        let additions = [
            evidence.map(|value| crate::capture::format_annotation("resolved_evidence", &value)),
            detail.map(|value| crate::capture::format_annotation("resolved_detail", &value)),
        ]
        .into_iter()
        .flatten();
        candidate
            .try_append(additions)
            .map_err(|_| CaptureAnnotationError::CapacityReached)?;
        let identity = Some(identity);
        if metadata.resolved_identity == identity && candidate.entries() == annotations.entries() {
            return Ok(());
        }
        metadata.resolved_identity.clone_from(&identity);
        metadata.annotations = candidate.entries().to_vec();
        if let Some(writer) = writer.as_ref() {
            match writer.update_metadata(metadata) {
                CaptureWriteOutcome::Accepted => {}
                CaptureWriteOutcome::AdmissionLost { dropped_messages } => {
                    return Err(CaptureAnnotationError::AdmissionLost { dropped_messages });
                }
                CaptureWriteOutcome::Failed => return Err(CaptureAnnotationError::WriterFailed),
            }
        }
        *self
            .resolved_identity
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = identity;
        *annotations = candidate;
        Ok(())
    }

    /// Admits a bounded annotation or label transition atomically.
    ///
    /// # Errors
    /// Returns an error if recording is required but inactive, the candidate exceeds its bound,
    /// or the writer cannot admit the metadata update.
    pub fn update_annotations(
        &self,
        recording_required: bool,
        update: impl FnOnce(&mut CaptureAnnotations) -> Result<(), CaptureAnnotationCapacityReached>,
    ) -> Result<Vec<CaptureSessionLabel>, CaptureAnnotationError> {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        match &*writer {
            WriterSlot::Recording(_) => {}
            WriterSlot::Ready if !recording_required => {}
            _ => return Err(CaptureAnnotationError::NotRecording),
        }
        let mut metadata = self.metadata();
        let mut annotations = self
            .annotations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut candidate = annotations.clone();
        update(&mut candidate).map_err(|_| CaptureAnnotationError::CapacityReached)?;
        let active = candidate.active_labels().to_vec();
        if candidate.entries() == annotations.entries() {
            return Ok(active);
        }
        metadata.annotations = candidate.entries().to_vec();
        if let Some(writer) = writer.as_ref() {
            match writer.update_metadata(metadata) {
                CaptureWriteOutcome::Accepted => {}
                CaptureWriteOutcome::AdmissionLost { dropped_messages } => {
                    return Err(CaptureAnnotationError::AdmissionLost { dropped_messages });
                }
                CaptureWriteOutcome::Failed => return Err(CaptureAnnotationError::WriterFailed),
            }
        }
        *annotations = candidate;
        Ok(active)
    }

    /// Appends one typed capture record through bounded admission.
    pub fn record(
        &self,
        record: PevcapRecord,
        admission: CaptureRecordAdmission,
    ) -> CaptureWriteOutcome {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        writer
            .as_ref()
            .map_or(CaptureWriteOutcome::Failed, |writer| {
                writer
                    .ingress()
                    .try_send_record_with_admission(record, admission)
            })
    }

    /// Appends one independent location observation.
    pub fn record_location(&self, location: PevcapLocationSample) -> CaptureWriteOutcome {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        writer
            .as_ref()
            .map_or(CaptureWriteOutcome::Failed, |writer| {
                writer.record_location(location)
            })
    }

    /// Appends an ordered location batch through cloneable bounded ingress.
    pub fn record_location_batch(&self, locations: &[PevcapLocationSample]) -> CaptureWriteOutcome {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        writer
            .as_ref()
            .map_or(CaptureWriteOutcome::Failed, |writer| {
                writer.ingress().record_location_batch(locations)
            })
    }

    /// Accounts for a location batch rejected before allocation because it exceeded the bound.
    pub fn reject_location_batch(&self, requested: usize) -> CaptureWriteOutcome {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        writer
            .as_ref()
            .map_or(CaptureWriteOutcome::Failed, |writer| {
                writer.ingress().reject_location_batch(requested)
            })
    }

    /// Begins a read barrier without retaining the writer lock while records are visited.
    ///
    /// # Errors
    /// Returns an error when recording is inactive or the writer cannot establish a read barrier.
    pub fn visit_transport_records(&self, visit: impl FnMut(PevcapRecord)) -> Result<(), String> {
        let slot = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let writer = slot
            .as_ref()
            .ok_or_else(|| "capture writer is not recording".to_owned())?;
        let receipt = writer.begin_transport_read()?;
        drop(slot);
        receipt.visit_transport_records(visit)
    }

    /// Builds, admits, durably acknowledges, and retains one settings snapshot.
    ///
    /// # Errors
    /// Returns a validation, capacity, admission, or durability error without caching the snapshot.
    pub fn record_settings_observation(
        &self,
        draft: CaptureSettingsDraft,
        monotonic_ms: u64,
    ) -> Result<CaptureSettingsObservation, CaptureSettingsError> {
        let started = (*self
            .capture_start_monotonic_ms
            .lock()
            .unwrap_or_else(PoisonError::into_inner))
        .ok_or(CaptureSettingsError::NotRecording)?;
        let recorded_at_ms =
            monotonic_ms
                .checked_sub(started)
                .ok_or_else(|| CaptureSettingsError::Invalid {
                    reason: "Capture time is before recording started".to_owned(),
                })?;
        let _save = self
            .settings_save_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self
            .settings_observations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
            >= cutout_core::MAX_CAPTURE_SETTINGS_OBSERVATIONS
        {
            return Err(CaptureSettingsError::CapacityReached);
        }
        let writer_slot = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let writer = writer_slot
            .as_ref()
            .ok_or(CaptureSettingsError::NotRecording)?;
        let mut profile = draft.profile;
        profile.selection = CaptureResearchProfileSelection::Manual;
        let observation = CaptureSettingsObservation {
            capture_id: writer.capture_identity(),
            profile,
            observed_at_ms: draft.observed_at_ms.unwrap_or(recorded_at_ms),
            recorded_at_ms,
            source: draft.source,
            values: draft.values,
            note: draft.note,
        };
        observation
            .validate()
            .map_err(|error| CaptureSettingsError::Invalid {
                reason: error.to_string(),
            })?;
        cutout_protocols::validate_capture_research_observation(&observation).map_err(|error| {
            CaptureSettingsError::Invalid {
                reason: error.to_string(),
            }
        })?;
        let receipt = match writer
            .begin_settings_observation(observation.clone())
            .map_err(|error| CaptureSettingsError::Invalid {
                reason: error.to_string(),
            })? {
            CaptureSettingsObservationAdmission::Accepted(receipt) => receipt,
            CaptureSettingsObservationAdmission::AdmissionLost { .. } => {
                return Err(CaptureSettingsError::AdmissionLost);
            }
            CaptureSettingsObservationAdmission::Failed => {
                return Err(CaptureSettingsError::WriterFailed);
            }
        };
        drop(writer_slot);
        receipt
            .wait()
            .map_err(|_| CaptureSettingsError::WriterFailed)?;
        self.settings_observations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(observation.clone());
        Ok(observation)
    }

    /// Internal admission test seam. The hook runs after queue admission and outside the
    /// writer lock, before the durable acknowledgement is awaited.
    #[cfg(test)]
    fn record_settings_observation_with_admission_hook(
        &self,
        observation: CaptureSettingsObservation,
        after_admission: impl FnOnce() -> Result<(), CaptureSettingsError>,
    ) -> Result<(), CaptureSettingsError> {
        let _save = self
            .settings_save_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self
            .settings_observations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
            >= cutout_core::MAX_CAPTURE_SETTINGS_OBSERVATIONS
        {
            return Err(CaptureSettingsError::CapacityReached);
        }
        let writer_slot = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let writer = writer_slot
            .as_ref()
            .ok_or(CaptureSettingsError::NotRecording)?;
        let receipt = match writer
            .begin_settings_observation(observation.clone())
            .map_err(|error| CaptureSettingsError::Invalid {
                reason: error.to_string(),
            })? {
            CaptureSettingsObservationAdmission::Accepted(receipt) => receipt,
            CaptureSettingsObservationAdmission::AdmissionLost { .. } => {
                return Err(CaptureSettingsError::AdmissionLost);
            }
            CaptureSettingsObservationAdmission::Failed => {
                return Err(CaptureSettingsError::WriterFailed);
            }
        };
        drop(writer_slot);
        after_admission()?;
        receipt
            .wait()
            .map_err(|_| CaptureSettingsError::WriterFailed)?;
        self.settings_observations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(observation);
        Ok(())
    }

    /// Returns the active writer's Rust-issued identity for one associated observation.
    #[must_use]
    pub fn active_capture_identity(&self) -> Option<String> {
        self.writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(CaptureWriter::capture_identity)
    }

    /// Returns only acknowledged settings observations.
    #[must_use]
    pub fn settings_observations(&self) -> Vec<CaptureSettingsObservation> {
        self.settings_observations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Returns a metadata snapshot for adapter verification and presentation.
    #[must_use]
    pub fn metadata_snapshot(&self) -> CaptureMetadata {
        self.metadata()
    }

    /// Installs the file or database writer once from the retained metadata.
    pub fn start(&self, path: PathBuf) -> bool {
        let _finish = self
            .finish_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut slot = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        if !slot.is_ready()
            || self
                .capture_start_monotonic_ms
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_none()
        {
            return false;
        }
        let metadata = self.metadata();
        let database = self
            .database
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let start = if let Some(database) = database {
            CaptureWriter::start_with_database_and_policy(
                path,
                self.wall_clock_start_unix_ms,
                &self.platform_id,
                self.write_limit,
                &metadata,
                database,
                *self
                    .recording_policy
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            )
        } else {
            CaptureWriter::start(
                path,
                self.wall_clock_start_unix_ms,
                &self.platform_id,
                self.write_limit,
                &metadata,
            )
        };
        match start {
            Ok(writer) => {
                *self
                    .writer_monitor
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(writer.monitor());
                *slot = WriterSlot::Recording(writer);
                true
            }
            Err(error) => {
                *self
                    .writer_monitor
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) =
                    Some(CaptureWriterMonitor::failed(error.clone()));
                *slot = WriterSlot::Complete(Err(error));
                false
            }
        }
    }

    /// Converts absolute monotonic uptime to capture-relative time for an active writer.
    ///
    /// # Errors
    /// Returns `NotRecording` without an active writer/origin and `BeforeStart` for earlier uptime.
    pub fn relative_timestamp(
        &self,
        monotonic_ms: u64,
    ) -> Result<MonotonicTimestamp, CaptureTimestampError> {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        if !writer.is_recording() {
            return Err(CaptureTimestampError::NotRecording);
        }
        drop(writer);
        let started = (*self
            .capture_start_monotonic_ms
            .lock()
            .unwrap_or_else(PoisonError::into_inner))
        .ok_or(CaptureTimestampError::NotRecording)?;
        monotonic_ms
            .checked_sub(started)
            .map(MonotonicTimestamp::new)
            .ok_or(CaptureTimestampError::BeforeStart)
    }

    /// Requests a writer flush and reports its durable barrier outcome.
    pub fn flush(&self) -> CaptureFlushOutcome {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        writer
            .as_ref()
            .map_or(CaptureFlushOutcome::Rejected, CaptureWriter::flush_outcome)
    }

    /// Finishes the writer and retains its result for later reads.
    pub fn finish(&self, export_jsonl: bool) -> CaptureFinishState {
        let _finish = self
            .finish_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.finish_locked(export_jsonl)
    }

    /// Finishes without mandatory JSONL export and publishes retained history provenance under
    /// the same terminal lock, so competing finish callers cannot split the operation.
    pub fn finish_and_publish(
        &self,
        publication: Option<CaptureHistoryPublication>,
    ) -> CaptureCompletion {
        let _finish = self
            .finish_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let finish = self.finish_locked(false);
        let artifact = self.completed_artifact();
        let database_receipt = {
            let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
            match &*writer {
                WriterSlot::Complete(Ok(CaptureWriterFinish::DatabaseFinished {
                    receipt, ..
                })) => Some((**receipt).clone()),
                _ => None,
            }
        };
        let database = self
            .database
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let database_publication_succeeded = match (publication, database) {
            (None, _) => None,
            (Some(_), None) => Some(false),
            (Some(publication), Some(database)) => {
                if database_receipt.is_none() && artifact.is_none() {
                    None
                } else {
                    Some(publication.published_at_unix_ms.is_some_and(|time| {
                        time.as_milliseconds() > 0
                            && if let Some(receipt) = database_receipt.as_ref() {
                                database
                                    .retain_finished_database_capture(
                                        receipt,
                                        publication.origin,
                                        publication.advertised_name.as_deref(),
                                        time,
                                    )
                                    .is_ok()
                            } else if let Some(artifact) = artifact.as_ref() {
                                database
                                    .retain_finished_capture(
                                        artifact,
                                        publication.origin,
                                        publication.advertised_name.as_deref(),
                                        time,
                                    )
                                    .is_ok()
                            } else {
                                false
                            }
                    }))
                }
            }
        };
        CaptureCompletion {
            finish,
            database_publication_succeeded,
        }
    }

    fn finish_locked(&self, export_jsonl: bool) -> CaptureFinishState {
        let writer = {
            let mut slot = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
            match &*slot {
                WriterSlot::Complete(result) => {
                    return CaptureFinishState::Complete(result.clone());
                }
                WriterSlot::Ready => return CaptureFinishState::NotStarted,
                WriterSlot::Finalizing => return CaptureFinishState::Finalizing,
                WriterSlot::Recording(_) => {}
            }
            match std::mem::replace(&mut *slot, WriterSlot::Finalizing) {
                WriterSlot::Recording(writer) => writer,
                _ => unreachable!("recording state was checked while holding the lock"),
            }
        };
        let result = if export_jsonl {
            writer.finish()
        } else {
            writer.finish_without_export()
        };
        *self.writer.lock().unwrap_or_else(PoisonError::into_inner) =
            WriterSlot::Complete(result.clone());
        CaptureFinishState::Complete(result)
    }

    /// Returns a file artifact only after successful durable finalization.
    #[must_use]
    pub fn completed_artifact(&self) -> Option<SavedCaptureArtifact> {
        let slot = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let WriterSlot::Complete(Ok(finish)) = &*slot else {
            return None;
        };
        match finish {
            CaptureWriterFinish::FileSaved(artifact)
            | CaptureWriterFinish::DatabaseFinished {
                jsonl_export: CaptureJsonlExport::Available(artifact),
                ..
            } => Some((**artifact).clone()),
            CaptureWriterFinish::DatabaseFinished { .. } => None,
        }
    }

    /// Returns active or retained terminal writer instrumentation.
    #[must_use]
    pub fn status(&self) -> CaptureWriterStatus {
        if let Some(writer) = self
            .writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            return writer.monitor().status();
        }
        self.writer_monitor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map_or_else(CaptureWriterStatus::default, CaptureWriterMonitor::status)
    }

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
                .entries()
                .to_vec(),
        }
    }

    fn update_metadata(&self) -> CaptureWriteOutcome {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let metadata = self.metadata();
        match &*writer {
            WriterSlot::Ready => CaptureWriteOutcome::Accepted,
            WriterSlot::Recording(writer) => writer.update_metadata(metadata),
            WriterSlot::Finalizing | WriterSlot::Complete(_) => CaptureWriteOutcome::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc::sync_channel,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn active_writer() -> (CaptureWriterService, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "libcutout-capture-writer-{}-{}.pevcap",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let service =
            CaptureWriterService::new(WallClockUnixTimestamp::new(1), "test".into(), None);
        assert!(service.set_start_monotonic_ms(0));
        assert!(service.start(path.clone()));
        (service, path)
    }

    fn settings_draft(profile_id: &str, setting_id: &str, value: &str) -> CaptureSettingsDraft {
        CaptureSettingsDraft {
            profile: CaptureResearchProfile {
                profile_id: profile_id.into(),
                pev_category: None,
                model: None,
                firmware: None,
                selection: CaptureResearchProfileSelection::Resolved,
            },
            observed_at_ms: None,
            source: CaptureObservationSource::WheelDisplay,
            values: vec![CaptureSettingObservationValue {
                setting_id: setting_id.into(),
                value: cutout_core::CaptureObservedValue::Entered(value.into()),
                unit: Some("%".into()),
                note: None,
            }],
            note: None,
        }
    }

    #[test]
    fn public_settings_operation_validates_catalog_and_assigns_manual_capture_context() {
        let (service, path) = active_writer();
        assert!(matches!(
            service.record_settings_observation(
                settings_draft("unknown", "display_brightness", "50"),
                1
            ),
            Err(CaptureSettingsError::Invalid { .. })
        ));
        assert!(matches!(
            service.record_settings_observation(
                settings_draft("nosfet_aero", "display_brightness", "NaN"),
                1
            ),
            Err(CaptureSettingsError::Invalid { .. })
        ));

        let saved = service
            .record_settings_observation(
                settings_draft("nosfet_aero", "display_brightness", "50"),
                1,
            )
            .expect("valid reported value is acknowledged");
        assert_eq!(
            saved.profile.selection,
            CaptureResearchProfileSelection::Manual
        );
        assert_eq!(saved.capture_id, service.active_capture_identity().unwrap());
        assert_eq!(saved.recorded_at_ms, 1);
        assert_eq!(service.settings_observations(), vec![saved]);
        let _ = service.finish(false);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pending_settings_receipt_does_not_block_cache_reads_or_record_admission() {
        let (service, path) = active_writer();
        let (entered_tx, entered_rx) = sync_channel(1);
        let (release_tx, release_rx) = sync_channel(1);
        let capture_id = service.active_capture_identity().unwrap();
        let saving = std::sync::Arc::new(service);
        let worker = std::sync::Arc::clone(&saving);
        let save = std::thread::spawn(move || {
            worker.record_settings_observation_with_admission_hook(
                CaptureSettingsObservation {
                    capture_id,
                    profile: CaptureResearchProfile {
                        profile_id: "test".into(),
                        pev_category: None,
                        model: None,
                        firmware: None,
                        selection: CaptureResearchProfileSelection::Manual,
                    },
                    observed_at_ms: 1,
                    recorded_at_ms: 1,
                    source: CaptureObservationSource::WheelDisplay,
                    values: vec![CaptureSettingObservationValue {
                        setting_id: "test_setting".into(),
                        value: cutout_core::CaptureObservedValue::Off,
                        unit: None,
                        note: None,
                    }],
                    note: None,
                },
                || {
                    entered_tx
                        .send(())
                        .map_err(|_| CaptureSettingsError::WriterFailed)?;
                    release_rx
                        .recv()
                        .map_err(|_| CaptureSettingsError::WriterFailed)?;
                    Ok(())
                },
            )
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let (snapshot_tx, snapshot_rx) = sync_channel(1);
        let snapshot_service = std::sync::Arc::clone(&saving);
        let snapshot = std::thread::spawn(move || {
            snapshot_tx
                .send(snapshot_service.settings_observations())
                .unwrap();
        });
        let (record_tx, record_rx) = sync_channel(1);
        let record_service = std::sync::Arc::clone(&saving);
        let record = std::thread::spawn(move || {
            record_tx
                .send(record_service.record(
                    PevcapRecord::link_down(MonotonicTimestamp::new(1)),
                    CaptureRecordAdmission::EveryObservation,
                ))
                .unwrap();
        });
        let snapshot_result = snapshot_rx.recv_timeout(Duration::from_secs(1));
        let record_result = record_rx.recv_timeout(Duration::from_secs(1));
        release_tx.send(()).unwrap();
        save.join().unwrap().unwrap();
        snapshot.join().unwrap();
        record.join().unwrap();
        assert_eq!(snapshot_result.unwrap(), Vec::new());
        assert_eq!(record_result.unwrap(), CaptureWriteOutcome::Accepted);
        assert_eq!(saving.settings_observations().len(), 1);
        let _ = saving.finish(false);
        let _ = std::fs::remove_file(path);
    }
}
