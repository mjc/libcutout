use crate::{
    Error, Store,
    model::{
        AuditPage, AuditRecord, ModerationAction, ModerationRequest, ModeratorSpot,
        MutationReceipt, OperationId, ReportReason, ReportReceipt, ReportRecord, ReportRequest,
        ReportStatus, ReportsPage, ResolveRequest, SpotStatus, ValidatedChangesQuery,
        ValidatedReportsQuery, ValidatedSpotInput, WithdrawRequest,
    },
    store::{
        Actor, OperationKind, Role, begin_write, clear_spot_votes, db_error, ensure_actor,
        fingerprint, json, mutation_receipt, next_change, next_revision, now_ms, quota, remember,
        replay, state, write_quota,
    },
};
use cutout_ride_maps::ChargingCost;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

impl Store {
    pub(crate) async fn verify_access(
        &self,
        actor: Actor,
        target: String,
        request: WithdrawRequest,
    ) -> Result<MutationReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            let request_fingerprint = fingerprint(&format!("verify:{target}"), &request)?;

            if let Some((replayed_target, revision)) = replay(
                &transaction,
                &actor,
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

            let current = state(&transaction, &target)?;
            if current.status != SpotStatus::Published {
                return Err(Error::NotFound);
            }
            if current.revision != request.expected_revision {
                return Err(Error::Conflict);
            }

            let revision = next_revision(current.revision)?;
            let now = now_ms()?;
            write_quota(&transaction, &actor, now)?;
            insert_verification(
                &transaction,
                &target,
                &actor,
                &request,
                current.revision,
                now,
            )?;
            update_verification_state(&transaction, &target, revision, now)?;
            remember(
                &transaction,
                &actor,
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

    pub(crate) async fn report(
        &self,
        actor: Actor,
        target: String,
        request: ReportRequest,
    ) -> Result<ReportReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            let request_fingerprint = fingerprint(&format!("report:{target}"), &request)?;

            if let Some((report_id, revision)) = replay(
                &transaction,
                &actor,
                &request.operation_id,
                &request_fingerprint,
            )? {
                return report_receipt(&transaction, &report_id, request.operation_id, revision);
            }
            if state(&transaction, &target)?.status != SpotStatus::Published {
                return Err(Error::NotFound);
            }

            let now = now_ms()?;
            write_quota(&transaction, &actor, now)?;
            quota(
                &transaction,
                &format!("report:{}:{target}", actor.id),
                86_400_000,
                3,
                now,
            )?;

            let report_id = Uuid::new_v4().to_string();
            insert_report(&transaction, &report_id, &target, &actor, &request, now)?;
            remember(
                &transaction,
                &actor,
                &request.operation_id,
                &request_fingerprint,
                &report_id,
                OperationKind::Report,
                1,
            )?;
            let receipt = report_receipt(&transaction, &report_id, request.operation_id, 1)?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        })
        .await
    }

    pub(crate) async fn reports(
        &self,
        actor: Actor,
        query: ValidatedReportsQuery,
    ) -> Result<ReportsPage, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            moderator(&transaction, &actor)?;

            let page_size = query.limit();
            let targets = report_ids(
                &transaction,
                query.after(),
                query.status().map(ReportStatus::as_str),
                page_size.lookahead(),
            )?;
            let has_more = targets.len() > page_size.count();
            let items = targets
                .into_iter()
                .take(page_size.count())
                .map(|report_id| report_record(&transaction, &report_id))
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = if has_more {
                items.last().map(|record| record.id.clone())
            } else {
                None
            };
            Ok(ReportsPage { items, next_cursor })
        })
        .await
    }

    pub(crate) async fn moderator_spot(
        &self,
        actor: Actor,
        target: String,
    ) -> Result<ModeratorSpot, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            moderator(&transaction, &actor)?;

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
        actor: Actor,
        target: String,
        request: ModerationRequest<ValidatedSpotInput>,
    ) -> Result<MutationReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            moderator(&transaction, &actor)?;
            let request_fingerprint = fingerprint(&format!("moderate:{target}"), &request)?;

            if let Some((replayed_target, revision)) = replay(
                &transaction,
                &actor,
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
            write_quota(&transaction, &actor, now)?;
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
                &actor,
                &target,
                decision.audit_action().as_str(),
                &request.note,
                decision.canonical_id(),
                now,
            )?;
            remember(
                &transaction,
                &actor,
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

    pub(crate) async fn resolve(
        &self,
        actor: Actor,
        target: String,
        request: ResolveRequest,
    ) -> Result<ReportReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            moderator(&transaction, &actor)?;
            let request_fingerprint = fingerprint(&format!("resolve:{target}"), &request)?;

            if let Some((report_id, revision)) = replay(
                &transaction,
                &actor,
                &request.operation_id,
                &request_fingerprint,
            )? {
                return report_receipt(&transaction, &report_id, request.operation_id, revision);
            }

            let current = ReportRevision::load(&transaction, &target, request.expected_revision)?;
            let revision = next_revision(current.revision)?;
            let now = now_ms()?;
            write_quota(&transaction, &actor, now)?;
            resolve_report(&transaction, &target, revision, now, &request.note)?;
            audit(
                &transaction,
                &actor,
                &target,
                AuditAction::ResolveReport.as_str(),
                &request.note,
                None,
                now,
            )?;
            remember(
                &transaction,
                &actor,
                &request.operation_id,
                &request_fingerprint,
                &target,
                OperationKind::Report,
                revision,
            )?;

            let receipt = report_receipt(&transaction, &target, request.operation_id, revision)?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        })
        .await
    }

    pub(crate) async fn audit(
        &self,
        actor: Actor,
        query: ValidatedChangesQuery,
    ) -> Result<AuditPage, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            moderator(&transaction, &actor)?;

            let after = query.after();
            let page_size = query.limit();
            let mut items = audit_records(&transaction, after, page_size.lookahead())?;
            let has_more = items.len() > page_size.count();
            items.truncate(page_size.count());
            let next_cursor = items.last().map_or(after, |record| record.sequence);
            Ok(AuditPage {
                items,
                next_cursor,
                has_more,
            })
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

