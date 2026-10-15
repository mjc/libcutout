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
operation receipts, quotas, authored reports, moderation audit entries, and
verification idempotency records. It has no published-route table, cloud
capture, private-upload, or outbox service. Deletion removes the contributor
row, credential, votes, operation receipts, quotas, authored reports, and
verification records. It anonymizes moderation audit authors as the system
principal and replaces their note text with a fixed deletion notice.

All charging spots owned by the deleted contributor remain with their exact
content and status. The server changes only their owner to the single revoked
`Anonymous` system principal and advances their discovery sequence. Other
contributors' votes and content remain intact. There is no username field in
this server schema. Local rides and Bluetooth captures live on the device and
are outside this API.

The system principal has a fixed identifier, a non-credential sentinel hash,
and a revocation timestamp. Authentication rejects it because it is revoked;
the sentinel is not a bearer credential. Operator revocation remains a separate
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
