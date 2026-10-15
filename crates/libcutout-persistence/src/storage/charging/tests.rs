use super::records::{detail, find_remote};
use super::sync::{
    acknowledge_vote_operation, apply_changes_page, block_operation, changes_cursor,
    directory_last_updated, list_blocked_operations, replace_blocked_operation, resolve_operation,
};
use super::*;
use cutout_ride_maps::{
    ChargingConnector, ChargingCost, ChargingDetails, ChargingSpotId, ChargingSupply, ChargingVenue,
};
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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
        revision: ChargingRevision::new(revision).unwrap(),
        change_sequence: ChargingRevision::new(revision).unwrap(),
        verified_at_ms: None,
        vote: None,
        vote_revision: ChargingRevision::new(0).unwrap(),
        votes_up: 0,
        votes_down: 0,
    }
}
fn owned_remote(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    published: &ChargingDirectoryEntry,
) -> MapPointId {
    apply_changes_page(
        connection,
        spatial,
        std::slice::from_ref(published),
        ChargingChangeCursor::new(published.change_sequence.get()).unwrap(),
    )
    .unwrap();
    let point_id = find_remote(connection, published.id).unwrap().unwrap();
    connection
        .execute(
            "UPDATE charging_spots SET owned=1 WHERE point_id=?1",
            [point_id.uuid().as_bytes()],
        )
        .unwrap();
    point_id
}
fn bounds() -> GeoBounds {
    GeoBounds::new(-1.0, 1.0, 179.0, -179.0).unwrap()
}

#[test]
fn outlet_count_survives_local_record_readback() {
    let supply = ChargingSupply::single(None, ChargingConnector::Unknown)
        .with_outlet_count(Some(4))
        .unwrap();
    let details = ChargingDetails::new(None, None, None, None, None, vec![supply]).unwrap();
    let input = input(0.5).with_details(details).unwrap();
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let receipt = save(&mut connection, &mut spatial, &input).unwrap();

    let stored = detail(&connection, receipt.point_id).unwrap().unwrap();
    let recovered = stored.input.unwrap();
    let supply = &recovered.details().supplies()[0];
    assert_eq!(supply.outlet_count().map(u16::from), Some(4));
}

#[test]
fn charging_change_page_rolls_back_remote_rows_and_cursor_together() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let snapshot = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    connection.authorizer(Some(|context: AuthContext<'_>| match context.action {
        AuthAction::Update {
            table_name: "charging_sync_state",
            ..
        } => Authorization::Deny,
        _ => Authorization::Allow,
    }));
    assert!(
        apply_changes_page(
            &mut connection,
            &mut spatial,
            std::slice::from_ref(&snapshot),
            ChargingChangeCursor::new(1).unwrap(),
        )
        .is_err()
    );
    connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    assert_eq!(
        changes_cursor(&connection).unwrap(),
        ChargingChangeCursor::ZERO
    );
    assert!(find_remote(&connection, snapshot.id).unwrap().is_none());
}

#[test]
fn charging_tombstone_hides_public_point_and_retains_blocked_local_draft() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let mut published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let remote_id = published.id;
    apply_changes_page(
        &mut connection,
        &mut spatial,
        std::slice::from_ref(&published),
        ChargingChangeCursor::new(1).unwrap(),
    )
    .unwrap();
    let point_id = find_remote(&connection, remote_id).unwrap().unwrap();
    connection
        .execute(
            "UPDATE charging_spots SET owned=1 WHERE point_id=?1",
            [point_id.uuid().as_bytes()],
        )
        .unwrap();
    let before = detail(&connection, point_id).unwrap().unwrap();
    let changed = change(
        &mut connection,
        &mut spatial,
        point_id,
        before.local_revision,
        &ChargingAction::Edit(input(1.5)),
    )
    .unwrap();
    published.change_sequence = ChargingRevision::new(2).unwrap();
    published.revision = ChargingRevision::new(2).unwrap();
    published.status = ChargingStatus::Hidden;
    published.input = None;
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[published],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    let hidden = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(hidden.status, ChargingStatus::Hidden);
    assert_eq!(hidden.input, None);
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
        .spots,
        []
    );
    let blocked = list_blocked_operations(&connection).unwrap();
    assert_eq!(blocked.len(), 1);
    let review = &blocked[0];
    assert_eq!(review.operation.id, changed.operation_id);
    assert_eq!(review.local_draft, Some(input(1.5)));
    assert!(!review.can_rebase);
    assert_eq!(
        blocked[0].remote_entry.as_ref().unwrap().status,
        ChargingStatus::Hidden
    );
    assert!(matches!(
        resolve_operation(
            &mut connection,
            &mut spatial,
            changed.operation_id,
            review.local_revision,
            ChargingOperationResolution::RebaseKeepingLocal,
        ),
        Err(StorageError::ChargingConflict)
    ));
    resolve_operation(
        &mut connection,
        &mut spatial,
        changed.operation_id,
        review.local_revision,
        ChargingOperationResolution::AcceptServer,
    )
    .unwrap();
    assert_eq!(count(&connection).unwrap(), 0);
    assert_eq!(changes_cursor(&connection).unwrap().get(), 2);
    let accepted = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(accepted.status, ChargingStatus::Hidden);
    assert!(accepted.local_revision > review.local_revision);
    assert!(matches!(
        change(
            &mut connection,
            &mut spatial,
            point_id,
            review.local_revision,
            &ChargingAction::Edit(input(1.5)),
        ),
        Err(StorageError::ChargingConflict)
    ));
}

#[test]
fn malformed_or_oversized_change_page_does_not_advance_cursor() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let mut snapshot = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    snapshot.change_sequence = ChargingRevision::new(2).unwrap();
    assert!(
        apply_changes_page(
            &mut connection,
            &mut spatial,
            &[snapshot],
            ChargingChangeCursor::new(1).unwrap(),
        )
        .is_err()
    );
    assert_eq!(
        changes_cursor(&connection).unwrap(),
        ChargingChangeCursor::ZERO
    );
    let oversized =
        vec![entry(Some(input(0.5)), ChargingStatus::Published, 1); CHARGING_SYNC_PAGE_LIMIT + 1];
    assert!(
        apply_changes_page(
            &mut connection,
            &mut spatial,
            &oversized,
            ChargingChangeCursor::new(1).unwrap(),
        )
        .is_err()
    );
    assert_eq!(
        changes_cursor(&connection).unwrap(),
        ChargingChangeCursor::ZERO
    );
}

#[test]
fn charging_change_cursor_survives_database_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("charging.sqlite");
    {
        let mut connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        super::super::migrations::migrate(&mut connection).unwrap();
        let mut spatial = SpatialSchemaState::Uninitialized;
        let snapshot = entry(Some(input(0.5)), ChargingStatus::Published, 7);
        apply_changes_page(
            &mut connection,
            &mut spatial,
            &[snapshot],
            ChargingChangeCursor::new(7).unwrap(),
        )
        .unwrap();
        assert!(directory_last_updated(&connection).unwrap().is_some());
    }
    let mut reopened = Connection::open(path).unwrap();
    reopened.pragma_update(None, "foreign_keys", "ON").unwrap();
    super::super::migrations::migrate(&mut reopened).unwrap();
    assert_eq!(changes_cursor(&reopened).unwrap().get(), 7);
    assert!(directory_last_updated(&reopened).unwrap().is_some());
}

#[test]
fn tokenless_page_preserves_confirmed_own_vote_and_revision() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let mut snapshot = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    snapshot.vote = Some(ChargingVote::Up);
    snapshot.vote_revision = ChargingRevision::new(4).unwrap();
    snapshot.votes_up = 3;
    snapshot.votes_down = 1;
    let remote_id = snapshot.id;
    apply_changes_page(
        &mut connection,
        &mut spatial,
        std::slice::from_ref(&snapshot),
        ChargingChangeCursor::new(1).unwrap(),
    )
    .unwrap();
    snapshot.change_sequence = ChargingRevision::new(2).unwrap();
    snapshot.revision = ChargingRevision::new(2).unwrap();
    snapshot.vote = None;
    snapshot.vote_revision = ChargingRevision::new(0).unwrap();
    snapshot.votes_up = 4;
    snapshot.votes_down = 1;
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[snapshot],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    let point_id = find_remote(&connection, remote_id).unwrap().unwrap();
    let cached = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(cached.vote, Some(ChargingVote::Up));
    assert_eq!(cached.vote_revision.get(), 4);
    assert_eq!(cached.votes_up, 4);
    assert_eq!(cached.votes_down, 1);
}

