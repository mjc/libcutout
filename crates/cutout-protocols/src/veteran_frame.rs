use arrayvec::ArrayVec;
use thiserror::Error;

use crate::parser::{ByteCursor, ByteOffset};

/// Maximum complete Veteran/LeaperKim/NOSFET frame length.
pub const MAX_VETERAN_FRAME_LEN: usize = 259;

const VETERAN_MAGIC: [u8; 3] = [0xdc, 0x5a, 0x5c];
const VETERAN_SHORT_FRAME_MAX_LEN: u8 = 38;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct VeteranDeclaredLen(usize);

impl VeteranDeclaredLen {
    const fn from_wire(value: u8) -> Self {
        Self(value as usize)
    }

    const fn get(self) -> usize {
        self.0
    }

    const fn complete_frame_len(self) -> VeteranCompleteFrameLen {
        VeteranCompleteFrameLen(self.0 + 4)
    }

    fn crc_offset(self) -> ByteOffset {
        ByteOffset::new(self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct VeteranCompleteFrameLen(usize);

impl VeteranCompleteFrameLen {
    const fn get(self) -> usize {
        self.0
    }
}

/// Complete Veteran/LeaperKim/NOSFET frame reassembled from BLE notifications.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VeteranFrame {
    bytes: ArrayVec<u8, MAX_VETERAN_FRAME_LEN>,
}

impl VeteranFrame {
    /// Builds a frame from already-reassembled bytes.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranReassemblyError::InvalidFrame`] when the bytes do not
    /// contain the Veteran magic, length byte, and declared frame length.
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, VeteranReassemblyError> {
        if !bytes.starts_with(&VETERAN_MAGIC) {
            return Err(VeteranReassemblyError::InvalidFrame);
        }
        let Some(len) = bytes.get(3) else {
            return Err(VeteranReassemblyError::InvalidFrame);
        };
        if bytes.len()
            != VeteranDeclaredLen::from_wire(*len)
                .complete_frame_len()
                .get()
        {
            return Err(VeteranReassemblyError::InvalidFrame);
        }
        let Ok(bytes) = ArrayVec::try_from(bytes) else {
            return Err(VeteranReassemblyError::InvalidFrame);
        };
        Ok(Self { bytes })
    }

    /// Returns the complete frame bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

/// Error emitted while reassembling Veteran/LeaperKim/NOSFET frames.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum VeteranReassemblyError {
    /// A candidate frame exceeded the bounded parser buffer.
    #[error("Veteran frame exceeded maximum length")]
    OversizedFrame {
        /// Observed frame length.
        claimed: usize,

        /// Configured maximum accepted frame length.
        max: usize,
    },

    /// A complete long frame failed CRC32 validation.
    #[error("Veteran frame CRC mismatch")]
    CrcMismatch,

    /// A complete frame was structurally invalid.
    #[error("invalid Veteran frame")]
    InvalidFrame,
}

/// Parser-owned result for one Veteran/LeaperKim/NOSFET stream byte.
#[allow(
    clippy::large_enum_variant,
    reason = "complete frames stay inline to keep parser hot paths allocation-free"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VeteranFrameParseResult {
    /// The parser has not accepted a frame prefix yet.
    Seeking,

    /// The parser accepted bytes into a bounded partial frame.
    Buffered,

    /// The parser completed and validated one frame.
    Complete(VeteranFrame),
}

/// Sync reassembler for Veteran/LeaperKim/NOSFET `dc5a5c` notification streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VeteranFrameReassembler {
    buffer: ArrayVec<u8, MAX_VETERAN_FRAME_LEN>,
    state: VeteranFrameParseState,
}

impl Default for VeteranFrameReassembler {
    fn default() -> Self {
        Self {
            buffer: ArrayVec::new(),
            state: VeteranFrameParseState::default(),
        }
    }
}

impl VeteranFrameReassembler {
    #[cfg(test)]
    pub(crate) fn saturated_candidate_for_test() -> Self {
        Self {
            buffer: ArrayVec::from([0x80; MAX_VETERAN_FRAME_LEN]),
            state: VeteranFrameParseState::Collecting,
        }
    }

