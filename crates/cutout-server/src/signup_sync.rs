//! Bounded synchronization from the durable signup outbox to the private Lific intake page.

use crate::{
    Error, Store,
    store::{ModeratorActor, begin_write, db_error, ensure_actor, now_ms},
};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::{env, time::Duration};
use tokio::time::{MissedTickBehavior, interval};

const LIFIC_BASE: &str = "https://lific.mjc.lol";
const LIFIC_PAGE_ID: i64 = 191;
const LIFIC_IDENTIFIER: &str = "LIBCU-DOC-39";
const PAGE_MARKER: &str = "<!-- CUTOUT-ALPHA-SIGNUPS-V1 -->";
const END_MARKER: &str = "<!-- /CUTOUT-ALPHA-SIGNUPS-V1 -->";
const RETENTION_MS: u64 = 90 * 24 * 60 * 60 * 1_000;
const MAX_BATCH: usize = 20;

#[derive(Clone)]
pub(crate) struct LificSync {
    client: Client,
    authorization: String,
    base_url: String,
}

impl LificSync {
    pub(crate) fn from_env() -> Option<Self> {
        let key = env::var("LIFIC_SIGNUP_API_KEY").ok()?;
        if key.is_empty() || key.len() > 4096 || !key.bytes().all(|byte| (33..=126).contains(&byte))
        {
            return None;
        }
        let client = cutout_core::http::client_builder()
            .ok()?
            .timeout(Duration::from_secs(2))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .ok()?;
        Some(Self {
            client,
            authorization: format!("Bearer {key}"),
            base_url: LIFIC_BASE.to_owned(),
        })
    }

    async fn get_page(&self) -> Result<LificPage, Error> {
        let page = self
            .client
            .get(format!("{}/api/pages/{LIFIC_PAGE_ID}", self.base_url))
            .header(reqwest::header::AUTHORIZATION, &self.authorization)
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        if !page.status().is_success() {
            return Err(Error::Unavailable);
        }
        let page = page
            .json::<LificPage>()
            .await
            .map_err(|_| Error::Unavailable)?;
        if page.id != LIFIC_PAGE_ID || page.identifier != LIFIC_IDENTIFIER || page.seq <= 0 {
            return Err(Error::Unavailable);
        }
        Ok(page)
    }

    async fn put_page(&self, page: &LificPage, content: String) -> Result<bool, Error> {
        let response = self
            .client
            .put(format!("{}/api/pages/{LIFIC_PAGE_ID}", self.base_url))
            .header(reqwest::header::AUTHORIZATION, &self.authorization)
            .json(&PageUpdate {
                content,
                expected_seq: page.seq,
            })
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        if response.status().is_success() {
            return Ok(true);
        }
        if response.status() == StatusCode::CONFLICT {
            return Ok(false);
        }
        if response.status().is_server_error() {
            // A server error may arrive after the write committed. Re-read
            // before deciding whether to retry, resolving ambiguous writes.
            return Ok(false);
        }
        Err(Error::Unavailable)
    }
}

#[derive(Debug, Deserialize)]
struct LificPage {
    id: i64,
    identifier: String,
    content: String,
    seq: i64,
}

#[derive(Deserialize, Serialize)]
struct PageUpdate {
    content: String,
    expected_seq: i64,
}

#[derive(Debug)]
struct OutboxItem {
    request_id: String,
    operation: String,
    attempts: u32,
    email: String,
    pev_type: Option<String>,
    pev_model: String,
    requested_accessories: String,
    locale: String,
    received_at_ms: u64,
}