#[test]
fn rejected_create_can_be_replaced_or_explicitly_discarded() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let original = save(&mut connection, &mut spatial, &input(0.5)).unwrap();
    let operation = next_operation(&mut connection).unwrap().unwrap();
    block_operation(
        &mut connection,
        operation.id,
        ChargingOperationBlockKind::Invalid,
    )
    .unwrap();
    let newer_local = change(
        &mut connection,
        &mut spatial,
        original.point_id,
        original.revision,
        &ChargingAction::Edit(input(0.75)),
    )
    .unwrap();
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_eq!(review.operation.id, operation.id);
    assert_ne!(review.operation.local_revision, review.local_revision);
    assert_eq!(review.local_revision, newer_local.revision);
    assert!(!review.can_rebase);
    assert!(matches!(
        resolve_operation(
            &mut connection,
            &mut spatial,
            operation.id,
            review.local_revision,
            ChargingOperationResolution::RebaseKeepingLocal,
        ),
        Err(StorageError::ChargingConflict)
    ));
    let replacement = replace_blocked_operation(
        &mut connection,
        &mut spatial,
        operation.id,
        review.local_revision,
        &input(1.5),
    )
    .unwrap();
    assert_ne!(replacement.operation_id, operation.id);
    assert_eq!(replacement.revision, newer_local.revision.next().unwrap());
    assert_eq!(count(&connection).unwrap(), 1);
    let replaced = detail(&connection, original.point_id).unwrap().unwrap();
    assert_eq!(replaced.input, Some(input(1.5)));
    assert!(matches!(
        next_operation(&mut connection).unwrap().unwrap().action,
        ChargingAction::Create(_)
    ));

    let mut discard_connection = database();
    let mut discard_spatial = SpatialSchemaState::Uninitialized;
    let another = save(&mut discard_connection, &mut discard_spatial, &input(2.5)).unwrap();
    let rejected = next_operation(&mut discard_connection).unwrap().unwrap();
    block_operation(
        &mut discard_connection,
        rejected.id,
        ChargingOperationBlockKind::Invalid,
    )
    .unwrap();
    resolve_operation(
        &mut discard_connection,
        &mut discard_spatial,
        rejected.id,
        another.revision,
        ChargingOperationResolution::AcceptServer,
    )
    .unwrap();
    let discarded = detail(&discard_connection, another.point_id)
        .unwrap()
        .unwrap();
    assert_eq!(discarded.status, ChargingStatus::Withdrawn);
    assert_eq!(discarded.input, Some(input(2.5)));
}

#[test]
fn replacing_rejected_create_stays_ahead_of_its_pending_vote() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let created = save(&mut connection, &mut spatial, &input(0.5)).unwrap();
    let create = next_operation(&mut connection).unwrap().unwrap();
    block_operation(
        &mut connection,
        create.id,
        ChargingOperationBlockKind::Invalid,
    )
    .unwrap();
    let before_vote = detail(&connection, created.point_id).unwrap().unwrap();
    change(
        &mut connection,
        &mut spatial,
        created.point_id,
        before_vote.local_revision,
        &ChargingAction::Vote(Some(ChargingVote::Up)),
    )
    .unwrap();
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    let replacement = replace_blocked_operation(
        &mut connection,
        &mut spatial,
        create.id,
        review.local_revision,
        &input(1.5),
    )
    .unwrap();

    let replacement_operation = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(replacement_operation.id, replacement.operation_id);
    assert_eq!(
        replacement_operation.action,
        ChargingAction::Create(input(1.5))
    );
    let published = entry(Some(input(1.5)), ChargingStatus::Published, 1);
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        replacement_operation.id,
        ChargingRevision::new(1).unwrap(),
        &published,
    )
    .unwrap();
    let vote = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(vote.action, ChargingAction::Vote(Some(ChargingVote::Up)));
    assert_eq!(
        vote.target,
        ChargingTarget::Existing {
            id: published.id,
            expected_revision: ChargingRevision::ZERO,
        }
    );
    let mut voted = published;
    voted.vote = Some(ChargingVote::Up);
    voted.vote_revision = ChargingRevision::new(1).unwrap();
    voted.votes_up = 1;
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        vote.id,
        ChargingRevision::new(1).unwrap(),
        &voted,
    )
    .unwrap();
    assert_eq!(count(&connection).unwrap(), 0);
}

#[test]
fn conflict_rebase_uses_fresh_operation_identity_and_latest_target() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let remote_id = published.id;
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let before = detail(&connection, point_id).unwrap().unwrap();
    let edit = change(
        &mut connection,
        &mut spatial,
        point_id,
        before.local_revision,
        &ChargingAction::Edit(input(1.5)),
    )
    .unwrap();
    assert_eq!(
        next_operation(&mut connection).unwrap().unwrap().id,
        edit.operation_id
    );
    let mut newer = published;
    newer.change_sequence = ChargingRevision::new(2).unwrap();
    newer.revision = ChargingRevision::new(2).unwrap();
    newer.input = Some(input(0.75));
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[newer],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_ne!(review.operation.local_revision, review.local_revision);
    resolve_operation(
        &mut connection,
        &mut spatial,
        review.operation.id,
        review.local_revision,
        ChargingOperationResolution::RebaseKeepingLocal,
    )
    .unwrap();
    let rebased = next_operation(&mut connection).unwrap().unwrap();
    assert_ne!(rebased.id, edit.operation_id);
    assert_eq!(rebased.action, ChargingAction::Edit(input(1.5)));
    assert_eq!(
        rebased.target,
        ChargingTarget::Existing {
            id: remote_id,
            expected_revision: ChargingRevision::new(2).unwrap(),
        }
    );
}

#[test]
fn queued_edit_after_vote_keeps_its_base_and_local_draft_across_feed_and_retry() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let remote_id = published.id;
    let point_id = owned_remote(&mut connection, &mut spatial, &published);

    let before_vote = detail(&connection, point_id).unwrap().unwrap();
    let vote = change(
        &mut connection,
        &mut spatial,
        point_id,
        before_vote.local_revision,
        &ChargingAction::Vote(Some(ChargingVote::Up)),
    )
    .unwrap();
    let before_edit = detail(&connection, point_id).unwrap().unwrap();
    let edit = change(
        &mut connection,
        &mut spatial,
        point_id,
        before_edit.local_revision,
        &ChargingAction::Edit(input(1.5)),
    )
    .unwrap();
    let vote_operation = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(vote_operation.id, vote.operation_id);
    assert_eq!(
        vote_operation.target,
        ChargingTarget::Existing {
            id: remote_id,
            expected_revision: ChargingRevision::ZERO,
        }
    );

    let mut changed = published.clone();
    changed.change_sequence = ChargingRevision::new(2).unwrap();
    changed.revision = ChargingRevision::new(2).unwrap();
    changed.input = Some(input(0.75));
    let result = apply_changes_page(
        &mut connection,
        &mut spatial,
        &[changed.clone()],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    assert_eq!(result.blocked_operations, 1);
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_eq!(review.operation.id, edit.operation_id);
    assert_eq!(review.operation.action, ChargingAction::Edit(input(1.5)));
    assert_eq!(review.local_draft, Some(input(1.5)));
    assert_eq!(
        review.operation.target,
        ChargingTarget::Existing {
            id: remote_id,
            expected_revision: ChargingRevision::new(1).unwrap(),
        }
    );
    assert_eq!(
        detail(&connection, point_id).unwrap().unwrap().input,
        Some(input(1.5))
    );

    changed.vote = Some(ChargingVote::Up);
    changed.vote_revision = ChargingRevision::new(1).unwrap();
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        vote_operation.id,
        ChargingRevision::new(1).unwrap(),
        &changed,
    )
    .unwrap();
    assert!(next_operation(&mut connection).unwrap().is_none());
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_eq!(review.operation.id, edit.operation_id);
    resolve_operation(
        &mut connection,
        &mut spatial,
        review.operation.id,
        review.local_revision,
        ChargingOperationResolution::Retry,
    )
    .unwrap();
    let retried = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(retried.id, edit.operation_id);
    assert_eq!(retried.action, ChargingAction::Edit(input(1.5)));
    assert_eq!(
        retried.target,
        ChargingTarget::Existing {
            id: remote_id,
            expected_revision: ChargingRevision::new(1).unwrap(),
        }
    );
}

