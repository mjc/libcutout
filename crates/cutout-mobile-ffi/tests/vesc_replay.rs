use cutout_mobile_ffi::{
    MobileMonotonicMillisDto, MobileSessionInputDto, MobileSessionInputKindDto,
    MobileTransportWriteLimitDto, VescReadOnlySession,
};
use serde_json::Value;

#[test]
fn shared_vesc_fixture_replays_through_mobile_ffi() {
    let fixture = load_fixture();
    assert_eq!(fixture["version"], 1);
    let session = VescReadOnlySession::new();
    let link = session.ingest_checked(MobileSessionInputDto {
        kind: MobileSessionInputKindDto::LinkUp,
        monotonic_ms: MobileMonotonicMillisDto { milliseconds: 0 },
        max_write_len: Some(MobileTransportWriteLimitDto { bytes: 20 }),
        channel: Vec::new(),
        bytes: Vec::new(),
        command: None,
    });
    assert_eq!(link.error, None);
    let mut previous_time = 0;
    for notification in fixture["notifications"]
        .as_array()
        .expect("notification sequence")
    {
        let at = notification["monotonic_ms"].as_u64().expect("timestamp");
        assert!(at > previous_time);
        previous_time = at;
        let bytes = byte_array(&notification["bytes"]);
        assert!(bytes.len() <= 1024);
        let channel = byte_array(&notification["channel"]);
        let step = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Notification,
            monotonic_ms: MobileMonotonicMillisDto { milliseconds: at },
            max_write_len: None,
            channel: channel.clone(),
            bytes: bytes.clone(),
            command: None,
        });
        assert_eq!(step.error, None, "{}", notification["name"]);
        assert_eq!(
            step.outputs
                .iter()
                .filter(|output| output.ingest.is_some())
                .count() as u64,
            notification["ingest_count"].as_u64().expect("ingest count"),
            "{}: every coalesced reply crosses the FFI boundary",
            notification["name"]
        );
        let ingest = step
            .outputs
            .iter()
            .find_map(|output| output.ingest.as_ref())
            .unwrap_or_else(|| {
                panic!(
                    "{}: missing parser outcome: {:?}",
                    notification["name"], step.outputs
                )
            });
        assert_eq!(
            format!("{:?}", ingest.kind),
            notification["ingest"].as_str().expect("ingest kind"),
            "{}",
            notification["name"]
        );
        let fresh_refloat_events = step
            .outputs
            .iter()
            .filter(|output| output.vesc_realtime_telemetry)
            .count();
        if notification["name"] == "ordinary values complete" {
            assert_eq!(
                fresh_refloat_events, 0,
                "generic VESC values must not satisfy the Refloat startup retry"
            );
        }
        if notification["name"] == "Refloat 1.3 complete runtime data with alerts" {
            assert_eq!(
                fresh_refloat_events, 1,
                "a fresh Refloat realtime event must satisfy the startup retry"
            );
        }
        if let Some(evidence) = &ingest.notification {
            assert_eq!(evidence.channel, channel);
            assert!(
                evidence.len.bytes <= bytes.len() as u64,
                "frame-level ingest evidence cannot exceed its source notification"
            );
            assert_eq!(evidence.monotonic_ms.milliseconds, at);
        }
        let snapshot = session.current_snapshot();
        assert_eq!(
            snapshot
                .voltage
                .map(|reading| i64::from(reading.value.value)),
            notification["voltage_mv"].as_i64(),
            "{}",
            notification["name"]
        );
        assert_eq!(
            snapshot.speed.map(|reading| i64::from(reading.value.value)),
            notification["speed_mmps"].as_i64(),
            "{}",
            notification["name"]
        );
    }
}

#[test]
fn complete_refloat_descriptor_survives_every_notification_split() {
    let fixture = load_fixture();
    let notifications = fixture["notifications"].as_array().expect("notifications");
    let descriptor = notifications
        .iter()
        .find(|notification| notification["name"] == "Refloat 1.3 complete 393-byte descriptor")
        .expect("complete descriptor fixture");
    let realtime = notifications
        .iter()
        .find(|notification| {
            notification["name"] == "Refloat 1.3 complete runtime data with alerts"
        })
        .expect("complete runtime fixture");
    let descriptor_bytes = byte_array(&descriptor["bytes"]);
    let realtime_bytes = byte_array(&realtime["bytes"]);
    assert_eq!(descriptor_bytes.len(), 393);

    for split in 1..descriptor_bytes.len() {
        let session = VescReadOnlySession::new();
        let link = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::LinkUp,
            monotonic_ms: MobileMonotonicMillisDto { milliseconds: 0 },
            max_write_len: Some(MobileTransportWriteLimitDto { bytes: 20 }),
            channel: Vec::new(),
            bytes: Vec::new(),
            command: None,
        });
        assert_eq!(link.error, None);
        for (index, bytes) in [
            descriptor_bytes[..split].to_vec(),
            descriptor_bytes[split..].to_vec(),
            realtime_bytes.clone(),
        ]
        .into_iter()
        .enumerate()
        {
            let step = session.ingest_checked(MobileSessionInputDto {
                kind: MobileSessionInputKindDto::Notification,
                monotonic_ms: MobileMonotonicMillisDto {
                    milliseconds: u64::try_from(index + 1).expect("small split index"),
                },
                max_write_len: None,
                channel: byte_array(&descriptor["channel"]),
                bytes,
                command: None,
            });
            assert_eq!(step.error, None, "split {split}, part {index}");
        }
        let snapshot = session.current_snapshot();
        assert_eq!(
            snapshot.speed.map(|reading| reading.value.value),
            Some(10_000)
        );
        assert_eq!(
            snapshot.voltage.map(|reading| reading.value.value),
            Some(75_500)
        );
    }
}

fn load_fixture() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/vesc-replay-v1.json"
        ))
        .expect("the Rust-owned shared VESC replay fixture must exist"),
    )
    .expect("valid fixture JSON")
}

fn byte_array(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .expect("byte array")
        .iter()
        .map(|byte| u8::try_from(byte.as_u64().expect("byte integer")).expect("u8"))
        .collect()
}
