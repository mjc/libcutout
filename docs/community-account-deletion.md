# Community account deletion

Implemented for LIBCU-972 and LIBCU-973. The release gate is LIBCU-953.

## Product and data contract

Account deletion removes the first-party contributor account and its private
records. Retained public charging points belong to one generic, non-login
Anonymous principal. Their content and publication status remain unchanged;
the departing account cannot subsequently edit them. There is no username
column in the current service. A future public-route service must implement the
same ownership transfer; no such service currently exists.

Home → gear → Community account → Delete Account explains this behavior before
submission. Users who want their public contributions removed can cancel and
delete those contributions manually first. Confirmation does not delete local
rides, Bluetooth captures, or third-party music/leaderboard accounts.

The current server inventory and external backup obligations are documented in
[the server contract](../crates/cutout-server/ACCOUNT_DELETION.md). Public content
may itself contain user-authored personal details; reassignment does not rewrite
the contribution body. Do not describe reassignment as content redaction.

## Operation and crash recovery

`DELETE /v1/identities/current` uses the current bearer credential and accepts no
identity selector. The server completes deletion in one transaction and returns
`{ "status": "completed", "completed_at_ms": ... }`. There is no asynchronous
deletion job. The same credential retrieves the same confirmation for 30 days,
but cannot authenticate ordinary service requests after deletion. Only a hash
of that credential remains in the temporary confirmation record.

Swift owns Keychain access and presentation. Rust owns the authenticated request,
durable local fence, receipt validation, and transactional queue cleanup. Before
submission Swift pauses and drains its active sync task and durably marks the
Keychain credential as deletion pending. The Rust operation then commits
`Active → Deleting` before waiting for older synchronization to settle. Storage
transactions reject new mutations and late acknowledgements while deleting.

An error or lost response leaves the credential and pending state available for
retry. Completion atomically clears the contribution outbox, cached personal
votes, and ownership flags, then stores `Deleted { completed_at_milliseconds }`.
Retained local charging geometry, rides, and captures remain. Native credential
cleanup follows this durable confirmation and leaves a token-free tombstone.
If Keychain cleanup fails, the stored Rust receipt permits cleanup retry without
depending on the remote confirmation window.

Pending and deleted Keychain records never trigger automatic registration, even
through explicit synchronization retry. Public directory refresh remains usable
and strips personal vote data while the account fence is closed. A queued draft
whose upload was cancelled remains labeled “Saved on this device.” There is no
automatic replacement-account or account-reactivation feature.

Mobile database schema 46 adds the durable state to `charging_sync_state`.
Migration from 45 preserves rides, pins, and existing queue contents; only a
confirmed deletion removes obsolete queued contributions. Inconsistent stored
phase/timestamp pairs fail closed.

## Verification boundaries

Server tests cover current-owner authentication, revoked-owner deletion,
Anonymous non-login behavior, retained public content, private-record cleanup,
unrelated contributors, stale actors, retries, expiry collection, and restart.
Persistence tests cover migration, transaction rollback, durable fences, late
receipts, public refresh, and retained local artifacts. Rust integration tests
use the real HTTP server and deliberately lose a successful deletion response.
Native tests cover platform credential effects and retry/relaunch state.

Simulator UI tests use an isolated database, isolated Keychain account, and an
explicit HTTP loopback service through the real Rust operation. They traverse
Home → gear → confirmation → cancel → confirm → force quit/relaunch, inspect
server authorization, and verify retained local ride identifiers and captures.
The accessibility variant runs at the largest accessibility text size.
Run both cases with the [Simulator integration harness](account-deletion-ui-test-harness.md),
which provisions fresh test accounts and verifies server ownership and authorization
after the UI flow. The harness reuses the existing build caches.

These tests do not establish production deployment, external backup enforcement,
or App Review acceptance. LIBCU-953 retains the existing review question about
the product's public-content retention policy. Operators must enforce the
documented 30-day backup expiry and deletion reconciliation before restoring
older snapshots; the current service does not automate external backup handling.
