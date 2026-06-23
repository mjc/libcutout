use arrayvec::ArrayVec;
use cutout_core::{MonotonicMillis, ProtocolSelector, ProtocolTag};
use thiserror::Error;

/// Complete fixed-size Begode/Gotway frame length.
pub const BEGODE_FRAME_LEN: usize = 24;

const BEGODE_HEADER: [u8; 2] = [0x55, 0xaa];
const BEGODE_TERMINATOR: [u8; 4] = [0x5a; 4];

/// Complete Begode/Gotway 24-byte frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BegodeFrame {
    bytes: [u8; BEGODE_FRAME_LEN],
}

impl BegodeFrame {
    /// Attempts to copy and validate a complete Begode frame.
    ///
    /// # Errors
    ///
    /// Returns [`BegodeFrameError::InvalidFrame`] when the input is not exactly
    /// one Begode frame with `55 aa` header and `5a5a5a5a` terminator.
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, BegodeFrameError> {
        if bytes.len() != BEGODE_FRAME_LEN
            || !bytes.starts_with(&BEGODE_HEADER)
            || bytes.get(BEGODE_FRAME_LEN - BEGODE_TERMINATOR.len()..BEGODE_FRAME_LEN)
                != Some(BEGODE_TERMINATOR.as_slice())
        {
            return Err(BegodeFrameError::InvalidFrame);
        }

        let mut frame = [0; BEGODE_FRAME_LEN];
        frame.copy_from_slice(bytes);
        Ok(Self { bytes: frame })
    }

    /// Returns the complete frame bytes.
    #[must_use]
    pub const fn as_slice(&self) -> &[u8; BEGODE_FRAME_LEN] {
        &self.bytes
    }

    /// Returns the Begode frame tag at offset 18.
    #[must_use]
    pub fn tag(&self) -> ProtocolTag {
        ProtocolTag::new(u16::from(self.bytes.get(18).copied().unwrap_or_default()))
    }

    /// Returns the Begode sub-index byte at offset 19.
    #[must_use]
    pub fn sub_index(&self) -> ProtocolSelector {
        ProtocolSelector::new(self.bytes.get(19).copied().unwrap_or_default())
    }
}

/// Error emitted while decoding Begode/Gotway frames.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BegodeFrameError {
    /// Bytes did not form a valid fixed-size Begode frame.
    #[error("invalid Begode frame")]
    InvalidFrame,
}

/// Sync reassembler for fixed 24-byte Begode/Gotway notification streams.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BegodeFrameReassembler {
    buffer: ArrayVec<u8, BEGODE_FRAME_LEN>,
    last_byte_ms: Option<MonotonicMillis>,
}

impl BegodeFrameReassembler {
    /// Drops any partial frame state.
    pub fn reset(&mut self) {
        self.clear_buffer();
    }

    /// Drops partial frame state if no byte has arrived within `timeout_ms`.
    ///
    /// Returns `true` when a partial frame was expired.
    #[must_use]
    pub fn expire_idle(
        &mut self,
        monotonic_ms: MonotonicMillis,
        timeout_ms: MonotonicMillis,
    ) -> bool {
        let Some(last_byte_ms) = self.last_byte_ms else {
            return false;
        };
        if self.buffer.is_empty() || monotonic_ms.saturating_sub(last_byte_ms) <= timeout_ms {
            return false;
        }

        self.clear_buffer();
        true
    }

    /// Feeds one byte and returns a complete frame when one is assembled.
    ///
    /// # Errors
    ///
    /// Returns [`BegodeFrameError::InvalidFrame`] when a 24-byte candidate has
    /// the Begode header but not the required terminator.
    pub fn feed_byte(&mut self, byte: u8) -> Result<Option<BegodeFrame>, BegodeFrameError> {
        if self.buffer.is_empty() {
            if byte == BEGODE_HEADER[0] {
                self.buffer.push(byte);
                self.last_byte_ms = None;
            }
            return Ok(None);
        }

        if self.buffer.len() == 1 {
            if byte == BEGODE_HEADER[1] {
                self.buffer.push(byte);
            } else if byte != BEGODE_HEADER[0] {
                self.clear_buffer();
            }
            return Ok(None);
        }

        let pushed = self.buffer.try_push(byte);
        debug_assert!(pushed.is_ok());
        self.apply_embedded_header_resync();

        if self.buffer.len() != BEGODE_FRAME_LEN {
            return Ok(None);
        }

        let frame = BegodeFrame::try_from_slice(self.buffer.as_slice());
        self.clear_buffer();
        frame.map(Some)
    }