struct ReportRevision {
    revision: u64,
}

impl ReportRevision {
    fn load(connection: &Connection, target: &str, expected: u64) -> Result<Self, Error> {
        let current = report_record(connection, target)?;
        if current.revision != expected || current.status != ReportStatus::Open {
            return Err(Error::Conflict);
        }
        Ok(Self {
            revision: current.revision,
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

fn moderator(connection: &Connection, actor: &Actor) -> Result<(), Error> {
    ensure_actor(connection, actor)?;
    if actor.role != Role::Moderator {
        return Err(Error::Forbidden);
    }
    Ok(())
}

fn insert_verification(
    connection: &Connection,
    target: &str,
    actor: &Actor,
    request: &WithdrawRequest,
    revision: u64,
    now: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO verifications VALUES (?1, ?2, ?3, ?4, ?5)",
            params![target, actor.id, request.operation_id, revision, now],
        )
        .map_err(db_error)?;
    Ok(())
}

fn update_verification_state(
    connection: &Connection,
    target: &str,
    revision: u64,
    now: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE spots
             SET revision = ?2, verified_at_ms = ?3, change_sequence = ?4
             WHERE id = ?1",
            params![target, revision, now, next_change(connection)?],
        )
        .map_err(db_error)?;
    Ok(())
}

fn insert_report(
    connection: &Connection,
    id: &str,
    target: &str,
    actor: &Actor,
    request: &ReportRequest,
    now: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO reports
             (id, spot_id, reporter_id, revision, reason, message, status, created_at_ms)
             VALUES (?1, ?2, ?3, 1, ?4, ?5, 'open', ?6)",
            params![
                id,
                target,
                actor.id,
                json(&request.reason)?,
                request.message,
                now
            ],
        )
        .map_err(db_error)?;
    Ok(())
}

fn report_ids(
    connection: &Connection,
    after: &str,
    status: Option<&str>,
    limit: u32,
) -> Result<Vec<String>, Error> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM reports
             WHERE id > ?1 AND (?2 IS NULL OR status = ?2)
             ORDER BY id LIMIT ?3",
        )
        .map_err(db_error)?;
    statement
        .query_map(params![after, status, limit], |row| row.get(0))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)
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

