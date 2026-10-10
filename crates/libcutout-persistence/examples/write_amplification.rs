//! File-backed write workload for an external process-I/O sampler.
//!
//! Run with `<scenario> <count>`; writer scenarios pair every/material policies for stationary,
//! parked field changes, moving telemetry, and diagnostics. Each stdin newline releases
//! one boundary: READY starts writes; DONE permits shutdown and temporary cleanup.
//! Uses the production storage configuration without overriding its journal policy.

use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
    time::Instant,
};

use cutout_core::{
    GattChannel, MonotonicTimestamp, PevcapRecord, PevcapSemanticTelemetry,
    PevcapTelemetryProvenance, RawTelemetryReadback, WallClockUnixTimestamp,
};
use cutout_ride_maps::{Coordinate, LocationAdmission, LocationSample, LocationSource};
use libcutout_persistence::{
    CaptureJsonlExport, CaptureMetadata, CaptureRecordAdmission, CaptureRecordingPolicy,
    CaptureWriteOutcome, CaptureWriter, CaptureWriterFinish, LiveCaptureEventKind, LiveCaptureId,
    LiveCaptureIntegrity, QueryLimit, RideDatabase, RideId,
};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const WALL_CLOCK_MS: u64 = 1_700_000_000_000;
const HEADER: &[u8] = br#"{"format":"pevcap","measurement":"write-amplification-v1"}"#;

#[derive(Clone, Copy)]
enum Scenario {
    Route,
    RawCapture,
    BleCapture,
    Writer(WriterInput, CaptureRecordingPolicy),
}

impl Scenario {
    const fn name(self) -> &'static str {
        match self {
            Self::Route => "route",
            Self::RawCapture => "raw-capture",
            Self::BleCapture => "ble-capture",
            Self::Writer(input, policy) => input.name(policy),
        }
    }
}

#[derive(Clone, Copy)]
enum WriterInput {
    Stationary,
    ParkedChanges,
    Moving,
    Diagnostic,
}

impl WriterInput {
    const fn name(self, policy: CaptureRecordingPolicy) -> &'static str {
        match (self, policy) {
            (Self::Stationary, CaptureRecordingPolicy::EveryObservation) => "stationary-every",
            (Self::Stationary, CaptureRecordingPolicy::MaterialChanges) => "stationary-material",
            (Self::ParkedChanges, CaptureRecordingPolicy::EveryObservation) => {
                "parked-changes-every"
            }
            (Self::ParkedChanges, CaptureRecordingPolicy::MaterialChanges) => {
                "parked-changes-material"
            }
            (Self::Moving, CaptureRecordingPolicy::EveryObservation) => "moving-every",
            (Self::Moving, CaptureRecordingPolicy::MaterialChanges) => "moving-material",
            (Self::Diagnostic, CaptureRecordingPolicy::EveryObservation) => "diagnostic-every",
            (Self::Diagnostic, CaptureRecordingPolicy::MaterialChanges) => "diagnostic-material",
        }
    }

    const fn admission(self) -> CaptureRecordAdmission {
        match self {
            Self::Stationary | Self::ParkedChanges => CaptureRecordAdmission::StationaryTelemetry,
            Self::Moving | Self::Diagnostic => CaptureRecordAdmission::EveryObservation,
        }
    }
}

enum Workload {
    Route(RideId),
    Capture(LiveCaptureId),
}

fn require(condition: bool, message: &str) -> Result<()> {
    if !condition {
        return Err(io::Error::other(message).into());
    }
    Ok(())
}

fn boundary(label: &str, details: &Value) -> Result<()> {
    println!("{label} {} {details}", std::process::id());
    io::stdout().flush()?;
    let mut line = String::new();
    require(
        io::stdin().read_line(&mut line)? != 0,
        "sampler closed stdin before releasing boundary",
    )
}