#[test]
fn non_field_conflict_rebase_keeps_latest_body_even_with_legacy_saved_draft() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let current = detail(&connection, point_id).unwrap().unwrap();
    let vote = change(
        &mut connection,
        &mut spatial,
        point_id,
        current.local_revision,
        &ChargingAction::Vote(None),
    )
    .unwrap();
    let operation = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(operation.id, vote.operation_id);
    block_operation(
        &mut connection,
        operation.id,
        ChargingOperationBlockKind::Conflict,
    )
    .unwrap();

    let mut newer = published;
    newer.change_sequence = ChargingRevision::new(2).unwrap();
    newer.revision = ChargingRevision::new(2).unwrap();
    newer.input = Some(input(0.75));
    newer.verified_at_ms = Some(1_000);
    newer.vote = Some(ChargingVote::Down);
    newer.vote_revision = ChargingRevision::new(1).unwrap();
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[newer],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_eq!(review.local_draft, None);
    connection
        .execute(
            "UPDATE charging_sync_conflicts SET local_draft=?2 WHERE operation_id=?1",
            rusqlite::params![
                vote.operation_id.uuid().to_string(),
                codec::input_json(&input(0.5)).unwrap(),
            ],
        )
        .unwrap();
    assert_eq!(
        list_blocked_operations(&connection).unwrap()[0].local_draft,
        None
    );
    resolve_operation(
        &mut connection,
        &mut spatial,
        vote.operation_id,
        review.local_revision,
        ChargingOperationResolution::RebaseKeepingLocal,
    )
    .unwrap();
    let current = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(current.input, Some(input(0.75)));
    assert_eq!(current.vote, None);
    assert!(current.pending_vote);
    let visible = page(
        &connection,
        &mut spatial,
        GeoBounds::new(-1.0, 1.0, -1.0, 1.0).unwrap(),
        &ChargingFilter::default(),
        None,
        QueryLimit(10),
    )
    .unwrap();
    assert_eq!(visible.spots.len(), 1);
    assert_eq!(visible.spots[0].point_id, point_id);
}

#[test]
fn pending_edit_preserves_local_body_and_applies_authoritative_vote() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let current = detail(&connection, point_id).unwrap().unwrap();
    let edit = change(
        &mut connection,
        &mut spatial,
        point_id,
        current.local_revision,
        &ChargingAction::Edit(input(1.5)),
    )
    .unwrap();
    let mut newer = published;
    newer.change_sequence = ChargingRevision::new(2).unwrap();
    newer.revision = ChargingRevision::new(2).unwrap();
    newer.input = Some(input(0.75));
    newer.vote = Some(ChargingVote::Down);
    newer.vote_revision = ChargingRevision::new(1).unwrap();
    let result = apply_changes_page(
        &mut connection,
        &mut spatial,
        &[newer],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    assert_eq!(result.blocked_operations, 1);
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_eq!(review.operation.id, edit.operation_id);
    let current = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(current.input, Some(input(1.5)));
    assert_eq!(current.vote, Some(ChargingVote::Down));
    assert_eq!(current.vote_revision, ChargingRevision::new(1).unwrap());
    assert_eq!(current.verified_at_ms, None);
}

#[test]
fn pending_vote_ignores_field_revision_changes_without_an_edit() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let remote_id = published.id;
    apply_changes_page(
        &mut connection,
        &mut spatial,
        std::slice::from_ref(&published),
        ChargingChangeCursor::new(1).unwrap(),
    )
    .unwrap();
    let point_id = find_remote(&connection, remote_id).unwrap().unwrap();
    let current = detail(&connection, point_id).unwrap().unwrap();
    let vote = change(
        &mut connection,
        &mut spatial,
        point_id,
        current.local_revision,
        &ChargingAction::Vote(Some(ChargingVote::Up)),
    )
    .unwrap();
    let operation = next_operation(&mut connection).unwrap().unwrap();
    let mut newer = published;
    newer.change_sequence = ChargingRevision::new(2).unwrap();
    newer.revision = ChargingRevision::new(2).unwrap();
    newer.input = Some(input(0.75));
    let result = apply_changes_page(
        &mut connection,
        &mut spatial,
        &[newer],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    assert_eq!(result.blocked_operations, 0);
    assert_eq!(list_blocked_operations(&connection).unwrap().len(), 0);
    assert_eq!(
        detail(&connection, point_id).unwrap().unwrap().input,
        Some(input(0.75))
    );
    let still_pending = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(still_pending.id, vote.operation_id);
    assert_eq!(still_pending.target, operation.target);
}

#[test]
fn conflict_scan_freezes_later_operation_in_its_own_revision_domain() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let remote_id = published.id;
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let current = detail(&connection, point_id).unwrap().unwrap();
    let vote = change(
        &mut connection,
        &mut spatial,
        point_id,
        current.local_revision,
        &ChargingAction::Vote(None),
    )
    .unwrap();
    let current = detail(&connection, point_id).unwrap().unwrap();
    let edit = change(
        &mut connection,
        &mut spatial,
        point_id,
        current.local_revision,
        &ChargingAction::Edit(input(1.5)),
    )
    .unwrap();
    let first = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(first.id, vote.operation_id);
    let mut newer = published;
    newer.change_sequence = ChargingRevision::new(2).unwrap();
    newer.revision = ChargingRevision::new(2).unwrap();
    newer.input = Some(input(0.75));
    newer.vote_revision = ChargingRevision::new(1).unwrap();
    newer.vote = Some(ChargingVote::Down);
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[newer.clone()],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_eq!(review.operation.id, vote.operation_id);
    resolve_operation(
        &mut connection,
        &mut spatial,
        vote.operation_id,
        review.local_revision,
        ChargingOperationResolution::RebaseKeepingLocal,
    )
    .unwrap();
    let rebased_vote = next_operation(&mut connection).unwrap().unwrap();
    assert_ne!(rebased_vote.id, vote.operation_id);
    assert_eq!(
        rebased_vote.target,
        ChargingTarget::Existing {
            id: remote_id,
            expected_revision: ChargingRevision::new(1).unwrap(),
        }
    );
    newer.change_sequence = ChargingRevision::new(3).unwrap();
    newer.vote = None;
    newer.vote_revision = ChargingRevision::new(2).unwrap();
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        rebased_vote.id,
        ChargingRevision::new(2).unwrap(),
        &newer,
    )
    .unwrap();
    let still_pending_edit = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(still_pending_edit.id, edit.operation_id);
    assert_eq!(
        still_pending_edit.target,
        ChargingTarget::Existing {
            id: remote_id,
            expected_revision: ChargingRevision::new(1).unwrap(),
        }
    );
}

