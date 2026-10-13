use crate::{
    Error, Store,
    model::{MutationReceipt, WithdrawRequest},
    store::{
        Actor, OperationKind, begin_write, db_error, ensure_actor, fingerprint, mutation_receipt,
        next_change, next_revision, now_ms, published_spot, remember, replay, write_quota,
    },
};
use rusqlite::{Connection, params};

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

            let current = published_spot(&transaction, &target, Error::NotFound)?;
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