impl Store {
    async fn due_signups(&self) -> Result<Vec<OutboxItem>, Error> {
        self.call(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT r.id,o.operation,o.attempts,r.email,r.pev_type,r.pev_model,
                    r.requested_accessories,r.locale,r.received_at_ms
             FROM alpha_signup_outbox o JOIN alpha_signup_requests r ON r.id=o.request_id
             WHERE o.state IN ('pending','failed') AND o.next_attempt_ms <= ?1
             ORDER BY o.next_attempt_ms,r.received_at_ms LIMIT ?2",
                )
                .map_err(db_error)?;
            let rows = statement
                .query_map(rusqlite::params![now_ms()?, MAX_BATCH], |row| {
                    Ok(OutboxItem {
                        request_id: row.get(0)?,
                        operation: row.get(1)?,
                        attempts: row.get(2)?,
                        email: row.get(3)?,
                        pev_type: row.get(4)?,
                        pev_model: row.get(5)?,
                        requested_accessories: row.get(6)?,
                        locale: row.get(7)?,
                        received_at_ms: row.get(8)?,
                    })
                })
                .map_err(db_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
        })
        .await
    }

    async fn mark_signup_synced(&self, id: String) -> Result<(), Error> {
        self.call(move |connection| {
            connection.execute("UPDATE alpha_signup_outbox SET state='synced',last_error=NULL,next_attempt_ms=0 WHERE request_id=?1 AND operation='sync'", [&id]).map_err(db_error)?;
            Ok(())
        }).await
    }

    async fn mark_signup_error(
        &self,
        id: String,
        operation: String,
        attempts: u32,
    ) -> Result<(), Error> {
        let delay = (5_000_u64.saturating_mul(1_u64 << attempts.min(9))).min(3_600_000);
        self.call(move |connection| {
            connection.execute("UPDATE alpha_signup_outbox SET state='failed',attempts=attempts+1,last_error='lific_unavailable',next_attempt_ms=?2 WHERE request_id=?1 AND operation=?3", rusqlite::params![id, now_ms()?.saturating_add(delay), operation]).map_err(db_error)?;
            Ok(())
        }).await
    }

    pub(crate) async fn remove_withdrawn_signup(&self, id: String) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            transaction.execute(
                "UPDATE alpha_signup_removal_idempotency SET payload_hash=NULL WHERE removal_request_id IN
                   (SELECT id FROM alpha_signup_removal_requests WHERE signup_id=?1 AND status='pending')",
                [&id],
            ).map_err(db_error)?;
            transaction.execute(
                "UPDATE alpha_signup_removal_requests
                 SET status='completed',completed_at_ms=?2,email_hash=NULL,signup_id=NULL
                 WHERE signup_id=?1 AND status='pending'",
                rusqlite::params![id, now_ms()?],
            ).map_err(db_error)?;
            transaction
                .execute(
                    "DELETE FROM alpha_signup_idempotency WHERE request_id=?1",
                    [&id],
                )
                .map_err(db_error)?;
            transaction
                .execute("DELETE FROM alpha_signup_outbox WHERE request_id=?1", [&id])
                .map_err(db_error)?;
            transaction
                .execute("DELETE FROM alpha_signup_requests WHERE id=?1", [&id])
                .map_err(db_error)?;
            transaction.commit().map_err(db_error)
        })
        .await
    }

    async fn expire_signup_requests(&self) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            let cutoff = now_ms()?.saturating_sub(RETENTION_MS);
            transaction.execute(
                "UPDATE alpha_signup_outbox SET operation='withdraw',state='pending',next_attempt_ms=0,last_error=NULL
                 WHERE operation='sync' AND request_id IN
                    (SELECT r.id FROM alpha_signup_requests r WHERE r.received_at_ms <= ?1)",
                [cutoff],
            ).map_err(db_error)?;
            transaction.execute(
                "DELETE FROM alpha_signup_removal_idempotency WHERE removal_request_id IN
                   (SELECT id FROM alpha_signup_removal_requests WHERE status IN ('completed','unmatched') AND requested_at_ms <= ?1)",
                [cutoff],
            ).map_err(db_error)?;
            transaction.execute(
                "DELETE FROM alpha_signup_removal_requests WHERE status IN ('completed','unmatched') AND requested_at_ms <= ?1",
                [cutoff],
            ).map_err(db_error)?;
            transaction.commit().map_err(db_error)
        }).await
    }

    /// Local operator CLI action. HTTP handlers must use the actor-checked variant.
    pub(crate) async fn request_signup_withdrawal_local(&self, id: String) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            let changed = transaction.execute(
                "UPDATE alpha_signup_outbox SET operation='withdraw',state='pending',next_attempt_ms=0,last_error=NULL WHERE request_id=?1",
                [&id],
            ).map_err(db_error)?;
            if changed == 0 { return Err(Error::NotFound); }
            transaction.commit().map_err(db_error)
        }).await
    }

    /// Local operator CLI action. HTTP handlers must use the actor-checked variant.
    pub(crate) async fn retry_signup_sync_local(&self, id: String) -> Result<(), Error> {
        self.call(move |connection| {
            let changed = connection.execute(
                "UPDATE alpha_signup_outbox SET state='pending',next_attempt_ms=0,last_error=NULL WHERE request_id=?1 AND state='failed'",
                [&id],
            ).map_err(db_error)?;
            if changed == 0 { return Err(Error::NotFound); }
            Ok(())
        }).await
    }

    pub(crate) async fn moderator_request_signup_withdrawal(
        &self,
        actor: ModeratorActor,
        id: String,
    ) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, actor.as_actor())?;
            let changed = transaction.execute(
                "UPDATE alpha_signup_outbox SET operation='withdraw',state='pending',next_attempt_ms=0,last_error=NULL WHERE request_id=?1",
                [&id],
            ).map_err(db_error)?;
            if changed == 0 { return Err(Error::NotFound); }
            transaction.commit().map_err(db_error)
        }).await
    }

    pub(crate) async fn moderator_retry_signup_sync(
        &self,
        actor: ModeratorActor,
        id: String,
    ) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, actor.as_actor())?;
            let changed = transaction.execute(
                "UPDATE alpha_signup_outbox SET state='pending',next_attempt_ms=0,last_error=NULL WHERE request_id=?1 AND state='failed'",
                [&id],
            ).map_err(db_error)?;
            if changed == 0 { return Err(Error::NotFound); }
            transaction.commit().map_err(db_error)
        }).await
    }

    pub(crate) async fn signup_sync_status(&self) -> Result<Vec<serde_json::Value>, Error> {
        self.call(|connection| signup_sync_status_rows(connection))
            .await
    }

    pub(crate) async fn moderator_signup_sync_status(
        &self,
        actor: ModeratorActor,
    ) -> Result<Vec<serde_json::Value>, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            ensure_actor(&transaction, actor.as_actor())?;
            let rows = signup_sync_status_rows(&transaction)?;
            transaction.commit().map_err(db_error)?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn signup_removal_status(&self) -> Result<Vec<serde_json::Value>, Error> {
        self.call(|connection| signup_removal_status_rows(connection))
            .await
    }

    pub(crate) async fn moderator_signup_removal_status(
        &self,
        actor: ModeratorActor,
    ) -> Result<Vec<serde_json::Value>, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            ensure_actor(&transaction, actor.as_actor())?;
            let rows = signup_removal_status_rows(&transaction)?;
            transaction.commit().map_err(db_error)?;
            Ok(rows)
        })
        .await
    }
}

