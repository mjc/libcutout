# Local storage maintenance

Setup → Storage → Optimize storage runs an explicit maintenance command. It
does not run during startup and does not upload or delete data. Keep Cutout open
until it finishes; large databases can take several minutes.

The existing Rust database worker serializes maintenance with database reads and
writes. It refuses maintenance during an active or paused ride, a live Bluetooth
capture, or an unfinished PEVCAP import. Before changing data, it checks free space
on the database filesystem against twice the main database size, existing WAL and
shared-memory sizes, and 64 MiB of headroom. This conservative check is a preflight,
not a reservation against other applications consuming disk space.

Maintenance has two parts:

1. Compress legacy uncompressed capture payloads in transactions of at most 256
   events. Each replacement must decode to the exact original bytes. Payloads
   that do not shrink remain unchanged. Event identity, ordering, metadata and
   logical byte counters stay unchanged.
2. Checkpoint the WAL, leave WAL mode, and run SQLite's in-place `VACUUM` with
   16 KiB pages. Check the resulting page size and database integrity, then
   restore WAL mode and full commit synchronization.

SQLite's rollback journal protects the database during compaction. An interrupted
compression pass can retain completed batches; retry safely continues from the
remaining legacy rows. If restoring the connection's operating mode fails, the
worker retires and the existing service recovery path reopens and configures a
new connection before accepting more work.

All capture bytes are retained, including raw packets from known devices,
unsupported or unverified devices, and unknown payload fields. Routes, telemetry,
music history, charging state and import artifacts remain. Managed PEVCAP sidecar
files also remain: published receipts and duplicate-import validation currently
depend on them.

New databases use 16 KiB pages from creation. Existing databases change page size
only through the explicit maintenance action. New capture events continue to use
the existing lossless compression path. These choices reduce avoidable storage
amplification; storage still grows as users retain more recordings.

Tests cover exact payload recovery, multi-batch scans, malformed metadata and
write-failure rollback, active-work refusals, free-space arithmetic, journal mode
restoration, checkpoint retry, and reads after reopen. Full copied-database
comparisons and installed-device acceptance are recorded separately in LIBCU-713.
