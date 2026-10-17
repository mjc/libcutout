# Server account deletion

`DELETE /v1/identities/current` accepts only the current bearer credential and
has no identity selector or request body. It completes synchronously in one
SQLite write transaction and returns
`{"status":"completed","completed_at_ms":<milliseconds since Unix epoch>}`.
The same bearer credential can retrieve that identical receipt from this route
for 30 days after completion. The server stores only its token hash and receipt
timestamps during that period; after expiry it removes the receipt and the old
credential no longer authorizes any operation. Expired receipts are reclaimed
at the start of every deletion-route request, including a request whose
credential is invalid. There is no background expiry worker; cleanup therefore
occurs when that route is next called.

The server currently stores contributor credentials, charging spots, votes,
operation receipts, quotas, authored reports, moderation audit entries,
verification idempotency records, and explicit private diagnostic capture
submissions. It has no published-route table or ride-backup service.
Deletion removes the contributor
row, credential, votes, operation receipts, quotas, authored reports,
verification records, private capture bytes, their metadata and receipts,
cancellation tombstones, capture request quotas, and the private capture review
capability. It anonymizes moderation audit authors as the system
principal and replaces their note text with a fixed deletion notice.

All charging spots owned by the deleted contributor remain with their exact
content and status. The server changes only their owner to the single revoked
`Anonymous` system principal and advances their discovery sequence. Other
contributors' votes and content remain intact. There is no username field in
this server schema. Local rides and Bluetooth captures live on the device and
are outside this API. Deleting a server submission leaves the local original
intact. A submission upload rechecks its actor before commit, so an upload in
progress cannot publish after that account's deletion.

Private diagnostic evidence remains until the owner deletes the submission or
account. `DELETE /v1/capture-submissions/{operation_id}` removes its bytes and
metadata and retains only an owner-bound cancellation tombstone. The same route
can cancel an operation before upload completes; later PUT retries cannot
recreate it. Wrong-owner access returns not found. Private capture review needs
the separately provisioned `identity --private-capture-reviewer` capability;
charging moderation alone does not permit access. The upload API permits up to
100 MB (100,000,000 bytes) per capture, 1 GiB retained per owner, and 10 GiB
retained globally, with two concurrent transfers globally, one per account, and
bounded request budgets.
It uses private staging
files, removed on failure and reclaimed on service startup, then commits bytes
and the receipt together in SQLite. Request cancellation can race with a
completed commit; receipt lookup or explicit DELETE reconciles that outcome.
The service cannot retract evidence a reviewer has already downloaded. Review
access does not grant fixture publication, redistribution, or third-party use.

The system principal has a fixed identifier, a non-credential sentinel hash,
and a revocation timestamp. Authentication and write authorization explicitly
reject this identifier independently of its revocation flag; the sentinel is
not a bearer credential. Operator revocation remains a separate
access-revocation command and does not perform this cleanup or ownership
transfer.

Database snapshots are immutable point-in-time copies. A snapshot made before
deletion can still contain the account and its private data. Operators must
expire every pre-deletion snapshot within the same 30-day retention window and
must reconcile still-retained deletion receipts against any pre-deletion
snapshot before serving a restore. Do not put such a snapshot back into active
service without that reconciliation. The server cannot erase copies held by an
external backup system, so its backup retention and restore process must enforce
these rules.