fn resolve_report(
    connection: &Connection,
    target: &str,
    revision: u64,
    now: u64,
    note: &str,
) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE reports
             SET revision = ?2, status = 'resolved', resolved_at_ms = ?3,
                 resolution_note = ?4
             WHERE id = ?1",
            params![target, revision, now, note],
        )
        .map_err(db_error)?;
    Ok(())
}

fn report_record(connection: &Connection, target: &str) -> Result<ReportRecord, Error> {
    let row = connection
        .query_row(
            "SELECT id, spot_id, reporter_id, revision, reason, message, status,
                    created_at_ms, resolved_at_ms, resolution_note
             FROM reports WHERE id = ?1",
            [target],
            decode_report_row,
        )
        .optional()
        .map_err(db_error)?
        .ok_or(Error::NotFound)?;
    ReportRecord::try_from(row).map_err(|_| Error::Unavailable)
}

struct ReportRow {
    id: String,
    spot_id: String,
    reporter_id: String,
    revision: u64,
    reason_json: String,
    message: Option<String>,
    status: ReportStatus,
    created_at_ms: u64,
    resolved_at_ms: Option<u64>,
    resolution_note: Option<String>,
}

fn decode_report_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReportRow> {
    Ok(ReportRow {
        id: row.get(0)?,
        spot_id: row.get(1)?,
        reporter_id: row.get(2)?,
        revision: row.get(3)?,
        reason_json: row.get(4)?,
        message: row.get(5)?,
        status: row.get(6)?,
        created_at_ms: row.get(7)?,
        resolved_at_ms: row.get(8)?,
        resolution_note: row.get(9)?,
    })
}

impl TryFrom<ReportRow> for ReportRecord {
    type Error = serde_json::Error;
    fn try_from(row: ReportRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            spot_id: row.spot_id,
            reporter_id: row.reporter_id,
            revision: row.revision,
            reason: serde_json::from_str::<ReportReason>(&row.reason_json)?,
            message: row.message,
            status: row.status,
            created_at_ms: row.created_at_ms,
            resolved_at_ms: row.resolved_at_ms,
            resolution_note: row.resolution_note,
        })
    }
}

fn report_receipt(
    connection: &Connection,
    target: &str,
    operation: OperationId,
    applied: u64,
) -> Result<ReportReceipt, Error> {
    let report = report_record(connection, target)?;
    Ok(ReportReceipt {
        operation_id: operation,
        id: report.id,
        applied_revision: applied,
        revision: report.revision,
        status: report.status,
    })
}

fn audit(
    connection: &Connection,
    actor: &Actor,
    target: &str,
    action: &str,
    note: &str,
    canonical: Option<&str>,
    now: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO moderation_audit
             (actor_id, target_id, action, note, canonical_id, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![actor.id, target, action, note, canonical, now],
        )
        .map_err(db_error)?;
    Ok(())
}

fn audit_records(
    connection: &Connection,
    after: u64,
    limit: u32,
) -> Result<Vec<AuditRecord>, Error> {
    let mut statement = connection
        .prepare(
            "SELECT sequence, actor_id, target_id, action, note, canonical_id, created_at_ms
             FROM moderation_audit WHERE sequence > ?1 ORDER BY sequence LIMIT ?2",
        )
        .map_err(db_error)?;
    statement
        .query_map(params![after, limit], decode_audit_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)
}

fn decode_audit_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditRecord> {
    Ok(AuditRecord {
        sequence: row.get(0)?,
        actor_id: row.get(1)?,
        target_id: row.get(2)?,
        action: row.get(3)?,
        note: row.get(4)?,
        canonical_id: row.get(5)?,
        created_at_ms: row.get(6)?,
    })
}
