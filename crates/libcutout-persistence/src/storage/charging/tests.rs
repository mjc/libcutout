use super::*;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    super::super::migrations::migrate(&mut connection).unwrap();
    connection
}
fn input(longitude: f64) -> NewChargingSpot {
    NewChargingSpot::new(
        Coordinate::from_degrees(0.0, longitude).unwrap(),
        "Outside outlet",
        ChargingCost::Free,
    )
    .unwrap()
}
fn entry(
    input: Option<NewChargingSpot>,
    status: ChargingStatus,
    revision: u64,
) -> ChargingDirectoryEntry {
    ChargingDirectoryEntry {
        id: ChargingSpotId::new(),
        input,
        status,
        revision: ChargingRevision(revision),
        change_sequence: ChargingRevision(revision),
        verified_at_ms: None,
        vote: None,
        vote_revision: ChargingRevision(0),
        votes_up: 0,
        votes_down: 0,
    }
}
fn bounds() -> GeoBounds {
    GeoBounds::new(-1.0, 1.0, 179.0, -179.0).unwrap()
}
#[test]
fn charging_write_failure_rolls_back_pin_and_spatial_keys() {
    let mut connection = database();
    connection.authorizer(Some(|context: AuthContext<'_>| match context.action {
        AuthAction::Insert {
            table_name: "charging_outbox",
        } => Authorization::Deny,
        _ => Authorization::Allow,
    }));
    assert!(
        save(
            &mut connection,
            &mut SpatialSchemaState::Uninitialized,
            &input(179.5)
        )
        .is_err()
    );
    connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    for table in [
        "charging_outbox",
        "charging_spots",
        "map_points",
        "map_point_spatial_keys",
        "map_points_rtree",
    ] {
        assert_eq!(
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
    }
}
#[test]
fn charging_migration_preserves_legacy_and_rolls_back_on_failure() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let id = super::super::create_map_point(
        &mut connection,
        &mut spatial,
        "Original",
        input(179.5).coordinate(),
    )
    .unwrap();
    connection
        .execute_batch(
            "DROP TABLE charging_outbox; DROP TABLE charging_spots; PRAGMA user_version=41;",
        )
        .unwrap();
    connection.authorizer(Some(|context: AuthContext<'_>| match context.action {
        AuthAction::CreateTable {
            table_name: "charging_outbox",
        } => Authorization::Deny,
        _ => Authorization::Allow,
    }));
    assert!(super::super::migrations::migrate(&mut connection).is_err());
    connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    assert!(!super::super::migrations::table_exists(&connection, "charging_spots").unwrap());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        41
    );
    super::super::migrations::migrate(&mut connection).unwrap();
    assert!(detail(&connection, id).unwrap().is_none());
    assert_eq!(count(&connection).unwrap(), 0);
    let points = super::super::map_points_in_bounds(
        &connection,
        &mut spatial,
        bounds(),
        None,
        QueryLimit(10),
    )
    .unwrap();
    assert_eq!(points.points()[0].id, id);
    assert_eq!(points.points()[0].name, "Original");
    assert_eq!(points.points()[0].coordinate, input(179.5).coordinate());
}
#[test]
fn charging_edits_and_withdrawals_keep_ids_and_index_consistent() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let saved = save(&mut connection, &mut spatial, &input(179.5)).unwrap();
    let edited = change(
        &mut connection,
        &mut spatial,
        saved.point_id,
        saved.revision,
        &ChargingAction::Edit(input(-179.5)),
    )
    .unwrap();
    assert_eq!(edited.point_id, saved.point_id);
    assert_eq!(
        page(
            &connection,
            &mut spatial,
            bounds(),
            &ChargingFilter::default(),
            None,
            QueryLimit(1)
        )
        .unwrap()
        .spots[0]
            .input
            .as_ref()
            .unwrap()
            .coordinate(),
        input(-179.5).coordinate()
    );
    assert!(matches!(
        change(
            &mut connection,
            &mut spatial,
            saved.point_id,
            saved.revision,
            &ChargingAction::Withdraw
        ),
        Err(StorageError::ChargingConflict)
    ));
    change(
        &mut connection,
        &mut spatial,
        saved.point_id,
        edited.revision,
        &ChargingAction::Withdraw,
    )
    .unwrap();
    assert_eq!(
        page(
            &connection,
            &mut spatial,
            bounds(),
            &ChargingFilter::default(),
            None,
            QueryLimit(10)
        )
        .unwrap()
        .spots,
        []
    );
    super::super::rebuild_spatial_indexes(&mut connection, &mut spatial).unwrap();
    assert_eq!(
        super::super::map_points_in_bounds(
            &connection,
            &mut spatial,
            bounds(),
            None,
            QueryLimit(10)
        )
        .unwrap()
        .points(),
        []
    );
    assert_eq!(count(&connection).unwrap(), 3);
}
#[test]
fn charging_receipts_bind_remote_identity_without_overwriting_later_edits() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let saved = save(&mut connection, &mut spatial, &input(179.5)).unwrap();
    let operation = next(&mut connection).unwrap().unwrap();
    let edited = change(
        &mut connection,
        &mut spatial,
        saved.point_id,
        saved.revision,
        &ChargingAction::Edit(input(-179.5)),
    )
    .unwrap();
    let snapshot = entry(Some(input(179.5)), ChargingStatus::Published, 1);
    assert!(matches!(
        acknowledge(
            &mut connection,
            &mut spatial,
            edited.operation_id,
            ChargingRevision(1),
            &snapshot
        ),
        Err(StorageError::ChargingConflict)
    ));
    assert_eq!(
        acknowledge(
            &mut connection,
            &mut spatial,
            operation.id,
            ChargingRevision(1),
            &snapshot
        )
        .unwrap(),
        saved.point_id
    );
    assert_eq!(
        detail(&connection, saved.point_id)
            .unwrap()
            .unwrap()
            .input
            .unwrap(),
        input(-179.5)
    );
    let edit = next(&mut connection).unwrap().unwrap();
    assert_eq!(
        edit.target,
        ChargingTarget::Existing {
            id: snapshot.id,
            expected_revision: ChargingRevision(1)
        }
    );
    assert_eq!(next(&mut connection).unwrap().unwrap(), edit);
    let mut snapshot = snapshot;
    snapshot.input = Some(input(-179.5));
    snapshot.revision = ChargingRevision(2);
    snapshot.change_sequence = ChargingRevision(2);
    acknowledge(
        &mut connection,
        &mut spatial,
        edit.id,
        ChargingRevision(2),
        &snapshot,
    )
    .unwrap();
    acknowledge(
        &mut connection,
        &mut spatial,
        edit.id,
        ChargingRevision(2),
        &snapshot,
    )
    .unwrap();
    assert_eq!(count(&connection).unwrap(), 0);
    assert_eq!(
        detail(&connection, saved.point_id).unwrap().unwrap().status,
        ChargingStatus::Published
    );
}
#[test]
fn charging_filters_preserve_supply_pairings_and_paginate_matching_pins() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let supplied = input(179.5)
        .with_details(
            ChargingDetails::new(
                None,
                None,
                None,
                None,
                None,
                vec![
                    ChargingSupply::single(
                        Some(cutout_ride_maps::SupplyVoltage::new(120).unwrap()),
                        ChargingConnector::Nema5_15,
                    ),
                    ChargingSupply::single(
                        Some(cutout_ride_maps::SupplyVoltage::new(240).unwrap()),
                        ChargingConnector::Type2,
                    ),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let first = save(&mut connection, &mut spatial, &supplied).unwrap();
    save(&mut connection, &mut spatial, &input(-179.8)).unwrap();
    let filter = ChargingFilter {
        cost: Some(ChargingCost::Free),
        connector: Some(ChargingConnector::Nema5_15),
        voltage: Some(cutout_ride_maps::SupplyVoltage::new(120).unwrap()),
    };
    assert_eq!(
        page(
            &connection,
            &mut spatial,
            bounds(),
            &filter,
            None,
            QueryLimit(1)
        )
        .unwrap()
        .spots[0]
            .point_id,
        first.point_id
    );
    let wrong = ChargingFilter {
        voltage: Some(cutout_ride_maps::SupplyVoltage::new(240).unwrap()),
        ..filter
    };
    assert_eq!(
        page(
            &connection,
            &mut spatial,
            bounds(),
            &wrong,
            None,
            QueryLimit(1)
        )
        .unwrap()
        .spots,
        []
    );
    let page1 = page(
        &connection,
        &mut spatial,
        bounds(),
        &ChargingFilter::default(),
        None,
        QueryLimit(1),
    )
    .unwrap();
    let page2 = page(
        &connection,
        &mut spatial,
        bounds(),
        &ChargingFilter::default(),
        page1.next_cursor,
        QueryLimit(1),
    )
    .unwrap();
    assert_ne!(page1.spots[0].point_id, page2.spots[0].point_id);
    assert!(page2.next_cursor.is_none());
}
#[test]
fn charging_nearby_ranks_beyond_the_first_candidate_page() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    for _ in 0..501 {
        save(&mut connection, &mut spatial, &input(179.1)).unwrap();
    }
    let nearest = save(&mut connection, &mut spatial, &input(-179.99)).unwrap();
    let values = nearby(
        &connection,
        &mut spatial,
        bounds(),
        Coordinate::from_degrees(0.0, 180.0).unwrap(),
        &ChargingFilter::default(),
        QueryLimit(1),
    )
    .unwrap();
    assert_eq!(values[0].point_id, nearest.point_id);
}
#[test]
fn charging_queue_limit_rejects_save_and_edit_atomically() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let saved = save(&mut connection, &mut spatial, &input(179.5)).unwrap();
    for _ in 1..CHARGING_OUTBOX_CAPACITY {
        let revision = detail(&connection, saved.point_id)
            .unwrap()
            .unwrap()
            .local_revision;
        change(
            &mut connection,
            &mut spatial,
            saved.point_id,
            revision,
            &ChargingAction::Vote(Some(ChargingVote::Up)),
        )
        .unwrap();
    }
    let before = detail(&connection, saved.point_id).unwrap().unwrap();
    assert!(matches!(
        save(&mut connection, &mut spatial, &input(-179.5)),
        Err(StorageError::ChargingOutboxFull)
    ));
    assert!(matches!(
        change(
            &mut connection,
            &mut spatial,
            saved.point_id,
            before.local_revision,
            &ChargingAction::Edit(input(-179.5))
        ),
        Err(StorageError::ChargingOutboxFull)
    ));
    assert_eq!(
        detail(&connection, saved.point_id).unwrap().unwrap(),
        before
    );
    assert_eq!(count(&connection).unwrap(), CHARGING_OUTBOX_CAPACITY);
}
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "Exercises the cache, vote, verification and removal lifecycle together"
)]
fn charging_directory_cache_vote_verification_and_tombstone_use_one_owner() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let snapshot = entry(Some(input(179.5)), ChargingStatus::Published, 5);
    let id = cache(&mut connection, &mut spatial, &snapshot).unwrap();
    assert_eq!(id.uuid(), snapshot.id.as_uuid());
    assert!(!detail(&connection, id).unwrap().unwrap().owned);
    assert!(matches!(
        change(
            &mut connection,
            &mut spatial,
            id,
            ChargingRevision(1),
            &ChargingAction::Edit(input(-179.5))
        ),
        Err(StorageError::ChargingNotOwned)
    ));
    let vote = change(
        &mut connection,
        &mut spatial,
        id,
        ChargingRevision(1),
        &ChargingAction::Vote(Some(ChargingVote::Down)),
    )
    .unwrap();
    assert_eq!(detail(&connection, id).unwrap().unwrap().votes_down, 0);
    assert!(matches!(
        cache(&mut connection, &mut spatial, &snapshot),
        Err(StorageError::ChargingConflict)
    ));
    assert_eq!(
        next(&mut connection).unwrap().unwrap().target,
        ChargingTarget::Existing {
            id: snapshot.id,
            expected_revision: ChargingRevision(0)
        }
    );
    let mut snapshot = snapshot;
    snapshot.vote = Some(ChargingVote::Down);
    snapshot.vote_revision = ChargingRevision(1);
    snapshot.votes_down = 1;
    snapshot.change_sequence = ChargingRevision(6);
    acknowledge(
        &mut connection,
        &mut spatial,
        vote.operation_id,
        ChargingRevision(1),
        &snapshot,
    )
    .unwrap();
    let current = detail(&connection, id).unwrap().unwrap();
    let verified = change(
        &mut connection,
        &mut spatial,
        id,
        current.local_revision,
        &ChargingAction::Verify,
    )
    .unwrap();
    assert!(
        detail(&connection, id)
            .unwrap()
            .unwrap()
            .verified_at_ms
            .is_none()
    );
    next(&mut connection).unwrap();
    snapshot.revision = ChargingRevision(6);
    snapshot.change_sequence = ChargingRevision(7);
    snapshot.verified_at_ms = Some(123);
    acknowledge(
        &mut connection,
        &mut spatial,
        verified.operation_id,
        ChargingRevision(6),
        &snapshot,
    )
    .unwrap();
    assert_eq!(
        detail(&connection, id).unwrap().unwrap().verified_at_ms,
        Some(123)
    );
    let mut tombstone = snapshot;
    tombstone.status = ChargingStatus::Removed;
    tombstone.input = None;
    tombstone.verified_at_ms = None;
    tombstone.revision = ChargingRevision(7);
    tombstone.change_sequence = ChargingRevision(8);
    cache(&mut connection, &mut spatial, &tombstone).unwrap();
    assert!(detail(&connection, id).unwrap().unwrap().input.is_none());
    super::super::rebuild_spatial_indexes(&mut connection, &mut spatial).unwrap();
    assert_eq!(
        page(
            &connection,
            &mut spatial,
            bounds(),
            &ChargingFilter::default(),
            None,
            QueryLimit(10)
        )
        .unwrap()
        .spots,
        []
    );
}

