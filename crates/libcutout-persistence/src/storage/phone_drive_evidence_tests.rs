use std::any::TypeId;

use super::{RideDatabase, test_guard};
use crate::PhoneDriveSettingsRecord;
use cutout_core::{
    MonotonicTimestamp, PhoneControlMode, PhoneDriveAction, PhoneDriveContactId,
    PhoneDriveEvidence, PhoneDriveFact, PhoneDriveFactPhase, PhoneDriveLimits, PhoneDriveWrite,
};
use rusqlite::Connection;

trait RecordedFact {}

impl RecordedFact for PhoneDriveEvidence {}

impl<Phase: PhoneDriveFactPhase> RecordedFact for PhoneDriveFact<Phase> {}

fn assert_recorded_fact<T: RecordedFact + 'static>(value: &T) {
    assert_ne!(
        TypeId::of::<T>(),
        TypeId::of::<PhoneDriveWrite>(),
        "loaded phone-drive evidence is a recorded fact, not a live write"
    );
    let _ = value;
}

fn contact(id: u64) -> PhoneDriveContactId {
    PhoneDriveContactId::new(id).expect("contact id")
}

#[test]
fn phone_drive_evidence_insert_reloads_in_order_and_is_not_a_write() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-drive-evidence.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let disabled = PhoneDriveSettingsRecord::default();
    assert_eq!(
        database.phone_drive_preferences("board-a").unwrap(),
        disabled
    );

    let facts = [
        PhoneDriveEvidence::intent(
            PhoneDriveAction::Throttle,
            contact(1),
            MonotonicTimestamp::new(10),
        ),
        PhoneDriveEvidence::observed_effect(
            PhoneDriveAction::Brake,
            contact(4),
            MonotonicTimestamp::new(5),
        ),
        PhoneDriveEvidence::admitted(
            PhoneDriveAction::Throttle,
            contact(1),
            MonotonicTimestamp::new(11),
        ),
        PhoneDriveEvidence::native_receipt(
            PhoneDriveAction::Throttle,
            contact(1),
            MonotonicTimestamp::new(12),
        ),
    ];
    let other_board = PhoneDriveEvidence::intent(
        PhoneDriveAction::Brake,
        contact(9),
        MonotonicTimestamp::new(8),
    );
    for evidence in facts {
        database
            .record_phone_drive_evidence("board-a", evidence)
            .unwrap();
    }
    database
        .record_phone_drive_evidence("board-b", other_board)
        .unwrap();
    assert_eq!(
        database.phone_drive_preferences("board-a").unwrap(),
        disabled
    );
    assert_eq!(
        database.phone_drive_preferences("board-b").unwrap(),
        disabled
    );

    let loaded: Vec<PhoneDriveEvidence> = database.phone_drive_evidence("board-a").unwrap();
    assert_eq!(loaded, facts);
    assert_recorded_facts(&loaded);
    assert_eq!(
        database.phone_drive_evidence("board-b").unwrap(),
        vec![other_board]
    );
    database.shutdown().unwrap();

    let database = RideDatabase::open(&path).unwrap();
    let reloaded: Vec<PhoneDriveEvidence> = database.phone_drive_evidence("board-a").unwrap();
    assert_eq!(reloaded, facts);
    assert_recorded_facts(&reloaded);
    assert_eq!(
        database.phone_drive_preferences("board-a").unwrap(),
        disabled
    );
    database.shutdown().unwrap();
}

#[test]
fn phone_drive_evidence_is_absent_when_the_table_has_no_rows() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone-drive-evidence-empty.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let loaded: Vec<PhoneDriveEvidence> = database.phone_drive_evidence("board-a").unwrap();
    assert_eq!(loaded, Vec::new());
    assert_recorded_facts(&loaded);
    assert_eq!(
        database.phone_drive_preferences("board-a").unwrap(),
        PhoneDriveSettingsRecord::default()
    );

    let layout = PhoneDriveSettingsRecord::new(
        PhoneControlMode::Buttons,
        PhoneDriveLimits::new(7_125, 4_500).unwrap(),
    );
    database
        .save_phone_drive_preferences("board-a", layout)
        .unwrap();
    assert_eq!(
        database.phone_drive_evidence("board-a").unwrap(),
        Vec::new()
    );
    database.shutdown().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE phone_drive_evidence;")
        .unwrap();
    drop(connection);

    let database = RideDatabase::open(&path).unwrap();
    let loaded: Vec<PhoneDriveEvidence> = database.phone_drive_evidence("board-a").unwrap();
    assert_eq!(loaded, Vec::new());
    assert_recorded_facts(&loaded);
    assert_eq!(database.phone_drive_preferences("board-a").unwrap(), layout);
    database.shutdown().unwrap();
}

fn assert_recorded_facts(facts: &[PhoneDriveEvidence]) {
    assert_ne!(
        TypeId::of::<Vec<PhoneDriveEvidence>>(),
        TypeId::of::<PhoneDriveWrite>()
    );
    for fact in facts {
        assert_recorded_fact(fact);
        match fact {
            PhoneDriveEvidence::Intent(fact) => assert_recorded_fact(fact),
            PhoneDriveEvidence::Admitted(fact) => assert_recorded_fact(fact),
            PhoneDriveEvidence::NativeReceipt(fact) => assert_recorded_fact(fact),
            PhoneDriveEvidence::ObservedEffect(fact) => assert_recorded_fact(fact),
        }
    }
}
