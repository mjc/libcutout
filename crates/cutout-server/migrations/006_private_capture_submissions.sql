ALTER TABLE contributors ADD COLUMN private_capture_review INTEGER NOT NULL DEFAULT 0
    CHECK (private_capture_review IN (0, 1));
CREATE TABLE capture_submissions (
    operation_id TEXT PRIMARY KEY NOT NULL,
    owner_id TEXT NOT NULL REFERENCES contributors(id),
    metadata TEXT,
    receipt TEXT,
    summary TEXT,
    bytes INTEGER NOT NULL DEFAULT 0 CHECK (bytes >= 0 AND bytes <= 536870912),
    payload BLOB,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    CHECK ((deleted = 1 AND payload IS NULL AND metadata IS NULL AND receipt IS NULL
            AND summary IS NULL AND bytes = 0)
        OR (deleted = 0 AND payload IS NOT NULL AND metadata IS NOT NULL
            AND receipt IS NOT NULL AND summary IS NOT NULL AND length(payload) = bytes))
);
CREATE INDEX capture_submissions_owner ON capture_submissions(owner_id, operation_id);
PRAGMA user_version = 6;