    /// Feeds one notification byte and returns a parser-owned typed result.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranReassemblyError::CrcMismatch`] when a long frame's
    /// CRC32 trailer does not match the frame contents.
    pub fn feed_byte_result(
        &mut self,
        byte: u8,
    ) -> Result<VeteranFrameParseResult, VeteranReassemblyError> {
        let (state, frame) = match self.state.feed_byte(byte, &mut self.buffer) {
            Ok(result) => result,
            Err(error) => {
                self.reset();
                return Err(error);
            }
        };
        self.state = state;
        Ok(frame.map_or_else(
            || {
                if self.buffer.is_empty() {
                    VeteranFrameParseResult::Seeking
                } else {
                    VeteranFrameParseResult::Buffered
                }
            },
            VeteranFrameParseResult::Complete,
        ))
    }

    /// Resets parser state and drops any partial frame bytes.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.state = VeteranFrameParseState::default();
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum VeteranFrameParseState {
    #[default]
    SeekingMagic0,
    SeekingMagic1,
    SeekingMagic2,
    Collecting,
}

impl VeteranFrameParseState {
    fn feed_byte(
        self,
        byte: u8,
        buffer: &mut ArrayVec<u8, MAX_VETERAN_FRAME_LEN>,
    ) -> Result<(Self, Option<VeteranFrame>), VeteranReassemblyError> {
        match self {
            Self::SeekingMagic0 => Ok(if byte == VETERAN_MAGIC[0] {
                buffer.clear();
                push_candidate_byte(buffer, byte)?;
                (Self::SeekingMagic1, None)
            } else {
                (Self::SeekingMagic0, None)
            }),
            Self::SeekingMagic1 => Ok(match byte {
                0x5a => {
                    push_candidate_byte(buffer, byte)?;
                    (Self::SeekingMagic2, None)
                }
                0xdc => {
                    buffer.clear();
                    push_candidate_byte(buffer, byte)?;
                    (Self::SeekingMagic1, None)
                }
                _ => {
                    buffer.clear();
                    (Self::SeekingMagic0, None)
                }
            }),
            Self::SeekingMagic2 => {
                if byte == VETERAN_MAGIC[2] {
                    push_candidate_byte(buffer, byte)?;
                    let frame = Self::try_finish(buffer)?;
                    Ok((
                        if frame.is_some() {
                            Self::SeekingMagic0
                        } else {
                            Self::Collecting
                        },
                        frame,
                    ))
                } else if byte == VETERAN_MAGIC[0] {
                    buffer.clear();
                    push_candidate_byte(buffer, byte)?;
                    Ok((Self::SeekingMagic1, None))
                } else {
                    buffer.clear();
                    Ok((Self::SeekingMagic0, None))
                }
            }
            Self::Collecting => {
                push_candidate_byte(buffer, byte)?;
                let frame = Self::try_finish(buffer)?;
                Ok((
                    if frame.is_some() {
                        Self::SeekingMagic0
                    } else {
                        Self::Collecting
                    },
                    frame,
                ))
            }
        }
    }

    fn try_finish(
        buffer: &mut ArrayVec<u8, MAX_VETERAN_FRAME_LEN>,
    ) -> Result<Option<VeteranFrame>, VeteranReassemblyError> {
        let Some(expected_len) = veteran_expected_len(buffer.as_slice()) else {
            return Ok(None);
        };
        if buffer.len() < expected_len.get() {
            return Ok(None);
        }

        if veteran_uses_crc(buffer.as_slice()) && !veteran_crc_matches(buffer.as_slice()) {
            buffer.clear();
            return Err(VeteranReassemblyError::CrcMismatch);
        }

        let frame = VeteranFrame::try_from_slice(buffer.as_slice())?;
        buffer.clear();
        Ok(Some(frame))
    }
}

fn push_candidate_byte(
    buffer: &mut ArrayVec<u8, MAX_VETERAN_FRAME_LEN>,
    byte: u8,
) -> Result<(), VeteranReassemblyError> {
    if buffer.try_push(byte).is_err() {
        let claimed = buffer.len().saturating_add(1);
        buffer.clear();
        Err(VeteranReassemblyError::OversizedFrame {
            claimed,
            max: MAX_VETERAN_FRAME_LEN,
        })
    } else {
        Ok(())
    }
}

fn veteran_expected_len(bytes: &[u8]) -> Option<VeteranCompleteFrameLen> {
    ByteCursor::new(bytes)
        .byte(ByteOffset::new(3))
        .map(VeteranDeclaredLen::from_wire)
        .map(VeteranDeclaredLen::complete_frame_len)
}

fn veteran_uses_crc(bytes: &[u8]) -> bool {
    ByteCursor::new(bytes)
        .byte(ByteOffset::new(3))
        .is_some_and(|len| len > VETERAN_SHORT_FRAME_MAX_LEN)
}

fn veteran_crc_matches(bytes: &[u8]) -> bool {
    let cursor = ByteCursor::new(bytes);
    let Some(declared_len) = cursor
        .byte(ByteOffset::new(3))
        .map(VeteranDeclaredLen::from_wire)
    else {
        return false;
    };
    let Some(expected_crc) = cursor.be_u32(declared_len.crc_offset()) else {
        return false;
    };
    let Some(crc_bytes) = bytes.get(..declared_len.get()) else {
        return false;
    };
    crc32fast::hash(crc_bytes) == expected_crc
}

#[cfg(test)]
mod tests {
    const fn ms(value: u64) -> cutout_core::MonotonicTimestamp {
        cutout_core::MonotonicTimestamp::new(value)
    }