    /// Feeds one byte and records the byte arrival time for timeout handling.
    ///
    /// # Errors
    ///
    /// Returns [`BegodeFrameError::InvalidFrame`] when a 24-byte candidate has
    /// the Begode header but not the required terminator.
    pub fn feed_byte_at(
        &mut self,
        byte: u8,
        monotonic_ms: MonotonicMillis,
    ) -> Result<Option<BegodeFrame>, BegodeFrameError> {
        let frame = self.feed_byte(byte)?;
        self.last_byte_ms = if self.buffer.is_empty() {
            None
        } else {
            Some(monotonic_ms)
        };
        Ok(frame)
    }

    fn apply_embedded_header_resync(&mut self) {
        let single_terminator_glitch = self.buffer.as_slice() == [0x55, 0xaa, 0x5a, 0x55, 0xaa];
        let double_terminator_glitch =
            self.buffer.as_slice() == [0x55, 0xaa, 0x5a, 0x5a, 0x55, 0xaa];
        if single_terminator_glitch || double_terminator_glitch {
            self.keep_trailing_header();
        }
    }

    fn keep_trailing_header(&mut self) {
        self.buffer.clear();
        self.buffer.push(BEGODE_HEADER[0]);
        self.buffer.push(BEGODE_HEADER[1]);
    }

