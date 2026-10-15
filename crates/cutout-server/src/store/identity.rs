use super::votes::advance_spot_change;
use super::{QuotaWindow, Store, begin_write, db_error, hash, next_change, now_ms, quota};
use crate::Error;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::{fmt, net::IpAddr};
use uuid::Uuid;

pub(crate) const ANONYMOUS_ID: &str = "00000000-0000-4000-8000-000000000001";
const DELETION_RECEIPT_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Clone, Debug)]
pub(crate) struct Actor {
    pub id: String,
    pub role: Role,
}

/// A credential whose moderation role was checked at authentication.
/// Transactional checks still reject revoked credentials or changed roles.
#[derive(Clone, Debug)]
pub(crate) struct ModeratorActor(Actor);

impl Actor {
    pub(crate) fn into_moderator(self) -> Result<ModeratorActor, Error> {
        match self.role {
            Role::Moderator => Ok(ModeratorActor(self)),
            Role::Contributor => Err(Error::Forbidden),
        }
    }
}

impl ModeratorActor {
    pub(crate) fn as_actor(&self) -> &Actor {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Role {
    Contributor,
    Moderator,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Self::Contributor => "contributor",
            Self::Moderator => "moderator",
        }
    }
}

impl rusqlite::types::FromSql for Role {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value.as_str()? {
            "contributor" => Ok(Self::Contributor),
            "moderator" => Ok(Self::Moderator),
            _ => Err(rusqlite::types::FromSqlError::Other(Box::new(
                Error::Unavailable,
            ))),
        }
    }
}

/// Public registration always has a source budget and cannot grant moderation.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Registration {
    Public { source: IpAddr },
    Operator { role: Role },
}

#[derive(Serialize)]
pub(crate) struct Identity {
    pub id: String,
    pub token: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct DeletionReceipt {
    pub status: &'static str,
    pub completed_at_ms: u64,
}

struct ContributorTokenHash(String);

impl ContributorTokenHash {
    fn from_token(token: &str) -> Result<Self, Error> {
        if token.len() != 64 || !token.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err(Error::Unauthorized);
        }
        Ok(Self(hash(token.as_bytes())))
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Identity")
            .field("id", &self.id)
            .field("token", &"[redacted]")
            .finish()
    }
}

impl Store {
    pub(crate) async fn identity(&self, registration: Registration) -> Result<Identity, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            let now = now_ms()?;
            let role = registration_role(&transaction, registration, now)?;
            let identity = new_identity();
            insert_identity(&transaction, &identity, role, now)?;
            transaction.commit().map_err(db_error)?;
            Ok(identity)
        })
        .await
    }

    pub(crate) async fn authenticate(&self, token: String) -> Result<Actor, Error> {
        let token = ContributorTokenHash::from_token(&token)?;
        self.call(move |connection| authenticated_actor(connection, &token))
            .await
    }

    pub(crate) async fn delete_current(&self, token: String) -> Result<DeletionReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            let now = now_ms()?;
            purge_expired_deletion_receipts(&transaction, now)?;
            let token = match ContributorTokenHash::from_token(&token) {
                Ok(token) => token,
                Err(error) => {
                    transaction.commit().map_err(db_error)?;
                    return Err(error);
                }
            };
            if let Some(receipt) = completed_deletion(&transaction, &token, now)? {
                transaction.commit().map_err(db_error)?;
                return Ok(receipt);
            }
            let actor = match deletion_actor(&transaction, &token) {
                Ok(actor) => actor,
                Err(error @ Error::Unauthorized) => {
                    transaction.commit().map_err(db_error)?;
                    return Err(error);
                }
                Err(error) => return Err(error),
            };

            let affected_spots = deletion_affected_spots(&transaction, &actor.id)?;
            retain_public_spots(&transaction, &actor.id)?;
            delete_private_account_data(&transaction, &actor.id)?;
            for spot_id in affected_spots {
                let sequence = next_change(&transaction)?;
                transaction
                    .execute(
                        "UPDATE spots SET change_sequence = ?2 WHERE id = ?1",
                        params![spot_id, sequence],
                    )
                    .map_err(db_error)?;
            }
            transaction
                .execute("DELETE FROM contributors WHERE id = ?1", [&actor.id])
                .map_err(db_error)?;

            let expires_at = now
                .checked_add(DELETION_RECEIPT_RETENTION_MS)
                .filter(|value| i64::try_from(*value).is_ok())
                .ok_or(Error::Conflict)?;
            transaction
                .execute(
                    "INSERT INTO deletion_receipts (token_hash, completed_at_ms, expires_at_ms)
                     VALUES (?1, ?2, ?3)",
                    params![token.0, now, expires_at],
                )
                .map_err(db_error)?;
            transaction.commit().map_err(db_error)?;
            Ok(DeletionReceipt {
                status: "completed",
                completed_at_ms: now,
            })
        })
        .await
    }

    pub(crate) async fn revoke(&self, target: String) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            revoke_contributor(&transaction, &target)?;
            let targets = contributor_voted_spots(&transaction, &target)?;
            clear_contributor_votes(&transaction, &target)?;
            for spot in targets {
                advance_spot_change(&transaction, &spot)?;
            }
            transaction.commit().map_err(db_error)
        })
        .await
    }
}

