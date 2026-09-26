#![cfg_attr(test, allow(clippy::disallowed_macros))]

use cutout_mobile_ffi::*;
use std::sync::{Arc, Mutex, MutexGuard};
static DATABASE_LOCK: Mutex<()> = Mutex::new(());

fn mobile_ride_id(value: &str) -> MobileRideIdDto {
    MobileRideIdDto {
        bytes: uuid::Uuid::parse_str(value)
            .expect("ride identifier is a UUID")
            .into_bytes()
            .to_vec(),
    }
}

struct Fixture {
    db: Arc<RideDatabaseHandle>,
    core: Arc<MobileRideMapCore>,
    id: MobileRideIdDto,
    path: std::path::PathBuf,
    _guard: MutexGuard<'static, ()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.db.shutdown();
        let _ = std::fs::remove_file(&self.path);
    }
}

fn setup() -> Fixture {
    setup_policy(Some(MobileMusicHistoryPolicyDto::HumanReadable))
}

fn setup_policy(policy: Option<MobileMusicHistoryPolicyDto>) -> Fixture {
    let guard = DATABASE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let path =
        std::env::temp_dir().join(format!("cutout-music-test-{}.sqlite", uuid::Uuid::new_v4()));
    let db = open_ride_database(path.to_string_lossy().into_owned()).unwrap();
    let core = MobileRideMapCore::with_database(db.clone());
    core.restore(1_000).unwrap();
    let started = core.start_gps_only(1_000).unwrap();
    if let Some(policy) = policy {
        core.set_music_history_policy(policy).unwrap();
    }
    Fixture {
        db,
        core,
        id: mobile_ride_id(&started.ride_id),
        path,
        _guard: guard,
    }
}

fn snapshot(observed_at_ms: u64) -> MobileMusicSnapshotDto {
    MobileMusicSnapshotDto {
        provider: MobileMusicProviderDto::Spotify,
        session_id: "session".into(),
        state: MobileMusicPlaybackStateDto::Playing,
        item: Some(MobileMusicItemDto {
            identifier: "track".into(),
            title: Some("Private title".into()),
            artist: Some("Artist".into()),
        }),
        position_milliseconds: Some(10),
        duration_milliseconds: Some(100),
        observed_at_ms,
        capabilities: MobileMusicCapabilitiesDto {
            previous: true,
            play: true,
            pause: true,
            next: true,
            open_provider: true,
        },
    }
}

fn record(
    core: &MobileRideMapCore,
    observed: u64,
    event: u64,
    kind: MobileMusicRideEventKindDto,
) -> Result<MobileMusicTimelineOutcomeDto, MobileRideMapCoreErrorDto> {
    core.record_music_event(
        snapshot(observed),
        kind,
        event,
        1_700_000_000_000 + event,
        5,
    )
}

fn poll_music_command(
    command: &MobileRideMapMusicCommand,
) -> Result<MobileMusicTimelineRecordResultDto, MobileRideMapCoreErrorDto> {
    loop {
        match command.poll()? {
            MobileRideMapMusicPollDto::Pending => std::thread::yield_now(),
            MobileRideMapMusicPollDto::Completed { result } => return Ok(result),
        }
    }
}

fn poll_music_history_command(
    command: &MobileRideMapMusicHistoryCommand,
) -> Result<Option<MobileMusicHistoryDto>, MobileRideMapCoreErrorDto> {
    loop {
        match command.poll()? {
            MobileRideMapMusicHistoryPollDto::Pending => std::thread::yield_now(),
            MobileRideMapMusicHistoryPollDto::Completed { history } => return Ok(history),
        }
    }
}

fn poll_music_policy_command(
    command: &MobileRideMapMusicPolicyCommand,
) -> Result<(), MobileRideMapCoreErrorDto> {
    loop {
        match command.poll()? {
            MobileRideMapMusicPolicyPollDto::Pending => std::thread::yield_now(),
            MobileRideMapMusicPolicyPollDto::Completed => return Ok(()),
        }
    }
}

