# VESC and Refloat Boundary

Date: 2026-06-22

This note records the integration decision for VESC/Refloat read-only support.
It is intentionally an architecture and provenance note, not an implementation
port.

## Sources Checked

- `ikalnytskyi/vesc-rs`
  - Clone used for audit: `.protocol-references/vesc-rs`.
  - Commit: `5e154fe2b69dc6269bebd31a519d0b10782ff38b`.
  - License: MIT.
  - Rust edition: 2024.
  - Relevant public API shape: `Command`, `CommandReply`, `ValuesMask`,
    `ValuesSetupMask`, `StatsMask`, `Decoder<const BUFLEN: usize>`, `encode`,
    and `decode`.
  - Notable fit: no-std/no-alloc design, const-generic streaming decoder,
    preallocated encode buffers, and existing coverage for firmware version,
    firmware info, ordinary values, selective values, setup-selective values,
    stats, and CAN forwarding.

- VESC project / Benjamin Vedder UART protocol note
  - URL: `https://vedder.se/2015/10/communicating-with-the-vesc-using-uart/`.
  - Use: packet framing evidence. The note describes UART packets as start byte
    `2` for short payloads or `3` for long payloads, payload length, payload,
    CRC bytes, and stop byte `3`.

- `vedderb/bldc`
  - URL: `https://github.com/vedderb/bldc`.
  - License: GPL-3.0.
  - Use: upstream behavioral/protocol evidence only. Do not copy firmware
    implementation code into libcutout.

## Decision

Use libcutout-owned public types for VESC and Refloat. Do not expose `vesc-rs`
types across any public crate boundary, mobile FFI boundary, capture schema, or
CLI output.

`vesc-rs` is a good private codec candidate because its design matches this
repository's constraints: Rust 2024, no-std/no-alloc, bounded buffers, and a
streaming decoder. The library also already models the read-only commands this
project needs first: firmware version, firmware info, values, selective values,
setup-selective values, stats, and CAN forwarding.

The boundary still needs to stay owned by libcutout for three reasons:

- Public API stability: `vesc-rs` command and reply enums are not libcutout's
  domain model, and future crate changes should not force mobile/CLI/capture
  schema churn.
- Safety scope: `vesc-rs` supports actuator commands. libcutout's VESC support
  must expose read-only requests by default and keep actuator commands
  unavailable unless the safe-controls architecture explicitly enables them.
- Refloat extension scope: Refloat package traffic uses VESC custom-app
  payloads. Those payloads should be represented as namespaced libcutout
  extension requests/responses, not as ad hoc raw bytes leaking through generic
  VESC command enums.

## Implementation Shape

- Add a VESC protocol module that owns libcutout request/response enums,
  telemetry mapping, diagnostics, and session state.
- Keep packet framing/CRC and command payload encoding behind a private module.
  That module may wrap `vesc-rs` after dependency review, or may implement the
  small read-only subset directly from protocol docs and captures if the wrapper
  gets in the way.
- Use const-generic, bounded buffers for frame decode and request encoding.
- Preserve raw eRPM and raw VESC values in diagnostics/read-only responses when
  road-speed configuration is not verified.
- Model CAN forwarding as an explicit read-only target address plus nested
  read-only request. Do not allow nested actuator commands through the read-only
  session.
- Model Refloat custom-app payloads as data-only extension structs with bounded
  strings/arrays. Optional Refloat discovery failure must leave generic VESC
  read-only telemetry usable.

## Follow-Up Work

- Add TDD-first VESC packet framing and request-encoding fixtures from protocol
  docs/captures.
- Private `vesc-rs` adapter spike result: implemented in `cutout-protocols` as
  libcutout-owned request/reply DTOs over a private `vesc` dependency. The first
  adapter covers firmware-info decode, values/selective-values telemetry decode,
  stats decode, read-only request encoding, and CAN-forwarded read-only request
  encoding without exposing `vesc-rs` types or actuator commands.
- Add Refloat custom-app INFO payload tests before implementing realtime field
  discovery.
- Add hardware-gated acceptance for a real Refloat board: capture, stream,
  replay whole notifications, replay one byte at a time, and replay arbitrary
  chunks.