fn pragmas(connection: &Connection) -> Result<Value> {
    Ok(json!({
        "journal_mode": connection.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))?,
        "synchronous": connection.pragma_query_value(None, "synchronous", |row| row.get::<_, u64>(0))?,
        "page_size": connection.pragma_query_value(None, "page_size", |row| row.get::<_, u64>(0))?,
        "page_count": connection.pragma_query_value(None, "page_count", |row| row.get::<_, u64>(0))?,
        "freelist_count": connection.pragma_query_value(None, "freelist_count", |row| row.get::<_, u64>(0))?,
        "wal_autocheckpoint": connection.pragma_query_value(None, "wal_autocheckpoint", |row| row.get::<_, u64>(0))?,
        "sqlite_version": connection.query_row("SELECT sqlite_version()", [], |row| row.get::<_, String>(0))?,
    }))
}

fn file_sizes(path: &Path) -> Result<Value> {
    let size = |suffix: &str| -> Result<u64> {
        let name = format!("{}{suffix}", path.display());
        match fs::metadata(name) {
            Ok(metadata) => Ok(metadata.len()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(error) => Err(error.into()),
        }
    };
    Ok(
        json!({"database": size("")?, "wal": size("-wal")?, "shm": size("-shm")?, "journal": size("-journal")?}),
    )
}

fn ble_template() -> Result<PevcapRecord> {
    let telemetry: RawTelemetryReadback = serde_json::from_value(json!({
        "fields": (0..8).map(|id| json!({"id": id, "value": 1234 + id})).collect::<Vec<_>>(),
        "float_fields": (0..4).map(|id| json!({"id": 100 + id, "value_bits": 1_065_353_216_u32})).collect::<Vec<_>>()
    }))?;
    require(
        telemetry.fields.len() == 8 && telemetry.float_fields.len() == 4,
        "telemetry fixture exceeded supported field bounds",
    )?;
    Ok(PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(0),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![0xaa; 160],
    )
    .with_telemetry(telemetry)
    .with_semantic_telemetry(PevcapSemanticTelemetry {
        observed_at_ms: Some(MonotonicTimestamp::new(0)),
        provenance: PevcapTelemetryProvenance::LiveSession,
        snapshot_schema_version: 1,
        library_version: "write-amplification-v1".to_owned(),
        snapshot_json: r#"{"speed":{"value":{"value":1234},"source":"reported"},"voltage":84000}"#
            .to_owned(),
    }))
}

fn run(
    database: &RideDatabase,
    workload: &Workload,
    scenario: Scenario,
    count: u32,
    template: &PevcapRecord,
) -> Result<u64> {
    let mut payload_bytes = 0_u64;
    for index in 0..count {
        match workload {
            Workload::Route(ride) => {
                // Exactly 400 E7 latitude units north per second; 4,448 rounded mm per edge.
                let latitude = 400_000_000 + i32::try_from(index)? * 400;
                let sample = LocationSample::new(
                    Coordinate::from_fixed_parts(latitude, -1_050_000_000)?,
                    1_000 + u64::from(index) * 1_000,
                    WALL_CLOCK_MS + 1_000 + u64::from(index) * 1_000,
                    Some(1_000),
                    LocationSource::Live,
                );
                require(
                    database.append_location(*ride, sample)? == LocationAdmission::Accepted,
                    "generated route sample was not accepted",
                )?;
            }
            Workload::Capture(capture) => {
                let mut record = template.clone();
                record.monotonic_ms = MonotonicTimestamp::new(u64::from(index) * 100);
                if let Some(telemetry) = record.semantic_telemetry.as_mut() {
                    telemetry.observed_at_ms = Some(record.monotonic_ms);
                }
                let payload = record.to_jsonl_line()?.into_bytes();
                payload_bytes += u64::try_from(payload.len())?;
                let sequence = match scenario {
                    Scenario::RawCapture => database.append_live_capture_event(
                        *capture,
                        LiveCaptureEventKind::Notification,
                        record.monotonic_ms.get(),
                        None,
                        None,
                        payload,
                    )?,
                    Scenario::BleCapture => {
                        database.append_live_capture_record(*capture, record, payload)?
                    }
                    Scenario::Route | Scenario::Writer(..) => {
                        return Err(io::Error::other("route/capture setup mismatch").into());
                    }
                };
                require(
                    sequence == u64::from(index),
                    "capture sequence differs from submitted order",
                )?;
            }
        }
    }
    database.wait_for_ride_checkpoint()?;
    Ok(payload_bytes)
}

