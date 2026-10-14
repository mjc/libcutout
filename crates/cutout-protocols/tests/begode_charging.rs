#![cfg_attr(test, allow(clippy::disallowed_macros))]

use cutout_core::{ChargeMode, HostSession, LinkInfo, Measured, MonotonicTimestamp, SessionInput};
use cutout_protocols::{BEGODE_DATA_CHANNEL, BegodeFalconModel, ReadOnlySession};

type Falcon = HostSession<ReadOnlySession<BegodeFalconModel>>;

fn frame(tag: u8) -> [u8; 24] {
    let mut bytes = [0; 24];
    bytes[..2].copy_from_slice(&[0x55, 0xaa]);
    bytes[18] = tag;
    bytes[20..].fill(0x5a);
    bytes
}

fn live(raw_voltage: u16, speed_cm_s: i16) -> [u8; 24] {
    let mut bytes = frame(0);
    bytes[2..4].copy_from_slice(&raw_voltage.to_be_bytes());
    bytes[4..6].copy_from_slice(&speed_cm_s.to_be_bytes());
    bytes
}

fn placeholder() -> [u8; 24] {
    let mut bytes = frame(1);
    bytes[2..4].copy_from_slice(&75_u16.to_be_bytes());
    bytes[6..8].copy_from_slice(&985_u16.to_be_bytes());
    bytes[12..14].copy_from_slice(&5000_i16.to_be_bytes());
    bytes
}

fn connected() -> Falcon {
    let mut host = HostSession::new(ReadOnlySession::default());
    host.ingest_link_up(LinkInfo {
        monotonic_ms: MonotonicTimestamp::new(0),
        max_write_len: None,
    });
    host
}

fn notify(host: &mut Falcon, bytes: &[u8], seconds: u64) {
    host.ingest(SessionInput::Notification {
        channel: BEGODE_DATA_CHANNEL,
        bytes,
        monotonic_ms: MonotonicTimestamp::new(seconds * 1000),
    });
    // The real transport drains outputs after each input.
    let _ = host.drain_outputs();
}

fn progress(host: &mut Falcon, placeholders: bool) {
    for second in 0_u64..=200 {
        let rise = u16::try_from(second.saturating_sub(60) / 2).unwrap();
        notify(host, &live(6400 + rise, 0), second);
        if placeholders {
            notify(host, &placeholder(), second);
            notify(host, &frame(2), second);
        }
    }
}

#[test]
fn placeholder_fallback_reaches_shared_snapshot_and_motion_clears_it() {
    let mut host = connected();
    progress(&mut host, true);
    assert_eq!(
        host.current_snapshot().charge_mode,
        Some(Measured::estimated(ChargeMode::Charging))
    );
    // Placeholders never overwrite main pack measurements.
    assert!(
        host.current_snapshot()
            .voltage
            .unwrap()
            .value
            .as_millivolts()
            > 96_000
    );
    for speed in [50, -50] {
        notify(&mut host, &live(6470, speed), 201);
        assert_eq!(
            host.current_snapshot().charge_mode.unwrap().value,
            ChargeMode::NotCharging
        );
    }
}

#[test]
fn absent_bms_does_not_enable_the_placeholder_fallback() {
    let mut host = connected();
    progress(&mut host, false);
    assert_eq!(
        host.current_snapshot().charge_mode.unwrap().value,
        ChargeMode::Unknown
    );
}

#[test]
fn stale_or_disconnected_placeholder_charge_is_unknown() {
    let mut host = connected();
    progress(&mut host, true);
    assert_eq!(
        host.current_snapshot().charge_mode.unwrap().value,
        ChargeMode::Charging
    );
    host.tick(MonotonicTimestamp::new(206_000));
    assert_eq!(
        host.current_snapshot().charge_mode.unwrap().value,
        ChargeMode::Unknown
    );
    host.ingest_link_down();
    assert_eq!(host.current_snapshot().charge_mode, None);
    host.ingest_link_up(LinkInfo {
        monotonic_ms: MonotonicTimestamp::new(207_000),
        max_write_len: None,
    });
    notify(&mut host, &live(6470, 0), 207);
    assert_eq!(
        host.current_snapshot().charge_mode.unwrap().value,
        ChargeMode::Unknown
    );
}
