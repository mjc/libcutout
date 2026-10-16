CREATE TABLE alpha_signup_requests (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL,
    email_hash TEXT NOT NULL UNIQUE,
    pev_type TEXT,
    pev_model TEXT NOT NULL,
    requested_accessories TEXT NOT NULL,
    locale TEXT NOT NULL CHECK (locale IN ('en', 'es')),
    disclosure_revision TEXT NOT NULL,
    received_at_ms INTEGER NOT NULL
);
CREATE TABLE alpha_signup_idempotency (
    idempotency_key TEXT PRIMARY KEY NOT NULL,
    payload_hash TEXT NOT NULL,
    request_id TEXT NOT NULL REFERENCES alpha_signup_requests(id),
    receipt_id TEXT NOT NULL UNIQUE
);
CREATE TABLE alpha_signup_outbox (
    request_id TEXT PRIMARY KEY NOT NULL REFERENCES alpha_signup_requests(id),
    state TEXT NOT NULL CHECK (state IN ('pending', 'failed', 'synced')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);
PRAGMA user_version = 3;
