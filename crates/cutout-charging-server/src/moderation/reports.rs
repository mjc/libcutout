use super::{AuditAction, audit};
use crate::{
    Error, Store,
    model::{
        AuditPage, AuditRecord, ModerationNote, OperationId, ReportReason, ReportReceipt,
        ReportRecord, ReportRequest, ReportStatus, ReportsPage, ResolveRequest,
        ValidatedChangesQuery, ValidatedReportRequest, ValidatedReportsQuery,
    },
    store::{
        Actor, ModeratorActor, OperationKind, QuotaWindow, begin_write, db_error, ensure_actor,
        fingerprint, json, next_revision, now_ms, published_spot, quota, remember, replay,
        write_quota,
    },
};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

impl Store {
    pub(crate) async fn report(
        &self,
        actor: Actor,
        target: String,
        request: ValidatedReportRequest,
    ) -> Result<ReportReceipt, Error> {
        self.call(move |connection| {
            let request = request.as_request();
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            let request_fingerprint = fingerprint(&format!("report:{target}"), request)?;

            if let Some((report_id, revision)) = replay(
                &transaction,
                &actor,
                &request.operation_id,
                &request_fingerprint,
            )? {
                return report_receipt(
                    &transaction,
                    &report_id,
                    request.operation_id.clone(),
                    revision,
                );
            }
            published_spot(&transaction, &target, Error::NotFound)?;

            let now = now_ms()?;
            write_quota(&transaction, &actor, now)?;
            quota(
                &transaction,
                &format!("report:{}:{target}", actor.id),
                QuotaWindow::Day,
                3,
                now,
            )?;

            let report_id = Uuid::new_v4().to_string();
            insert_report(&transaction, &report_id, &target, &actor, request, now)?;
            remember(
                &transaction,
                &actor,
                &request.operation_id,
                &request_fingerprint,
                &report_id,
                OperationKind::Report,
                1,
            )?;
            let receipt =
                report_receipt(&transaction, &report_id, request.operation_id.clone(), 1)?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        })
        .await
    }

    pub(crate) async fn reports(
        &self,
        actor: ModeratorActor,
        query: ValidatedReportsQuery,
    ) -> Result<ReportsPage, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            let actor = actor.as_actor();
            ensure_actor(&transaction, actor)?;

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

    pub(crate) async fn resolve(
        &self,
        actor: ModeratorActor,
        target: String,
        request: ResolveRequest<ModerationNote>,
    ) -> Result<ReportReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            let actor = actor.as_actor();
            ensure_actor(&transaction, actor)?;
            let request_fingerprint = fingerprint(&format!("resolve:{target}"), &request)?;

            if let Some((report_id, revision)) = replay(
                &transaction,
                actor,
                &request.operation_id,
                &request_fingerprint,
            )? {
                return report_receipt(&transaction, &report_id, request.operation_id, revision);
            }

            let current = ReportRevision::load(&transaction, &target, request.expected_revision)?;
            let revision = next_revision(current.revision)?;
            let now = now_ms()?;
            write_quota(&transaction, actor, now)?;
            resolve_report(&transaction, &target, revision, now, &request.note)?;
            audit(
                &transaction,
                actor,
                &target,
                AuditAction::ResolveReport,
                &request.note,
                None,
                now,
            )?;
            remember(
                &transaction,
                actor,
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
        actor: ModeratorActor,
        query: ValidatedChangesQuery,
    ) -> Result<AuditPage, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            let actor = actor.as_actor();
            ensure_actor(&transaction, actor)?;

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

fn resolve_report(
    connection: &Connection,
    target: &str,
    revision: u64,
    now: u64,
    note: &ModerationNote,
) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE reports
             SET revision = ?2, status = 'resolved', resolved_at_ms = ?3,
                 resolution_note = ?4
             WHERE id = ?1",
            params![target, revision, now, note.as_str()],
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
