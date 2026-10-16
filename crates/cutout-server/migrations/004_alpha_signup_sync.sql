ALTER TABLE alpha_signup_outbox
    ADD COLUMN next_attempt_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE alpha_signup_outbox
    ADD COLUMN operation TEXT NOT NULL DEFAULT 'sync'
    CHECK (operation IN ('sync', 'withdraw'));
CREATE TABLE alpha_signup_idempotency_v4 (
    idempotency_key TEXT PRIMARY KEY NOT NULL,
    payload_hash TEXT NOT NULL,
    request_id TEXT NOT NULL,
    receipt_id TEXT NOT NULL UNIQUE
);
INSERT INTO alpha_signup_idempotency_v4(idempotency_key,payload_hash,request_id,receipt_id)
    SELECT idempotency_key,payload_hash,request_id,receipt_id FROM alpha_signup_idempotency;
DROP TABLE alpha_signup_idempotency;
ALTER TABLE alpha_signup_idempotency_v4 RENAME TO alpha_signup_idempotency;
CREATE INDEX alpha_signup_outbox_due
    ON alpha_signup_outbox(state, next_attempt_ms);
PRAGMA user_version = 4;
