use super::Actor;
use crate::Error;
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuotaWindow {
    Minute,
    Hour,
    Day,
}

impl QuotaWindow {
    pub(crate) const fn period_ms(self) -> u64 {
        match self {
            Self::Minute => 60_000,
            Self::Hour => 3_600_000,
            Self::Day => 86_400_000,
        }
    }
}

pub(crate) fn quota(
    connection: &Connection,
    principal: &str,
    window: QuotaWindow,
    limit: u32,
    now: u64,
) -> Result<(), Error> {
    let period = window.period_ms();
    let start = now / period * period;
    let count: Option<u32> = connection
        .query_row(
            "SELECT count FROM quotas WHERE principal=?1 AND period_ms=?2 AND window_start_ms=?3",
            params![principal, period, start],
            |row| row.get(0),
        )
        .optional()
        .map_err(super::db_error)?;
    if count.unwrap_or_default() >= limit {
        return Err(Error::Throttled);
    }
    connection
        .execute(
            "INSERT INTO quotas (principal, period_ms, window_start_ms, count)
         VALUES (?1, ?2, ?3, 1)
         ON CONFLICT (principal, period_ms) DO UPDATE
         SET window_start_ms = excluded.window_start_ms,
             count = CASE WHEN quotas.window_start_ms = excluded.window_start_ms
                          THEN quotas.count + 1 ELSE 1 END",
            params![principal, period, start],
        )
        .map_err(super::db_error)?;
    Ok(())
}

pub(crate) fn write_quota(connection: &Connection, actor: &Actor, now: u64) -> Result<(), Error> {
    quota(
        connection,
        &format!("write:{}", actor.id),
        QuotaWindow::Minute,
        30,
        now,
    )?;
    quota(
        connection,
        &format!("write:{}", actor.id),
        QuotaWindow::Hour,
        600,
        now,
    )
}
