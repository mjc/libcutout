use crate::{
    Error, Store,
    model::{
        ModerationAction, ModerationNote, ModerationRequest, ModeratorSpot, MutationReceipt,
        SpotStatus, ValidatedSpotInput,
    },
    store::{
        Actor, ModeratorActor, OperationKind, begin_write, clear_spot_votes, db_error,
        ensure_actor, fingerprint, json, mutation_receipt, next_change, next_revision, now_ms,
        remember, replay, state, write_quota,
    },
};
use cutout_ride_maps::ChargingCost;
use rusqlite::{Connection, params};

mod reports;
mod verification;

impl Store {
    pub(crate) async fn moderator_spot(
        &self,
        actor: ModeratorActor,
        target: String,
    ) -> Result<ModeratorSpot, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            let actor = actor.as_actor();
            ensure_actor(&transaction, actor)?;

            let current = state(&transaction, &target)?;
            let (owner_id, body) = spot_owner_and_body(&transaction, &target)?;
            let input = body
                .map(|body| serde_json::from_str(&body).map_err(|_| Error::Unavailable))
                .transpose()?;
            Ok(ModeratorSpot {
                state: current,
                owner_id,
                input,
            })
        })
        .await
    }

    pub(crate) async fn moderate(
        &self,
        actor: ModeratorActor,
        target: String,
        request: ModerationRequest<ValidatedSpotInput, ModerationNote>,
    ) -> Result<MutationReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            let actor = actor.as_actor();
            ensure_actor(&transaction, actor)?;
            let request_fingerprint = fingerprint(&format!("moderate:{target}"), &request)?;

            if let Some((replayed_target, revision)) = replay(
                &transaction,
                actor,
                &request.operation_id,
                &request_fingerprint,
            )? {
                return mutation_receipt(
                    &transaction,
                    &replayed_target,
                    request.operation_id,
                    revision,
                );
            }

            let current = ModeratableSpot::load(&transaction, &target, request.expected_revision)?;
            let revision = next_revision(current.revision)?;
            let now = now_ms()?;
            write_quota(&transaction, actor, now)?;
            let sequence = next_change(&transaction)?;
            let decision =
                moderation_decision(&transaction, &target, current.status, request.action)?;

            apply_moderation_decision(&transaction, &target, &decision)?;
            update_moderated_spot(&transaction, &target, &decision, revision, now, sequence)?;
            if decision.status() == SpotStatus::Removed {
                discard_removed_spot_content(&transaction, &target)?;
            }
            audit(
                &transaction,
                actor,
                &target,
                decision.audit_action(),
                &request.note,
                decision.canonical_id(),
                now,
            )?;
            remember(
                &transaction,
                actor,
                &request.operation_id,
                &request_fingerprint,
                &target,
                OperationKind::Spot,
                revision,
            )?;

            let receipt = mutation_receipt(&transaction, &target, request.operation_id, revision)?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        })
        .await
    }
}

#[derive(Clone, Copy, Debug)]
enum EditableStatus {
    Published,
    Hidden,
}

impl EditableStatus {
    const fn as_spot_status(self) -> SpotStatus {
        match self {
            Self::Published => SpotStatus::Published,
            Self::Hidden => SpotStatus::Hidden,
        }
    }
}

struct ModeratableSpot {
    revision: u64,
    status: EditableStatus,
}

impl ModeratableSpot {
    fn load(connection: &Connection, target: &str, expected_revision: u64) -> Result<Self, Error> {
        let current = state(connection, target)?;
        if current.revision != expected_revision {
            return Err(Error::Conflict);
        }
        let status = match current.status {
            SpotStatus::Published => EditableStatus::Published,
            SpotStatus::Hidden => EditableStatus::Hidden,
            SpotStatus::Withdrawn | SpotStatus::Removed => return Err(Error::Conflict),
        };
        Ok(Self {
            revision: current.revision,
            status,
        })
    }
}

#[derive(Debug)]
enum ModerationDecision {
    Hide,
    Restore,
    Remove,
    Correct {
        spot: ValidatedSpotInput,
        status: EditableStatus,
    },
    Duplicate {
        canonical_id: String,
    },
}

#[derive(Clone, Copy)]
enum AuditAction {
    Hide,
    Restore,
    Remove,
    Correct,
    Duplicate,
    ResolveReport,
}

