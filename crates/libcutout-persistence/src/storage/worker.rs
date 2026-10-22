#[cfg(test)]
use super::append_location_with_result;
use super::{
    Command, SegmentStartReasonInput, SpatialSchemaState, abort_pevcap_import, append_location,
    append_location_with_moving_time, append_pevcap_location_batch, append_trail_segment,
    apply_verified_connection_admission, backup, begin_pevcap_import,
    checkpoint_location_observation, checkpoint_stationary_location_observation,
    clear_last_connected_device, clear_ride_session_marker, clear_selected_device,
    create_map_point, create_ride, create_started_live_ride, create_started_ride, create_trail,
    delete_music_history, device_name, export_ride_json, find_ride, find_ride_filtered,
    finish_pevcap_import, integrity_check, last_connected_device,
    list_ride_history_vehicle_options, list_rides, load_ride_restore_snapshot, load_summary,
    load_summary_with_duration, map_points_in_bounds, migrate_device_name, music_events,
    music_history, music_history_policy, music_history_state, newest_recoverable_ride,
    pevcap_import_receipt, phone_alarm_preferences, project_history_context, project_route_points,
    rebuild_spatial_indexes, record_bms_voltage_samples, record_music_event,
    remember_last_connected_device, remember_selected_device, remove_voltage_sag_model,
    ride_session_marker, route_points, save_device_name, save_music_event,
    save_music_history_policy, save_phone_alarm_preferences, save_ride_session_marker,
    save_selected_device, save_voltage_sag_model, selected_device, settle_recovered_ride,
    sqlite_capabilities, trail_segments_in_bounds, transition_ride_with_telemetry,
    update_ride_map_metadata_with_moving_time, voltage_sag_model,
};
use rusqlite::Connection;
use std::ops::ControlFlow;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::mpsc::SyncSender;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::Receiver,
};
use std::time::{Duration, Instant};

#[cfg(test)]
static DROP_NEXT_PEVCAP_FINISH_RESPONSE: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
pub(super) fn drop_next_pevcap_finish_response_for_test() {
    DROP_NEXT_PEVCAP_FINISH_RESPONSE.store(true, Ordering::Release);
}

