CREATE TABLE alpha_signup_removal_requests (
    id TEXT PRIMARY KEY NOT NULL,
    email_hash TEXT,
    signup_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending', 'unmatched', 'completed')),
    requested_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER
);
CREATE TABLE alpha_signup_removal_idempotency (
    idempotency_key TEXT PRIMARY KEY NOT NULL,
    payload_hash TEXT,
    removal_request_id TEXT NOT NULL REFERENCES alpha_signup_removal_requests(id),
    receipt_id TEXT NOT NULL UNIQUE
);
CREATE INDEX alpha_signup_removal_review
    ON alpha_signup_removal_requests(status, requested_at_ms);
PRAGMA user_version = 5;