#[test]
fn remote_page_invalidates_stale_editor_and_conflict_review_revisions() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let first = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let remote_id = first.id;
    apply_changes_page(
        &mut connection,
        &mut spatial,
        std::slice::from_ref(&first),
        ChargingChangeCursor::new(1).unwrap(),
    )
    .unwrap();
    let point_id = find_remote(&connection, remote_id).unwrap().unwrap();
    connection
        .execute(
            "UPDATE charging_spots SET owned=1 WHERE point_id=?1",
            [point_id.uuid().as_bytes()],
        )
        .unwrap();
    let initial_revision = detail(&connection, point_id)
        .unwrap()
        .unwrap()
        .local_revision;
    let mut second = first.clone();
    second.revision = ChargingRevision::new(2).unwrap();
    second.change_sequence = ChargingRevision::new(2).unwrap();
    second.input = Some(input(0.75));
    apply_changes_page(
        &mut connection,
        &mut spatial,
        std::slice::from_ref(&second),
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    let after_refresh = detail(&connection, point_id).unwrap().unwrap();
    assert_ne!(after_refresh.local_revision, initial_revision);
    assert!(matches!(
        change(
            &mut connection,
            &mut spatial,
            point_id,
            initial_revision,
            &ChargingAction::Edit(input(1.5)),
        ),
        Err(StorageError::ChargingConflict)
    ));

    let edit = change(
        &mut connection,
        &mut spatial,
        point_id,
        after_refresh.local_revision,
        &ChargingAction::Edit(input(1.5)),
    )
    .unwrap();
    let mut third = second;
    third.revision = ChargingRevision::new(3).unwrap();
    third.change_sequence = ChargingRevision::new(3).unwrap();
    third.input = Some(input(0.25));
    apply_changes_page(
        &mut connection,
        &mut spatial,
        std::slice::from_ref(&third),
        ChargingChangeCursor::new(3).unwrap(),
    )
    .unwrap();
    let first_review = detail(&connection, point_id)
        .unwrap()
        .unwrap()
        .local_revision;
    let mut fourth = third;
    fourth.revision = ChargingRevision::new(4).unwrap();
    fourth.change_sequence = ChargingRevision::new(4).unwrap();
    fourth.input = Some(input(0.1));
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[fourth],
        ChargingChangeCursor::new(4).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        resolve_operation(
            &mut connection,
            &mut spatial,
            edit.operation_id,
            first_review,
            ChargingOperationResolution::AcceptServer,
        ),
        Err(StorageError::ChargingConflict)
    ));
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
            "DROP TABLE charging_outbox; DROP TABLE charging_spots; PRAGMA user_version=42;",
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
        42
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
    let operation = next_operation(&mut connection).unwrap().unwrap();
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
        acknowledge_operation(
            &mut connection,
            &mut spatial,
            edited.operation_id,
            ChargingRevision::new(1).unwrap(),
            &snapshot
        ),
        Err(StorageError::ChargingConflict)
    ));
    assert_eq!(
        acknowledge_operation(
            &mut connection,
            &mut spatial,
            operation.id,
            ChargingRevision::new(1).unwrap(),
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
    let edit = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(
        edit.target,
        ChargingTarget::Existing {
            id: snapshot.id,
            expected_revision: ChargingRevision::new(1).unwrap()
        }
    );
    assert_eq!(next_operation(&mut connection).unwrap().unwrap(), edit);
    let mut snapshot = snapshot;
    snapshot.input = Some(input(-179.5));
    snapshot.revision = ChargingRevision::new(2).unwrap();
    snapshot.change_sequence = ChargingRevision::new(2).unwrap();
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        edit.id,
        ChargingRevision::new(2).unwrap(),
        &snapshot,
    )
    .unwrap();
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        edit.id,
        ChargingRevision::new(2).unwrap(),
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
fn acknowledgment_targets_its_operation_even_when_another_point_becomes_eligible() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let rejected = save(&mut connection, &mut spatial, &input(0.5)).unwrap();
    let operation_a = next_operation(&mut connection).unwrap().unwrap();
    block_operation(
        &mut connection,
        operation_a.id,
        ChargingOperationBlockKind::Conflict,
    )
    .unwrap();

    let saved_b = save(&mut connection, &mut spatial, &input(1.5)).unwrap();
    let operation_b = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(operation_b.point_id, saved_b.point_id);
    resolve_operation(
        &mut connection,
        &mut spatial,
        operation_a.id,
        rejected.revision,
        ChargingOperationResolution::Retry,
    )
    .unwrap();

    let receipt = entry(Some(input(1.5)), ChargingStatus::Published, 1);
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        operation_b.id,
        ChargingRevision::new(1).unwrap(),
        &receipt,
    )
    .unwrap();
    assert_eq!(count(&connection).unwrap(), 1);
    assert_eq!(
        next_operation(&mut connection).unwrap().unwrap().point_id,
        rejected.point_id
    );
}

#[test]
fn rejected_withdrawal_can_retry_after_credentials_are_corrected() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let before = detail(&connection, point_id).unwrap().unwrap();
    let withdrawn = change(
        &mut connection,
        &mut spatial,
        point_id,
        before.local_revision,
        &ChargingAction::Withdraw,
    )
    .unwrap();
    let operation = next_operation(&mut connection).unwrap().unwrap();
    block_operation(
        &mut connection,
        operation.id,
        ChargingOperationBlockKind::Invalid,
    )
    .unwrap();
    resolve_operation(
        &mut connection,
        &mut spatial,
        operation.id,
        withdrawn.revision,
        ChargingOperationResolution::Retry,
    )
    .unwrap();
    let retry = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(retry.id, operation.id);
    assert_eq!(retry.target, operation.target);
    assert_eq!(retry.action, ChargingAction::Withdraw);
    assert_eq!(count(&connection).unwrap(), 1);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the replacement and subsequent independent sync operations form one ordering regression"
)]
fn replacing_blocked_edit_preserves_later_vote_and_verification() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let mut newer = published.clone();
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let before = detail(&connection, point_id).unwrap().unwrap();
    let edit = change(
        &mut connection,
        &mut spatial,
        point_id,
        before.local_revision,
        &ChargingAction::Edit(input(1.5)),
    )
    .unwrap();
    let after_edit = detail(&connection, point_id).unwrap().unwrap();
    change(
        &mut connection,
        &mut spatial,
        point_id,
        after_edit.local_revision,
        &ChargingAction::Vote(Some(ChargingVote::Up)),
    )
    .unwrap();
    let after_vote = detail(&connection, point_id).unwrap().unwrap();
    change(
        &mut connection,
        &mut spatial,
        point_id,
        after_vote.local_revision,
        &ChargingAction::Verify,
    )
    .unwrap();

    newer.change_sequence = ChargingRevision::new(2).unwrap();
    newer.revision = ChargingRevision::new(2).unwrap();
    newer.input = Some(input(0.75));
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[newer],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_eq!(review.operation.id, edit.operation_id);
    replace_blocked_operation(
        &mut connection,
        &mut spatial,
        edit.operation_id,
        review.local_revision,
        &input(2.5),
    )
    .unwrap();

    let current = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(current.vote, Some(ChargingVote::Up));
    assert!(current.pending_vote);
    assert_eq!(count(&connection).unwrap(), 3);
    let first = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(first.action, ChargingAction::Edit(input(2.5)));
    assert_eq!(
        first.target,
        ChargingTarget::Existing {
            id: published.id,
            expected_revision: ChargingRevision::new(2).unwrap()
        }
    );
    let mut edited = entry(Some(input(2.5)), ChargingStatus::Published, 3);
    edited.id = published.id;
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        first.id,
        ChargingRevision::new(3).unwrap(),
        &edited,
    )
    .unwrap();
    let vote = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(vote.action, ChargingAction::Vote(Some(ChargingVote::Up)));
    let mut voted = edited.clone();
    voted.vote = Some(ChargingVote::Up);
    voted.vote_revision = ChargingRevision::new(1).unwrap();
    voted.votes_up = 1;
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        vote.id,
        ChargingRevision::new(1).unwrap(),
        &voted,
    )
    .unwrap();
    let verify = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(verify.action, ChargingAction::Verify);
    assert_eq!(
        verify.target,
        ChargingTarget::Existing {
            id: edited.id,
            expected_revision: ChargingRevision::new(1).unwrap(),
        }
    );
    let mut server_after_vote = edited.clone();
    server_after_vote.revision = ChargingRevision::new(4).unwrap();
    server_after_vote.change_sequence = ChargingRevision::new(4).unwrap();
    server_after_vote.vote = Some(ChargingVote::Up);
    server_after_vote.vote_revision = ChargingRevision::new(1).unwrap();
    server_after_vote.votes_up = 1;
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[server_after_vote],
        ChargingChangeCursor::new(4).unwrap(),
    )
    .unwrap();
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_eq!(review.operation.id, verify.id);
    assert_eq!(
        review.operation.target,
        ChargingTarget::Existing {
            id: edited.id,
            expected_revision: ChargingRevision::new(1).unwrap(),
        }
    );
    resolve_operation(
        &mut connection,
        &mut spatial,
        verify.id,
        review.local_revision,
        ChargingOperationResolution::RebaseKeepingLocal,
    )
    .unwrap();
    let verify = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(verify.action, ChargingAction::Verify);
    assert_eq!(
        verify.target,
        ChargingTarget::Existing {
            id: edited.id,
            expected_revision: ChargingRevision::new(4).unwrap(),
        }
    );
    let mut verified = edited;
    verified.revision = ChargingRevision::new(5).unwrap();
    verified.change_sequence = ChargingRevision::new(5).unwrap();
    verified.verified_at_ms = Some(2_000);
    verified.vote = Some(ChargingVote::Up);
    verified.vote_revision = ChargingRevision::new(1).unwrap();
    verified.votes_up = 1;
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        verify.id,
        ChargingRevision::new(5).unwrap(),
        &verified,
    )
    .unwrap();
    assert_eq!(count(&connection).unwrap(), 0);
    assert_eq!(
        detail(&connection, point_id)
            .unwrap()
            .unwrap()
            .verified_at_ms,
        Some(2_000)
    );
}

