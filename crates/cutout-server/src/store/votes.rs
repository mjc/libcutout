use super::{
    Actor, OperationKind, Store, begin_write, db_error, ensure_actor, fingerprint, next_change,
    next_revision, now_ms, published_spot, remember, replay, write_quota,
};
use crate::{
    Error,
    model::{Counts, OperationId, SpotStatus, VoteReceipt, VoteRequest, VoteState},
};
use cutout_ride_maps::ChargingVote;
use rusqlite::{Connection, OptionalExtension, params};

impl Store {
    pub(crate) async fn vote(
        &self,
        actor: Actor,
        target: String,
        request: VoteRequest,
    ) -> Result<VoteReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            let fingerprint = fingerprint(&format!("vote:{target}"), &request)?;
            if let Some((target, revision)) =
                replay(&transaction, &actor, &request.operation_id, &fingerprint)?
            {
                return vote_receipt(
                    &transaction,
                    &target,
                    &actor,
                    request.operation_id,
                    revision,
                );
            }
            let revision =
                next_vote_revision(&transaction, &target, &actor, request.expected_revision)?;
            write_quota(&transaction, &actor, now_ms()?)?;
            save_vote(&transaction, &target, &actor, revision, request.vote)?;
            advance_spot_change(&transaction, &target)?;
            remember(
                &transaction,
                &actor,
                &request.operation_id,
                &fingerprint,
                &target,
                OperationKind::Vote,
                revision,
            )?;
            let receipt = vote_receipt(
                &transaction,
                &target,
                &actor,
                request.operation_id,
                revision,
            )?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        })
        .await
    }

    pub(crate) async fn current_vote(
        &self,
        actor: Actor,
        target: String,
    ) -> Result<VoteState, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            ensure_actor(&transaction, &actor)?;
            let state = vote_state(&transaction, &target, &actor)?;
            if state.spot_status != SpotStatus::Published {
                return Err(Error::NotFound);
            }
            Ok(state)
        })
        .await
    }
}

pub(crate) fn clear_spot_votes(connection: &Connection, target: &str) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE votes SET value = NULL,
            revision = CASE WHEN revision < 9223372036854775807 THEN revision + 1 ELSE revision END
         WHERE spot_id = ?1",
            [target],
        )
        .map_err(db_error)?;
    Ok(())
}

fn next_vote_revision(
    connection: &Connection,
    target: &str,
    actor: &Actor,
    expected_revision: u64,
) -> Result<u64, Error> {
    published_spot(connection, target, Error::NotFound)?;
    let current = vote_state(connection, target, actor)?;
    if current.revision != expected_revision {
        return Err(Error::Conflict);
    }
    next_revision(current.revision)
}

fn save_vote(
    connection: &Connection,
    target: &str,
    actor: &Actor,
    revision: u64,
    vote: Option<ChargingVote>,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO votes (spot_id, contributor_id, revision, value)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (spot_id, contributor_id) DO UPDATE
         SET revision = excluded.revision, value = excluded.value",
            params![target, actor.id, revision, vote.map(vote_value)],
        )
        .map_err(db_error)?;
    Ok(())
}

pub(super) fn advance_spot_change(connection: &Connection, target: &str) -> Result<(), Error> {
    // Vote counts advance discovery/sync without advancing the spot's edit revision.
    connection
        .execute(
            "UPDATE spots SET change_sequence = ?2 WHERE id = ?1",
            params![target, next_change(connection)?],
        )
        .map_err(db_error)?;
    Ok(())
}

fn vote_receipt(
    connection: &Connection,
    target: &str,
    actor: &Actor,
    operation_id: OperationId,
    applied_revision: u64,
) -> Result<VoteReceipt, Error> {
    Ok(VoteReceipt {
        operation_id,
        applied_revision,
        state: vote_state(connection, target, actor)?,
    })
}

fn vote_value(value: ChargingVote) -> &'static str {
    match value {
        ChargingVote::Up => "up",
        ChargingVote::Down => "down",
    }
}

fn vote_state(connection: &Connection, target: &str, actor: &Actor) -> Result<VoteState, Error> {
    let (status, sequence) = connection
        .query_row(
            "SELECT status,change_sequence FROM spots WHERE id=?1",
            [target],
            |row| Ok((row.get::<_, SpotStatus>(0)?, row.get::<_, u64>(1)?)),
        )
        .optional()
        .map_err(db_error)?
        .ok_or(Error::NotFound)?;
    let current = connection
        .query_row(
            "SELECT revision,value FROM votes WHERE spot_id=?1 AND contributor_id=?2",
            params![target, actor.id],
            StoredVote::read,
        )
        .optional()
        .map_err(db_error)?
        .unwrap_or_default();
    Ok(VoteState {
        revision: current.revision,
        vote: current.vote,
        votes: if status == SpotStatus::Published {
            counts(connection, target)?
        } else {
            Counts::default()
        },
        spot_status: status,
        change_sequence: sequence,
    })
}

#[derive(Default)]
struct StoredVote {
    revision: u64,
    vote: Option<ChargingVote>,
}

impl StoredVote {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let revision = row.get(0)?;
        let value: Option<String> = row.get(1)?;
        let vote = match value.as_deref() {
            Some("up") => Some(ChargingVote::Up),
            Some("down") => Some(ChargingVote::Down),
            None => None,
            _ => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(Error::Unavailable),
                ));
            }
        };
        Ok(Self { revision, vote })
    }
}

pub(super) fn counts(connection: &Connection, target: &str) -> Result<Counts, Error> {
    connection
        .query_row(
            "SELECT COALESCE(SUM(value = 'up'), 0), COALESCE(SUM(value = 'down'), 0)
         FROM votes WHERE spot_id = ?1",
            [target],
            |row| {
                Ok(Counts {
                    up: row.get(0)?,
                    down: row.get(1)?,
                })
            },
        )
        .map_err(db_error)
}
