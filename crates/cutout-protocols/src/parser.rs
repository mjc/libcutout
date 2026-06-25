use cutout_core::{Information, Quantity, Unit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParserInputByte;

impl Unit for ParserInputByte {
    type Dimension = Information;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParserSpanByte;

impl Unit for ParserSpanByte {
    type Dimension = Information;
}

pub(crate) type ParserOffset = Quantity<Information, ParserInputByte, usize>;

pub(crate) type ParserSpan = Quantity<Information, ParserSpanByte, usize>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParserRange {
    start: ParserOffset,
    len: ParserSpan,
}

impl ParserRange {
    #[must_use]
    pub(crate) const fn new(start: ParserOffset, len: ParserSpan) -> Self {
        Self { start, len }
    }

    #[must_use]
    fn bounds(self) -> Option<std::ops::Range<usize>> {
        let start = self.start.as_bytes();
        let end = start.checked_add(self.len.as_bytes())?;
        Some(start..end)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParserCursor<'bytes> {
    bytes: &'bytes [u8],
}

impl<'bytes> ParserCursor<'bytes> {
    #[must_use]
    pub(crate) const fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes }
    }

    #[must_use]
    pub(crate) fn byte(self, offset: ParserOffset) -> Option<u8> {
        self.bytes.get(offset.as_bytes()).copied()
    }

    #[must_use]
    pub(crate) fn range(self, range: ParserRange) -> Option<&'bytes [u8]> {
        self.bytes.get(range.bounds()?)
    }

    #[must_use]
    pub(crate) fn be_u16(self, offset: ParserOffset) -> Option<u16> {
        let bytes = self.range(ParserRange::new(offset, ParserSpan::from_bytes(2)))?;
        Some(u16::from_be_bytes([*bytes.first()?, *bytes.get(1)?]))
    }

    #[must_use]
    pub(crate) fn be_i16(self, offset: ParserOffset) -> Option<i16> {
        self.be_u16(offset)
            .map(|value| i16::from_be_bytes(value.to_be_bytes()))
    }

    #[must_use]
    pub(crate) fn be_u32(self, offset: ParserOffset) -> Option<u32> {
        let bytes = self.range(ParserRange::new(offset, ParserSpan::from_bytes(4)))?;
        Some(u32::from_be_bytes([
            *bytes.first()?,
            *bytes.get(1)?,
            *bytes.get(2)?,
            *bytes.get(3)?,
        ]))
    }

    #[must_use]
    pub(crate) fn veteran_swapped_u32(self, offset: ParserOffset) -> Option<u32> {
        let bytes = self.range(ParserRange::new(offset, ParserSpan::from_bytes(4)))?;
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
    fn parser_cursor_reads_big_endian_values_at_typed_offsets() {
        let bytes = [0x12, 0x34, 0xfe, 0xdc, 0xba, 0x98];
        let cursor = ParserCursor::new(&bytes);

        assert_eq!(cursor.be_u16(ParserOffset::from_bytes(0)), Some(0x1234));
        assert_eq!(cursor.be_i16(ParserOffset::from_bytes(2)), Some(-292));
        assert_eq!(
            cursor.be_u32(ParserOffset::from_bytes(2)),
            Some(0xfedc_ba98)
        );
    }

    #[test]
    fn parser_cursor_rejects_short_reads_without_exposing_partial_values() {
        let cursor = ParserCursor::new(&[0x12, 0x34, 0x56]);

        assert_eq!(cursor.be_u16(ParserOffset::from_bytes(2)), None);
        assert_eq!(cursor.be_u32(ParserOffset::from_bytes(0)), None);
        assert_eq!(cursor.byte(ParserOffset::from_bytes(3)), None);
    }

    #[test]
    fn parser_cursor_returns_borrowed_ranges_from_typed_offsets() {
        let bytes = [1, 2, 3, 4, 5];
        let cursor = ParserCursor::new(&bytes);
        let range = ParserRange::new(ParserOffset::from_bytes(1), ParserSpan::from_bytes(3));

        assert_eq!(cursor.range(range), Some(&bytes[1..4]));
    }

    #[test]
    fn veteran_swapped_u32_is_an_explicit_protocol_read() {
        let cursor = ParserCursor::new(&[0x12, 0x34, 0x56, 0x78]);

        assert_eq!(
            cursor.veteran_swapped_u32(ParserOffset::from_bytes(0)),
            Some(0x5678_1234)
        );
    }
}
