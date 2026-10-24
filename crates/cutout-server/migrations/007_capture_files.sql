CREATE TABLE capture_submissions_files (
    operation_id TEXT PRIMARY KEY NOT NULL,
    owner_id TEXT NOT NULL REFERENCES contributors(id),
    metadata TEXT,
    receipt TEXT,
    summary TEXT,
    bytes INTEGER NOT NULL DEFAULT 0 CHECK (bytes >= 0 AND bytes <= 536870912),
    deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    CHECK ((deleted = 1 AND metadata IS NULL AND receipt IS NULL
            AND summary IS NULL AND bytes = 0)
        OR (deleted = 0 AND metadata IS NOT NULL
            AND receipt IS NOT NULL AND summary IS NOT NULL))
);
INSERT INTO capture_submissions_files(
    operation_id, owner_id, metadata, receipt, summary, bytes, deleted
)
SELECT operation_id, owner_id, metadata, receipt, summary, bytes, deleted
FROM capture_submissions;
DROP TABLE capture_submissions;
ALTER TABLE capture_submissions_files RENAME TO capture_submissions;
CREATE INDEX capture_submissions_owner ON capture_submissions(owner_id, operation_id);
PRAGMA user_version = 7;