#[test]
fn failed_edit_queue_insert_rolls_back_input_revision_and_spatial_move() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let saved = save(&mut connection, &mut spatial, &input(179.5)).unwrap();
    let queued = next(&mut connection).unwrap().unwrap();
    let before = detail(&connection, saved.point_id).unwrap().unwrap();
    connection.authorizer(Some(|context: AuthContext<'_>| match context.action {
        AuthAction::Insert {
            table_name: "charging_outbox",
        } => Authorization::Deny,
        _ => Authorization::Allow,
    }));
    assert!(
        change(
            &mut connection,
            &mut spatial,
            saved.point_id,
            saved.revision,
            &ChargingAction::Edit(input(-179.5)),
        )
        .is_err()
    );
    connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    assert_eq!(
        detail(&connection, saved.point_id).unwrap().unwrap(),
        before
    );
    assert_eq!(count(&connection).unwrap(), 1);
    assert_eq!(next(&mut connection).unwrap().unwrap(), queued);
    assert_eq!(
        page(
            &connection,
            &mut spatial,
            bounds(),
            &ChargingFilter::default(),
            None,
            QueryLimit(10),
        )
        .unwrap()
        .spots[0]
            .input
            .as_ref()
            .unwrap()
            .coordinate(),
        input(179.5).coordinate()
    );
}