    use super::*;
    use proptest::prelude::*;

    fn feed_chunk(reassembler: &mut VeteranFrameReassembler, bytes: &[u8]) -> Vec<VeteranFrame> {
        feed_chunk_result(reassembler, bytes).expect("chunk reassembles without protocol error")
    }

    fn feed_chunk_result(
        reassembler: &mut VeteranFrameReassembler,
        bytes: &[u8],
    ) -> Result<Vec<VeteranFrame>, VeteranReassemblyError> {
        let mut frames = Vec::new();
        for byte in bytes {
            match reassembler.feed_byte_result(*byte)? {
                VeteranFrameParseResult::Complete(frame) => frames.push(frame),
                VeteranFrameParseResult::Seeking | VeteranFrameParseResult::Buffered => {}
            }
        }
        Ok(frames)
    }

    fn long_veteran_frame() -> Vec<u8> {
        let mut frame = vec![0xdc, 0x5a, 0x5c, 39];
        frame.extend(0_u8..35);
        let crc = crc32fast::hash(&frame);
        frame.extend(crc.to_be_bytes());
        frame
    }

    fn fixture_stream() -> Vec<u8> {
        [
            &hex_literal::hex!("dc5a5c532a7c000000000000ab41001700000cff")[..],
            &hex_literal::hex!("000000000226021ca8f607801afa000080c80000")[..],
            &hex_literal::hex!("808080808080022880803080800e310e310e2f0e")[..],
            &hex_literal::hex!("2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e")[..],
            &hex_literal::hex!("310e2e9e05e3ad")[..],
        ]
        .concat()
    }

    #[test]
    fn veteran_frame_rejects_malformed_bytes() {
        assert_eq!(
            VeteranFrame::try_from_slice(b"\x00\x5a\x5c\x01\xaa"),
            Err(VeteranReassemblyError::InvalidFrame)
        );
        assert_eq!(
            VeteranFrame::try_from_slice(b"\xdc\x5a\x5c\x02\xaa"),
            Err(VeteranReassemblyError::InvalidFrame)
        );
    }

    #[test]
    fn veteran_frame_exposes_complete_frame_bytes() {
        let frame = VeteranFrame::try_from_slice(b"\xdc\x5a\x5c\x04\x01\x02\x03\x04")
            .expect("fixture frame fits");

        assert_eq!(frame.as_slice(), b"\xdc\x5a\x5c\x04\x01\x02\x03\x04");
    }

    #[test]
    fn reassembler_reassembles_fragmented_short_frame() {
        let mut reassembler = VeteranFrameReassembler::default();
        let mut frames = Vec::new();

        frames.extend(feed_chunk(&mut reassembler, b"\xdc\x5a\x5c\x04\x01"));
        frames.extend(feed_chunk(&mut reassembler, b"\x02\x03\x04"));

        assert_eq!(
            frames,
            vec![
                VeteranFrame::try_from_slice(b"\xdc\x5a\x5c\x04\x01\x02\x03\x04")
                    .expect("fixture frame fits")
            ]
        );
    }

    #[test]
    fn reassembler_resyncs_before_magic() {
        let mut reassembler = VeteranFrameReassembler::default();

        let frames = feed_chunk(
            &mut reassembler,
            b"\x00\xff\xdc\x5a\x00\xdc\x5a\x5c\x04\x01\x02\x03\x04",
        );

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_slice(), b"\xdc\x5a\x5c\x04\x01\x02\x03\x04");
    }

    #[test]
    fn reassembler_recovers_overlapping_magic_prefixes() {
        let mut reassembler = VeteranFrameReassembler::default();

        let frames = feed_chunk(&mut reassembler, b"\xdc\xdc\x5a\x5c\x01\xaa");

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_slice(), b"\xdc\x5a\x5c\x01\xaa");
    }