fn verify(
    database: &RideDatabase,
    connection: &Connection,
    workload: &Workload,
    scenario: Scenario,
    count: u32,
    payload_bytes: u64,
    raw_fields_per_event: u64,
) -> Result<Value> {
    let mut rows = serde_json::Map::new();
    for table in [
        "rides",
        "ride_points",
        "ride_segments",
        "live_capture_sessions",
        "live_capture_events",
        "live_capture_ble_observations",
        "live_capture_ble_raw_telemetry_fields",
        "live_capture_ble_semantic_telemetry",
    ] {
        let actual: u64 =
            connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })?;
        let expected = match (scenario, table) {
            (Scenario::Route, "rides" | "ride_segments")
            | (Scenario::RawCapture | Scenario::BleCapture, "live_capture_sessions") => 1,
            (Scenario::Route, "ride_points")
            | (Scenario::RawCapture | Scenario::BleCapture, "live_capture_events")
            | (
                Scenario::BleCapture,
                "live_capture_ble_observations" | "live_capture_ble_semantic_telemetry",
            ) => u64::from(count),
            (Scenario::BleCapture, "live_capture_ble_raw_telemetry_fields") => {
                u64::from(count) * raw_fields_per_event
            }
            _ => 0,
        };
        require(
            actual == expected,
            &format!("{table} rows: expected {expected}, got {actual}"),
        )?;
        rows.insert(table.to_owned(), json!(actual));
    }
    match workload {
        Workload::Route(ride) => {
            let summary = database.summary(*ride)?;
            require(
                summary.point_count().as_u64() == u64::from(count),
                "summary point count mismatch",
            )?;
            require(
                summary.distance_millimetres() == u64::from(count - 1) * 4_448,
                "summary distance mismatch",
            )?;
            let (segment_points, last_time): (u64, u64) = connection.query_row(
                "SELECT point_count, ended_monotonic_ms FROM ride_segments",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            require(
                segment_points == u64::from(count) && last_time == u64::from(count) * 1_000,
                "segment summary mismatch",
            )?;
            rows.insert(
                "summary_distance_mm".to_owned(),
                json!(summary.distance_millimetres()),
            );
        }
        Workload::Capture(capture) => {
            let (next, stored, bytes, first, last): (u64, u64, u64, u64, u64) = connection.query_row(
                "SELECT next_sequence, stored_bytes, (SELECT SUM(COALESCE(payload_original_bytes, length(payload))) FROM live_capture_events),
                    (SELECT MIN(sequence) FROM live_capture_events), (SELECT MAX(sequence) FROM live_capture_events)
                 FROM live_capture_sessions", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?;
            require(
                next == u64::from(count) && first == 0 && last == u64::from(count - 1),
                "capture sequence bounds mismatch",
            )?;
            require(
                bytes == payload_bytes && stored == payload_bytes + u64::try_from(HEADER.len())?,
                "capture byte accounting mismatch",
            )?;
            rows.insert("original_payload_bytes".to_owned(), json!(bytes));
            let stored_payload_bytes: u64 = connection.query_row(
                "SELECT SUM(length(payload)) FROM live_capture_events",
                [],
                |row| row.get(0),
            )?;
            rows.insert(
                "stored_payload_bytes".to_owned(),
                json!(stored_payload_bytes),
            );
            let verified_events = verify_capture_payloads(database, *capture, count)?;
            rows.insert("byte_exact_events".to_owned(), json!(verified_events));
        }
    }
    Ok(Value::Object(rows))
}

fn verify_capture_payloads(
    database: &RideDatabase,
    capture: LiveCaptureId,
    count: u32,
) -> Result<u64> {
    let template = ble_template()?;
    let mut cursor = None;
    let mut verified_events = 0_u64;
    loop {
        let page = database.live_capture_page(capture, cursor, QueryLimit::new(256)?)?;
        if page.events.is_empty() {
            break;
        }
        for event in page.events {
            let mut record = template.clone();
            record.monotonic_ms = MonotonicTimestamp::new(event.sequence * 100);
            if let Some(telemetry) = record.semantic_telemetry.as_mut() {
                telemetry.observed_at_ms = Some(record.monotonic_ms);
            }
            require(
                event.payload == record.to_jsonl_line()?.into_bytes(),
                "capture payload bytes changed",
            )?;
            require(
                event.sequence == verified_events,
                "readback sequence mismatch",
            )?;
            cursor = Some(event.sequence);
            verified_events += 1;
        }
    }
    require(
        verified_events == u64::from(count),
        "readback count mismatch",
    )?;
    Ok(verified_events)
}

fn writer_record(input: WriterInput, index: u64) -> Result<PevcapRecord> {
    let mut record = ble_template()?;
    let at = index * 100;
    let voltage = match input {
        WriterInput::ParkedChanges => {
            let voltage = 84_000 + index;
            let mut bytes = record.bytes.to_vec();
            bytes[..8].copy_from_slice(&voltage.to_le_bytes());
            record.bytes = bytes.into();
            record
                .telemetry
                .as_mut()
                .ok_or_else(|| io::Error::other("raw telemetry fixture missing"))?
                .fields[0] = cutout_core::RawFieldValue::new(0, i64::try_from(voltage)?);
            voltage
        }
        WriterInput::Stationary | WriterInput::Moving | WriterInput::Diagnostic => 84_000,
    };
    let speed = match input {
        WriterInput::Moving => 2_000,
        WriterInput::Stationary | WriterInput::ParkedChanges | WriterInput::Diagnostic => 0,
    };
    record.monotonic_ms = MonotonicTimestamp::new(at);
    let semantic = record
        .semantic_telemetry
        .as_mut()
        .ok_or_else(|| io::Error::other("semantic fixture missing"))?;
    semantic.observed_at_ms = Some(record.monotonic_ms);
    semantic.snapshot_json = json!({
        "at_ms": {"milliseconds": at},
        "speed_observed_at_ms": {"milliseconds": at},
        "speed": {"value": {"value": speed}, "source": "reported", "quality": "known", "verification": "hardware_verified"},
        "voltage": voltage,
    }).to_string();
    Ok(record)
}

fn verify_writer(
    database: &RideDatabase,
    id: LiveCaptureId,
    export: &Path,
    input: WriterInput,
    expected: u32,
) -> Result<Value> {
    let contents = fs::read_to_string(export)?;
    let lines = contents.lines().skip(1).collect::<Vec<_>>();
    require(
        lines.len() == usize::try_from(expected)?,
        "writer export count mismatch",
    )?;
    let mut verified = 0_u64;
    let mut payload_bytes = 0_u64;
    let mut cursor = None;
    loop {
        let page = database.live_capture_page(id, cursor, QueryLimit::new(256)?)?;
        if page.events.is_empty() {
            break;
        }
        for event in page.events {
            let original = writer_record(input, verified)?.to_jsonl_line()?;
            require(
                event.sequence == verified && event.payload == original.as_bytes(),
                "writer stored sequence/payload changed",
            )?;
            require(
                lines.get(usize::try_from(verified)?).copied() == Some(original.as_str()),
                "writer export changed original bytes",
            )?;
            cursor = Some(event.sequence);
            verified += 1;
            payload_bytes += u64::try_from(original.len())?;
        }
    }
    require(
        verified == u64::from(expected),
        "writer readback count mismatch",
    )?;
    Ok(
        json!({"byte_exact_events": verified, "record_payload_bytes": payload_bytes, "export_bytes": contents.len()}),
    )
}

fn write_observations(writer: &CaptureWriter, input: WriterInput, count: u32) -> Result<()> {
    let ingress = writer.ingress();
    for index in 0..count {
        require(
            ingress.try_send_record_with_admission(
                writer_record(input, u64::from(index))?,
                input.admission(),
            ) == CaptureWriteOutcome::Accepted,
            "writer queue rejected observation",
        )?;
        // Identical bounded batches avoid queue overflow without moving any writes beyond DONE.
        if (index + 1) % 32 == 0 {
            writer.flush()?;
        }
    }
    Ok(())
}

fn run_writer(input: WriterInput, policy: CaptureRecordingPolicy, count: u32) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("write.sqlite");
    let export = directory.path().join("capture.jsonl");
    let database = RideDatabase::open(&path)?;
    let scenario = Scenario::Writer(input, policy);
    let writer = CaptureWriter::start_with_database_and_policy(
        export.clone(),
        WallClockUnixTimestamp::new(WALL_CLOCK_MS),
        "write-amplification-v1",
        None,
        &CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        },
        database.clone(),
        policy,
    )?;
    writer.flush()?;
    let observer = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let before = pragmas(&observer)?;
    require(
        before["journal_mode"].as_str() == Some("wal") && before["synchronous"].as_u64() == Some(2),
        "expected WAL/FULL",
    )?;
    let sizes_before = file_sizes(&path)?;
    boundary(
        "READY",
        &json!({"scenario": scenario.name(), "count": count, "database": path, "observer_pragmas": before, "file_sizes": sizes_before}),
    )?;
    let started = Instant::now();
    write_observations(&writer, input, count)?;
    let CaptureWriterFinish::DatabaseFinished {
        live_capture_id: id,
        integrity: LiveCaptureIntegrity::Complete,
        jsonl_export: CaptureJsonlExport::Available(_),
        status,
        ..
    } = writer.finish()?
    else {
        return Err(io::Error::other("writer failed complete finalization/export").into());
    };
    require(
        status.dropped_messages == 0 && status.queued_messages == 0 && !status.failed,
        "writer did not complete admitted observations",
    )?;
    let checkpointer = Connection::open(&path)?;
    checkpointer.pragma_update(None, "synchronous", "FULL")?;
    let checkpoint: (u64, u64, u64) =
        checkpointer.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    require(
        checkpoint == (0, 0, 0),
        "writer final checkpoint incomplete",
    )?;
    drop(checkpointer);
    let elapsed = started.elapsed();
    let expected = match (input, policy) {
        (WriterInput::Stationary, CaptureRecordingPolicy::MaterialChanges) => 1,
        _ => count,
    };
    let verified = verify_writer(&database, id, &export, input, expected)?;
    let sizes_after = file_sizes(&path)?;
    boundary(
        "DONE",
        &json!({"scenario": scenario.name(), "count": count, "elapsed_ns": elapsed.as_nanos(), "verified": verified, "status_logical_bytes": status.bytes_written, "file_sizes_before": sizes_before, "file_sizes_after": sizes_after, "final_checkpoint": checkpoint,
        "measurement_note": "includes writer queue processing, per-retained-event FULL commits, complete finalization, JSONL export, final FULL/TRUNCATE checkpoint and byte-exact readback; excludes setup/shutdown; libproc process I/O is not phone flash I/O"}),
    )?;
    drop(observer);
    database.shutdown()?;
    Ok(())
}