#[test]
fn queued_music_event_returns_durable_sequence_and_applies_current_privacy_policy() {
    let fixture = setup();
    let command = fixture
        .core
        .begin_record_music_event(
            snapshot(2_000),
            MobileMusicRideEventKindDto::Play,
            2_000,
            1_700_000_002_000,
            5,
        )
        .unwrap();
    let result = poll_music_command(&command).unwrap();
    assert_eq!(result.outcome, MobileMusicTimelineOutcomeDto::Recorded);
    assert_eq!(result.sequence, Some(0));
    let stored = fixture.db.music_events(fixture.id.clone()).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].title.as_deref(), Some("Private title"));

    let policy = fixture
        .core
        .begin_set_music_history_policy(MobileMusicHistoryPolicyDto::OpaqueItem)
        .unwrap();
    let command = fixture
        .core
        .begin_record_music_event(
            snapshot(3_000),
            MobileMusicRideEventKindDto::Pause,
            3_000,
            1_700_000_003_000,
            5,
        )
        .unwrap();
    poll_music_policy_command(&policy).unwrap();
    let result = poll_music_command(&command).unwrap();
    assert_eq!(result.outcome, MobileMusicTimelineOutcomeDto::Recorded);
    assert_eq!(result.sequence, Some(1));
    let stored = fixture.db.music_events(fixture.id.clone()).unwrap();
    assert!(stored.iter().all(|event| event.title.is_none()));
    assert!(stored.iter().all(|event| event.artist.is_none()));

    let history = poll_music_history_command(&fixture.core.begin_current_music_history().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(history.status, MobileMusicHistoryStatusDto::Redacted);
    assert_eq!(history.events.len(), 2);
}

#[test]
fn external_redaction_reaches_active_timeline() {
    let fixture = setup();
    let (db, core, id) = (&fixture.db, &fixture.core, fixture.id.clone());
    record(core, 2_000, 2_000, MobileMusicRideEventKindDto::Play).unwrap();
    db.save_music_history_policy(id.clone(), MobileMusicHistoryPolicyDto::OpaqueItem)
        .unwrap();
    assert!(db.music_events(id).unwrap()[0].title.is_none());
    assert!(
        core.current_music_events().unwrap()[0].title.is_none(),
        "active timeline still discloses redacted title"
    );
}

#[test]
fn forget_then_reenable_records_from_empty_sequence() {
    let fixture = setup();
    let (db, core, id) = (&fixture.db, &fixture.core, fixture.id.clone());
    record(core, 2_000, 2_000, MobileMusicRideEventKindDto::Play).unwrap();
    db.delete_music_history(id.clone()).unwrap();
    assert!(db.music_events(id).unwrap().is_empty());
    core.set_music_history_policy(MobileMusicHistoryPolicyDto::HumanReadable)
        .unwrap();
    assert_eq!(
        record(core, 3_000, 3_000, MobileMusicRideEventKindDto::Pause).unwrap(),
        MobileMusicTimelineOutcomeDto::Recorded
    );
}

#[test]
fn explicitly_stale_observation_is_not_a_fresh_play() {
    let fixture = setup();
    let core = &fixture.core;
    let mut stale = snapshot(100);
    stale.state = MobileMusicPlaybackStateDto::Stale;
    let outcome = core
        .record_music_event(
            stale,
            MobileMusicRideEventKindDto::Play,
            1_000_000,
            1_700_001_000_000,
            5,
        )
        .unwrap();
    assert_eq!(
        outcome,
        MobileMusicTimelineOutcomeDto::OutOfOrder,
        "stale observations must be refused as out of order"
    );
}

#[test]
fn timeline_rejects_event_before_ride_start() {
    let fixture = setup();
    let core = &fixture.core;
    assert_eq!(
        record(core, 500, 500, MobileMusicRideEventKindDto::Play).unwrap(),
        MobileMusicTimelineOutcomeDto::OutOfOrder
    );
}