    #[test]
    fn reassembler_recovers_when_third_magic_byte_restarts_magic() {
        let mut reassembler = VeteranFrameReassembler::default();

        let frames = feed_chunk(&mut reassembler, b"\xdc\x5a\xdc\x5a\x5c\x01\xaa");

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_slice(), b"\xdc\x5a\x5c\x01\xaa");
    }

    #[test]
    fn reassembler_returns_multiple_frames_from_one_stream() {
        let mut reassembler = VeteranFrameReassembler::default();

        let frames = feed_chunk(
            &mut reassembler,
            b"\xdc\x5a\x5c\x01\xaa\xdc\x5a\x5c\x02\xbb\xcc",
        );

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].as_slice(), b"\xdc\x5a\x5c\x01\xaa");
        assert_eq!(frames[1].as_slice(), b"\xdc\x5a\x5c\x02\xbb\xcc");
    }

    #[test]
    fn reassembler_waits_for_complete_header_before_using_length() {
        let mut reassembler = VeteranFrameReassembler::default();

        assert_eq!(
            reassembler.feed_byte_result(0xdc),
            Ok(VeteranFrameParseResult::Buffered)
        );
        assert_eq!(
            reassembler.feed_byte_result(0x5a),
            Ok(VeteranFrameParseResult::Buffered)
        );
        assert_eq!(
            reassembler.feed_byte_result(0x5c),
            Ok(VeteranFrameParseResult::Buffered)
        );
        assert_eq!(
            reassembler.feed_byte_result(0x01),
            Ok(VeteranFrameParseResult::Buffered)
        );
        assert_eq!(
            reassembler.feed_byte_result(0xaa),
            Ok(VeteranFrameParseResult::Complete(
                VeteranFrame::try_from_slice(b"\xdc\x5a\x5c\x01\xaa").expect("fixture frame fits")
            ))
        );
    }

    #[test]
    fn reassembler_reports_typed_parser_progress() {
        let mut reassembler = VeteranFrameReassembler::default();

        assert_eq!(
            reassembler.feed_byte_result(0x00),
            Ok(VeteranFrameParseResult::Seeking)
        );
        assert_eq!(
            reassembler.feed_byte_result(0xdc),
            Ok(VeteranFrameParseResult::Buffered)
        );
        assert_eq!(
            reassembler.feed_byte_result(0x5a),
            Ok(VeteranFrameParseResult::Buffered)
        );
    }

    #[test]
    fn reassembler_reports_typed_complete_frame() {
        let mut reassembler = VeteranFrameReassembler::default();
        let frame_bytes = hex_literal::hex!("dc5a5c0100");
        let mut result = VeteranFrameParseResult::Seeking;

        for byte in frame_bytes {
            result = reassembler
                .feed_byte_result(byte)
                .expect("short frame parses");
        }

        assert_eq!(
            result,
            VeteranFrameParseResult::Complete(
                VeteranFrame::try_from_slice(&frame_bytes).expect("fixture frame fits")
            )
        );
    }

    #[test]
    fn reset_drops_partial_frame_state() {
        let mut reassembler = VeteranFrameReassembler::default();

        assert_eq!(
            reassembler.feed_byte_result(0xdc),
            Ok(VeteranFrameParseResult::Buffered)
        );
        assert_eq!(
            reassembler.feed_byte_result(0x5a),
            Ok(VeteranFrameParseResult::Buffered)
        );
        reassembler.reset();
        let frames = feed_chunk(&mut reassembler, b"\x5c\x01\xaa");

        assert!(frames.is_empty());
    }

    #[test]
    fn reassembler_rejects_long_frame_with_bad_crc() {
        let mut reassembler = VeteranFrameReassembler::default();
        let mut frame = long_veteran_frame();
        let last = frame.last_mut().expect("fixture has a CRC trailer");
        *last ^= 0xff;

        let error = feed_chunk_result(&mut reassembler, &frame)
            .expect_err("bad CRC should reject the long frame");

        assert_eq!(error, VeteranReassemblyError::CrcMismatch);
    }

    #[test]
    fn reassembler_accepts_long_frame_with_valid_crc() {
        let mut reassembler = VeteranFrameReassembler::default();
        let frame = long_veteran_frame();

        let frames = feed_chunk_result(&mut reassembler, &frame).expect("valid CRC is accepted");

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_slice(), frame.as_slice());
    }

    #[test]
    fn short_frame_at_crc_threshold_does_not_require_crc() {
        let mut reassembler = VeteranFrameReassembler::default();
        let mut frame = vec![0xdc, 0x5a, 0x5c, VETERAN_SHORT_FRAME_MAX_LEN];
        frame.extend(std::iter::repeat_n(
            0xa5,
            usize::from(VETERAN_SHORT_FRAME_MAX_LEN),
        ));

        let frames = feed_chunk_result(&mut reassembler, &frame)
            .expect("threshold short frame should not require CRC");

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_slice(), frame.as_slice());
    }

    #[test]
    fn reassembler_consumes_live_aero_fixture_bytes() {
        let mut reassembler = VeteranFrameReassembler::default();
        let frames = feed_chunk(&mut reassembler, &fixture_stream());

        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0].as_slice().get(..4),
            Some(&[0xdc, 0x5a, 0x5c, 0x53][..])
        );
    }

    #[test]
    fn reassembler_reports_crc_mismatch_at_max_declared_length() {
        let mut reassembler = VeteranFrameReassembler::default();
        let mut frame = vec![0xdc, 0x5a, 0x5c, 0xff];
        frame.extend(std::iter::repeat_n(0x80, 260));

        let error = feed_chunk_result(&mut reassembler, &frame)
            .expect_err("max-length frame without valid CRC should be rejected");

        assert_eq!(error, VeteranReassemblyError::CrcMismatch);
    }

    #[test]
    fn reassembler_reports_oversized_candidate_without_panicking() {
        let mut reassembler = VeteranFrameReassembler {
            buffer: ArrayVec::from([0x80; MAX_VETERAN_FRAME_LEN]),
            state: VeteranFrameParseState::Collecting,
        };

        let error = reassembler
            .feed_byte_result(0x80)
            .expect_err("saturated candidate is a typed parser error");

        assert_eq!(
            error,
            VeteranReassemblyError::OversizedFrame {
                claimed: MAX_VETERAN_FRAME_LEN + 1,
                max: MAX_VETERAN_FRAME_LEN,
            }
        );
        assert_eq!(
            reassembler.feed_byte_result(0x00),
            Ok(VeteranFrameParseResult::Seeking)
        );
    }

    proptest! {
        #[test]
        fn arbitrary_noisy_streams_do_not_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let mut reassembler = VeteranFrameReassembler::default();

            for byte in bytes {
                let _ = reassembler.feed_byte_result(byte);
            }
        }

        #[test]
        fn arbitrary_fragmentation_matches_whole_frame(chunk_sizes in proptest::collection::vec(1usize..16, 1..16)) {
            let frame = long_veteran_frame();
            let mut whole = VeteranFrameReassembler::default();
            let whole_frames = feed_chunk(&mut whole, &frame);
            let mut fragmented = VeteranFrameReassembler::default();
            let mut fragmented_frames = Vec::new();
            let mut offset = 0usize;
            let mut size_index = 0usize;

            while offset < frame.len() {
                let size = chunk_sizes[size_index % chunk_sizes.len()];
                let end = offset.saturating_add(size).min(frame.len());
                fragmented_frames.extend(feed_chunk(&mut fragmented, &frame[offset..end]));
                offset = end;
                size_index += 1;
            }

            prop_assert_eq!(fragmented_frames, whole_frames);
        }

        #[test]
        fn notification_boundary_cases_match_whole_frame_reassembly(
            chunk_sizes in proptest::collection::vec(1usize..16, 1..16),
        ) {
            let frame = long_veteran_frame();
            let channel = cutout_core::GattChannel::from_bytes([0x5c; 16]);
            let chunk_sizes = chunk_sizes
                .into_iter()
                .map(cutout_core::NotificationChunkLen::from_bytes)
                .collect::<Vec<_>>();
            let cases = cutout_core::notification_boundary_replay_cases(
                channel,
                &[frame.as_slice()], ms(1),
                &chunk_sizes,
            );
            let expected = feed_chunk(&mut VeteranFrameReassembler::default(), &frame);

            for case in cases {
                let mut reassembler = VeteranFrameReassembler::default();
                let mut observed = Vec::new();
                for record in case.records {
                    let cutout_core::CaptureRecord::Notification { bytes, .. } = record else {
                        continue;
                    };
                    observed.extend(feed_chunk(&mut reassembler, &bytes));
                }

                prop_assert_eq!(&observed, &expected, "case {}", case.name);
            }
        }
    }
}
