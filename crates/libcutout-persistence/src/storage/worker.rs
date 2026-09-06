use super::{
    Command, SegmentStartReasonInput, SpatialSchemaState, abort_pevcap_import, append_location,
    append_location_with_result, append_pevcap_location_batch, append_trail_segment, backup,
    begin_pevcap_import, clear_ride_session_marker, clear_selected_device, create_map_point,
    create_ride, create_started_live_ride, create_started_ride, create_trail, delete_music_history,
    device_name, export_ride_json, find_ride, finish_pevcap_import,
    list_ride_history_vehicle_options, list_rides, load_summary, load_summary_with_duration,
    map_points_in_bounds, migrate_device_name, music_events, music_history_policy,
    music_history_state,
    newest_recoverable_ride, pevcap_import_receipt, project_history_context, project_route_points,
    rebuild_spatial_indexes, remember_selected_device, remove_voltage_sag_model,
    ride_session_marker, route_points, save_device_name, save_music_event,
    save_music_history_policy, save_ride_session_marker, save_selected_device,
    save_voltage_sag_model, selected_device, sqlite_capabilities, trail_segments_in_bounds,
    transition_ride, update_ride_map_metadata, voltage_sag_model,
};
use rusqlite::Connection;
use std::ops::ControlFlow;
#[cfg(test)]
use std::sync::{Arc, mpsc::SyncSender};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Receiver,
};

#[cfg(test)]
static DROP_NEXT_PEVCAP_FINISH_RESPONSE: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
pub(super) fn drop_next_pevcap_finish_response_for_test() {
    DROP_NEXT_PEVCAP_FINISH_RESPONSE.store(true, Ordering::Release);
}

pub(super) fn run(connection: Connection, receiver: &Receiver<Command>, worker_alive: &AtomicBool) {
    let mut worker = DatabaseWorker {
        connection,
        spatial_schema: SpatialSchemaState::Uninitialized,
        worker_alive,
    };
    while let Ok(command) = receiver.recv() {
        if worker.dispatch(command).is_break() {
            break;
        }
    }
    worker.stop();
}

struct DatabaseWorker<'a> {
    connection: Connection,
    spatial_schema: SpatialSchemaState,
    worker_alive: &'a AtomicBool,
}

impl DatabaseWorker<'_> {
    #[allow(
        clippy::too_many_lines,
        reason = "the exhaustive command protocol remains visible in one dispatch table"
    )]
    fn dispatch(&mut self, command: Command) -> ControlFlow<()> {
        let connection = &mut self.connection;
        let spatial_schema = &mut self.spatial_schema;
        let worker_alive = self.worker_alive;
        match command {
            Command::Capabilities { reply } => {
                let _ = reply.send(sqlite_capabilities(connection));
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
                reply,
            } => {
                let _ = reply.send(create_started_live_ride(
                    connection,
                    created_at_ms,
                    monotonic_created_at_ms,
                    candidate_vehicle.as_deref(),
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
                reply,
            } => {
                let _ = reply.send(update_ride_map_metadata(
                    connection,
                    ride_id,
                    candidate_vehicle.as_deref(),
                    associated_vehicle.as_deref(),
                    associated_at_ms,
                    last_telemetry_at_ms,
                ));
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
            Command::FinishPevcapImport {
                digest,
                ride_id,
                managed_path,
                outcome,
                artifact_size,
                record_count,
                location_count,
                duration_milliseconds,
                imported_at_ms,
                reply,
            } => {
                let result = finish_pevcap_import(
                    connection,
                    &digest,
                    ride_id,
                    &managed_path,
                    outcome,
                    artifact_size,
                    record_count,
                    location_count,
                    duration_milliseconds,
                    imported_at_ms,
                );
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
                reply,
            } => {
                let _ = reply.send(transition_ride(
                    connection,
                    ride_id,
                    event,
                    occurred_at_ms,
                    monotonic_at_ms,
                ));
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
                reply,
            } => {
                let _ = reply.send(append_location_with_result(
                    connection,
                    ride_id,
                    sample,
                    segment_id,
                    telemetry_state,
                    SegmentStartReasonInput::Recorded(start_reason),
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
            Command::NewestRecoverableRide { reply } => {
                let _ = reply.send(newest_recoverable_ride(connection));
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
                connection.progress_handler(0, None::<fn() -> bool>);
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
                connection.progress_handler(0, None::<fn() -> bool>);
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
    connection.progress_handler(
        1,
        Some(move || {
            if callback_is_first.swap(false, Ordering::AcqRel) && entered.send(()).is_ok() {
                let _ = release.recv();
            }
            false
        }),
    );
}