#[test]
fn accepting_server_edit_preserves_later_vote_and_invalidates_stale_editor() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let before = detail(&connection, point_id).unwrap().unwrap();
    let edit = change(
        &mut connection,
        &mut spatial,
        point_id,
        before.local_revision,
        &ChargingAction::Edit(input(1.5)),
    )
    .unwrap();
    let after_edit = detail(&connection, point_id).unwrap().unwrap();
    change(
        &mut connection,
        &mut spatial,
        point_id,
        after_edit.local_revision,
        &ChargingAction::Vote(Some(ChargingVote::Up)),
    )
    .unwrap();
    let after_vote = detail(&connection, point_id).unwrap().unwrap();
    change(
        &mut connection,
        &mut spatial,
        point_id,
        after_vote.local_revision,
        &ChargingAction::Verify,
    )
    .unwrap();
    let stale_editor_revision = after_vote.local_revision;

    let mut newer = published;
    newer.change_sequence = ChargingRevision::new(2).unwrap();
    newer.revision = ChargingRevision::new(2).unwrap();
    newer.input = Some(input(0.75));
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[newer],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    resolve_operation(
        &mut connection,
        &mut spatial,
        edit.operation_id,
        review.local_revision,
        ChargingOperationResolution::AcceptServer,
    )
    .unwrap();

    let accepted = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(accepted.input, Some(input(0.75)));
    assert_eq!(accepted.vote, Some(ChargingVote::Up));
    assert!(accepted.pending_vote);
    assert!(accepted.local_revision > review.local_revision);
    assert!(matches!(
        change(
            &mut connection,
            &mut spatial,
            point_id,
            stale_editor_revision,
            &ChargingAction::Edit(input(2.5)),
        ),
        Err(StorageError::ChargingConflict)
    ));
    assert_eq!(count(&connection).unwrap(), 2);
    assert_eq!(
        next_operation(&mut connection).unwrap().unwrap().action,
        ChargingAction::Vote(Some(ChargingVote::Up))
    );
}

#[test]
fn accepting_server_vote_preserves_a_later_local_edit() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let before = detail(&connection, point_id).unwrap().unwrap();
    let vote = change(
        &mut connection,
        &mut spatial,
        point_id,
        before.local_revision,
        &ChargingAction::Vote(Some(ChargingVote::Up)),
    )
    .unwrap();
    let blocked_vote = next_operation(&mut connection).unwrap().unwrap();
    let before_edit = detail(&connection, point_id).unwrap().unwrap();
    let edit = change(
        &mut connection,
        &mut spatial,
        point_id,
        before_edit.local_revision,
        &ChargingAction::Edit(input(1.5)),
    )
    .unwrap();
    let remote_id = published.id;
    let mut newer = published;
    newer.change_sequence = ChargingRevision::new(2).unwrap();
    newer.revision = ChargingRevision::new(2).unwrap();
    newer.input = Some(input(0.75));
    newer.vote = Some(ChargingVote::Down);
    newer.vote_revision = ChargingRevision::new(1).unwrap();
    apply_changes_page(
        &mut connection,
        &mut spatial,
        &[newer],
        ChargingChangeCursor::new(2).unwrap(),
    )
    .unwrap();
    let review = list_blocked_operations(&connection).unwrap().remove(0);
    assert_eq!(review.operation.id, vote.operation_id);
    assert_eq!(blocked_vote.id, vote.operation_id);
    resolve_operation(
        &mut connection,
        &mut spatial,
        vote.operation_id,
        review.local_revision,
        ChargingOperationResolution::AcceptServer,
    )
    .unwrap();

    let accepted = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(accepted.input, Some(input(1.5)));
    assert_eq!(accepted.vote, Some(ChargingVote::Down));
    assert!(!accepted.pending_vote);
    assert_eq!(count(&connection).unwrap(), 1);
    let retained_edit = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(retained_edit.action, ChargingAction::Edit(input(1.5)));
    assert_eq!(
        retained_edit.target,
        ChargingTarget::Existing {
            id: remote_id,
            expected_revision: ChargingRevision::new(1).unwrap(),
        }
    );
    assert_eq!(retained_edit.action, ChargingAction::Edit(input(1.5)));
    assert_eq!(retained_edit.id, edit.operation_id);
}

#[test]
fn accepting_server_vote_keeps_moderated_later_edit_out_of_the_map() {
    for (status, status_name) in [
        (ChargingStatus::Hidden, "hidden"),
        (ChargingStatus::Removed, "removed"),
    ] {
        let mut connection = database();
        let mut spatial = SpatialSchemaState::Uninitialized;
        let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
        let point_id = owned_remote(&mut connection, &mut spatial, &published);
        let before_vote = detail(&connection, point_id).unwrap().unwrap();
        let vote = change(
            &mut connection,
            &mut spatial,
            point_id,
            before_vote.local_revision,
            &ChargingAction::Vote(Some(ChargingVote::Up)),
        )
        .unwrap();
        assert_eq!(
            next_operation(&mut connection).unwrap().unwrap().id,
            vote.operation_id
        );
        let before_edit = detail(&connection, point_id).unwrap().unwrap();
        let edit = change(
            &mut connection,
            &mut spatial,
            point_id,
            before_edit.local_revision,
            &ChargingAction::Edit(input(1.5)),
        )
        .unwrap();
        block_operation(
            &mut connection,
            vote.operation_id,
            ChargingOperationBlockKind::Conflict,
        )
        .unwrap();
        let remote_json = format!(
            r#"{{"id":"{}","input_json":null,"revision":2,"change_sequence":2,"status":"{}","verified_at_ms":null,"vote":null,"vote_revision":0,"votes_up":0,"votes_down":0}}"#,
            published.id.as_uuid(),
            status_name,
        );
        connection
            .execute(
                "UPDATE charging_sync_conflicts SET remote_entry=?2 WHERE operation_id=?1",
                rusqlite::params![vote.operation_id.uuid().to_string(), remote_json],
            )
            .unwrap();
        let review = list_blocked_operations(&connection).unwrap().remove(0);
        assert_eq!(review.operation.id, vote.operation_id);
        assert_eq!(review.remote_entry.as_ref().unwrap().status, status);

        resolve_operation(
            &mut connection,
            &mut spatial,
            vote.operation_id,
            review.local_revision,
            ChargingOperationResolution::AcceptServer,
        )
        .unwrap();

        let accepted = detail(&connection, point_id).unwrap().unwrap();
        assert_eq!(accepted.status, status);
        assert_eq!(accepted.input, None);
        assert_eq!(accepted.pending_operations, 1);
        assert_eq!(count(&connection).unwrap(), 1);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM map_points_rtree", [], |row| row
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
        let retained = next_operation(&mut connection).unwrap().unwrap();
        assert_eq!(retained.id, edit.operation_id);
        assert_eq!(retained.action, ChargingAction::Edit(input(1.5)));
    }
}

#[test]
fn vote_receipt_ack_does_not_resurrect_a_local_withdrawal() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let before = detail(&connection, point_id).unwrap().unwrap();
    let vote = change(
        &mut connection,
        &mut spatial,
        point_id,
        before.local_revision,
        &ChargingAction::Vote(Some(ChargingVote::Up)),
    )
    .unwrap();
    let frozen_vote = next_operation(&mut connection).unwrap().unwrap();
    let after_vote = detail(&connection, point_id).unwrap().unwrap();
    change(
        &mut connection,
        &mut spatial,
        point_id,
        after_vote.local_revision,
        &ChargingAction::Withdraw,
    )
    .unwrap();
    let vote_receipt = ChargingVoteReceipt {
        applied_revision: ChargingRevision::new(1).unwrap(),
        vote_revision: ChargingRevision::new(1).unwrap(),
        vote: Some(ChargingVote::Up),
        votes_up: 1,
        votes_down: 0,
        change_sequence: ChargingChangeCursor::new(1).unwrap(),
    };
    acknowledge_vote_operation(&mut connection, vote.operation_id, vote_receipt).unwrap();
    acknowledge_vote_operation(&mut connection, vote.operation_id, vote_receipt).unwrap();
    let withdrawn = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(withdrawn.status, ChargingStatus::Withdrawn);
    assert_eq!(withdrawn.input, None);
    assert_eq!(withdrawn.vote, Some(ChargingVote::Up));
    assert_eq!(withdrawn.vote_revision, ChargingRevision::new(1).unwrap());
    assert!(!withdrawn.pending_vote);
    assert_eq!(count(&connection).unwrap(), 1);
    let withdrawal = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(withdrawal.action, ChargingAction::Withdraw);
    assert_ne!(withdrawal.id, frozen_vote.id);
}