pub(super) fn run(
    connection: Connection,
    database_path: PathBuf,
    receiver: &Receiver<Command>,
    worker_alive: &AtomicBool,
    maintenance_demand: &Arc<AtomicU64>,
    diagnostic_recording_enabled: &AtomicBool,
) {
    let mut worker = DatabaseWorker {
        connection,
        database_path,
        legacy_artifact_scan: None,
        spatial_schema: SpatialSchemaState::Uninitialized,
        worker_alive,
        maintenance_demand,
        diagnostic_recording_enabled,
    };
    if super::live_capture::interrupt_active(&worker.connection).is_err() {
        worker.worker_alive.store(false, Ordering::Release);
        return;
    }
    let mut normalization_state = worker.normalize_maintenance_batch();
    let mut last_command_at = Instant::now();
    let mut next_compaction_attempt = last_command_at + Duration::from_secs(5);
    loop {
        let idle_delay = match normalization_state {
            super::normalization::BatchState::Pending => Duration::from_millis(10),
            super::normalization::BatchState::Deferred
            | super::normalization::BatchState::Finished => Duration::from_secs(1),
        };
        match receiver.recv_timeout(idle_delay) {
            Ok(command) => {
                last_command_at = Instant::now();
                if worker.dispatch(command).is_break() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let observed_demand = worker.maintenance_demand.load(Ordering::Acquire);
                match receiver.try_recv() {
                    Ok(command) => {
                        last_command_at = Instant::now();
                        if worker.dispatch(command).is_break() {
                            break;
                        }
                        continue;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                if normalization_state != super::normalization::BatchState::Finished {
                    normalization_state = worker.normalize_maintenance_batch();
                } else if last_command_at.elapsed() >= Duration::from_secs(5)
                    && Instant::now() >= next_compaction_attempt
                {
                    let compacted = super::normalization::compact_if_safe(
                        &worker.connection,
                        &worker.database_path,
                        worker.maintenance_demand,
                        observed_demand,
                    );
                    next_compaction_attempt = Instant::now()
                        + match compacted {
                            Ok(super::normalization::CompactionState::Complete) => {
                                Duration::from_secs(86_400)
                            }
                            Ok(super::normalization::CompactionState::Deferred) | Err(_) => {
                                Duration::from_secs(60)
                            }
                        };
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    worker.stop();
}

struct DatabaseWorker<'a> {
    connection: Connection,
    database_path: PathBuf,
    legacy_artifact_scan: Option<super::normalization::LegacyArtifactScan>,
    spatial_schema: SpatialSchemaState,
    worker_alive: &'a AtomicBool,
    maintenance_demand: &'a Arc<AtomicU64>,
    diagnostic_recording_enabled: &'a AtomicBool,
}

impl DatabaseWorker<'_> {
    fn normalize_maintenance_batch(&mut self) -> super::normalization::BatchState {
        match super::normalization::remove_redundant_semantic_batch(
            &mut self.connection,
            self.legacy_artifact_scan
                .as_ref()
                .and_then(super::normalization::LegacyArtifactScan::backfill_digest),
        ) {
            Ok(super::normalization::BatchState::Pending) => {
                super::normalization::BatchState::Pending
            }
            Ok(super::normalization::BatchState::Deferred) | Err(_) => {
                super::normalization::BatchState::Deferred
            }
            Ok(super::normalization::BatchState::Finished) => {
                match super::normalization::normalize_legacy_artifact_batch(
                    &mut self.connection,
                    &self.database_path,
                    &mut self.legacy_artifact_scan,
                ) {
                    Ok(super::normalization::ArchiveState::Pending) => {
                        super::normalization::BatchState::Pending
                    }
                    Ok(super::normalization::ArchiveState::Deferred) | Err(_) => {
                        super::normalization::BatchState::Deferred
                    }
                    Ok(super::normalization::ArchiveState::Finished) => {
                        super::normalization::BatchState::Finished
                    }
                }
            }
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the exhaustive command protocol remains visible in one dispatch table"
    )]
    fn dispatch(&mut self, command: Command) -> ControlFlow<()> {
        let connection = &mut self.connection;
        let spatial_schema = &mut self.spatial_schema;
        let worker_alive = self.worker_alive;
        match command {
            Command::PhoneControl(command) => {
                super::phone_control::dispatch(connection, command);
            }
            Command::DeviceDrivetrain(command) => {
                super::device_drivetrain::dispatch(connection, command);
            }
            Command::DeviceVehicle(command) => {
                super::device_vehicle::dispatch(connection, command);
            }
            Command::CaptureSubmission(command) => {
                super::capture_submission::dispatch(connection, command);
            }
            Command::BeginLiveCapture {
                id,
                header_json,
                started_at_ms,
                reply,
            } => {
                let _ = reply.send(super::live_capture::begin(
                    connection,
                    id,
                    &header_json,
                    started_at_ms,
                ));
            }
            Command::AppendLiveCaptureEvent {
                id,
                kind,
                receipt_monotonic_ms,
                source_monotonic_offset_ms,
                source_wall_clock_unix_ms,
                payload,
                location,
                ble_record,
                reply,
            } => {
                let _ = reply.send(super::live_capture::append(
                    connection,
                    &super::live_capture::LiveCaptureEventAppend {
                        id,
                        kind,
                        receipt_monotonic_ms,
                        source_monotonic_offset_ms,
                        source_wall_clock_unix_ms,
                        payload: &payload,
                        location: location.as_ref(),
                        ble_record: ble_record.as_deref(),
                    },
                ));
            }
            Command::FinishLiveCapture {
                id,
                finished_at_ms,
                integrity,
                reply,
            } => {
                let _ = reply.send(super::live_capture::finish(
                    connection,
                    id,
                    finished_at_ms,
                    integrity,
                ));
            }
            Command::FinalizeLiveCapture {
                id,
                header_json,
                finished_at_ms,
                integrity,
                reply,
            } => {
                let _ = reply.send(super::live_capture::finalize(
                    connection,
                    id,
                    &header_json,
                    finished_at_ms,
                    integrity,
                ));
            }
            Command::UpdateLiveCaptureHeader {
                id,
                header_json,
                reply,
            } => {
                let _ = reply.send(super::live_capture::update_header(
                    connection,
                    id,
                    &header_json,
                ));
            }
            Command::ReadLiveCapture {
                id,
                after_sequence,
                limit,
                reply,
            } => {
                let _ = reply.send(super::live_capture::read(
                    connection,
                    id,
                    after_sequence,
                    limit,
                ));
            }
            Command::ReadLiveCaptureSemanticTelemetry {
                id,
                sequence,
                reply,
            } => {
                let _ = reply.send(super::live_capture::read_semantic_telemetry(
                    connection, id, sequence,
                ));
            }
            Command::RetainLiveCaptureContext {
                id,
                header_json,
                context_json,
                reply,
            } => {
                let _ = reply.send(super::live_capture_history::retain_context(
                    connection,
                    id,
                    &header_json,
                    &context_json,
                ));
            }
            Command::ListLiveCaptureHistory {
                cursor,
                limit,
                reply,
            } => {
                let _ = reply.send(super::live_capture_history::list(
                    connection,
                    cursor.as_ref(),
                    limit,
                ));
            }
            Command::RecordedCaptureLookup { id, reply } => {
                let _ = reply.send(super::recorded_capture::receipt_for_id(connection, id));
            }
            Command::ListPevcapCaptures {
                cursor,
                limit,
                reply,
            } => {
                let _ = reply.send(super::capture_history::list(connection, cursor, limit));
            }
            Command::BeginCaptureData {
                digest,
                encoding,
                artifact_size,
                reply,
            } => {
                let _ = reply.send(super::capture_data::begin(
                    connection,
                    &digest,
                    encoding,
                    artifact_size,
                ));
            }
            Command::AppendCaptureData {
                digest,
                sequence,
                chunk,
                reply,
            } => {
                let _ = reply.send(super::capture_data::append(
                    connection, &digest, sequence, &chunk,
                ));
            }
            Command::PublishCaptureData { digest, reply } => {
                let _ = reply.send(super::capture_data::publish(connection, &digest));
            }
            Command::AbortCaptureData { digest, reply } => {
                let _ = reply.send(super::capture_data::abort(connection, &digest));
            }
            Command::CaptureDataChunk {
                digest,
                sequence,
                reply,
            } => {
                let _ = reply.send(super::capture_data::chunk(connection, &digest, sequence));
            }
            Command::Capabilities { reply } => {
                let _ = reply.send(sqlite_capabilities(connection));
            }
            Command::IntegrityCheck { reply } => {
                let _ = reply.send(integrity_check(connection));
            }
            Command::CreateRide {
                source,
                created_at_ms,
                monotonic_created_at_ms,
                reply,
            } => {
                let _ = reply.send(create_ride(
                    connection,
                    source,
                    created_at_ms,
                    monotonic_created_at_ms,
                ));
            }
            Command::CreateStartedLiveRide {
                created_at_ms,
                monotonic_created_at_ms,
                candidate_vehicle,
                music_history_policy,
                reply,
            } => {
                let result = create_started_live_ride(
                    connection,
                    created_at_ms,
                    monotonic_created_at_ms,
                    candidate_vehicle.as_deref(),
                    music_history_policy,
                );
                let _ = reply.send(result);
            }
            Command::SettleRecoveredRide {
                ride_id,
                discard_empty,
                occurred_at_ms,
                monotonic_at_ms,
                replacement_candidate_vehicle,
                reply,
            } => {
                let _ = reply.send(settle_recovered_ride(
                    connection,
                    ride_id,
                    discard_empty,
                    occurred_at_ms,
                    monotonic_at_ms,
                    replacement_candidate_vehicle.as_deref(),
                ));
            }
            Command::CreateStartedRide {
                source,
                created_at_ms,
                monotonic_created_at_ms,
                candidate_vehicle,
                occurred_at_ms,
                reply,
            } => {
                let _ = reply.send(create_started_ride(
                    connection,
                    source,
                    created_at_ms,
                    monotonic_created_at_ms,
                    candidate_vehicle.as_deref(),
                    occurred_at_ms,
                ));
            }
            Command::UpdateRideMapMetadata {
                ride_id,
                candidate_vehicle,
                associated_vehicle,
                associated_at_ms,
                last_telemetry_at_ms,
                motion,
                moving_time_observation,
                reply,
            } => {
                let _ = reply.send(update_ride_map_metadata_with_moving_time(
                    connection,
                    ride_id,
                    candidate_vehicle.as_deref(),
                    associated_vehicle.as_deref(),
                    associated_at_ms,
                    last_telemetry_at_ms,
                    motion,
                    moving_time_observation,
                ));
            }
            Command::RecordBmsVoltageSamples { samples, reply } => {
                let _ = reply.send(record_bms_voltage_samples(connection, &samples));
            }
            Command::SaveSelectedDevice {
                platform_identifier,
                updated_at_ms,
                reply,
            } => {
                let _ = reply.send(save_selected_device(
                    connection,
                    &platform_identifier,
                    updated_at_ms,
                ));
            }
            Command::RememberSelectedDevice {
                platform_identifier,
                display_name,
                updated_at_ms,
                reply,
            } => {
                let _ = reply.send(remember_selected_device(
                    connection,
                    &platform_identifier,
                    display_name.as_deref(),
                    updated_at_ms,
                ));
            }
            Command::SaveDeviceName {
                platform_identifier,
                display_name,
                updated_at_ms,
                reply,
            } => {
                let _ = reply.send(save_device_name(
                    connection,
                    &platform_identifier,
                    &display_name,
                    updated_at_ms,
                ));
            }
            Command::MigrateDeviceName {
                platform_identifier,
                display_name,
                updated_at_ms,
                reply,
            } => {
                let _ = reply.send(migrate_device_name(
                    connection,
                    &platform_identifier,
                    &display_name,
                    updated_at_ms,
                ));
            }
            Command::DeviceName {
                platform_identifier,
                reply,
            } => {
                let _ = reply.send(device_name(connection, &platform_identifier));
            }
            Command::SelectedDevice { reply } => {
                let _ = reply.send(selected_device(connection));
            }
            Command::ClearSelectedDevice { reply } => {
                let _ = reply.send(clear_selected_device(connection));
            }
            Command::RememberLastConnectedDevice {
                platform_identifier,
                updated_at_ms,
                reply,
            } => {
                let _ = reply.send(remember_last_connected_device(
                    connection,
                    &platform_identifier,
                    updated_at_ms,
                ));
            }
            Command::VerifiedConnectionAdmission {
                platform_identifier,
                updated_at_ms,
                lifecycle_mutations,
                metadata,
                unselected_metadata,
                reply,
            } => {
                let result = apply_verified_connection_admission(
                    connection,
                    &platform_identifier,
                    updated_at_ms,
                    &lifecycle_mutations,
                    metadata.as_ref(),
                    unselected_metadata.as_ref(),
                );
                let _ = reply.send(result);
            }
            Command::LastConnectedDevice { reply } => {
                let _ = reply.send(last_connected_device(connection));
            }
            Command::ClearLastConnectedDevice { reply } => {
                let _ = reply.send(clear_last_connected_device(connection));
            }
            Command::RideAutostartEnabled { reply } => {
                let result = connection.query_row(
                    "SELECT COALESCE((SELECT autostart_enabled FROM ride_recording_preferences WHERE id = 1), 1)",
                    [],
                    |row| row.get::<_, bool>(0),
                ).map_err(Into::into);
                let _ = reply.send(result);
            }
            Command::SaveAutomaticDiagnosticRecordingEnabled { enabled, reply } => {
                let result = connection.execute(
                    "INSERT INTO diagnostic_recording_preferences (id, automatic_enabled) VALUES (1, ?1)
                     ON CONFLICT(id) DO UPDATE SET automatic_enabled = excluded.automatic_enabled",
                    [enabled],
                ).map(|_| ()).map_err(Into::into);
                if result.is_ok() {
                    self.diagnostic_recording_enabled
                        .store(enabled, Ordering::Release);
                }
                let _ = reply.send(result);
            }
            Command::SaveRideAutostartEnabled { enabled, reply } => {
                let result = connection.execute(
                    "INSERT INTO ride_recording_preferences (id, autostart_enabled) VALUES (1, ?1)
                     ON CONFLICT(id) DO UPDATE SET autostart_enabled = excluded.autostart_enabled",
                    [enabled],
                ).map(|_| ()).map_err(Into::into);
                let _ = reply.send(result);
            }
            Command::SavePhoneAlarmPreferences {
                device_identity,
                preferences,
                reply,
            } => {
                let _ = reply.send(save_phone_alarm_preferences(
                    connection,
                    &device_identity,
                    preferences,
                ));
            }
            Command::PhoneAlarmPreferences {
                device_identity,
                reply,
            } => {
                let _ = reply.send(phone_alarm_preferences(connection, &device_identity));
            }
            Command::RecordMusicEvent {
                ride_id,
                event,
                reply,
            } => {
                let _ = reply.send(record_music_event(connection, ride_id, &event));
            }
            Command::MusicHistory { ride_id, reply } => {
                let _ = reply.send(music_history(connection, ride_id));
            }
            Command::SaveMusicHistoryPolicy {
                ride_id,
                policy,
                reply,
            } => {
                let _ = reply.send(save_music_history_policy(connection, ride_id, policy));
            }
            Command::SaveMusicEvent {
                ride_id,
                policy,
                sequence,
                event,
                reply,
            } => {
                let _ = reply.send(save_music_event(
                    connection, ride_id, policy, sequence, &event,
                ));
            }
            Command::DeleteMusicHistory { ride_id, reply } => {
                let _ = reply.send(delete_music_history(connection, ride_id));
            }
            Command::DeleteSpotifyMusicHistory { reply } => {
                let _ = reply.send(super::music_privacy::delete_spotify_music_history(
                    connection,
                ));
            }
            Command::MusicEvents { ride_id, reply } => {
                let _ = reply.send(music_events(connection, ride_id));
            }
            Command::MusicHistoryPolicy { ride_id, reply } => {
                let _ = reply.send(music_history_policy(connection, ride_id));
            }
            Command::MusicHistoryState { ride_id, reply } => {
                let _ = reply.send(music_history_state(connection, ride_id));
            }
            Command::SaveVoltageSagModel {
                device_identity,
                model,
                reply,
            } => {
                let _ = reply.send(save_voltage_sag_model(connection, &device_identity, model));
            }
            Command::VoltageSagModel {
                device_identity,
                reply,
            } => {
                let _ = reply.send(voltage_sag_model(connection, &device_identity));
            }
            Command::RemoveVoltageSagModel {
                device_identity,
                reply,
            } => {
                let _ = reply.send(remove_voltage_sag_model(connection, &device_identity));
            }
            Command::SaveRideSessionMarker { marker, reply } => {
                let _ = reply.send(save_ride_session_marker(connection, &marker));
            }
            Command::RideSessionMarker { reply } => {
                let _ = reply.send(ride_session_marker(connection));
            }
            Command::ClearRideSessionMarker { reply } => {
                let _ = reply.send(clear_ride_session_marker(connection));
            }
            Command::PevcapImportLookup { digest, reply } => {
                let _ = reply.send(pevcap_import_receipt(connection, &digest, true));
            }
            Command::BeginPevcapImport {
                digest,
                managed_path,
                outcome,
                created_at_ms,
                reply,
            } => {
                let _ = reply.send(begin_pevcap_import(
                    connection,
                    &digest,
                    &managed_path,
                    outcome,
                    created_at_ms,
                ));
            }
            Command::AppendPevcapLocationBatch {
                ride_id,
                samples,
                reply,
            } => {
                let _ = reply.send(append_pevcap_location_batch(connection, ride_id, &samples));
            }
            Command::FinishPevcapImport { completion, reply } => {
                let result = finish_pevcap_import(connection, &completion);
                #[cfg(test)]
                if DROP_NEXT_PEVCAP_FINISH_RESPONSE.swap(false, Ordering::AcqRel) {
                    drop(reply);
                } else {
                    let _ = reply.send(result);
                }
                #[cfg(not(test))]
                let _ = reply.send(result);
            }
            Command::AbortPevcapImport {
                digest,
                ride_id,
                reply,
            } => {
                let _ = reply.send(abort_pevcap_import(connection, &digest, ride_id));
            }
            Command::CreateTrail { name, reply } => {
                let _ = reply.send(create_trail(connection, spatial_schema, &name));
            }
            Command::AppendTrailSegment {
                trail_id,
                sequence,
                start,
                end,
                reply,
            } => {
                let _ = reply.send(append_trail_segment(
                    connection,
                    spatial_schema,
                    trail_id,
                    sequence,
                    start,
                    end,
                ));
            }
            Command::TrailSegmentsInBounds {
                bounds,
                cursor,
                limit,
                reply,
            } => {
                let _ = reply.send(trail_segments_in_bounds(
                    connection,
                    spatial_schema,
                    bounds,
                    cursor,
                    limit,
                ));
            }
            Command::Charging(command) => {
                super::charging::execute(connection, spatial_schema, command);
            }
            Command::CreateMapPoint {
                name,
                coordinate,
                reply,
            } => {
                let _ = reply.send(create_map_point(
                    connection,
                    spatial_schema,
                    &name,
                    coordinate,
                ));
            }
            Command::MapPointsInBounds {
                bounds,
                cursor,
                limit,
                reply,
            } => {
                let _ = reply.send(map_points_in_bounds(
                    connection,
                    spatial_schema,
                    bounds,
                    cursor,
                    limit,
                ));
            }
            Command::RebuildSpatialIndexes { reply } => {
                let _ = reply.send(rebuild_spatial_indexes(connection, spatial_schema));
            }
            Command::Backup { destination, reply } => {
                let _ = reply.send(backup(connection, &destination));
            }
            Command::ExportRideJson {
                ride_id,
                destination,
                reply,
            } => {
                let _ = reply.send(export_ride_json(connection, ride_id, &destination));
            }
            Command::Transition {
                ride_id,
                event,
                occurred_at_ms,
                monotonic_at_ms,
                last_telemetry_at_ms,
                reply,
            } => {
                let result = transition_ride_with_telemetry(
                    connection,
                    ride_id,
                    event,
                    occurred_at_ms,
                    monotonic_at_ms,
                    last_telemetry_at_ms,
                );
                let _ = reply.send(result);
            }
            Command::AppendLocation {
                ride_id,
                sample,
                segment_id,
                telemetry_state,
                reply,
            } => {
                let _ = reply.send(append_location(
                    connection,
                    ride_id,
                    sample,
                    segment_id,
                    telemetry_state,
                ));
            }
            Command::AppendLocationAsync {
                ride_id,
                sample,
                segment_id,
                start_reason,
                telemetry_state,
                moving_time_observation,
                latest_vehicle_moving_time_observation,
                reply,
            } => {
                let _ = reply.send(append_location_with_moving_time(
                    connection,
                    ride_id,
                    sample,
                    segment_id,
                    telemetry_state,
                    SegmentStartReasonInput::Recorded(start_reason),
                    moving_time_observation,
                    latest_vehicle_moving_time_observation,
                ));
            }
            #[cfg(test)]
            Command::AppendLocationWithWorkerGate {
                ride_id,
                sample,
                segment_id,
                telemetry_state,
                entered,
                release,
                reply,
            } => {
                let _ = entered.send(());
                let _ = release.recv();
                let result = append_location_with_result(
                    connection,
                    ride_id,
                    sample,
                    segment_id,
                    telemetry_state,
                    SegmentStartReasonInput::Infer,
                );
                let _ = reply.send(result);
            }
            #[cfg(test)]
            Command::AppendLocationWithWorkerFailure { entered, reply } => {
                worker_alive.store(false, Ordering::Release);
                let _ = entered.send(());
                drop(reply);
                return ControlFlow::Break(());
            }
            Command::Summary { ride_id, reply } => {
                let _ = reply.send(load_summary(connection, ride_id));
            }
            Command::SummaryWithDuration { ride_id, reply } => {
                let _ = reply.send(load_summary_with_duration(connection, ride_id));
            }
            Command::FindRide { ride_id, reply } => {
                let _ = reply.send(find_ride(connection, ride_id));
            }
            Command::FindRideFiltered {
                ride_id,
                query,
                reply,
            } => {
                let _ = reply.send(find_ride_filtered(connection, ride_id, &query));
            }
            Command::NewestRecoverableRide { reply } => {
                let _ = reply.send(newest_recoverable_ride(connection));
            }
            Command::RestoreSnapshot { reply } => {
                let _ = reply.send(load_ride_restore_snapshot(connection));
            }
            Command::ListRides {
                cursor,
                limit,
                query,
                reply,
            } => {
                let _ = reply.send(list_rides(connection, cursor, limit, &query));
            }
            Command::ProjectHistoryContext {
                query,
                selected_ride,
                budget,
                viewport,
                privacy,
                cancellation,
                reply,
            } => {
                cancellation.install_interrupt(connection.get_interrupt_handle());
                let result = project_history_context(
                    connection,
                    &query,
                    selected_ride,
                    budget,
                    viewport,
                    privacy,
                    &cancellation,
                );
                #[cfg(test)]
                connection
                    .progress_handler(0, None::<fn() -> bool>)
                    .expect("SQLite progress handler hook installs");
                cancellation.clear_interrupt();
                let _ = reply.send(result);
            }
            Command::ListRideHistoryVehicleOptions { reply } => {
                let _ = reply.send(list_ride_history_vehicle_options(connection));
            }
            Command::RoutePoints {
                ride_id,
                cursor,
                limit,
                reply,
            } => {
                let _ = reply.send(route_points(connection, ride_id, cursor, limit));
            }
            Command::ProjectRoutePoints {
                ride_id,
                viewport,
                budget,
                privacy,
                cancellation,
                reply,
            } => {
                if let Some(cancellation) = cancellation.as_ref() {
                    cancellation.install_interrupt(connection.get_interrupt_handle());
                }
                let result = project_route_points(
                    connection,
                    ride_id,
                    viewport,
                    budget,
                    privacy,
                    cancellation.as_ref(),
                );
                #[cfg(test)]
                connection
                    .progress_handler(0, None::<fn() -> bool>)
                    .expect("SQLite progress handler hook installs");
                if let Some(cancellation) = cancellation.as_ref() {
                    cancellation.clear_interrupt();
                }
                let _ = reply.send(result);
            }
            #[cfg(test)]
            Command::InstallRouteProjectionTestGate {
                entered,
                release,
                reply,
            } => {
                install_sqlite_progress_gate(connection, entered, release);
                let _ = reply.send(Ok(()));
            }
            #[cfg(test)]
            Command::StopForTest { reply } => {
                worker_alive.store(false, Ordering::Release);
                let _ = reply.send(Ok(()));
                return ControlFlow::Break(());
            }
            Command::RideCheckpoint { reply } => {
                let _ = reply.send(Ok(()));
            }
            Command::CheckpointStationaryLocationObservation { checkpoint, reply } => {
                let _ = reply.send(checkpoint_stationary_location_observation(
                    connection, checkpoint,
                ));
            }
            Command::CheckpointLocationObservation { checkpoint, reply } => {
                let _ = reply.send(checkpoint_location_observation(connection, checkpoint));
            }
            Command::Shutdown { reply } => {
                worker_alive.store(false, Ordering::Release);
                let _ = reply.send(Ok(()));
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    }

    fn stop(&self) {
        self.worker_alive.store(false, Ordering::Release);
    }
}

#[cfg(test)]
fn install_sqlite_progress_gate(
    connection: &Connection,
    entered: SyncSender<()>,
    release: Receiver<()>,
) {
    let first_progress_callback = Arc::new(AtomicBool::new(true));
    let callback_is_first = Arc::clone(&first_progress_callback);
    connection
        .progress_handler(
            1,
            Some(move || {
                if callback_is_first.swap(false, Ordering::AcqRel) && entered.send(()).is_ok() {
                    let _ = release.recv();
                }
                false
            }),
        )
        .expect("SQLite progress handler hook installs");
}