fn signup_sync_status_rows(
    connection: &rusqlite::Connection,
) -> Result<Vec<serde_json::Value>, Error> {
    let mut statement = connection
        .prepare(
            "SELECT request_id,state,operation,attempts,last_error,next_attempt_ms
         FROM alpha_signup_outbox
         ORDER BY CASE state WHEN 'failed' THEN 0 WHEN 'pending' THEN 1 ELSE 2 END,
                  next_attempt_ms LIMIT 1000",
        )
        .map_err(db_error)?;
    let rows = statement.query_map([], |row| Ok(serde_json::json!({
        "request_id": row.get::<_, String>(0)?, "state": row.get::<_, String>(1)?,
        "operation": row.get::<_, String>(2)?, "attempts": row.get::<_, u32>(3)?,
        "last_error": row.get::<_, Option<String>>(4)?, "next_attempt_ms": row.get::<_, u64>(5)?
    }))).map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn signup_removal_status_rows(
    connection: &rusqlite::Connection,
) -> Result<Vec<serde_json::Value>, Error> {
    let mut statement = connection
        .prepare(
            "SELECT r.id,r.signup_id,s.email,r.status,r.requested_at_ms,r.completed_at_ms
         FROM alpha_signup_removal_requests r
         LEFT JOIN alpha_signup_requests s ON s.id=r.signup_id
         ORDER BY CASE r.status WHEN 'pending' THEN 0 WHEN 'unmatched' THEN 1 ELSE 2 END,
                  r.requested_at_ms LIMIT 500",
        )
        .map_err(db_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(serde_json::json!({
                "removal_request_id": row.get::<_, String>(0)?,
                "signup_request_id": row.get::<_, Option<String>>(1)?,
                "email": row.get::<_, Option<String>>(2)?,
                "status": row.get::<_, String>(3)?,
                "requested_at_ms": row.get::<_, u64>(4)?,
                "completed_at_ms": row.get::<_, Option<u64>>(5)?,
            }))
        })
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) async fn run_worker(store: Store, lific: LificSync) {
    let mut ticker = interval(Duration::from_secs(10));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        if let Err(error) = store.expire_signup_requests().await {
            tracing::warn!(?error, "alpha signup retention scheduling failed");
        }
        let items = match store.due_signups().await {
            Ok(items) => items,
            Err(error) => {
                tracing::warn!(?error, "alpha signup outbox read failed");
                continue;
            }
        };
        for item in items {
            let result = sync_item(&lific, &item).await;
            match result {
                Ok(()) if item.operation == "withdraw" => {
                    if store
                        .remove_withdrawn_signup(item.request_id)
                        .await
                        .is_err()
                    {
                        tracing::warn!("alpha signup withdrawal local cleanup failed");
                    }
                }
                Ok(()) => {
                    if store.mark_signup_synced(item.request_id).await.is_err() {
                        tracing::warn!("alpha signup outbox completion failed");
                    }
                }
                Err(_) => {
                    if store
                        .mark_signup_error(item.request_id, item.operation, item.attempts)
                        .await
                        .is_err()
                    {
                        tracing::warn!("alpha signup retry scheduling failed");
                    }
                }
            }
        }
    }
}