#[test]
fn vote_receipt_ack_keeps_a_later_optimistic_vote() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(0.5)), ChargingStatus::Published, 1);
    let point_id = owned_remote(&mut connection, &mut spatial, &published);
    let before = detail(&connection, point_id).unwrap().unwrap();
    let first = change(
        &mut connection,
        &mut spatial,
        point_id,
        before.local_revision,
        &ChargingAction::Vote(Some(ChargingVote::Up)),
    )
    .unwrap();
    let frozen_first = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(frozen_first.id, first.operation_id);
    let after_first = detail(&connection, point_id).unwrap().unwrap();
    let second = change(
        &mut connection,
        &mut spatial,
        point_id,
        after_first.local_revision,
        &ChargingAction::Vote(Some(ChargingVote::Down)),
    )
    .unwrap();
    acknowledge_vote_operation(
        &mut connection,
        first.operation_id,
        ChargingVoteReceipt {
            applied_revision: ChargingRevision::new(1).unwrap(),
            vote_revision: ChargingRevision::new(1).unwrap(),
            vote: Some(ChargingVote::Up),
            votes_up: 1,
            votes_down: 0,
            change_sequence: ChargingChangeCursor::new(1).unwrap(),
        },
    )
    .unwrap();
    let current = detail(&connection, point_id).unwrap().unwrap();
    assert_eq!(current.vote, Some(ChargingVote::Down));
    assert!(current.pending_vote);
    assert_eq!(current.vote_revision, ChargingRevision::new(1).unwrap());
    assert_eq!(count(&connection).unwrap(), 1);
    let queued = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(queued.id, second.operation_id);
    assert_eq!(
        queued.action,
        ChargingAction::Vote(Some(ChargingVote::Down))
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
        venue: None,
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
        nearby(
            &connection,
            &mut spatial,
            bounds(),
            input(180.0).coordinate(),
            &wrong,
            QueryLimit(1),
        )
        .unwrap(),
        []
    );
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
    let detail_reads = Arc::new(AtomicUsize::new(0));
    let candidate_scans = Arc::new(AtomicUsize::new(0));
    let counted_details = Arc::clone(&detail_reads);
    let counted_scans = Arc::clone(&candidate_scans);
    connection.authorizer(Some(move |context: AuthContext<'_>| {
        match context.action {
            AuthAction::Read {
                table_name: "charging_spots",
                column_name: "local_revision",
            } => {
                counted_details.fetch_add(1, Ordering::Relaxed);
            }
            AuthAction::Read {
                table_name: "map_points_rtree",
                column_name: "max_lat_e7",
            } => {
                counted_scans.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        Authorization::Allow
    }));
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
    assert_eq!(detail_reads.load(Ordering::Relaxed), 1);
    assert_eq!(candidate_scans.load(Ordering::Relaxed), 1);
}

#[test]
fn charging_nearby_orders_equal_distances_by_point_id() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let mut expected = Vec::new();
    for value in [3, 1, 2] {
        let mut snapshot = entry(Some(input(-179.9)), ChargingStatus::Published, 1);
        snapshot.id = ChargingSpotId::parse(&uuid::Uuid::from_u128(value).to_string()).unwrap();
        expected.push(cache_directory_entry(&mut connection, &mut spatial, &snapshot).unwrap());
    }
    expected.sort_unstable();
    expected.truncate(2);
    let actual = nearby(
        &connection,
        &mut spatial,
        bounds(),
        input(180.0).coordinate(),
        &ChargingFilter::default(),
        QueryLimit(2),
    )
    .unwrap();
    assert_eq!(
        actual
            .into_iter()
            .map(|spot| spot.point_id)
            .collect::<Vec<_>>(),
        expected
    );
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
    let id = cache_directory_entry(&mut connection, &mut spatial, &snapshot).unwrap();
    assert_eq!(id.uuid(), snapshot.id.as_uuid());
    assert!(!detail(&connection, id).unwrap().unwrap().owned);
    assert!(matches!(
        change(
            &mut connection,
            &mut spatial,
            id,
            ChargingRevision::new(1).unwrap(),
            &ChargingAction::Edit(input(-179.5))
        ),
        Err(StorageError::ChargingNotOwned)
    ));
    let vote = change(
        &mut connection,
        &mut spatial,
        id,
        ChargingRevision::new(1).unwrap(),
        &ChargingAction::Vote(Some(ChargingVote::Down)),
    )
    .unwrap();
    assert_eq!(detail(&connection, id).unwrap().unwrap().votes_down, 0);
    assert!(matches!(
        cache_directory_entry(&mut connection, &mut spatial, &snapshot),
        Err(StorageError::ChargingConflict)
    ));
    assert_eq!(
        next_operation(&mut connection).unwrap().unwrap().target,
        ChargingTarget::Existing {
            id: snapshot.id,
            expected_revision: ChargingRevision::new(0).unwrap()
        }
    );
    let mut snapshot = snapshot;
    snapshot.vote = Some(ChargingVote::Down);
    snapshot.vote_revision = ChargingRevision::new(1).unwrap();
    snapshot.votes_down = 1;
    snapshot.change_sequence = ChargingRevision::new(6).unwrap();
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        vote.operation_id,
        ChargingRevision::new(1).unwrap(),
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
    next_operation(&mut connection).unwrap();
    snapshot.revision = ChargingRevision::new(6).unwrap();
    snapshot.change_sequence = ChargingRevision::new(7).unwrap();
    snapshot.verified_at_ms = Some(123);
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        verified.operation_id,
        ChargingRevision::new(6).unwrap(),
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
    tombstone.revision = ChargingRevision::new(7).unwrap();
    tombstone.change_sequence = ChargingRevision::new(8).unwrap();
    cache_directory_entry(&mut connection, &mut spatial, &tombstone).unwrap();
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
    let queued = next_operation(&mut connection).unwrap().unwrap();
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
    assert_eq!(next_operation(&mut connection).unwrap().unwrap(), queued);
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
    let create = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(create.target, ChargingTarget::Create);
    let published = entry(Some(input(179.5)), ChargingStatus::Published, 1);
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        create.id,
        ChargingRevision::new(1).unwrap(),
        &published,
    )
    .unwrap();
    let operation = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(operation.id, withdrawal.operation_id);
    assert_eq!(
        operation.target,
        ChargingTarget::Existing {
            id: published.id,
            expected_revision: ChargingRevision::new(1).unwrap(),
        }
    );
    let mut withdrawn = published;
    withdrawn.status = ChargingStatus::Withdrawn;
    withdrawn.input = None;
    withdrawn.revision = ChargingRevision::new(2).unwrap();
    withdrawn.change_sequence = ChargingRevision::new(2).unwrap();
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        operation.id,
        ChargingRevision::new(2).unwrap(),
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
    let id = cache_directory_entry(&mut connection, &mut spatial, &current).unwrap();
    let mut stale = current.clone();
    stale.input = Some(input(-179.5));
    stale.revision = ChargingRevision::new(4).unwrap();
    stale.change_sequence = ChargingRevision::new(4).unwrap();
    assert!(matches!(
        cache_directory_entry(&mut connection, &mut spatial, &stale),
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
    let create = next_operation(&mut connection).unwrap().unwrap();
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
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        create.id,
        ChargingRevision::new(1).unwrap(),
        &created,
    )
    .unwrap();
    let stored = detail(&connection, saved.point_id).unwrap().unwrap();
    assert_eq!(stored.verified_at_ms, None);
    assert_eq!(stored.input, Some(input(-179.5)));
    assert_eq!(
        next_operation(&mut connection).unwrap().unwrap().id,
        edited.operation_id
    );
}

