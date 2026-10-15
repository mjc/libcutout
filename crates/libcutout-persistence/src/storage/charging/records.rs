use super::super::{MapPointId, SpatialRowId, StorageError};
use super::codec::{Input, decode, input_json};
use super::model::ValidatedDirectoryEntry;
use super::{ChargingRevision, ChargingStatus, StoredChargingSpot};
use cutout_ride_maps::{ChargingSpotId, ChargingVote, Coordinate, NewChargingSpot};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

pub(super) fn point_id_from_bytes(value: &[u8]) -> Result<MapPointId, StorageError> {
    Uuid::from_slice(value)
        .map(MapPointId::from_uuid)
        .map_err(|_| StorageError::ChargingInvalidInput)
}
pub(super) fn update_spatial_index(
    connection: &Connection,
    id: MapPointId,
    input: &NewChargingSpot,
    status: ChargingStatus,
) -> Result<(), StorageError> {
    let pin = input.coordinate();
    connection.execute("INSERT INTO map_points(id,name,latitude_e7,longitude_e7) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name,latitude_e7=excluded.latitude_e7,longitude_e7=excluded.longitude_e7", params![id.uuid().as_bytes(), input.details().name().unwrap_or("Charging spot"), pin.latitude().as_i32(), pin.longitude().as_i32()])?;
    connection.execute(
        "INSERT INTO map_point_spatial_keys(point_id) VALUES (?1) ON CONFLICT(point_id) DO NOTHING",
        [id.uuid().as_bytes()],
    )?;
    let spatial_id = SpatialRowId::from_sqlite(connection.query_row(
        "SELECT rtree_id FROM map_point_spatial_keys WHERE point_id=?1",
        [id.uuid().as_bytes()],
        |row| row.get(0),
    )?)?;
    connection.execute(
        "DELETE FROM map_points_rtree WHERE id=?1",
        [spatial_id.get()],
    )?;
    if status.visible() {
        connection.execute(
            "INSERT INTO map_points_rtree VALUES (?1,?2,?2,?3,?3)",
            params![
                spatial_id.get(),
                pin.latitude().as_i32(),
                pin.longitude().as_i32()
            ],
        )?;
    }
    Ok(())
}
pub(super) fn remove_spatial_index(
    connection: &Connection,
    id: MapPointId,
) -> Result<(), StorageError> {
    connection.execute("DELETE FROM map_points_rtree WHERE id IN (SELECT rtree_id FROM map_point_spatial_keys WHERE point_id=?1)", [id.uuid().as_bytes()])?;
    Ok(())
}
pub(super) fn detail(
    connection: &Connection,
    id: MapPointId,
) -> Result<Option<StoredChargingSpot>, StorageError> {
    let mut statement = connection.prepare("SELECT input,owned,status,local_revision,remote_id,remote_revision,change_sequence,verified_at_ms,vote,vote_revision,votes_up,votes_down,(SELECT COUNT(*) FROM charging_outbox o WHERE o.point_id=charging_spots.point_id),(SELECT EXISTS(SELECT 1 FROM charging_outbox o WHERE o.point_id=charging_spots.point_id AND json_type(o.action,'$.Vote') IS NOT NULL)) FROM charging_spots WHERE point_id=?1")?;
    let mut rows = statement.query([id.uuid().as_bytes()])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    decode_spot(row, id).map(Some)
}