async fn sync_item(lific: &LificSync, item: &OutboxItem) -> Result<(), Error> {
    for _ in 0..3 {
        let page = lific.get_page().await?;
        let content = if item.operation == "withdraw" {
            remove_entry(&page.content, &item.request_id)?
        } else {
            add_entry(&page.content, item)?
        };
        if content == page.content {
            return Ok(());
        }
        if lific.put_page(&page, content).await? {
            return Ok(());
        }
    }
    Err(Error::Unavailable)
}

fn entry_marker(id: &str) -> (String, String) {
    (
        format!("<!-- CUTOUT-ALPHA-SIGNUP:{id}:BEGIN -->"),
        format!("<!-- CUTOUT-ALPHA-SIGNUP:{id}:END -->"),
    )
}

fn add_entry(page: &str, item: &OutboxItem) -> Result<String, Error> {
    let (begin, end) = entry_marker(&item.request_id);
    let mut content = ensure_section(page)?;
    let (section_start, section_end) =
        marker_pair(&content, PAGE_MARKER, END_MARKER)?.ok_or(Error::Unavailable)?;
    if let Some((entry_start, entry_end)) = marker_pair(&content, &begin, &end)? {
        if entry_start <= section_start || entry_end >= section_end {
            return Err(Error::Unavailable);
        }
        return Ok(content);
    }
    let entry = format!(
        "\n\n{begin}\n**Received:** {}\n\n- **Email:** {}\n- **Locale:** {}\n- **PEV type:** {}\n- **PEV model:** {}\n- **Requested accessories:** {}\n- **Status:** requested\n- **Maintainer notes:** \n{end}",
        item.received_at_ms,
        escape_markdown(&item.email),
        escape_markdown(item.locale.as_str()),
        escape_markdown(item.pev_type.as_deref().unwrap_or("not specified")),
        escape_markdown(&item.pev_model),
        escape_markdown(&item.requested_accessories)
    );
    let offset = content.find(END_MARKER).unwrap_or(content.len());
    content.insert_str(offset, &entry);
    Ok(content)
}