    fn clear_buffer(&mut self) {
        self.buffer.clear();
        self.last_byte_ms = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const LIVE_A: [u8; BEGODE_FRAME_LEN] =
        hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
    const LIVE_B: [u8; BEGODE_FRAME_LEN] =
        hex_literal::hex!("55aa0032000004b10000000013880000000001005a5a5a5a");

    fn feed_bytes(reassembler: &mut BegodeFrameReassembler, bytes: &[u8]) -> Vec<BegodeFrame> {
        bytes
            .iter()
            .filter_map(|byte| {
                reassembler
                    .feed_byte(*byte)
                    .expect("fixture frame is valid")
            })
            .collect()
    }

    #[test]
    fn begode_frame_accepts_complete_live_a_frame() {
        let frame = BegodeFrame::try_from_slice(&LIVE_A).expect("live frame is valid");

        assert_eq!(frame.as_slice(), &LIVE_A);
        assert_eq!(frame.tag(), ProtocolTag::new(0x00));
        assert_eq!(frame.sub_index(), ProtocolSelector::new(0x18));
    }

    #[test]
    fn begode_frame_rejects_bad_header_or_terminator() {
        let mut bad_header = LIVE_A;
        bad_header[0] = 0x54;
        let mut bad_terminator = LIVE_A;
        bad_terminator[23] = 0x00;

        assert_eq!(
            BegodeFrame::try_from_slice(&bad_header),
            Err(BegodeFrameError::InvalidFrame)
        );
        assert_eq!(
            BegodeFrame::try_from_slice(&bad_terminator),
            Err(BegodeFrameError::InvalidFrame)
        );
    }

    #[test]
    fn reassembler_reassembles_fragmented_frame() {
        let mut reassembler = BegodeFrameReassembler::default();

        assert!(feed_bytes(&mut reassembler, &LIVE_A[..9]).is_empty());
        let frames = feed_bytes(&mut reassembler, &LIVE_A[9..]);

        assert_eq!(frames, vec![BegodeFrame::try_from_slice(&LIVE_A).unwrap()]);
    }

    #[test]
    fn reassembler_returns_multiple_frames_from_one_stream() {
        let mut reassembler = BegodeFrameReassembler::default();
        let mut stream = Vec::new();
        stream.extend_from_slice(&LIVE_A);
        stream.extend_from_slice(&LIVE_B);

        let frames = feed_bytes(&mut reassembler, &stream);

        assert_eq!(
            frames,
            vec![
                BegodeFrame::try_from_slice(&LIVE_A).unwrap(),
                BegodeFrame::try_from_slice(&LIVE_B).unwrap(),
            ]
        );
    }

    #[test]
    fn reassembler_resyncs_after_garbage_before_magic() {
        let mut reassembler = BegodeFrameReassembler::default();
        let mut stream = [0x00, 0x01, 0x02].to_vec();
        stream.extend_from_slice(&LIVE_A);

        let frames = feed_bytes(&mut reassembler, &stream);

        assert_eq!(frames, vec![BegodeFrame::try_from_slice(&LIVE_A).unwrap()]);
    }

    #[test]
    fn reassembler_reset_drops_partial_frame() {
        let mut reassembler = BegodeFrameReassembler::default();

        assert!(feed_bytes(&mut reassembler, &LIVE_A[..12]).is_empty());
        reassembler.reset();
        let frames = feed_bytes(&mut reassembler, &LIVE_A[12..]);

        assert!(frames.is_empty());
    }

    #[test]
    fn reassembler_idle_timeout_drops_partial_frame() {
        let mut reassembler = BegodeFrameReassembler::default();

        for (offset, byte) in LIVE_A[..12].iter().enumerate() {
            assert_eq!(
                reassembler.feed_byte_at(*byte, offset as u64).unwrap(),
                None
            );
        }

        assert!(reassembler.expire_idle(1_012, 1_000));
        let frames = feed_bytes(&mut reassembler, &LIVE_A[12..]);

        assert!(frames.is_empty());
    }

    #[test]
    fn reassembler_timeout_recovery_accepts_next_complete_frame() {
        let mut reassembler = BegodeFrameReassembler::default();

        for (offset, byte) in LIVE_A[..12].iter().enumerate() {
            assert_eq!(
                reassembler.feed_byte_at(*byte, offset as u64).unwrap(),
                None
            );
        }

        assert!(reassembler.expire_idle(1_012, 1_000));
        let frames = feed_bytes(&mut reassembler, &LIVE_A);

        assert_eq!(frames, vec![BegodeFrame::try_from_slice(&LIVE_A).unwrap()]);
    }

    #[test]
    fn reassembler_reports_bad_candidate_and_recovers_next_frame() {
        let mut reassembler = BegodeFrameReassembler::default();
        let mut bad = LIVE_A;
        bad[20] = 0x00;

        let error = bad
            .iter()
            .find_map(|byte| reassembler.feed_byte(*byte).err())
            .expect("bad terminator reports invalid frame");
        let frames = feed_bytes(&mut reassembler, &LIVE_A);

        assert_eq!(error, BegodeFrameError::InvalidFrame);
        assert_eq!(frames, vec![BegodeFrame::try_from_slice(&LIVE_A).unwrap()]);
    }

    #[test]
    fn reassembler_resyncs_documented_single_terminator_glitch_before_header() {
        let mut reassembler = BegodeFrameReassembler::default();
        let mut stream = [0x55, 0xaa, 0x5a].to_vec();
        stream.extend_from_slice(&LIVE_A);

        let frames = feed_bytes(&mut reassembler, &stream);

        assert_eq!(frames, vec![BegodeFrame::try_from_slice(&LIVE_A).unwrap()]);
    }

    #[test]
    fn reassembler_resyncs_documented_double_terminator_glitch_before_header() {
        let mut reassembler = BegodeFrameReassembler::default();
        let mut stream = [0x55, 0xaa, 0x5a, 0x5a].to_vec();
        stream.extend_from_slice(&LIVE_A);

        let frames = feed_bytes(&mut reassembler, &stream);

        assert_eq!(frames, vec![BegodeFrame::try_from_slice(&LIVE_A).unwrap()]);
    }

    proptest! {
        #[test]
        fn arbitrary_fragmentation_matches_whole_frame(split in 0usize..=BEGODE_FRAME_LEN) {
            let mut whole = BegodeFrameReassembler::default();
            let whole_frames = feed_bytes(&mut whole, &LIVE_A);

            let mut fragmented = BegodeFrameReassembler::default();
            let mut fragmented_frames = feed_bytes(&mut fragmented, &LIVE_A[..split]);
            fragmented_frames.extend(feed_bytes(&mut fragmented, &LIVE_A[split..]));

            prop_assert_eq!(fragmented_frames, whole_frames);
        }
    }
}
