#![no_main]

use cutout_protocols::{MAX_VETERAN_FRAME_LEN, VeteranFrameReassembler};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT_LEN: usize = 4096;

fuzz_target!(|data: &[u8]| {
    let mut reassembler = VeteranFrameReassembler::default();
    let mut frames = 0usize;

    for (index, byte) in data.iter().take(MAX_INPUT_LEN).copied().enumerate() {
        match reassembler.feed_byte(byte) {
            Ok(Some(frame)) => {
                frames = frames.saturating_add(1);
                assert!(frame.as_slice().len() <= MAX_VETERAN_FRAME_LEN);
                assert!(frames <= index.saturating_add(1));
            }
            Ok(None) | Err(_) => {}
        }
    }
});
