#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use cutout_btle::{MonotonicMs, SessionBridgeEvent, SessionBridgeReport};
use cutout_core::{
    GattChannel, MonotonicTimestamp, NotificationByteLen, NotificationIngestOutcome,
    PayloadBodyLen, PayloadClassifier, ProtocolFamily, ProtocolSelector, ReservedPayloadEvidence,
    VerificationStatus,
};

const fn ms(value: u64) -> MonotonicTimestamp {
    MonotonicTimestamp::new(value)
}

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static REALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

// SAFETY: this wrapper only increments atomic counters and delegates all
// allocation operations to `System` with the original pointers and layouts.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: delegate to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: delegate to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: delegate to the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        REALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: delegate to the system allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

fn reset_allocations() {
    ALLOCATIONS.store(0, Ordering::SeqCst);
    REALLOCATIONS.store(0, Ordering::SeqCst);
}

fn assert_no_allocations(label: &str) {
    assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0, "{label} allocated");
    assert_eq!(
        REALLOCATIONS.load(Ordering::SeqCst),
        0,
        "{label} reallocated"
    );
}

#[test]
fn btle_report_records_typed_ingest_outcomes_without_allocating_after_setup() {
    let channel = GattChannel::from_bytes([0xA1; 16]);
    let outcome = NotificationIngestOutcome::known_reserved(
        ProtocolFamily::VeteranLeaperkimNosfet,
        channel,
        NotificationByteLen::new(75),
        ms(4),
        ReservedPayloadEvidence {
            classifier: PayloadClassifier::selector(ProtocolSelector::new(8)),
            body_len: PayloadBodyLen::new(24),
            verification: VerificationStatus::HardwareVerified,
        },
    );
    let mut report = SessionBridgeReport {
        events: Vec::with_capacity(1),
        ..SessionBridgeReport::default()
    };

    reset_allocations();

    report.record_notification_ingest(outcome, MonotonicMs::new(4));

    assert_no_allocations("btle report typed ingest recording");
    assert_eq!(
        report.events.as_slice(),
        &[SessionBridgeEvent::NotificationIngest {
            monotonic_ms: MonotonicMs::new(4),
            outcome,
        }]
    );
}