#[test]
fn restoring_core_preserves_observation_watermark() {
    let fixture = setup();
    let (db, core) = (&fixture.db, &fixture.core);
    record(core, 2_000, 5_000, MobileMusicRideEventKindDto::Play).unwrap();
    let restored = MobileRideMapCore::with_database(db.clone());
    restored.restore(6_000).unwrap();
    assert!(restored.initialization_error().is_none());
    assert_eq!(
        record(&restored, 3_000, 6_000, MobileMusicRideEventKindDto::Pause).unwrap(),
        MobileMusicTimelineOutcomeDto::Recorded
    );
}
#[test]
fn history_status_survives_core_recreation_and_privacy_changes() {
    let fixture = setup_policy(None);
    let history = || fixture.db.music_history(fixture.id.clone()).unwrap();
    assert_eq!(history().status, MobileMusicHistoryStatusDto::Missing);
    fixture
        .core
        .set_music_history_policy(MobileMusicHistoryPolicyDto::Disabled)
        .unwrap();
    assert_eq!(history().status, MobileMusicHistoryStatusDto::Disabled);
    fixture
        .core
        .set_music_history_policy(MobileMusicHistoryPolicyDto::HumanReadable)
        .unwrap();
    record(
        &fixture.core,
        2_000,
        3_000,
        MobileMusicRideEventKindDto::Play,
    )
    .unwrap();
    assert_eq!(history().events[0].observed_at_ms, Some(2_000));
    fixture
        .db
        .save_music_history_policy(fixture.id.clone(), MobileMusicHistoryPolicyDto::OpaqueItem)
        .unwrap();
    assert_eq!(history().status, MobileMusicHistoryStatusDto::Redacted);
    fixture.db.delete_music_history(fixture.id.clone()).unwrap();
    assert!(fixture.core.current_music_events().unwrap().is_empty());
    let restored = MobileRideMapCore::with_database(fixture.db.clone());
    restored.restore(4_000).unwrap();
    assert_eq!(
        restored.current_music_history().unwrap().status,
        MobileMusicHistoryStatusDto::Deleted
    );
    assert!(history().events.is_empty());
}

#[test]
fn unavailable_history_is_not_reported_as_missing() {
    let core = MobileRideMapCore::new();
    core.start_gps_only(1_000).unwrap();
    assert_eq!(
        core.current_music_history().unwrap().status,
        MobileMusicHistoryStatusDto::Unavailable
    );
}

#[test]
fn duplicate_observation_watermark_survives_core_recreation() {
    let fixture = setup();
    record(
        &fixture.core,
        2_000,
        2_000,
        MobileMusicRideEventKindDto::Play,
    )
    .unwrap();
    assert_eq!(
        record(
            &fixture.core,
            4_000,
            5_000,
            MobileMusicRideEventKindDto::Play
        )
        .unwrap(),
        MobileMusicTimelineOutcomeDto::Duplicate
    );
    let restored = MobileRideMapCore::with_database(fixture.db.clone());
    restored.restore(6_000).unwrap();
    assert_eq!(
        record(&restored, 3_000, 6_000, MobileMusicRideEventKindDto::Pause).unwrap(),
        MobileMusicTimelineOutcomeDto::OutOfOrder
    );
    assert_eq!(restored.current_music_events().unwrap().len(), 1);
}

#[test]
fn stale_core_cannot_write_after_durable_ride_stop() {
    let fixture = setup();
    let controller = MobileRideMapCore::with_database(fixture.db.clone());
    controller.restore(2_500).unwrap();
    controller.stop_at(2_500).unwrap();
    assert_eq!(
        fixture
            .db
            .find_ride(fixture.id.clone())
            .unwrap()
            .unwrap()
            .state,
        MobileRideLifecycleStateDto::Stopped
    );
    assert_eq!(
        record(
            &fixture.core,
            3_000,
            3_000,
            MobileMusicRideEventKindDto::Play
        ),
        Err(MobileRideMapCoreErrorDto::InvalidTransition)
    );
}

#[test]
fn pre_ride_observation_is_rejected_even_when_event_is_delayed() {
    let fixture = setup();
    assert_eq!(
        record(&fixture.core, 500, 2_000, MobileMusicRideEventKindDto::Skip).unwrap(),
        MobileMusicTimelineOutcomeDto::OutOfOrder
    );
    assert!(fixture.core.current_music_events().unwrap().is_empty());
}

#[test]
fn malformed_duplicate_cannot_poison_observation_watermark() {
    let fixture = setup();
    record(
        &fixture.core,
        2_000,
        2_000,
        MobileMusicRideEventKindDto::Play,
    )
    .unwrap();
    let malformed = fixture.core.record_music_event(
        snapshot(2_500),
        MobileMusicRideEventKindDto::Play,
        u64::MAX,
        u64::MAX,
        u64::MAX,
    );
    assert!(malformed.is_err());
    assert_eq!(
        record(
            &fixture.core,
            2_200,
            4_000,
            MobileMusicRideEventKindDto::Pause,
        )
        .unwrap(),
        MobileMusicTimelineOutcomeDto::Recorded
    );
}