fn purge_expired_deletion_receipts(connection: &Connection, now: u64) -> Result<(), Error> {
    connection
        .execute(
            "DELETE FROM deletion_receipts WHERE expires_at_ms <= ?1",
            [now],
        )
        .map_err(db_error)?;
    Ok(())
}

fn completed_deletion(
    connection: &Connection,
    token: &ContributorTokenHash,
    now: u64,
) -> Result<Option<DeletionReceipt>, Error> {
    connection
        .query_row(
            "SELECT completed_at_ms FROM deletion_receipts
             WHERE token_hash = ?1 AND expires_at_ms > ?2",
            params![token.0, now],
            |row| {
                Ok(DeletionReceipt {
                    status: "completed",
                    completed_at_ms: row.get(0)?,
                })
            },
        )
        .optional()
        .map_err(db_error)
}

fn deletion_actor(connection: &Connection, token: &ContributorTokenHash) -> Result<Actor, Error> {
    connection
        .query_row(
            "SELECT id, role FROM contributors WHERE token_hash = ?1 AND id != ?2",
            params![token.0, ANONYMOUS_ID],
            |row| {
                Ok(Actor {
                    id: row.get(0)?,
                    role: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(db_error)?
        .ok_or(Error::Unauthorized)
}

fn deletion_affected_spots(connection: &Connection, target: &str) -> Result<Vec<String>, Error> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM spots WHERE owner_id = ?1
             UNION SELECT spot_id FROM votes WHERE contributor_id = ?1 AND value IS NOT NULL
             ORDER BY 1",
        )
        .map_err(db_error)?;
    statement
        .query_map([target], |row| row.get(0))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)
}

fn retain_public_spots(connection: &Connection, target: &str) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE spots SET owner_id = ?2 WHERE owner_id = ?1",
            params![target, ANONYMOUS_ID],
        )
        .map_err(db_error)?;
    Ok(())
}

fn delete_private_account_data(connection: &Connection, target: &str) -> Result<(), Error> {
    connection
        .execute(
            "DELETE FROM moderation_audit
             WHERE target_id IN (SELECT id FROM reports WHERE reporter_id = ?1)",
            [target],
        )
        .map_err(db_error)?;
    connection
        .execute("DELETE FROM votes WHERE contributor_id = ?1", [target])
        .map_err(db_error)?;
    connection
        .execute("DELETE FROM operations WHERE contributor_id = ?1", [target])
        .map_err(db_error)?;
    connection
        .execute("DELETE FROM reports WHERE reporter_id = ?1", [target])
        .map_err(db_error)?;
    connection
        .execute(
            "DELETE FROM verifications WHERE contributor_id = ?1",
            [target],
        )
        .map_err(db_error)?;
    connection
        .execute(
            "DELETE FROM quotas WHERE principal = ?1 OR principal LIKE ?2 OR principal LIKE ?3",
            params![
                format!("write:{target}"),
                format!("report:{target}:%"),
                format!("new-spots:{target}")
            ],
        )
        .map_err(db_error)?;
    connection
        .execute(
            "UPDATE moderation_audit SET actor_id = ?2,
                 note = 'Account deleted; author anonymized.'
             WHERE actor_id = ?1",
            params![target, ANONYMOUS_ID],
        )
        .map_err(db_error)?;
    Ok(())
}

