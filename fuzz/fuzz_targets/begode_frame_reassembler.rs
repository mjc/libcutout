#![no_main]

use cutout_protocols::{BEGODE_FRAME_LEN, BegodeFrameParseResult, BegodeFrameReassembler};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT_LEN: usize = 4096;

fuzz_target!(|data: &[u8]| {
    let mut reassembler = BegodeFrameReassembler::default();
    let mut frames = 0usize;

    for (index, byte) in data.iter().take(MAX_INPUT_LEN).copied().enumerate() {
        match reassembler.feed_byte_result_at(byte, index as u64) {
            Ok(BegodeFrameParseResult::Complete(frame)) => {
                frames = frames.saturating_add(1);
                assert_eq!(frame.as_slice().len(), BEGODE_FRAME_LEN);
                assert!(frames <= index.saturating_add(1));
            }
            Ok(BegodeFrameParseResult::Seeking | BegodeFrameParseResult::Buffered) | Err(_) => {}
        }
    }
});