#[test]
fn corrupt_optional_music_does_not_disable_ride_recovery() {
    let guard = DATABASE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let path = std::env::temp_dir().join(format!(
        "cutout-music-recovery-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = open_ride_database(path.to_string_lossy().into_owned()).unwrap();
    let core = MobileRideMapCore::with_database(database.clone());
    core.restore(1_000).unwrap();
    let started = core.start_gps_only(1_000).unwrap();
    let ride_id = MobileRideIdDto {
        bytes: uuid::Uuid::parse_str(&started.ride_id)
            .unwrap()
            .into_bytes()
            .to_vec(),
    };
    core.set_music_history_policy(MobileMusicHistoryPolicyDto::HumanReadable)
        .unwrap();
    core.record_music_event(
        snapshot(2_000),
        MobileMusicRideEventKindDto::Play,
        3_000,
        1_700_000_003_000,
        5,
    )
    .unwrap();
    database.shutdown().unwrap();
    drop(core);
    drop(database);
    let connection = rusqlite::Connection::open(&path).unwrap();
    let invalid_identifier = " ";
    connection
        .execute(
            "UPDATE ride_music_event SET item_identifier = ?1",
            [invalid_identifier],
        )
        .unwrap();
    drop(connection);
    let reopened = open_ride_database(path.to_string_lossy().into_owned()).unwrap();
    let restored = MobileRideMapCore::with_database(reopened.clone());
    restored.restore(4_000).unwrap();
    assert!(restored.initialization_error().is_none());
    assert!(restored.current_snapshot(4_000).is_some());
    assert_eq!(
        restored.current_music_history().unwrap().status,
        MobileMusicHistoryStatusDto::Unavailable
    );
    reopened.delete_music_history(ride_id.clone()).unwrap();
    assert_eq!(
        restored.current_music_history().unwrap().status,
        MobileMusicHistoryStatusDto::Deleted
    );
    restored
        .set_music_history_policy(MobileMusicHistoryPolicyDto::HumanReadable)
        .unwrap();
    assert_eq!(
        record(&restored, 6_000, 6_000, MobileMusicRideEventKindDto::Play,).unwrap(),
        MobileMusicTimelineOutcomeDto::Recorded
    );
    reopened.shutdown().unwrap();
    std::fs::remove_file(path).unwrap();
    drop(guard);
}

#[test]
fn stale_core_cannot_change_policy_after_durable_ride_stop() {
    let fixture = setup();
    record(
        &fixture.core,
        2_000,
        2_000,
        MobileMusicRideEventKindDto::Play,
    )
    .unwrap();
    let controller = MobileRideMapCore::with_database(fixture.db.clone());
    controller.restore(3_000).unwrap();
    controller.stop_at(3_000).unwrap();
    assert_eq!(
        fixture
            .core
            .set_music_history_policy(MobileMusicHistoryPolicyDto::Disabled),
        Err(MobileRideMapCoreErrorDto::InvalidTransition)
    );
    assert_eq!(
        fixture.db.music_events(fixture.id.clone()).unwrap().len(),
        1
    );
}

#[test]
fn historical_redaction_does_not_opt_in_an_interrupted_ride() {
    let fixture = setup_policy(Some(MobileMusicHistoryPolicyDto::Disabled));
    fixture.core.interrupt_at(2_000).unwrap();
    let result = fixture
        .db
        .save_music_history_policy(fixture.id.clone(), MobileMusicHistoryPolicyDto::OpaqueItem);
    assert!(
        result.is_err(),
        "historical opt-in must be rejected: {result:?}"
    );
    assert_eq!(
        fixture.db.music_history(fixture.id.clone()).unwrap().status,
        MobileMusicHistoryStatusDto::Disabled
    );
}

#[test]
fn failed_durable_event_does_not_consume_sequence_or_observation() {
    let fixture = setup();
    let result = fixture.core.record_music_event(
        snapshot(4_000),
        MobileMusicRideEventKindDto::Play,
        4_000,
        u64::MAX,
        5,
    );
    assert!(result.is_err());
    assert!(fixture.core.current_music_events().unwrap().is_empty());
    assert_eq!(
        record(
            &fixture.core,
            2_000,
            2_000,
            MobileMusicRideEventKindDto::Pause
        )
        .unwrap(),
        MobileMusicTimelineOutcomeDto::Recorded
    );
}