#[test]
fn withdrawal_before_create_ack_freezes_remote_target_after_create_ack() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let saved = save(&mut connection, &mut spatial, &input(179.5)).unwrap();
    let withdrawal = change(
        &mut connection,
        &mut spatial,
        saved.point_id,
        saved.revision,
        &ChargingAction::Withdraw,
    )
    .unwrap();
    let create = next(&mut connection).unwrap().unwrap();
    assert_eq!(create.target, ChargingTarget::Create);
    let published = entry(Some(input(179.5)), ChargingStatus::Published, 1);
    acknowledge(
        &mut connection,
        &mut spatial,
        create.id,
        ChargingRevision(1),
        &published,
    )
    .unwrap();
    let operation = next(&mut connection).unwrap().unwrap();
    assert_eq!(operation.id, withdrawal.operation_id);
    assert_eq!(
        operation.target,
        ChargingTarget::Existing {
            id: published.id,
            expected_revision: ChargingRevision(1),
        }
    );
    let mut withdrawn = published;
    withdrawn.status = ChargingStatus::Withdrawn;
    withdrawn.input = None;
    withdrawn.revision = ChargingRevision(2);
    withdrawn.change_sequence = ChargingRevision(2);
    acknowledge(
        &mut connection,
        &mut spatial,
        operation.id,
        ChargingRevision(2),
        &withdrawn,
    )
    .unwrap();
    let stored = detail(&connection, saved.point_id).unwrap().unwrap();
    assert_eq!(stored.status, ChargingStatus::Withdrawn);
    assert!(stored.input.is_none());
    assert_eq!(stored.remote_id, Some(withdrawn.id));
}

