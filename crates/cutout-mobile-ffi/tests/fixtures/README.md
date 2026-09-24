# Shared VESC replay contract

`vesc-replay-v1.json` is a test-only, Rust-owned corpus. The mobile-FFI integration
test and `CutoutAppModelTests.testVescNotificationReachesRideAndDebugThroughTheAppRunner`
read this exact file; neither production code nor generated UniFFI bindings read
fixtures.

Version 1 records canonical 16-byte service/channel UUIDs, ordered raw notification
bytes, strictly increasing host monotonic milliseconds, expected Rust parser
outcome, and expected accumulated voltage/speed after each notification. Null
means the snapshot has no value, not zero. Each notification is bounded to 1024
bytes. Values are independently chosen physical quantities encoded according to
the source revisions in `provenance`; this corpus is synthetic, not hardware proof.

The Swift app-model consumer replays the fixture notification bytes through the
test session runner and verifies the final fixture speed and voltage reach the
app telemetry snapshot, Ride projection, and Debug projection. It uses
source-relative file access for the repository's macOS Swift package tests; this
is not an iOS bundled resource. The Rust integration test additionally checks
exact ingest outcome and receive metadata.

This advances LIBCU-602 without completing its entire acceptance matrix. Remaining:
every split boundary, warning and finite-only float16 cases at the Swift boundary,
backpressure, retry cancellation after a healthy sample, response loss/reconnect,
and full raw-telemetry capture metadata assertions. The fixture includes the
complete current 393-byte Refloat 1.3.0 descriptor and its 32-bit state-flags data
layout.
In particular, coalesced Rust input currently labels each frame length as a
notification length (12 bytes for the first frame of the 47-byte mixed fixture).
The fixture preserves the original notification and the Swift receive record
checks its full byte count; the Rust test only bounds each evidence length.
Unknown-channel policy also remains open: the current generic decoder sends
unknown-channel bytes into the parser rather than emitting a wrong-channel result.