impl AuditAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Hide => "hide",
            Self::Restore => "restore",
            Self::Remove => "remove",
            Self::Correct => "correct",
            Self::Duplicate => "duplicate",
            Self::ResolveReport => "resolve_report",
        }
    }
}

impl ModerationDecision {
    fn status(&self) -> SpotStatus {
        match self {
            Self::Hide => SpotStatus::Hidden,
            Self::Restore => SpotStatus::Published,
            Self::Remove | Self::Duplicate { .. } => SpotStatus::Removed,
            Self::Correct { status, .. } => status.as_spot_status(),
        }
    }

    fn audit_action(&self) -> AuditAction {
        match self {
            Self::Hide => AuditAction::Hide,
            Self::Restore => AuditAction::Restore,
            Self::Remove => AuditAction::Remove,
            Self::Correct { .. } => AuditAction::Correct,
            Self::Duplicate { .. } => AuditAction::Duplicate,
        }
    }

    fn canonical_id(&self) -> Option<&str> {
        match self {
            Self::Duplicate { canonical_id } => Some(canonical_id),
            Self::Hide | Self::Restore | Self::Remove | Self::Correct { .. } => None,
        }
    }
}

fn moderation_decision(
    connection: &Connection,
    target: &str,
    current: EditableStatus,
    action: ModerationAction<ValidatedSpotInput>,
) -> Result<ModerationDecision, Error> {
    match (action, current) {
        (ModerationAction::Hide, EditableStatus::Published) => Ok(ModerationDecision::Hide),
        (ModerationAction::Restore, EditableStatus::Hidden) => Ok(ModerationDecision::Restore),
        (ModerationAction::Remove, _) => Ok(ModerationDecision::Remove),
        (ModerationAction::Correct { spot }, status) => {
            Ok(ModerationDecision::Correct { spot, status })
        }
        (ModerationAction::Duplicate { canonical_id }, _) => {
            if canonical_id == target
                || state(connection, &canonical_id)?.status != SpotStatus::Published
            {
                return Err(Error::Invalid);
            }
            Ok(ModerationDecision::Duplicate { canonical_id })
        }
        (ModerationAction::Hide | ModerationAction::Restore, _) => Err(Error::Conflict),
    }
}

fn spot_owner_and_body(
    connection: &Connection,
    target: &str,
) -> Result<(String, Option<String>), Error> {
    connection
        .query_row(
            "SELECT owner_id, body FROM spots WHERE id = ?1",
            [target],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(db_error)
}

fn apply_moderation_decision(
    connection: &Connection,
    target: &str,
    decision: &ModerationDecision,
) -> Result<(), Error> {
    if let ModerationDecision::Correct { spot, .. } = decision {
        let spot = spot.as_input();
        let cost = match spot.cost {
            ChargingCost::Free => "free",
            ChargingCost::Paid => "paid",
        };
        connection
            .execute(
                "UPDATE spots
                 SET body = ?2, latitude_e7 = ?3, longitude_e7 = ?4,
                     cost = ?5, verified_at_ms = NULL
                 WHERE id = ?1",
                params![
                    target,
                    json(&spot)?,
                    spot.coordinate.latitude_e7,
                    spot.coordinate.longitude_e7,
                    cost
                ],
            )
            .map_err(db_error)?;
    }
    Ok(())
}

fn update_moderated_spot(
    connection: &Connection,
    target: &str,
    decision: &ModerationDecision,
    revision: u64,
    now: u64,
    sequence: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE spots
             SET status = ?2, revision = ?3, updated_at_ms = ?4,
                 change_sequence = ?5, duplicate_of = ?6
             WHERE id = ?1",
            params![
                target,
                decision.status().as_str(),
                revision,
                now,
                sequence,
                decision.canonical_id()
            ],
        )
        .map_err(db_error)?;
    Ok(())
}

fn discard_removed_spot_content(connection: &Connection, target: &str) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE spots
             SET body = NULL, latitude_e7 = 0, longitude_e7 = 0, verified_at_ms = NULL
             WHERE id = ?1",
            [target],
        )
        .map_err(db_error)?;
    clear_spot_votes(connection, target)
}

fn audit(
    connection: &Connection,
    actor: &Actor,
    target: &str,
    action: AuditAction,
    note: &ModerationNote,
    canonical: Option<&str>,
    now: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO moderation_audit
             (actor_id, target_id, action, note, canonical_id, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                actor.id,
                target,
                action.as_str(),
                note.as_str(),
                canonical,
                now
            ],
        )
        .map_err(db_error)?;
    Ok(())
}
