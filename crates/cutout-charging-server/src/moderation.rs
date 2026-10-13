use crate::{
    Error, Store,
    model::{
        AuditPage, AuditRecord, ChangesQuery, ModerationAction, ModerationRequest, ModeratorSpot,
        MutationReceipt, ReportReason, ReportReceipt, ReportRecord, ReportRequest, ReportStatus,
        ReportsPage, ReportsQuery, ResolveRequest, WithdrawRequest, id,
    },
    store::{
        Actor, db_error, ensure_actor, fingerprint, json, next_change, next_revision, now_ms,
        quota, remember, replay, state, write_quota,
    },
};
use cutout_ride_maps::ChargingCost;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

impl Store {
    pub(crate) async fn verify_access(
        &self,
        actor: Actor,
        target: String,
        request: WithdrawRequest,
    ) -> Result<MutationReceipt, Error> {
        self.call(move |connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            ensure_actor(&transaction, &actor)?;
            let fingerprint = fingerprint(&format!("verify:{target}"), &request)?;
            if let Some((target, revision)) =
                replay(&transaction, &actor, &request.operation_id, &fingerprint)?
            {
                return Ok(MutationReceipt {
                    operation_id: request.operation_id,
                    applied_revision: revision,
                    state: state(&transaction, &target)?,
                });
            }
            let current = state(&transaction, &target)?;
            if current.status != "published" {
                return Err(Error::NotFound);
            }
            if current.revision != request.expected_revision {
                return Err(Error::Conflict);
            }
            let revision = next_revision(current.revision)?;
            let now = now_ms()?;
            write_quota(&transaction, &actor, now)?;
            transaction
                .execute(
                    "INSERT INTO verifications VALUES (?1,?2,?3,?4,?5)",
                    params![
                        target,
                        actor.id,
                        request.operation_id,
                        current.revision,
                        now
                    ],
                )
                .map_err(db_error)?;
            transaction
                .execute(
                    "UPDATE spots SET revision=?2,verified_at_ms=?3,change_sequence=?4 WHERE id=?1",
                    params![target, revision, now, next_change(&transaction)?],
                )
                .map_err(db_error)?;
            remember(
                &transaction,
                &actor,
                &request.operation_id,
                &fingerprint,
                &target,
                "spot",
                revision,
            )?;
            let state = state(&transaction, &target)?;
            transaction.commit().map_err(db_error)?;
            Ok(MutationReceipt {
                operation_id: request.operation_id,
                applied_revision: revision,
                state,
            })
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
            let transaction=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            ensure_actor(&transaction,&actor)?;
            let fingerprint=fingerprint(&format!("report:{target}"),&request)?;
            if let Some((target,revision))=replay(&transaction,&actor,&request.operation_id,&fingerprint)? {return report_receipt(&transaction,&target,request.operation_id,revision);}
            if state(&transaction,&target)?.status!="published" {return Err(Error::NotFound);}
            let now=now_ms()?;
            write_quota(&transaction,&actor,now)?;
            quota(&transaction,&format!("report:{}:{target}",actor.id),86_400_000,3,now)?;
            let report=Uuid::new_v4().to_string();
            transaction.execute("INSERT INTO reports (id,spot_id,reporter_id,revision,reason,message,status,created_at_ms) VALUES (?1,?2,?3,1,?4,?5,'open',?6)",params![report,target,actor.id,json(&request.reason)?,request.message,now]).map_err(db_error)?;
            remember(&transaction,&actor,&request.operation_id,&fingerprint,&report,"report",1)?;
            let receipt=report_receipt(&transaction,&report,request.operation_id,1)?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        }).await
    }

    pub(crate) async fn reports(
        &self,
        actor: Actor,
        query: ReportsQuery,
    ) -> Result<ReportsPage, Error> {
        self.call(move |connection| {
            let transaction=connection.transaction().map_err(db_error)?;
            moderator(&transaction,&actor)?;
            let limit=query.limit.unwrap_or(50);
            let after=query.after.as_deref().map(id).transpose()?.unwrap_or_default();
            let status=query.status.map(|value|match value {ReportStatus::Open=>"open",ReportStatus::Resolved=>"resolved"});
            let mut statement=transaction.prepare("SELECT id FROM reports WHERE id>?1 AND (?2 IS NULL OR status=?2) ORDER BY id LIMIT ?3").map_err(db_error)?;
            let targets=statement.query_map(params![after,status,limit+1],|row|row.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            let has_more=targets.len()>limit as usize;
            let items=targets.into_iter().take(limit as usize).map(|target|report_record(&transaction,&target)).collect::<Result<Vec<_>,_>>()?;
            let next_cursor=if has_more {items.last().map(|record|record.id.clone())}else{None};
            Ok(ReportsPage {items,next_cursor})
        }).await
    }

    pub(crate) async fn moderator_spot(
        &self,
        actor: Actor,
        target: String,
    ) -> Result<ModeratorSpot, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            moderator(&transaction, &actor)?;
            let state = state(&transaction, &target)?;
            let (owner, body) = transaction
                .query_row(
                    "SELECT owner_id,body FROM spots WHERE id=?1",
                    [&target],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .map_err(db_error)?;
            let input = body
                .map(|body| serde_json::from_str(&body).map_err(|_| Error::Unavailable))
                .transpose()?;
            Ok(ModeratorSpot {
                state,
                owner_id: owner,
                input,
            })
        })
        .await
    }

    pub(crate) async fn moderate(
        &self,
        actor: Actor,
        target: String,
        request: ModerationRequest,
    ) -> Result<MutationReceipt, Error> {
        self.call(move |connection| {
            let transaction=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            moderator(&transaction,&actor)?;
            let fingerprint=fingerprint(&format!("moderate:{target}"),&request)?;
            if let Some((target,revision))=replay(&transaction,&actor,&request.operation_id,&fingerprint)? {return Ok(MutationReceipt {operation_id:request.operation_id,applied_revision:revision,state:state(&transaction,&target)?});}
            let current=state(&transaction,&target)?;
            if current.revision!=request.expected_revision || (current.status!="published" && current.status!="hidden") {return Err(Error::Conflict);}
            let revision=next_revision(current.revision)?;
            let now=now_ms()?;
            write_quota(&transaction,&actor,now)?;
            let sequence=next_change(&transaction)?;
            let (status,action,canonical)=match request.action {
                ModerationAction::Hide if current.status=="published" => ("hidden","hide",None),
                ModerationAction::Restore if current.status=="hidden" => ("published","restore",None),
                ModerationAction::Remove => ("removed","remove",None),
                ModerationAction::Correct {spot} => {
                    let cost=match spot.cost {ChargingCost::Free=>"free",ChargingCost::Paid=>"paid"};
                    transaction.execute("UPDATE spots SET body=?2,latitude_e7=?3,longitude_e7=?4,cost=?5,verified_at_ms=NULL WHERE id=?1",params![target,json(&spot)?,spot.coordinate.latitude_e7,spot.coordinate.longitude_e7,cost]).map_err(db_error)?;
                    (current.status.as_str(),"correct",None)
                },
                ModerationAction::Duplicate {canonical_id} => {
                    if canonical_id==target {return Err(Error::Invalid);}
                    if state(&transaction,&canonical_id)?.status!="published" {return Err(Error::Invalid);}
                    ("removed","duplicate",Some(canonical_id))
                },
                _=>return Err(Error::Conflict),
            };
            transaction.execute("UPDATE spots SET status=?2,revision=?3,updated_at_ms=?4,change_sequence=?5,duplicate_of=?6 WHERE id=?1",params![target,status,revision,now,sequence,canonical]).map_err(db_error)?;
            if status=="removed" {
                transaction.execute("UPDATE spots SET body=NULL,latitude_e7=0,longitude_e7=0,verified_at_ms=NULL WHERE id=?1",[&target]).map_err(db_error)?;
                transaction.execute("UPDATE votes SET value=NULL,revision=CASE WHEN revision<9223372036854775807 THEN revision+1 ELSE revision END WHERE spot_id=?1",[&target]).map_err(db_error)?;
            }
            audit(&transaction,&actor,&target,action,&request.note,canonical.as_deref(),now)?;
            remember(&transaction,&actor,&request.operation_id,&fingerprint,&target,"spot",revision)?;
            let state=state(&transaction,&target)?;
            transaction.commit().map_err(db_error)?;
            Ok(MutationReceipt {operation_id:request.operation_id,applied_revision:revision,state})
        }).await
    }

    pub(crate) async fn resolve(
        &self,
        actor: Actor,
        target: String,
        request: ResolveRequest,
    ) -> Result<ReportReceipt, Error> {
        self.call(move |connection| {
            let transaction=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            moderator(&transaction,&actor)?;
            let fingerprint=fingerprint(&format!("resolve:{target}"),&request)?;
            if let Some((target,revision))=replay(&transaction,&actor,&request.operation_id,&fingerprint)? {return report_receipt(&transaction,&target,request.operation_id,revision);}
            let current=report_record(&transaction,&target)?;
            if current.revision!=request.expected_revision || current.status!="open" {return Err(Error::Conflict);}
            let revision=next_revision(current.revision)?;
            let now=now_ms()?;
            write_quota(&transaction,&actor,now)?;
            transaction.execute("UPDATE reports SET revision=?2,status='resolved',resolved_at_ms=?3,resolution_note=?4 WHERE id=?1",params![target,revision,now,request.note]).map_err(db_error)?;
            audit(&transaction,&actor,&target,"resolve_report",&request.note,None,now)?;
            remember(&transaction,&actor,&request.operation_id,&fingerprint,&target,"report",revision)?;
            let receipt=report_receipt(&transaction,&target,request.operation_id,revision)?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        }).await
    }

    pub(crate) async fn audit(
        &self,
        actor: Actor,
        query: ChangesQuery,
    ) -> Result<AuditPage, Error> {
        self.call(move |connection| {
            let transaction=connection.transaction().map_err(db_error)?;
            moderator(&transaction,&actor)?;
            let after=query.after.unwrap_or_default();let limit=query.limit.unwrap_or(50);
            let mut statement=transaction.prepare("SELECT sequence,actor_id,target_id,action,note,canonical_id,created_at_ms FROM moderation_audit WHERE sequence>?1 ORDER BY sequence LIMIT ?2").map_err(db_error)?;
            let mut items=statement.query_map(params![after,limit+1],|row|Ok(AuditRecord {sequence:row.get(0)?,actor_id:row.get(1)?,target_id:row.get(2)?,action:row.get(3)?,note:row.get(4)?,canonical_id:row.get(5)?,created_at_ms:row.get(6)?})).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            let has_more=items.len()>limit as usize;items.truncate(limit as usize);
            let next_cursor=items.last().map_or(after,|record|record.sequence);
            Ok(AuditPage {items,next_cursor,has_more})
        }).await
    }
}

