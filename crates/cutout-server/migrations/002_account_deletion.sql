CREATE TABLE deletion_receipts (
    token_hash TEXT PRIMARY KEY NOT NULL,
    completed_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL CHECK (expires_at_ms > completed_at_ms)
);

INSERT INTO contributors (id, token_hash, role, created_at_ms, revoked_at_ms)
VALUES (
    '00000000-0000-4000-8000-000000000001',
    'anonymous-system-principal-no-login',
    'contributor',
    0,
    0
);

PRAGMA user_version = 2;
