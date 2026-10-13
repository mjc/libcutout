CREATE TABLE contributors (
    id TEXT PRIMARY KEY NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL CHECK (role IN ('contributor', 'moderator')),
    created_at_ms INTEGER NOT NULL,
    revoked_at_ms INTEGER
);
CREATE TABLE spots (
    id TEXT PRIMARY KEY NOT NULL,
    owner_id TEXT NOT NULL REFERENCES contributors(id),
    revision INTEGER NOT NULL CHECK (revision > 0),
    status TEXT NOT NULL CHECK (status IN ('published', 'hidden', 'withdrawn', 'removed')),
    latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
    longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000),
    cost TEXT NOT NULL CHECK (cost IN ('free', 'paid')),
    body TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    verified_at_ms INTEGER,
    change_sequence INTEGER NOT NULL UNIQUE,
    duplicate_of TEXT REFERENCES spots(id)
);
CREATE INDEX spots_discovery ON spots(status, latitude_e7, longitude_e7, id);
CREATE INDEX spots_paging ON spots(status, id);
CREATE TABLE votes (
    spot_id TEXT NOT NULL REFERENCES spots(id),
    contributor_id TEXT NOT NULL REFERENCES contributors(id),
    revision INTEGER NOT NULL CHECK (revision > 0),
    value TEXT CHECK (value IN ('up', 'down')),
    PRIMARY KEY (spot_id, contributor_id)
);
CREATE TABLE operations (
    contributor_id TEXT NOT NULL REFERENCES contributors(id),
    operation_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    target_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    applied_revision INTEGER NOT NULL,
    PRIMARY KEY (contributor_id, operation_id)
);
CREATE TABLE counters (name TEXT PRIMARY KEY NOT NULL, value INTEGER NOT NULL);
INSERT INTO counters VALUES ('change_sequence', 0);
CREATE TABLE quotas (
    principal TEXT NOT NULL,
    period_ms INTEGER NOT NULL,
    window_start_ms INTEGER NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY (principal, period_ms)
);
CREATE TABLE reports (
    id TEXT PRIMARY KEY NOT NULL,
    spot_id TEXT NOT NULL REFERENCES spots(id),
    reporter_id TEXT NOT NULL REFERENCES contributors(id),
    revision INTEGER NOT NULL CHECK (revision > 0),
    reason TEXT NOT NULL,
    message TEXT,
    status TEXT NOT NULL CHECK (status IN ('open','resolved')),
    created_at_ms INTEGER NOT NULL,
    resolved_at_ms INTEGER,
    resolution_note TEXT
);
CREATE INDEX reports_paging ON reports(status,id);
CREATE TABLE moderation_audit (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    actor_id TEXT NOT NULL REFERENCES contributors(id),
    target_id TEXT NOT NULL,
    action TEXT NOT NULL,
    note TEXT NOT NULL,
    canonical_id TEXT,
    created_at_ms INTEGER NOT NULL
);
CREATE TABLE verifications (
    spot_id TEXT NOT NULL REFERENCES spots(id),
    contributor_id TEXT NOT NULL REFERENCES contributors(id),
    operation_id TEXT NOT NULL,
    observed_revision INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (contributor_id, operation_id)
);
PRAGMA user_version = 1;
PRAGMA application_id = 0x43535054;