fn ensure_section(page: &str) -> Result<String, Error> {
    match marker_pair(page, PAGE_MARKER, END_MARKER)? {
        Some(_) => Ok(page.to_owned()),
        None => Ok(format!(
            "{}\n\n{PAGE_MARKER}\n{END_MARKER}",
            page.trim_end()
        )),
    }
}

fn remove_entry(page: &str, id: &str) -> Result<String, Error> {
    let (begin, end) = entry_marker(id);
    let Some((section_start, section_end)) = marker_pair(page, PAGE_MARKER, END_MARKER)? else {
        if marker_pair(page, &begin, &end)?.is_some() {
            return Err(Error::Unavailable);
        }
        return Ok(page.to_owned());
    };
    let Some((start, finish)) = marker_pair(page, &begin, &end)? else {
        return Ok(page.to_owned());
    };
    if start <= section_start || finish >= section_end {
        return Err(Error::Unavailable);
    }
    let mut result = page.to_owned();
    result.replace_range(start..finish, "");
    Ok(result)
}

/// Return the sole ordered marker pair. Partial, duplicated, or reversed pairs
/// are treated as page corruption so a remote PII entry is never reported as removed.
fn marker_pair(page: &str, begin: &str, end: &str) -> Result<Option<(usize, usize)>, Error> {
    let begins = page
        .match_indices(begin)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let ends = page
        .match_indices(end)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([start], [finish]) if start < finish => Ok(Some((*start, finish + end.len()))),
        _ => Err(Error::Unavailable),
    }
}