#[test]
fn stale_directory_cache_does_not_change_cached_body_or_index() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let current = entry(Some(input(179.5)), ChargingStatus::Published, 5);
    let id = cache(&mut connection, &mut spatial, &current).unwrap();
    let mut stale = current.clone();
    stale.input = Some(input(-179.5));
    stale.revision = ChargingRevision(4);
    stale.change_sequence = ChargingRevision(4);
    assert!(matches!(
        cache(&mut connection, &mut spatial, &stale),
        Err(StorageError::ChargingConflict)
    ));
    assert_eq!(
        detail(&connection, id).unwrap().unwrap().input,
        current.input
    );
    assert_eq!(
        page(
            &connection,
            &mut spatial,
            bounds(),
            &ChargingFilter::default(),
            None,
            QueryLimit(10),
        )
        .unwrap()
        .spots[0]
            .input
            .as_ref()
            .unwrap()
            .coordinate(),
        input(179.5).coordinate()
    );
}

#[test]
fn older_create_receipt_cannot_restore_verification_after_local_edit() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let saved = save(&mut connection, &mut spatial, &input(179.5)).unwrap();
    let create = next(&mut connection).unwrap().unwrap();
    let mut created = entry(Some(input(179.5)), ChargingStatus::Published, 1);
    created.verified_at_ms = Some(100);
    let edited = change(
        &mut connection,
        &mut spatial,
        saved.point_id,
        saved.revision,
        &ChargingAction::Edit(input(-179.5)),
    )
    .unwrap();
    assert_eq!(
        detail(&connection, saved.point_id)
            .unwrap()
            .unwrap()
            .verified_at_ms,
        None
    );
    acknowledge(
        &mut connection,
        &mut spatial,
        create.id,
        ChargingRevision(1),
        &created,
    )
    .unwrap();
    let stored = detail(&connection, saved.point_id).unwrap().unwrap();
    assert_eq!(stored.verified_at_ms, None);
    assert_eq!(stored.input, Some(input(-179.5)));
    assert_eq!(
        next(&mut connection).unwrap().unwrap().id,
        edited.operation_id
    );
}

