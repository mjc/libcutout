#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use cutout_core::{
    CommandKind, LinkInfo, MonotonicTimestamp, ProtocolSession, SessionInput, SessionOutput,
    TransportWriteLimit,
};
use cutout_protocols::{
    BEGODE_DATA_CHANNEL, BEGODE_FRAME_LEN, BegodeFalconModel, BegodeFrameParseResult,
    BegodeFrameReassembler, FalconRequestEncoder, NosfetAeroModel, ReadOnlyModelSpec,
    ReadOnlySession, RefloatStreamDecoder, RefloatStreamResult, VESC_NOTIFY_CHANNEL,
    VETERAN_DATA_CHANNEL, VescGenericModel, VescReadOnlyStreamDecoder, VescReadOnlyStreamResult,
    VescRequestEncoder,
};

const REFLOAT_IDS_FRAME: &[u8] = &[
    2, 35, 36, 101, 32, 2, 11, 109, 111, 116, 111, 114, 46, 115, 112, 101, 101, 100, 8, 105, 109,
    117, 46, 114, 111, 108, 108, 1, 8, 115, 101, 116, 112, 111, 105, 110, 116, 88, 149, 3,
];

struct CountingAllocator;

const fn ms(value: u64) -> MonotonicTimestamp {
    MonotonicTimestamp::new(value)
}

const fn write_len(value: u16) -> TransportWriteLimit {
    TransportWriteLimit::from_bytes(value)
}

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static REALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATION_TEST_LOCK: Mutex<()> = Mutex::new(());

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

const LIVE_AERO_SELECTOR_0: [u8; 87] = hex_literal::hex!(
    "dc5a5c532a7c000000000000ab41001700000cff\
     000000000226021ca8f607801afa000080c80000\
     808080808080022880803080800e310e310e2f0e\
     2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e\
     310e2e9e05e3ad"
);
const BEGODE_LIVE_A: [u8; BEGODE_FRAME_LEN] =
    hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
const VESC_VALUES: [u8; 28] = [
    2, 23, 50, 0, 2, 161, 138, 0, 0, 0, 0, 0, 4, 0, 0, 3, 221, 1, 119, 255, 255, 170, 43, 0, 20,
    45, 58, 3,
];

fn reset_counts() {
    ALLOCATIONS.store(0, Ordering::SeqCst);
    REALLOCATIONS.store(0, Ordering::SeqCst);
}

fn assert_no_allocations(label: &str, action: impl FnOnce()) {
    let _guard = ALLOCATION_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    reset_counts();
    action();

    assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0, "{label} allocated");
    assert_eq!(
        REALLOCATIONS.load(Ordering::SeqCst),
        0,
        "{label} reallocated"
    );
}

fn linked_session<M>() -> (ReadOnlySession<M, false>, Vec<SessionOutput>)
where
    M: Default + ReadOnlyModelSpec,
    ReadOnlySession<M, false>: ProtocolSession,
{
    let mut session = ReadOnlySession::<M, false>::default();
    let mut output = Vec::with_capacity(16);
    session.handle(
        SessionInput::LinkUp(LinkInfo {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len(185)),
        }),
        &mut output,
    );
    output.clear();
    reset_counts();
    (session, output)
}

#[test]
fn protocol_parser_owned_results_do_not_allocate() {
    veteran_parser_owned_results_do_not_allocate();
    begode_parser_owned_results_do_not_allocate();
    vesc_parser_owned_results_do_not_allocate();
}

#[test]
fn read_request_encoders_do_not_allocate() {
    assert_no_allocations("read request encoding", || {
        assert!(FalconRequestEncoder::encode_command(CommandKind::RequestIdentity).is_some());
        assert!(VescRequestEncoder::encode_command(CommandKind::RequestTelemetry).is_some());
        assert!(VescRequestEncoder::encode_command(CommandKind::RequestDiagnostics).is_some());
    });
}

