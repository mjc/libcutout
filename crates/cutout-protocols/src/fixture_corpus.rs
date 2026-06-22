use super::*;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct FixtureCorpus;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReassembledFrames {
    pub frames: Vec<VeteranFrame>,
    pub diagnostics: usize,
}

impl FixtureCorpus {
    pub(super) fn notification_chunks() -> Vec<Vec<u8>> {
        Self::hex_fixture_chunks(include_str!(
            "../fixtures/nosfet-aero/nf2557-2026-06-21-notifications.hex"
        ))
    }

    pub(super) fn bms_page_chunks() -> Vec<Vec<u8>> {
        Self::hex_fixture_chunks(include_str!(
            "../fixtures/nosfet-aero/nf2557-2026-06-21-bms-pages.hex"
        ))
    }

    pub(super) fn long_powered_on_chunks() -> Vec<Vec<u8>> {
        Self::hex_fixture_chunks(include_str!(
            "../fixtures/nosfet-aero/nf2557-2026-06-21-powered-on-long.hex"
        ))
    }

    pub(super) fn veteran_frames_from_chunks(
        chunks: impl IntoIterator<Item = Vec<u8>>,
    ) -> Vec<VeteranFrame> {
        let mut reassembler = VeteranFrameReassembler::default();
        let mut frames = Vec::new();

        for chunk in chunks {
            frames.extend(Self::feed_chunk(&mut reassembler, &chunk));
        }

        frames
    }

    pub(super) fn lossy_veteran_frames_from_chunks(
        chunks: impl IntoIterator<Item = Vec<u8>>,
    ) -> ReassembledFrames {
        let mut reassembler = VeteranFrameReassembler::default();
        let mut frames = Vec::new();
        let mut diagnostics = 0;

        for chunk in chunks {
            for byte in chunk {
                match reassembler.feed_byte(byte) {
                    Ok(Some(frame)) => frames.push(frame),
                    Ok(None) => {}
                    Err(_) => diagnostics += 1,
                }
            }
        }

        ReassembledFrames {
            frames,
            diagnostics,
        }
    }

    pub(super) fn arbitrary_fixture_chunks() -> Vec<Vec<u8>> {
        Self::fixture_chunks_with_pattern([1_usize, 7, 13, 2, 31, 5])
    }

    pub(super) fn fixture_chunks_with_pattern<const N: usize>(sizes: [usize; N]) -> Vec<Vec<u8>> {
        let bytes: Vec<_> = Self::notification_chunks().into_iter().flatten().collect();
        let mut chunks = Vec::new();
        let mut offset = 0;
        let mut size_index = 0;

        while offset < bytes.len() {
            let size = sizes[size_index % sizes.len()];
            let end = offset.saturating_add(size).min(bytes.len());
            chunks.push(bytes[offset..end].to_vec());
            offset = end;
            size_index += 1;
        }

        chunks
    }

    fn feed_chunk(reassembler: &mut VeteranFrameReassembler, bytes: &[u8]) -> Vec<VeteranFrame> {
        Self::feed_chunk_result(reassembler, bytes)
            .expect("chunk reassembles without protocol error")
    }

    fn feed_chunk_result(
        reassembler: &mut VeteranFrameReassembler,
        bytes: &[u8],
    ) -> Result<Vec<VeteranFrame>, VeteranReassemblyError> {
        let mut frames = Vec::new();
        for byte in bytes {
            if let Some(frame) = reassembler.feed_byte(*byte)? {
                frames.push(frame);
            }
        }
        Ok(frames)
    }

    fn hex_fixture_chunks(fixture: &str) -> Vec<Vec<u8>> {
        fixture.lines().filter_map(Self::hex_fixture_line).collect()
    }

    fn hex_fixture_line(line: &str) -> Option<Vec<u8>> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        Some(Self::hex_to_bytes(trimmed))
    }

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut nibbles = hex.bytes();
        while let Some(high) = nibbles.next() {
            let low = nibbles.next().expect("fixture hex has even length");
            bytes.push((Self::hex_nibble(high) << 4) | Self::hex_nibble(low));
        }
        bytes
    }

    fn hex_nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => 10 + byte - b'a',
            b'A'..=b'F' => 10 + byte - b'A',
            _ => panic!("fixture contains non-hex byte"),
        }
    }
}