fn escape_markdown(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '/' => escaped.push_str("&#47;"),
            '\n' | '\r' => escaped.push(' '),
            _ => {
                match character {
                    '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+'
                    | '-' | '.' | '!' | '|' | '<' | '>' => escaped.push('\\'),
                    _ => {}
                }
                escaped.push(character);
            }
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, http::StatusCode, routing::get};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakePage {
        content: String,
        seq: i64,
        put_attempts: usize,
        conflict_once: bool,
        lose_response_once: bool,
    }

    async fn fake_lific(
        initial: String,
        conflict_once: bool,
        lose_response_once: bool,
    ) -> (LificSync, Arc<Mutex<FakePage>>, tokio::task::JoinHandle<()>) {
        let page = Arc::new(Mutex::new(FakePage {
            content: initial,
            seq: 1,
            put_attempts: 0,
            conflict_once,
            lose_response_once,
        }));
        let get_page = page.clone();
        let put_page = page.clone();
        let app = Router::new().route(
            "/api/pages/191",
            get(move || {
                let page = get_page.clone();
                async move {
                    let page = page.lock().expect("page lock");
                    Json(serde_json::json!({
                        "id": LIFIC_PAGE_ID, "identifier": LIFIC_IDENTIFIER,
                        "content": page.content.clone(), "seq": page.seq
                    }))
                }
            })
            .put(move |Json(update): Json<PageUpdate>| {
                let page = put_page.clone();
                async move {
                    let mut page = page.lock().expect("page lock");
                    page.put_attempts += 1;
                    if page.conflict_once {
                        page.conflict_once = false;
                        page.content
                            .push_str("\n\nMaintainer note added during conflict.");
                        page.seq += 1;
                        return StatusCode::CONFLICT;
                    }
                    assert_eq!(update.expected_seq, page.seq);
                    page.content = update.content;
                    page.seq += 1;
                    if page.lose_response_once {
                        page.lose_response_once = false;
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                    StatusCode::OK
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let address = format!("http://{}", listener.local_addr().expect("mock address"));
        let task =
            tokio::spawn(async move { axum::serve(listener, app).await.expect("serve mock") });
        let client = cutout_core::http::client_builder()
            .expect("platform verifier")
            .no_proxy()
            .build()
            .expect("client");
        (
            LificSync {
                client,
                authorization: "Bearer test".into(),
                base_url: address,
            },
            page,
            task,
        )
    }

    #[test]
    fn page_entry_is_idempotent_and_untrusted_markdown_is_escaped() {
        let item = OutboxItem {
            request_id: "abc-123".into(),
            operation: "sync".into(),
            attempts: 0,
            email: "rider@example.com\n![x](attachment:secret)".into(),
            pev_type: None,
            pev_model: "**model**".into(),
            requested_accessories: "<!-- injected --> /api/attachments/12".into(),
            locale: "en".into(),
            received_at_ms: 1,
        };
        let page = "Existing operator note";
        let first = add_entry(page, &item).expect("entry");
        assert!(first.starts_with("Existing operator note"));
        assert!(first.contains("\\!\\[x\\]\\(attachment:secret\\)"));
        assert!(first.contains("\\*\\*model\\*\\*"));
        assert!(first.contains("\\<\\!\\-\\- injected \\-\\-\\>"));
        assert!(!first.contains("/api/attachments/"));
        assert!(first.contains("&#47;api&#47;attachments&#47;12"));
        assert_eq!(add_entry(&first, &item).expect("repeat entry"), first);
        let removed = remove_entry(&first, "abc-123").expect("remove entry");
        assert!(removed.contains("Existing operator note"));
        assert!(!removed.contains("abc-123"));
    }

    #[tokio::test]
    async fn stale_write_rereads_page_and_preserves_concurrent_maintainer_note() {
        let (lific, page, task) = fake_lific("Operator intro".into(), true, false).await;
        let item = sample_item("rider-1");
        sync_item(&lific, &item).await.expect("sync after conflict");
        let page = page.lock().expect("page lock").clone();
        assert!(page.content.contains("Operator intro"));
        assert!(
            page.content
                .contains("Maintainer note added during conflict.")
        );
        assert!(
            page.content.contains("rider@example\\.com"),
            "{}",
            page.content
        );
        assert_eq!(page.put_attempts, 2);
        task.abort();
    }

    #[tokio::test]
    async fn ambiguous_success_is_reconciled_by_read_before_repeating_append() {
        let (lific, page, task) = fake_lific("Operator intro".into(), false, true).await;
        let item = sample_item("rider-2");
        sync_item(&lific, &item)
            .await
            .expect("reconcile ambiguous put");
        let page = page.lock().expect("page lock").clone();
        assert_eq!(page.content.matches("rider@example\\.com").count(), 1);
        assert_eq!(page.put_attempts, 1);
        task.abort();
    }

    #[tokio::test]
    async fn withdrawal_removes_only_matching_entry_and_keeps_operator_notes() {
        let original =
            add_entry("Operator note: review on Tuesday.", &sample_item("rider-3")).expect("entry");
        let (lific, page, task) = fake_lific(original, false, false).await;
        let mut item = sample_item("rider-3");
        item.operation = "withdraw".into();
        sync_item(&lific, &item).await.expect("remote withdrawal");
        let page = page.lock().expect("page lock").clone();
        assert!(page.content.contains("Operator note: review on Tuesday."));
        assert!(!page.content.contains("rider@example\\.com"));
        task.abort();
    }

    #[test]
    fn malformed_markers_are_retryable_errors_instead_of_false_removal() {
        let id = "request-4";
        let (begin, end) = entry_marker(id);
        let partial = format!("{PAGE_MARKER}\n{begin}\n{END_MARKER}");
        assert_eq!(remove_entry(&partial, id), Err(Error::Unavailable));
        let duplicate = format!("{PAGE_MARKER}\n{END_MARKER}\n{PAGE_MARKER}\n{END_MARKER}");
        assert_eq!(
            add_entry(&duplicate, &sample_item(id)),
            Err(Error::Unavailable)
        );
        assert_eq!(
            add_entry(
                &partial.replace(&begin, &format!("{begin}\n{end}\n{begin}")),
                &sample_item(id)
            ),
            Err(Error::Unavailable)
        );
        let outside = format!("{begin}\nentry\n{end}\n{PAGE_MARKER}\n{END_MARKER}");
        assert_eq!(
            add_entry(&outside, &sample_item(id)),
            Err(Error::Unavailable)
        );
    }

    #[tokio::test]
    async fn retention_schedules_remote_removal_before_local_deletion() {
        let directory = tempfile::TempDir::new().expect("directory");
        let store = Store::open(&directory.path().join("signup.sqlite")).expect("store");
        let unsynced = "10000000-0000-4000-8000-000000000001".to_owned();
        let synced = "20000000-0000-4000-8000-000000000002".to_owned();
        store.call({
            let unsynced = unsynced.clone();
            let synced = synced.clone();
            move |connection| {
                let transaction = begin_write(connection)?;
                for (id, state, suffix) in [(&unsynced, "pending", "1"), (&synced, "synced", "2")] {
                    transaction.execute(
                        "INSERT INTO alpha_signup_requests(id,email,email_hash,pev_type,pev_model,requested_accessories,locale,disclosure_revision,received_at_ms)
                         VALUES (?1,?2,?3,'euc','Model','', 'en','test',0)",
                        rusqlite::params![id, format!("rider{suffix}@example.com"), format!("hash{suffix}")],
                    ).map_err(db_error)?;
                    transaction.execute(
                        "INSERT INTO alpha_signup_outbox(request_id,state) VALUES (?1,?2)",
                        rusqlite::params![id, state],
                    ).map_err(db_error)?;
                    transaction.execute(
                        "INSERT INTO alpha_signup_idempotency(idempotency_key,payload_hash,request_id,receipt_id) VALUES (?1,'payload',?2,?3)",
                        rusqlite::params![format!("key{suffix}"), id, format!("receipt{suffix}")],
                    ).map_err(db_error)?;
                }
                transaction.commit().map_err(db_error)
            }
        }).await.expect("seed old requests");

        store
            .expire_signup_requests()
            .await
            .expect("schedule retention");
        let state: String = store
            .call({
                let synced = synced.clone();
                move |connection| {
                    connection
                        .query_row(
                            "SELECT operation FROM alpha_signup_outbox WHERE request_id=?1",
                            [&synced],
                            |row| row.get(0),
                        )
                        .map_err(db_error)
                }
            })
            .await
            .expect("remote removal queued");
        assert_eq!(state, "withdraw");
        let unsynced_operation: String = store
            .call({
                let unsynced = unsynced.clone();
                move |connection| {
                    connection
                        .query_row(
                            "SELECT operation FROM alpha_signup_outbox WHERE request_id=?1",
                            [&unsynced],
                            |row| row.get(0),
                        )
                        .map_err(db_error)
                }
            })
            .await
            .expect("unsynced record queued for remote reconciliation");
        assert_eq!(unsynced_operation, "withdraw");

        store
            .remove_withdrawn_signup(synced.clone())
            .await
            .expect("complete remote removal");
        store
            .remove_withdrawn_signup(unsynced.clone())
            .await
            .expect("complete reconciliation for previously unsynced record");
        let remaining: u32 = store
            .call(|connection| {
                connection
                    .query_row(
                        "SELECT (SELECT count(*) FROM alpha_signup_requests) +
                    (SELECT count(*) FROM alpha_signup_outbox) +
                    (SELECT count(*) FROM alpha_signup_idempotency)",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
            .await
            .expect("verify local removal");
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn operator_sync_status_prioritizes_actionable_rows_over_synced_history() {
        let directory = tempfile::TempDir::new().expect("directory");
        let store = Store::open(&directory.path().join("signup.sqlite")).expect("store");
        store
            .call(|connection| {
                let transaction = begin_write(connection)?;
                for index in 0..100 {
                    let id = format!("{index:08x}-0000-4000-8000-000000000001");
                    transaction.execute(
                        "INSERT INTO alpha_signup_requests(id,email,email_hash,pev_model,requested_accessories,locale,disclosure_revision,received_at_ms)
                         VALUES (?1,'rider@example.com',?1,'Model','','en','test',?2)",
                        rusqlite::params![id, index],
                    ).map_err(db_error)?;
                    transaction.execute(
                        "INSERT INTO alpha_signup_outbox(request_id,state) VALUES (?1,'synced')",
                        [&id],
                    ).map_err(db_error)?;
                }
                let failed = "ffffffff-ffff-4fff-8fff-ffffffffffff";
                transaction.execute(
                    "INSERT INTO alpha_signup_requests(id,email,email_hash,pev_model,requested_accessories,locale,disclosure_revision,received_at_ms)
                     VALUES (?1,'failed@example.com',?1,'Model','','en','test',1000)",
                    [failed],
                ).map_err(db_error)?;
                transaction.execute(
                    "INSERT INTO alpha_signup_outbox(request_id,state,last_error,next_attempt_ms) VALUES (?1,'failed','lific_unavailable',9999999999999)",
                    [failed],
                ).map_err(db_error)?;
                transaction.commit().map_err(db_error)
            })
            .await
            .expect("seed queue status");
        let status = store.signup_sync_status().await.expect("status list");
        assert_eq!(status.len(), 101);
        assert_eq!(
            status[0]["request_id"],
            "ffffffff-ffff-4fff-8fff-ffffffffffff"
        );
        assert_eq!(status[0]["state"], "failed");
    }

    #[tokio::test]
    async fn moderator_outbox_reads_and_mutations_recheck_revocation_and_role_in_transaction() {
        let directory = tempfile::TempDir::new().expect("directory");
        let store = Store::open(&directory.path().join("signup.sqlite")).expect("store");
        let revoked_identity = store
            .identity(crate::store::Registration::Operator {
                role: crate::store::Role::Moderator,
            })
            .await
            .expect("revoked moderator");
        let stale_revoked_actor = store
            .authenticate(revoked_identity.token)
            .await
            .expect("authenticate before revocation")
            .into_moderator()
            .expect("moderator role");
        store
            .call(move |connection| {
                connection
                    .execute(
                        "UPDATE contributors SET revoked_at_ms=?2 WHERE id=?1",
                        rusqlite::params![revoked_identity.id, now_ms()?],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .await
            .expect("revoke after authentication");
        assert_eq!(
            store
                .moderator_signup_sync_status(stale_revoked_actor)
                .await
                .expect_err("revoked read is rejected"),
            Error::Unauthorized
        );

        let demoted_identity = store
            .identity(crate::store::Registration::Operator {
                role: crate::store::Role::Moderator,
            })
            .await
            .expect("demoted moderator");
        let stale_demoted_actor = store
            .authenticate(demoted_identity.token)
            .await
            .expect("authenticate before demotion")
            .into_moderator()
            .expect("moderator role");
        store
            .call(move |connection| {
                connection
                    .execute(
                        "UPDATE contributors SET role='contributor' WHERE id=?1",
                        [&demoted_identity.id],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .await
            .expect("demote after authentication");
        assert_eq!(
            store
                .moderator_request_signup_withdrawal(
                    stale_demoted_actor,
                    "ffffffff-ffff-4fff-8fff-ffffffffffff".into(),
                )
                .await
                .expect_err("demoted mutation is rejected"),
            Error::Forbidden
        );
    }

    fn sample_item(request_id: &str) -> OutboxItem {
        OutboxItem {
            request_id: request_id.into(),
            operation: "sync".into(),
            attempts: 0,
            email: "rider@example.com".into(),
            pev_type: Some("euc".into()),
            pev_model: "Model".into(),
            requested_accessories: "Footpads".into(),
            locale: "en".into(),
            received_at_ms: 7,
        }
    }
}