fn decode_spot(
    row: &rusqlite::Row<'_>,
    id: MapPointId,
) -> Result<StoredChargingSpot, StorageError> {
    let input: Option<String> = row.get(0)?;
    let remote: Option<String> = row.get(4)?;
    let vote: Option<String> = row.get(8)?;
    Ok(StoredChargingSpot {
        point_id: id,
        input: input
            .map(|value| decode::<Input>(&value)?.validated())
            .transpose()?,
        owned: row.get(1)?,
        status: ChargingStatus::parse(&row.get::<_, String>(2)?)?,
        local_revision: ChargingRevision::new(row.get(3)?)?,
        remote_id: remote
            .map(|value| {
                ChargingSpotId::parse(&value).map_err(|_| StorageError::ChargingInvalidInput)
            })
            .transpose()?,
        remote_revision: ChargingRevision::new(row.get(5)?)?,
        change_sequence: ChargingRevision::new(row.get(6)?)?,
        verified_at_ms: row.get(7)?,
        vote: vote
            .map(|value| decode(&format!("\"{value}\"")))
            .transpose()?,
        vote_revision: ChargingRevision::new(row.get(9)?)?,
        votes_up: row.get(10)?,
        votes_down: row.get(11)?,
        pending_operations: row.get(12)?,
        pending_vote: row.get(13)?,
    })
}
pub(super) fn find_remote(
    connection: &Connection,
    id: ChargingSpotId,
) -> Result<Option<MapPointId>, StorageError> {
    let point: Option<Vec<u8>> = connection
        .query_row(
            "SELECT point_id FROM charging_spots WHERE remote_id=?1",
            [id.as_uuid().to_string()],
            |row| row.get(0),
        )
        .optional()?;
    point.map(|value| point_id_from_bytes(&value)).transpose()
}
pub(super) fn insert_directory(
    connection: &Connection,
    id: MapPointId,
    checked: &ValidatedDirectoryEntry<'_>,
) -> Result<(), StorageError> {
    let entry = checked.as_entry();
    let coordinate = entry.input.as_ref().map_or(
        Coordinate::from_fixed_parts(0, 0).map_err(|_| StorageError::ChargingInvalidInput)?,
        NewChargingSpot::coordinate,
    );
    connection.execute("INSERT INTO map_points(id,name,latitude_e7,longitude_e7) VALUES (?1,'Charging spot',?2,?3)", params![id.uuid().as_bytes(), coordinate.latitude().as_i32(), coordinate.longitude().as_i32()])?;
    connection.execute(
        "INSERT INTO charging_spots(point_id,owned,status,local_revision) VALUES (?1,0,?2,1)",
        params![id.uuid().as_bytes(), entry.status.as_str()],
    )?;
    Ok(())
}
pub(super) fn apply_entry(
    connection: &Connection,
    id: MapPointId,
    checked: &ValidatedDirectoryEntry<'_>,
    update: EntryUpdate,
) -> Result<(), StorageError> {
    let anonymous;
    let entry = if super::account::state(connection)? == super::CommunityAccountState::Active {
        checked.as_entry()
    } else {
        anonymous = super::ChargingDirectoryEntry {
            vote: None,
            vote_revision: ChargingRevision::ZERO,
            ..checked.as_entry().clone()
        };
        &anonymous
    };
    match update {
        EntryUpdate::ReplaceBody => {
            if let Some(input) = &entry.input {
                update_spatial_index(connection, id, input, entry.status)?;
            } else {
                remove_spatial_index(connection, id)?;
            }
            connection.execute(
                "UPDATE charging_spots SET input=?2,status=?3,vote=?4 WHERE point_id=?1",
                params![
                    id.uuid().as_bytes(),
                    entry.input.as_ref().map(input_json).transpose()?,
                    entry.status.as_str(),
                    entry.vote.map(|value| match value {
                        ChargingVote::Up => "up",
                        ChargingVote::Down => "down",
                    })
                ],
            )?;
        }
        EntryUpdate::PreserveInput => {
            let input: Option<String> = connection.query_row(
                "SELECT input FROM charging_spots WHERE point_id=?1",
                [id.uuid().as_bytes()],
                |row| row.get(0),
            )?;
            if let Some(input) = input.map(|value| decode::<Input>(&value)).transpose()? {
                let input = input.validated()?;
                update_spatial_index(connection, id, &input, entry.status)?;
            } else {
                remove_spatial_index(connection, id)?;
            }
            connection.execute(
                "UPDATE charging_spots SET status=?2,vote=?3 WHERE point_id=?1",
                params![
                    id.uuid().as_bytes(),
                    entry.status.as_str(),
                    entry.vote.map(|value| match value {
                        ChargingVote::Up => "up",
                        ChargingVote::Down => "down",
                    })
                ],
            )?;
        }
        EntryUpdate::PreserveLocal => {}
    }
    connection.execute("UPDATE charging_spots SET remote_id=?2,remote_revision=?3,change_sequence=?4,verified_at_ms=CASE WHEN ?9 THEN ?5 ELSE verified_at_ms END,vote_revision=?6,votes_up=?7,votes_down=?8 WHERE point_id=?1", params![id.uuid().as_bytes(), entry.id.as_uuid().to_string(), entry.revision.get(), entry.change_sequence.get(), entry.verified_at_ms, entry.vote_revision.get(), entry.votes_up, entry.votes_down, update.updates_verification()])?;
    Ok(())
}

/// Whether a server response may replace local fields and requested vote state.
pub(super) enum EntryUpdate {
    ReplaceBody,
    PreserveInput,
    PreserveLocal,
}

impl EntryUpdate {
    fn updates_verification(self) -> bool {
        match self {
            Self::ReplaceBody => true,
            Self::PreserveInput | Self::PreserveLocal => false,
        }
    }
}

pub(super) fn update_local_revision(
    connection: &Connection,
    id: MapPointId,
    revision: ChargingRevision,
) -> Result<(), StorageError> {
    connection.execute(
        "UPDATE charging_spots SET local_revision=?2 WHERE point_id=?1",
        params![id.uuid().as_bytes(), revision.get()],
    )?;
    Ok(())
}