fn parse_scenario(value: Option<&str>) -> Result<Scenario> {
    Ok(match value {
        Some("route") => Scenario::Route,
        Some("raw-capture") => Scenario::RawCapture,
        Some("ble-capture") => Scenario::BleCapture,
        Some("stationary-every") => Scenario::Writer(
            WriterInput::Stationary,
            CaptureRecordingPolicy::EveryObservation,
        ),
        Some("stationary-material") => Scenario::Writer(
            WriterInput::Stationary,
            CaptureRecordingPolicy::MaterialChanges,
        ),
        Some("parked-changes-every") => Scenario::Writer(
            WriterInput::ParkedChanges,
            CaptureRecordingPolicy::EveryObservation,
        ),
        Some("parked-changes-material") => Scenario::Writer(
            WriterInput::ParkedChanges,
            CaptureRecordingPolicy::MaterialChanges,
        ),
        Some("moving-every") => Scenario::Writer(
            WriterInput::Moving,
            CaptureRecordingPolicy::EveryObservation,
        ),
        Some("moving-material") => {
            Scenario::Writer(WriterInput::Moving, CaptureRecordingPolicy::MaterialChanges)
        }
        Some("diagnostic-every") => Scenario::Writer(
            WriterInput::Diagnostic,
            CaptureRecordingPolicy::EveryObservation,
        ),
        Some("diagnostic-material") => Scenario::Writer(
            WriterInput::Diagnostic,
            CaptureRecordingPolicy::MaterialChanges,
        ),
        _ => {
            return Err(io::Error::other(
                "usage: write_amplification <route|raw-capture|ble-capture|stationary-{every,material}|parked-changes-{every,material}|moving-{every,material}|diagnostic-{every,material}> <count>",
            )
            .into());
        }
    })
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let scenario = parse_scenario(args.next().as_deref())?;
    let count: u32 = args
        .next()
        .ok_or_else(|| io::Error::other("count is required"))?
        .parse()?;
    require(
        (1..=100_000).contains(&count) && args.next().is_none(),
        "count must be 1..=100000; no extra arguments",
    )?;
    match scenario {
        Scenario::Writer(input, policy) => return run_writer(input, policy, count),
        Scenario::Route | Scenario::RawCapture | Scenario::BleCapture => {}
    }
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("write.sqlite");
    let database = RideDatabase::open(&path)?;
    let workload = match scenario {
        Scenario::Route => {
            Workload::Route(database.create_started_live_ride(WALL_CLOCK_MS, 0, None)?)
        }
        Scenario::RawCapture | Scenario::BleCapture => {
            Workload::Capture(database.begin_live_capture(HEADER.to_vec(), WALL_CLOCK_MS)?)
        }
        Scenario::Writer(..) => {
            return Err(io::Error::other("writer scenario setup mismatch").into());
        }
    };
    let template = ble_template()?;
    let telemetry = template
        .telemetry
        .as_ref()
        .ok_or_else(|| io::Error::other("telemetry fixture is missing"))?;
    let raw_fields_per_event =
        u64::try_from(telemetry.fields.len() + telemetry.float_fields.len())?;
    let observer = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let before = pragmas(&observer)?;
    require(
        before["journal_mode"].as_str() == Some("wal"),
        "journal mode readback mismatch",
    )?;
    require(
        before["synchronous"].as_u64() == Some(2),
        "expected default FULL synchronous setting",
    )?;
    let sizes_before = file_sizes(&path)?;
    boundary(
        "READY",
        &json!({"scenario": scenario.name(), "count": count, "database": path,
        "raw_fields_per_event": raw_fields_per_event,
        "observer_pragmas": before, "file_sizes": sizes_before}),
    )?;

    let started = Instant::now();
    let payload_bytes = run(&database, &workload, scenario, count, &template)?;
    let checkpointer = Connection::open(&path)?;
    checkpointer.pragma_update(None, "synchronous", "FULL")?;
    let checkpoint: (u64, u64, u64) =
        checkpointer.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    require(checkpoint == (0, 0, 0), "final checkpoint was incomplete")?;
    drop(checkpointer);
    let elapsed = started.elapsed();
    let verified = verify(
        &database,
        &observer,
        &workload,
        scenario,
        count,
        payload_bytes,
        raw_fields_per_event,
    )?;
    let after = pragmas(&observer)?;
    let sizes_after = file_sizes(&path)?;
    boundary(
        "DONE",
        &json!({"scenario": scenario.name(), "count": count, "elapsed_ns": elapsed.as_nanos(),
        "serialized_payload_bytes": payload_bytes, "verified": verified, "observer_pragmas": after,
        "file_sizes_before": sizes_before, "file_sizes_after": sizes_after,
        "database_growth_bytes": sizes_after["database"].as_u64().unwrap_or_default().saturating_sub(sizes_before["database"].as_u64().unwrap_or_default()),
        "final_checkpoint": checkpoint,
        "measurement_note": "includes final FULL/TRUNCATE WAL checkpoint and byte-exact event readback; observer pragma values do not independently verify worker settings; excludes setup, capture finalization and shutdown"}),
    )?;
    drop(observer);
    database.shutdown()?;
    Ok(())
}