#[test]
fn refloat_parser_owned_results_do_not_allocate() {
    let mut decoder = RefloatStreamDecoder::new();

    assert_no_allocations("Refloat parser owned result", || {
        let result = decoder
            .feed_result(REFLOAT_IDS_FRAME, |_| {})
            .expect("fixture frame decodes");
        assert_eq!(result, RefloatStreamResult::Replies(1));
    });
}

fn veteran_parser_owned_results_do_not_allocate() {
    let mut veteran = ReadOnlySession::<NosfetAeroModel, false>::default();
    let mut veteran_output = Vec::with_capacity(8);
    veteran.handle(
        SessionInput::LinkUp(LinkInfo {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len(185)),
        }),
        &mut veteran_output,
    );
    veteran_output.clear();
    assert_no_allocations("Veteran partial frame", || {
        veteran.handle(
            SessionInput::Notification {
                channel: VETERAN_DATA_CHANNEL,
                bytes: &LIVE_AERO_SELECTOR_0[..20],
                monotonic_ms: ms(2),
            },
            &mut veteran_output,
        );
        veteran_output.clear();
    });

    let (mut veteran, mut veteran_output) = linked_session::<NosfetAeroModel>();
    assert_no_allocations("Veteran complete frame", || {
        veteran.handle(
            SessionInput::Notification {
                channel: VETERAN_DATA_CHANNEL,
                bytes: &LIVE_AERO_SELECTOR_0,
                monotonic_ms: ms(3),
            },
            &mut veteran_output,
        );
        veteran_output.clear();
    });

    let mut reserved = LIVE_AERO_SELECTOR_0;
    reserved[60] = 8;
    let (mut veteran, mut veteran_output) = linked_session::<NosfetAeroModel>();
    assert_no_allocations("Veteran reserved frame", || {
        veteran.handle(
            SessionInput::Notification {
                channel: VETERAN_DATA_CHANNEL,
                bytes: &reserved,
                monotonic_ms: ms(4),
            },
            &mut veteran_output,
        );
        veteran_output.clear();
    });

    let mut gap = LIVE_AERO_SELECTOR_0;
    gap[60] = 9;
    let (mut veteran, mut veteran_output) = linked_session::<NosfetAeroModel>();
    assert_no_allocations("Veteran parser gap", || {
        veteran.handle(
            SessionInput::Notification {
                channel: VETERAN_DATA_CHANNEL,
                bytes: &gap,
                monotonic_ms: ms(5),
            },
            &mut veteran_output,
        );
        veteran_output.clear();
    });
}

fn begode_parser_owned_results_do_not_allocate() {
    let mut begode_reassembler = BegodeFrameReassembler::default();
    assert_no_allocations("Begode fixed-frame parser", || {
        for (offset, byte) in BEGODE_LIVE_A.iter().copied().enumerate() {
            let result = begode_reassembler
                .feed_byte_result_at(byte, ms(offset as u64))
                .expect("Begode fixture parses");
            if let BegodeFrameParseResult::Complete(_) = result {
                break;
            }
        }
    });

    let (mut begode, mut begode_output) = linked_session::<BegodeFalconModel>();
    assert_no_allocations("Begode notification session", || {
        begode.handle(
            SessionInput::Notification {
                channel: BEGODE_DATA_CHANNEL,
                bytes: &BEGODE_LIVE_A,
                monotonic_ms: ms(6),
            },
            &mut begode_output,
        );
        begode_output.clear();
    });
}

fn vesc_parser_owned_results_do_not_allocate() {
    let mut vesc_decoder = VescReadOnlyStreamDecoder::new();
    assert_no_allocations("VESC stream reply parser", || {
        let result = vesc_decoder
            .feed_result(&VESC_VALUES)
            .expect("VESC fixture parses");
        assert!(matches!(result, VescReadOnlyStreamResult::Replies(_)));
    });

    let (mut vesc, mut vesc_output) = linked_session::<VescGenericModel>();
    assert_no_allocations("VESC notification session", || {
        vesc.handle(
            SessionInput::Notification {
                channel: VESC_NOTIFY_CHANNEL,
                bytes: &VESC_VALUES,
                monotonic_ms: ms(7),
            },
            &mut vesc_output,
        );
        vesc_output.clear();
    });
}
