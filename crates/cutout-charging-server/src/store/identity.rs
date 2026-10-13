use super::votes::advance_spot_change;
use super::{Store, begin_write, db_error, hash, now_ms, quota};
use crate::Error;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::{fmt, net::IpAddr};
use uuid::Uuid;

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
         WHERE token_hash = ?1 AND revoked_at_ms IS NULL",
            [&token.0],
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
         WHERE contributor_id = ?1",
            [target],
        )
        .map_err(db_error)?;
    Ok(())
}

pub(crate) fn ensure_actor(connection: &Connection, actor: &Actor) -> Result<(), Error> {
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
        hash(source.to_canonical().to_string().as_bytes())
    );
    quota(connection, &principal, 60_000, 20, now)?;
    quota(connection, &principal, 3_600_000, 120, now)?;
    quota(connection, "registration-global", 60_000, 200, now)?;
    quota(connection, "registration-global", 3_600_000, 1_200, now)
}
