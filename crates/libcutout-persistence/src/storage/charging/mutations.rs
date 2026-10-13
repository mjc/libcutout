use super::super::{MapPointId, SpatialSchemaState, StorageError, ensure_spatial_schema};
use super::codec::input_json;
use super::records::{detail, remove_spatial_index, update_local_revision, update_spatial_index};
use super::sync::queue;
use super::{
    ChargingAction, ChargingRevision, ChargingStatus, ChargingWriteReceipt, StoredChargingSpot,
};
use cutout_ride_maps::{ChargingVote, NewChargingSpot};
use rusqlite::{Connection, params};

pub(super) fn save(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    input: &NewChargingSpot,
) -> Result<ChargingWriteReceipt, StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let id = MapPointId::new();
    update_spatial_index(&transaction, id, input, ChargingStatus::Queued)?;
    insert_local_spot(&transaction, id, input)?;
    let receipt = queue(
        &transaction,
        id,
        ChargingRevision::INITIAL,
        &ChargingAction::Create(input.clone()),
    )?;
    transaction.commit()?;
    Ok(receipt)
}
pub(super) fn change(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    id: MapPointId,
    expected: ChargingRevision,
    action: &ChargingAction,
) -> Result<ChargingWriteReceipt, StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let current = detail(&transaction, id)?.ok_or(StorageError::NotFound)?;
    let checked = CheckedChange::new(current, expected, action)?;
    checked.apply(&transaction)?;
    let revision = checked.revision;
    update_local_revision(&transaction, id, revision)?;
    let receipt = queue(&transaction, id, revision, action)?;
    transaction.commit()?;
    Ok(receipt)
}

/// A local mutation after revision, visibility and ownership checks.
struct CheckedChange<'a> {
    current: StoredChargingSpot,
    revision: ChargingRevision,
    action: SpotChange<'a>,
}

enum SpotChange<'a> {
    Edit(&'a NewChargingSpot),
    Withdraw,
    Verify,
    Vote(Option<ChargingVote>),
}

impl<'a> CheckedChange<'a> {
    fn new(
        current: StoredChargingSpot,
        expected: ChargingRevision,
        action: &'a ChargingAction,
    ) -> Result<Self, StorageError> {
        if current.local_revision != expected {
            return Err(StorageError::ChargingConflict);
        }
        if !(current.status.visible()
            || (current.owned
                && current.status == ChargingStatus::Hidden
                && match action {
                    ChargingAction::Edit(_) | ChargingAction::Withdraw => true,
                    ChargingAction::Create(_)
                    | ChargingAction::Verify
                    | ChargingAction::Vote(_) => false,
                }))
        {
            return Err(StorageError::ChargingConflict);
        }
        let revision = expected.next()?;
        let action = match action {
            ChargingAction::Create(_) => return Err(StorageError::ChargingInvalidInput),
            ChargingAction::Edit(_) | ChargingAction::Withdraw if !current.owned => {
                return Err(StorageError::ChargingNotOwned);
            }
            ChargingAction::Edit(input) => SpotChange::Edit(input),
            ChargingAction::Withdraw => SpotChange::Withdraw,
            ChargingAction::Verify => SpotChange::Verify,
            ChargingAction::Vote(vote) => SpotChange::Vote(*vote),
        };
        Ok(Self {
            current,
            revision,
            action,
        })
    }

    fn apply(&self, transaction: &Connection) -> Result<(), StorageError> {
        let id = self.current.point_id;
        match &self.action {
            SpotChange::Edit(input) => {
                update_spatial_index(transaction, id, input, self.current.status)?;
                transaction.execute(
                    "UPDATE charging_spots SET input=?2,verified_at_ms=NULL WHERE point_id=?1",
                    params![id.uuid().as_bytes(), input_json(input)?],
                )?;
            }
            SpotChange::Withdraw => {
                remove_spatial_index(transaction, id)?;
                transaction.execute("UPDATE charging_spots SET status='withdrawn',input=NULL,verified_at_ms=NULL WHERE point_id=?1", [id.uuid().as_bytes()])?;
            }
            SpotChange::Verify => {}
            SpotChange::Vote(vote) => {
                transaction.execute(
                    "UPDATE charging_spots SET vote=?2 WHERE point_id=?1",
                    params![
                        id.uuid().as_bytes(),
                        vote.map(|value| match value {
                            ChargingVote::Up => "up",
                            ChargingVote::Down => "down",
                        })
                    ],
                )?;
            }
        }
        Ok(())
    }
}

fn insert_local_spot(
    connection: &Connection,
    id: MapPointId,
    input: &NewChargingSpot,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO charging_spots(point_id,input,owned,status,local_revision)
         VALUES (?1,?2,1,'queued',1)",
        params![id.uuid().as_bytes(), input_json(input)?],
    )?;
    Ok(())
}