fn registration_role(
    connection: &Connection,
    registration: Registration,
    now: u64,
) -> Result<Role, Error> {
    match registration {
        Registration::Public { source } => {
            registration_quota(connection, source, now)?;
            Ok(Role::Contributor)
        }
        Registration::Operator { role } => Ok(role),
    }
}

fn new_identity() -> Identity {
    Identity {
        id: Uuid::new_v4().to_string(),
        token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
    }
}

fn insert_identity(
    connection: &Connection,
    identity: &Identity,
    role: Role,
    now: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO contributors (id, token_hash, role, created_at_ms)
         VALUES (?1, ?2, ?3, ?4)",
            params![
                identity.id,
                hash(identity.token.as_bytes()),
                role.as_str(),
                now
            ],
        )
        .map_err(db_error)?;
    Ok(())
}

fn authenticated_actor(
    connection: &Connection,
    token: &ContributorTokenHash,
) -> Result<Actor, Error> {
    connection
        .query_row(
            "SELECT id, role FROM contributors
         WHERE token_hash = ?1 AND revoked_at_ms IS NULL AND id != ?2",
            params![token.0, ANONYMOUS_ID],
            |row| {
                Ok(Actor {
                    id: row.get(0)?,
                    role: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(db_error)?
        .ok_or(Error::Unauthorized)
}

fn revoke_contributor(connection: &Connection, target: &str) -> Result<(), Error> {
    let updated = connection
        .execute(
            "UPDATE contributors SET revoked_at_ms = ?2 WHERE id = ?1",
            params![target, now_ms()?],
        )
        .map_err(db_error)?;
    if updated == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

fn contributor_voted_spots(connection: &Connection, target: &str) -> Result<Vec<String>, Error> {
    let mut statement = connection
        .prepare("SELECT spot_id FROM votes WHERE contributor_id = ?1 AND value IS NOT NULL")
        .map_err(db_error)?;
    statement
        .query_map([target], |row| row.get(0))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)
}

fn clear_contributor_votes(connection: &Connection, target: &str) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE votes SET value = NULL,
            revision = CASE WHEN revision < 9223372036854775807 THEN revision + 1 ELSE revision END
         WHERE contributor_id = ?1 AND value IS NOT NULL",
            [target],
        )
        .map_err(db_error)?;
    Ok(())
}

pub(crate) fn ensure_actor(connection: &Connection, actor: &Actor) -> Result<(), Error> {
    if actor.id == ANONYMOUS_ID {
        return Err(Error::Unauthorized);
    }
    let role: Role = connection
        .query_row(
            "SELECT role FROM contributors WHERE id=?1 AND revoked_at_ms IS NULL",
            [&actor.id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?
        .ok_or(Error::Unauthorized)?;
    if role != actor.role {
        return Err(Error::Forbidden);
    }
    Ok(())
}

pub(super) fn registration_quota(
    connection: &Connection,
    source: IpAddr,
    now: u64,
) -> Result<(), Error> {
    let principal = format!(
        "registration-source:{}",
        hash(registration_budget_source(source).to_string().as_bytes())
    );
    quota(connection, &principal, QuotaWindow::Minute, 20, now)?;
    quota(connection, &principal, QuotaWindow::Hour, 120, now)?;
    quota(
        connection,
        "registration-global",
        QuotaWindow::Minute,
        200,
        now,
    )?;
    quota(
        connection,
        "registration-global",
        QuotaWindow::Hour,
        1_200,
        now,
    )
}

/// IPv4 uses its canonical address; IPv6 privacy addresses share one /64 budget.
fn registration_budget_source(source: IpAddr) -> IpAddr {
    match source.to_canonical() {
        IpAddr::V4(address) => address.into(),
        IpAddr::V6(address) => {
            std::net::Ipv6Addr::from(u128::from(address) & (u128::MAX << 64)).into()
        }
    }
}