fn moderator(connection: &Connection, actor: &Actor) -> Result<(), Error> {
    ensure_actor(connection, actor)?;
    if !actor.moderator {
        return Err(Error::Forbidden);
    }
    Ok(())
}

fn report_record(connection: &Connection, target: &str) -> Result<ReportRecord, Error> {
    let mut report=connection.query_row("SELECT id,spot_id,reporter_id,revision,reason,message,status,created_at_ms,resolved_at_ms,resolution_note FROM reports WHERE id=?1",[target],|row|Ok((ReportRecord {id:row.get(0)?,spot_id:row.get(1)?,reporter_id:row.get(2)?,revision:row.get(3)?,reason:ReportReason::Other,message:row.get(5)?,status:row.get(6)?,created_at_ms:row.get(7)?,resolved_at_ms:row.get(8)?,resolution_note:row.get(9)?},row.get::<_,String>(4)?))).optional().map_err(db_error)?.ok_or(Error::NotFound)?;
    report.0.reason = serde_json::from_str(&report.1).map_err(|_| Error::Unavailable)?;
    Ok(report.0)
}

fn report_receipt(
    connection: &Connection,
    target: &str,
    operation: String,
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
    connection.execute("INSERT INTO moderation_audit (actor_id,target_id,action,note,canonical_id,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6)",params![actor.id,target,action,note,canonical,now]).map_err(db_error)?;
    Ok(())
}
