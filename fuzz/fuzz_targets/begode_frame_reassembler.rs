#![no_main]

use cutout_protocols::{BEGODE_FRAME_LEN, BegodeFrameReassembler};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT_LEN: usize = 4096;

fuzz_target!(|data: &[u8]| {
    let mut reassembler = BegodeFrameReassembler::default();
    let mut frames = 0usize;

    for (index, byte) in data.iter().take(MAX_INPUT_LEN).copied().enumerate() {
        match reassembler.feed_byte_at(byte, index as u64) {
            Ok(Some(frame)) => {
                frames = frames.saturating_add(1);
                assert_eq!(frame.as_slice().len(), BEGODE_FRAME_LEN);
                assert!(frames <= index.saturating_add(1));
            }
            Ok(None) | Err(_) => {}
        }
    }
});
