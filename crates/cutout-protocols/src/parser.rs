#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ByteOffset(usize);

impl ByteOffset {
    #[must_use]
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    #[must_use]
    const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ByteLen(usize);

impl ByteLen {
    #[must_use]
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    #[must_use]
    const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ByteRange {
    start: ByteOffset,
    len: ByteLen,
}

impl ByteRange {
    #[must_use]
    pub(crate) const fn new(start: ByteOffset, len: ByteLen) -> Self {
        Self { start, len }
    }

    #[must_use]
    fn bounds(self) -> Option<std::ops::Range<usize>> {
        let start = self.start.get();
        let end = start.checked_add(self.len.get())?;
        Some(start..end)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ByteCursor<'bytes> {
    bytes: &'bytes [u8],
}

impl<'bytes> ByteCursor<'bytes> {
    #[must_use]
    pub(crate) const fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes }
    }

    #[must_use]
    pub(crate) fn byte(self, offset: ByteOffset) -> Option<u8> {
        self.bytes.get(offset.get()).copied()
    }

    #[must_use]
    pub(crate) fn range(self, range: ByteRange) -> Option<&'bytes [u8]> {
        self.bytes.get(range.bounds()?)
    }

    #[must_use]
    pub(crate) fn be_u16(self, offset: ByteOffset) -> Option<u16> {
        let bytes = self.range(ByteRange::new(offset, ByteLen::new(2)))?;
        Some(u16::from_be_bytes([*bytes.first()?, *bytes.get(1)?]))
    }

    #[must_use]
    pub(crate) fn be_i16(self, offset: ByteOffset) -> Option<i16> {
        self.be_u16(offset)
            .map(|value| i16::from_be_bytes(value.to_be_bytes()))
    }

    #[must_use]
    pub(crate) fn be_u32(self, offset: ByteOffset) -> Option<u32> {
        let bytes = self.range(ByteRange::new(offset, ByteLen::new(4)))?;
        Some(u32::from_be_bytes([
            *bytes.first()?,
            *bytes.get(1)?,
            *bytes.get(2)?,
            *bytes.get(3)?,
        ]))
    }

    #[must_use]
    pub(crate) fn veteran_swapped_u32(self, offset: ByteOffset) -> Option<u32> {
        let bytes = self.range(ByteRange::new(offset, ByteLen::new(4)))?;
        let b0 = u32::from(*bytes.first()?);
        let b1 = u32::from(*bytes.get(1)?);
        let b2 = u32::from(*bytes.get(2)?);
        let b3 = u32::from(*bytes.get(3)?);
        Some((b2 << 24) | (b3 << 16) | (b0 << 8) | b1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_cursor_reads_big_endian_values_at_typed_offsets() {
        let bytes = [0x12, 0x34, 0xfe, 0xdc, 0xba, 0x98];
        let cursor = ByteCursor::new(&bytes);

        assert_eq!(cursor.be_u16(ByteOffset::new(0)), Some(0x1234));
        assert_eq!(cursor.be_i16(ByteOffset::new(2)), Some(-292));
        assert_eq!(cursor.be_u32(ByteOffset::new(2)), Some(0xfedc_ba98));
    }

    #[test]
    fn byte_cursor_rejects_short_reads_without_exposing_partial_values() {
        let cursor = ByteCursor::new(&[0x12, 0x34, 0x56]);

        assert_eq!(cursor.be_u16(ByteOffset::new(2)), None);
        assert_eq!(cursor.be_u32(ByteOffset::new(0)), None);
        assert_eq!(cursor.byte(ByteOffset::new(3)), None);
    }

    #[test]
    fn byte_cursor_returns_borrowed_ranges_from_typed_offsets() {
        let bytes = [1, 2, 3, 4, 5];
        let cursor = ByteCursor::new(&bytes);
        let range = ByteRange::new(ByteOffset::new(1), ByteLen::new(3));

        assert_eq!(cursor.range(range), Some(&bytes[1..4]));
    }

    #[test]
    fn veteran_swapped_u32_is_an_explicit_protocol_read() {
        let cursor = ByteCursor::new(&[0x12, 0x34, 0x56, 0x78]);

        assert_eq!(
            cursor.veteran_swapped_u32(ByteOffset::new(0)),
            Some(0x5678_1234)
        );
    }
}
