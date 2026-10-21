#![cfg_attr(test, allow(clippy::disallowed_macros))]
#![allow(unsafe_code)]

#[path = "../../../tests/support/allocation_counter.rs"]
mod allocation_counter;

use allocation_counter::assert_no_allocations;

use cutout_btle::{MonotonicMs, SessionBridgeEvent, SessionBridgeReport};
use cutout_core::{
    GattChannel, MonotonicTimestamp, NotificationByteLen, NotificationIngestOutcome,
    PayloadBodyLen, PayloadClassifier, ProtocolFamily, ProtocolSelector, ReservedPayloadEvidence,
    RetainedNotificationPayload, VerificationStatus,
};

const fn ms(value: u64) -> MonotonicTimestamp {
    MonotonicTimestamp::new(value)
}

#[test]
fn btle_report_records_typed_ingest_outcomes_without_allocating_after_setup() {
    let channel = GattChannel::from_bytes([0xA1; 16]);
    let outcome = NotificationIngestOutcome::known_reserved(
        ProtocolFamily::VeteranLeaperkimNosfet,
        channel,
        NotificationByteLen::from_bytes(75),
        ms(4),
        ReservedPayloadEvidence {
            classifier: PayloadClassifier::selector(ProtocolSelector::new(8)),
            body_len: PayloadBodyLen::from_bytes(24),
            retained_payload: RetainedNotificationPayload::from_bytes(&[0x08]),
            verification: VerificationStatus::HardwareVerified,
        },
    );
    let mut report = SessionBridgeReport {
        events: Vec::with_capacity(1),
        ..SessionBridgeReport::default()
    };

    assert_no_allocations("btle report typed ingest recording", || {
        report.record_notification_ingest(outcome, MonotonicMs::new(4));
    });
    let [
        SessionBridgeEvent::NotificationIngest {
            monotonic_ms,
            outcome,
        },
    ] = report.events.as_slice()
    else {
        panic!("expected one notification ingest event");
    };
    assert_eq!(*monotonic_ms, MonotonicMs::new(4));
    assert!(matches!(
        outcome,
        NotificationIngestOutcome::KnownReserved { .. }
    ));
}