#[test]
fn other_connector_label_matches_discovery_filter() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let connector =
        ChargingConnector::Other(cutout_ride_maps::ConnectorLabel::new("Socket X").unwrap());
    let spot = input(179.5)
        .with_details(
            ChargingDetails::new(
                None,
                None,
                None,
                None,
                None,
                vec![ChargingSupply::single(None, connector.clone())],
            )
            .unwrap(),
        )
        .unwrap();
    let saved = save(&mut connection, &mut spatial, &spot).unwrap();
    let page = page(
        &connection,
        &mut spatial,
        bounds(),
        &ChargingFilter {
            connector: Some(connector),
            ..ChargingFilter::default()
        },
        None,
        QueryLimit(10),
    )
    .unwrap();
    assert_eq!(page.spots.len(), 1);
    assert_eq!(page.spots[0].point_id, saved.point_id);
}

#[test]
fn hidden_directory_snapshot_without_body_removes_index_and_keeps_identity() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(179.5)), ChargingStatus::Published, 5);
    let id = cache(&mut connection, &mut spatial, &published).unwrap();
    let mut hidden = published.clone();
    hidden.input = None;
    hidden.status = ChargingStatus::Hidden;
    hidden.revision = ChargingRevision(6);
    hidden.change_sequence = ChargingRevision(6);
    assert_eq!(cache(&mut connection, &mut spatial, &hidden).unwrap(), id);
    let stored = detail(&connection, id).unwrap().unwrap();
    assert_eq!(stored.remote_id, Some(hidden.id));
    assert_eq!(stored.status, ChargingStatus::Hidden);
    assert!(stored.input.is_none());
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM map_points_rtree", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
}

#[test]
fn directory_cache_rejects_malformed_status_body_pairs() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published_without_body = entry(None, ChargingStatus::Published, 1);
    assert!(matches!(
        cache(&mut connection, &mut spatial, &published_without_body),
        Err(StorageError::ChargingInvalidInput)
    ));
    let hidden_with_body = entry(Some(input(179.5)), ChargingStatus::Hidden, 1);
    assert!(matches!(
        cache(&mut connection, &mut spatial, &hidden_with_body),
        Err(StorageError::ChargingInvalidInput)
    ));
    assert_eq!(count(&connection).unwrap(), 0);
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM charging_spots", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
}

#[test]
fn directory_cache_rejects_collision_with_legacy_map_point_uuid() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let collision = entry(Some(input(179.5)), ChargingStatus::Published, 1);
    connection
        .execute(
            "INSERT INTO map_points(id,name,latitude_e7,longitude_e7) VALUES (?1,'Legacy',0,0)",
            [collision.id.as_uuid().as_bytes()],
        )
        .unwrap();
    assert!(matches!(
        cache(&mut connection, &mut spatial, &collision),
        Err(StorageError::ChargingConflict)
    ));
    assert_eq!(
        connection
            .query_row(
                "SELECT name FROM map_points WHERE id=?1",
                [collision.id.as_uuid().as_bytes()],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "Legacy"
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM charging_spots", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
}

#[test]
fn corrupt_persisted_charging_inputs_are_invalid_stored_values() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let saved = save(&mut connection, &mut spatial, &input(179.5)).unwrap();
    connection
        .execute(
            "UPDATE charging_spots SET input=json_set(input,'$.access_note','') WHERE point_id=?1",
            [saved.point_id.uuid().as_bytes()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE charging_outbox SET action=json_set(action,'$.Create.access_note','')",
            [],
        )
        .unwrap();
    assert!(matches!(
        detail(&connection, saved.point_id),
        Err(StorageError::InvalidStoredValue {
            field: "charging input",
            ..
        })
    ));
    assert!(matches!(
        next(&mut connection),
        Err(StorageError::InvalidStoredValue {
            field: "charging input",
            ..
        })
    ));
}