#[test]
fn charging_venue_filter_runs_before_pagination() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let non_cafe = input(179.5)
        .with_details(
            ChargingDetails::new(
                None,
                Some(ChargingVenue::Restaurant),
                None,
                None,
                None,
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    let cafe = input(-179.5)
        .with_details(
            ChargingDetails::new(None, Some(ChargingVenue::Cafe), None, None, None, vec![])
                .unwrap(),
        )
        .unwrap();
    save(&mut connection, &mut spatial, &non_cafe).unwrap();
    let expected = save(&mut connection, &mut spatial, &cafe).unwrap();
    let filtered = page(
        &connection,
        &mut spatial,
        bounds(),
        &ChargingFilter {
            venue: Some(ChargingVenue::Cafe),
            ..ChargingFilter::default()
        },
        None,
        QueryLimit(1),
    )
    .unwrap();
    assert_eq!(filtered.spots.len(), 1);
    assert_eq!(filtered.spots[0].point_id, expected.point_id);
    assert!(filtered.next_cursor.is_none());
}

#[test]
fn unknown_connector_matches_missing_and_empty_connector_groups_without_defaulting() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let absent = save(&mut connection, &mut spatial, &input(179.5)).unwrap();
    let empty_group = input(-179.8)
        .with_details(ChargingDetails::new(None, None, None, None, None, vec![]).unwrap())
        .unwrap();
    let empty = save(&mut connection, &mut spatial, &empty_group).unwrap();
    let empty_known_voltage = input(-179.7)
        .with_details(
            ChargingDetails::new(
                None,
                None,
                None,
                None,
                None,
                vec![
                    ChargingSupply::new(
                        Some(cutout_ride_maps::SupplyVoltage::new(240).unwrap()),
                        vec![],
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let empty_voltage = save(&mut connection, &mut spatial, &empty_known_voltage).unwrap();
    let known = input(-179.5)
        .with_details(
            ChargingDetails::new(
                None,
                None,
                None,
                None,
                None,
                vec![ChargingSupply::single(
                    Some(cutout_ride_maps::SupplyVoltage::new(240).unwrap()),
                    ChargingConnector::Type2,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    save(&mut connection, &mut spatial, &known).unwrap();
    let unknown = ChargingFilter {
        connector: Some(ChargingConnector::Unknown),
        ..ChargingFilter::default()
    };
    let result = page(
        &connection,
        &mut spatial,
        bounds(),
        &unknown,
        None,
        QueryLimit(10),
    )
    .unwrap();
    assert_eq!(result.spots.len(), 3);
    assert!(
        result
            .spots
            .iter()
            .any(|spot| spot.point_id == absent.point_id)
    );
    assert!(
        result
            .spots
            .iter()
            .any(|spot| spot.point_id == empty.point_id)
    );
    assert!(
        result
            .spots
            .iter()
            .any(|spot| spot.point_id == empty_voltage.point_id)
    );

    let paired = ChargingFilter {
        voltage: Some(cutout_ride_maps::SupplyVoltage::new(240).unwrap()),
        ..unknown
    };
    let paired_result = page(
        &connection,
        &mut spatial,
        bounds(),
        &paired,
        None,
        QueryLimit(10),
    )
    .unwrap();
    assert_eq!(paired_result.spots.len(), 1);
    assert_eq!(paired_result.spots[0].point_id, empty_voltage.point_id);
}

#[test]
fn pending_vote_and_operations_are_read_from_outbox_and_clear_on_acknowledgement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("charging.sqlite");
    let mut connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    super::super::migrations::migrate(&mut connection).unwrap();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let saved = save(&mut connection, &mut spatial, &input(179.5)).unwrap();
    let create = next_operation(&mut connection).unwrap().unwrap();
    let snapshot = entry(Some(input(179.5)), ChargingStatus::Published, 1);
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        create.id,
        ChargingRevision::new(1).unwrap(),
        &snapshot,
    )
    .unwrap();
    let before = detail(&connection, saved.point_id).unwrap().unwrap();
    assert_eq!(before.pending_operations, 0);
    assert!(!before.pending_vote);
    let receipt = change(
        &mut connection,
        &mut spatial,
        saved.point_id,
        before.local_revision,
        &ChargingAction::Vote(None),
    )
    .unwrap();
    drop(connection);

    let mut connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let pending = detail(&connection, saved.point_id).unwrap().unwrap();
    assert_eq!(pending.pending_operations, 1);
    assert!(pending.pending_vote);
    let pending_page = page(
        &connection,
        &mut spatial,
        bounds(),
        &ChargingFilter::default(),
        None,
        QueryLimit(10),
    )
    .unwrap();
    assert_eq!(pending_page.spots[0].pending_operations, 1);
    assert!(pending_page.spots[0].pending_vote);
    let operation = next_operation(&mut connection).unwrap().unwrap();
    assert_eq!(operation.id, receipt.operation_id);
    assert_eq!(operation.action, ChargingAction::Vote(None));
    assert_eq!(
        connection
            .query_row(
                "SELECT json_type(action, '$.Vote') FROM charging_outbox WHERE operation_id=?1",
                [receipt.operation_id.uuid().to_string()],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "null"
    );

    let mut acknowledged = snapshot;
    acknowledged.vote = None;
    acknowledged.vote_revision = ChargingRevision::new(1).unwrap();
    acknowledge_operation(
        &mut connection,
        &mut spatial,
        operation.id,
        ChargingRevision::new(1).unwrap(),
        &acknowledged,
    )
    .unwrap();
    drop(connection);
    let connection = Connection::open(&path).unwrap();
    let after = detail(&connection, saved.point_id).unwrap().unwrap();
    assert_eq!(after.pending_operations, 0);
    assert!(!after.pending_vote);
    let acknowledged_page = page(
        &connection,
        &mut SpatialSchemaState::Uninitialized,
        bounds(),
        &ChargingFilter::default(),
        None,
        QueryLimit(10),
    )
    .unwrap();
    assert_eq!(acknowledged_page.spots[0].pending_operations, 0);
    assert!(!acknowledged_page.spots[0].pending_vote);
    assert_eq!(after.votes_up, 0);
    assert_eq!(after.votes_down, 0);
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
fn venue_and_unknown_connector_filters_match_before_page_and_nearby_selection() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let cafe_without_connector = input(179.5)
        .with_details(
            ChargingDetails::new(None, Some(ChargingVenue::Cafe), None, None, None, vec![])
                .unwrap(),
        )
        .unwrap();
    let restaurant_with_connector = input(-179.5)
        .with_details(
            ChargingDetails::new(
                None,
                Some(ChargingVenue::Restaurant),
                None,
                None,
                None,
                vec![ChargingSupply::single(None, ChargingConnector::Type2)],
            )
            .unwrap(),
        )
        .unwrap();
    let cafe_id = save(&mut connection, &mut spatial, &cafe_without_connector)
        .unwrap()
        .point_id;
    save(&mut connection, &mut spatial, &restaurant_with_connector).unwrap();

    let filter = ChargingFilter {
        connector: Some(ChargingConnector::Unknown),
        venue: Some(ChargingVenue::Cafe),
        ..ChargingFilter::default()
    };
    let page = page(
        &connection,
        &mut spatial,
        bounds(),
        &filter,
        None,
        QueryLimit(10),
    )
    .unwrap();
    assert_eq!(page.spots.len(), 1);
    assert_eq!(page.spots[0].point_id, cafe_id);

    let nearby = nearby(
        &connection,
        &mut spatial,
        bounds(),
        Coordinate::from_degrees(0.0, 179.6).unwrap(),
        &filter,
        QueryLimit(10),
    )
    .unwrap();
    assert_eq!(nearby.len(), 1);
    assert_eq!(nearby[0].point_id, cafe_id);
}

#[test]
fn hidden_directory_snapshot_without_body_removes_index_and_keeps_identity() {
    let mut connection = database();
    let mut spatial = SpatialSchemaState::Uninitialized;
    let published = entry(Some(input(179.5)), ChargingStatus::Published, 5);
    let id = cache_directory_entry(&mut connection, &mut spatial, &published).unwrap();
    let mut hidden = published.clone();
    hidden.input = None;
    hidden.status = ChargingStatus::Hidden;
    hidden.revision = ChargingRevision::new(6).unwrap();
    hidden.change_sequence = ChargingRevision::new(6).unwrap();
    assert_eq!(
        cache_directory_entry(&mut connection, &mut spatial, &hidden).unwrap(),
        id
    );
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
        cache_directory_entry(&mut connection, &mut spatial, &published_without_body),
        Err(StorageError::ChargingInvalidInput)
    ));
    let hidden_with_body = entry(Some(input(179.5)), ChargingStatus::Hidden, 1);
    assert!(matches!(
        cache_directory_entry(&mut connection, &mut spatial, &hidden_with_body),
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
        cache_directory_entry(&mut connection, &mut spatial, &collision),
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
        next_operation(&mut connection),
        Err(StorageError::InvalidStoredValue {
            field: "charging input",
            ..
        })
    ));
}

#[test]
fn charging_upgrade_preserves_existing_moving_time_checkpoints() {
    let mut connection = database();
    connection
        .execute_batch(
            "INSERT INTO rides
            (id,source,state,created_at_ms,updated_at_ms,point_count,distance_mm,
             moving_time_revision,moving_time_ms,moving_time_observed_ms,
             moving_time_unknown_ms,moving_time_cursor_ms,moving_time_availability)
         VALUES ('existing-ride','live','active',1,1,0,0,7,1200,2000,300,2300,'partial');
         DROP TABLE charging_outbox;
         DROP TABLE charging_spots;
         PRAGMA user_version=42;",
        )
        .unwrap();
    super::super::migrations::migrate(&mut connection).unwrap();
    assert!(super::super::migrations::table_exists(&connection, "charging_spots").unwrap());
    let checkpoint: (u64, u64, u64, u64, u64, String) = connection
        .query_row(
            "SELECT moving_time_revision,moving_time_ms,moving_time_observed_ms,
                moving_time_unknown_ms,moving_time_cursor_ms,moving_time_availability
         FROM rides WHERE id='existing-ride'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(checkpoint, (7, 1200, 2000, 300, 2300, "partial".into()));
    assert_eq!(count(&connection).unwrap(), 0);
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        super::super::migrations::CURRENT_SCHEMA_VERSION
    );
}

#[test]
fn charging_upgrade_accepts_earlier_charging_schema_without_moving_time() {
    let mut connection = database();
    let saved = save(
        &mut connection,
        &mut SpatialSchemaState::Uninitialized,
        &input(179.5),
    )
    .unwrap();
    let columns = connection
        .prepare(
            "SELECT name FROM pragma_table_info('rides')
         WHERE name LIKE 'moving_time_%' OR name LIKE 'last_recorded_point_moving_%'",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_ne!(columns.len(), 0);
    for column in columns {
        connection
            .execute_batch(&format!("ALTER TABLE rides DROP COLUMN {column};"))
            .unwrap();
    }
    connection.pragma_update(None, "user_version", 42).unwrap();
    super::super::migrations::migrate(&mut connection).unwrap();
    assert_eq!(
        detail(&connection, saved.point_id).unwrap().unwrap().input,
        Some(input(179.5))
    );
    assert_eq!(count(&connection).unwrap(), 1);
    let has_moving_time: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('rides') WHERE name='moving_time_revision')",
        [], |row|row.get(0),
    ).unwrap();
    assert!(has_moving_time);
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        super::super::migrations::CURRENT_SCHEMA_VERSION
    );
}

fn seed_elapsed_migration_ride(
    connection: &Connection,
    version: i64,
    include_elapsed_columns: bool,
) {
    connection
        .execute_batch(
            "INSERT INTO rides
            (id,source,state,created_at_ms,monotonic_created_at_ms,
             monotonic_last_event_ms,paused_duration_ms,completed_duration_ms,
             updated_at_ms,point_count,distance_mm,
             moving_time_revision,moving_time_ms,moving_time_observed_ms,
             moving_time_unknown_ms,moving_time_cursor_ms,
             last_recorded_point_moving_time_ms,
             last_recorded_point_moving_observed_ms,
             last_recorded_point_moving_unknown_ms,
             last_recorded_point_moving_availability,
             elapsed_time_ms,elapsed_time_anchor_ms,elapsed_time_running,
             elapsed_time_partial,last_recorded_point_elapsed_time_ms,
             last_recorded_point_elapsed_partial)
         VALUES ('existing-ride','live','saved',1,10,200,20,100,2,3,4,
                 7,100,150,50,200,90,130,40,'partial',
                 1500,2000,0,1,1400,1);",
        )
        .unwrap();
    if version < 45 {
        connection
            .execute_batch(
                "DROP TABLE charging_sync_conflicts;
                 DROP TABLE charging_sync_state;
                 ALTER TABLE charging_outbox DROP COLUMN blocked_kind;",
            )
            .unwrap();
    }
    if !include_elapsed_columns {
        connection
            .execute_batch(
                "ALTER TABLE rides DROP COLUMN elapsed_time_ms;
                 ALTER TABLE rides DROP COLUMN elapsed_time_anchor_ms;
                 ALTER TABLE rides DROP COLUMN elapsed_time_running;
                 ALTER TABLE rides DROP COLUMN elapsed_time_partial;
                 ALTER TABLE rides DROP COLUMN last_recorded_point_elapsed_time_ms;
                 ALTER TABLE rides DROP COLUMN last_recorded_point_elapsed_partial;",
            )
            .unwrap();
    }
    connection
        .pragma_update(None, "user_version", version)
        .unwrap();
}

fn assert_elapsed_migration_ride_preserved(connection: &Connection, has_elapsed: bool) {
    let elapsed = connection
        .query_row(
            "SELECT elapsed_time_ms,elapsed_time_anchor_ms,elapsed_time_running,
                    elapsed_time_partial,last_recorded_point_elapsed_time_ms,
                    last_recorded_point_elapsed_partial
             FROM rides WHERE id='existing-ride'",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<u64>>(0)?,
                    row.get::<_, Option<u64>>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, Option<u64>>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        elapsed,
        if has_elapsed {
            (Some(1500), Some(2000), false, true, Some(1400), true)
        } else {
            (None, None, false, false, None, false)
        }
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT created_at_ms,monotonic_created_at_ms,
                        monotonic_last_event_ms,paused_duration_ms,
                        completed_duration_ms,updated_at_ms,point_count,distance_mm,
                        moving_time_revision,moving_time_ms,moving_time_observed_ms,
                        moving_time_unknown_ms,moving_time_cursor_ms,
                        last_recorded_point_moving_time_ms,
                        last_recorded_point_moving_observed_ms,
                        last_recorded_point_moving_unknown_ms,
                        last_recorded_point_moving_availability
                 FROM rides WHERE id='existing-ride'",
                [],
                |row| {
                    Ok((
                        (
                            row.get::<_, i64>(0)?,
                            row.get::<_, Option<i64>>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                        ),
                        (
                            row.get::<_, i64>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, i64>(9)?,
                            row.get::<_, i64>(10)?,
                            row.get::<_, i64>(11)?,
                        ),
                        (
                            row.get::<_, Option<i64>>(12)?,
                            row.get::<_, i64>(13)?,
                            row.get::<_, i64>(14)?,
                            row.get::<_, i64>(15)?,
                            row.get::<_, String>(16)?,
                        ),
                    ))
                },
            )
            .unwrap(),
        (
            (1, Some(10), Some(200), 20, 100, 2),
            (3, 4, 7, 100, 150, 50),
            (Some(200), 90, 130, 40, "partial".to_owned()),
        )
    );
}

fn assert_charging_migration_state(
    connection: &mut Connection,
    point_id: MapPointId,
    pending: &ChargingOperation,
) {
    assert_eq!(
        detail(connection, point_id).unwrap().unwrap().input,
        Some(input(179.5))
    );
    assert_eq!(next_operation(connection).unwrap().as_ref(), Some(pending));
    assert_eq!(changes_cursor(connection).unwrap().get(), 0);
    assert_eq!(directory_last_updated(connection).unwrap(), None);
}

#[test]
fn charging_sync_upgrade_preserves_elapsed_time_and_pending_spots() {
    for (version, include_elapsed_columns) in [(45, false), (45, true), (43, false), (44, true)] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("charging.sqlite");
        let mut connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        super::super::migrations::migrate(&mut connection).unwrap();
        let saved = save(
            &mut connection,
            &mut SpatialSchemaState::Uninitialized,
            &input(179.5),
        )
        .unwrap();
        let pending = next_operation(&mut connection).unwrap().unwrap();
        seed_elapsed_migration_ride(&connection, version, include_elapsed_columns);

        if version == 45 && !include_elapsed_columns {
            assert!(super::super::migrations::verify_current_schema(&connection).is_err());
        }
        super::super::migrations::migrate(&mut connection).unwrap();
        super::super::migrations::verify_current_schema(&connection).unwrap();
        assert_elapsed_migration_ride_preserved(&connection, include_elapsed_columns);
        assert_charging_migration_state(&mut connection, saved.point_id, &pending);
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            46
        );

        drop(connection);
        let mut reopened = Connection::open(&path).unwrap();
        reopened.pragma_update(None, "foreign_keys", "ON").unwrap();
        super::super::migrations::migrate(&mut reopened).unwrap();
        super::super::migrations::verify_current_schema(&reopened).unwrap();
        assert_charging_migration_state(&mut reopened, saved.point_id, &pending);
        assert_eq!(
            reopened
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            46
        );
    }
}

#[test]
fn current_schema_verification_rejects_invalid_elapsed_column_shapes() {
    let wrong_type = database();
    wrong_type
        .execute_batch(
            "ALTER TABLE rides DROP COLUMN elapsed_time_ms;
             ALTER TABLE rides ADD COLUMN elapsed_time_ms TEXT;",
        )
        .unwrap();
    assert!(super::super::migrations::verify_current_schema(&wrong_type).is_err());

    let wrong_nullability = database();
    wrong_nullability
        .execute_batch(
            "ALTER TABLE rides DROP COLUMN elapsed_time_running;
             ALTER TABLE rides ADD COLUMN elapsed_time_running INTEGER DEFAULT 0;",
        )
        .unwrap();
    assert!(super::super::migrations::verify_current_schema(&wrong_nullability).is_err());
}
