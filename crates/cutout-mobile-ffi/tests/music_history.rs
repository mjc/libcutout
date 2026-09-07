use cutout_mobile_ffi::*;
use std::sync::{Arc, Mutex, MutexGuard};
static DATABASE_LOCK: Mutex<()> = Mutex::new(());
struct Fixture {
    db: Arc<RideDatabaseHandle>,
    core: Arc<MobileRideMapCore>,
    id: MobileRideIdDto,
    path: std::path::PathBuf,
    _guard: MutexGuard<'static, ()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.db.shutdown().expect("database shuts down");
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
    let started = core.start_gps_only(1_000, None).unwrap();
    if let Some(policy) = policy {
        core.set_music_history_policy(policy).unwrap();
    }
    Fixture {
        db,
        core,
        id: MobileRideIdDto {
            value: started.ride_id,
        },
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
    assert_ne!(
        outcome,
        MobileMusicTimelineOutcomeDto::Recorded,
        "stale pre-ride snapshot became fresh play with no freshness metadata"
    );
}

#[test]
fn timeline_rejects_event_before_ride_start() {
    let fixture = setup();
    let core = &fixture.core;
    assert_ne!(
        record(core, 500, 500, MobileMusicRideEventKindDto::Play).unwrap(),
        MobileMusicTimelineOutcomeDto::Recorded
    );
}

#[test]
fn restoring_core_preserves_observation_watermark() {
    let fixture = setup();
    let (db, core) = (&fixture.db, &fixture.core);
    record(core, 2_000, 5_000, MobileMusicRideEventKindDto::Play).unwrap();
    let restored = MobileRideMapCore::with_database(db.clone());
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
    assert_eq!(
        restored.current_music_history().unwrap().status,
        MobileMusicHistoryStatusDto::Deleted
    );
    assert!(history().events.is_empty());
}

#[test]
fn unavailable_history_is_not_reported_as_missing() {
    let core = MobileRideMapCore::new();
    core.start_gps_only(1_000, None).unwrap();
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
    assert_eq!(
        record(&restored, 3_000, 6_000, MobileMusicRideEventKindDto::Pause).unwrap(),
        MobileMusicTimelineOutcomeDto::OutOfOrder
    );
    assert_eq!(restored.current_music_events().unwrap().len(), 1);
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
