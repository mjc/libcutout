#![allow(
    clippy::indexing_slicing,
    reason = "const scans are length-bounded; runtime indices come from generated command discriminants"
)]

#[derive(Clone, Copy, Debug)]
pub(crate) struct BinaryField {
    pub(crate) magic: [u8; 4],
    pub(crate) bank: &'static [u8],
    pub(crate) offset: u8,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Layout {
    Field {
        magic: [u8; 4],
        bank: &'static [u8],
        offset: u8,
    },
    Literal(&'static [u8]),
    DecimalMenu {
        selector: u8,
        digits: u8,
    },
}

#[derive(Debug)]
pub(crate) struct Schema {
    parent: Option<&'static Schema>,
    layouts: &'static [Layout],
}

impl Schema {
    pub(crate) const fn new(layouts: &'static [Layout]) -> Self {
        Self::build(None, layouts)
    }

    pub(crate) const fn extend(parent: &'static Schema, layouts: &'static [Layout]) -> Self {
        Self::build(Some(parent), layouts)
    }

    const fn build(parent: Option<&'static Schema>, layouts: &'static [Layout]) -> Self {
        let schema = Self { parent, layouts };
        let inherited = match parent {
            Some(base) => base.len(),
            None => 0,
        };
        let mut i = 0;
        while i < layouts.len() {
            match layouts[i] {
                Layout::Field { bank, offset, .. } => validate_field(BinaryField {
                    magic: [0; 4],
                    bank,
                    offset,
                }),
                Layout::Literal(bytes) => assert!(!bytes.is_empty()),
                Layout::DecimalMenu { digits, .. } => assert!(digits == 1 || digits == 2),
            }
            let mut j = 0;
            while j < inherited + i {
                assert!(
                    !overlaps(layouts[i], schema.layout(j)),
                    "control wire destinations overlap"
                );
                j += 1;
            }
            i += 1;
        }
        schema
    }

    pub(crate) const fn len(&self) -> usize {
        self.layouts.len()
            + match self.parent {
                Some(base) => base.len(),
                None => 0,
            }
    }

    pub(crate) const fn layout(&self, index: usize) -> Layout {
        if let Some(base) = self.parent {
            if index < base.len() {
                return base.layout(index);
            }
            return self.layouts[index - base.len()];
        }
        self.layouts[index]
    }
}

const fn validate_field(field: BinaryField) {
    assert!(!field.bank.is_empty());
    assert!(field.offset as usize >= 5 + field.bank.len() && field.offset <= 28);
}

const fn bytes_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn fields_overlap(a: BinaryField, b: BinaryField) -> bool {
    if a.offset != b.offset || !bytes_equal(&a.magic, &b.magic) {
        return false;
    }
    // Unspecified bank bytes are 0x80 on the wire, not a new namespace.
    let mut i = 0;
    while i < a.bank.len() || i < b.bank.len() {
        let av = if i < a.bank.len() { a.bank[i] } else { 0x80 };
        let bv = if i < b.bank.len() { b.bank[i] } else { 0x80 };
        if av != bv {
            return false;
        }
        i += 1;
    }
    true
}

const fn field_overlaps_literal(field: BinaryField, bytes: &[u8]) -> bool {
    if bytes.len() != field.offset as usize + 5 {
        return false;
    }
    let mut i = 0;
    while i < 4 {
        if bytes[i] != field.magic[i] {
            return false;
        }
        i += 1;
    }
    if bytes[4] != field.offset + 5 {
        return false;
    }
    i = 5;
    while i < field.offset as usize {
        let expected = if i - 5 < field.bank.len() {
            field.bank[i - 5]
        } else {
            0x80
        };
        if bytes[i] != expected {
            return false;
        }
        i += 1;
    }
    // Conservatively reserve every value/CRC for this field destination.
    true
}

const fn layout_fields_overlap(layout: Layout, field: BinaryField) -> bool {
    match layout {
        Layout::Field {
            magic,
            bank,
            offset,
        } => fields_overlap(
            BinaryField {
                magic,
                bank,
                offset,
            },
            field,
        ),
        _ => false,
    }
}

const fn overlaps(a: Layout, b: Layout) -> bool {
    match (a, b) {
        (
            Layout::Field {
                magic,
                bank,
                offset,
            },
            other,
        )
        | (
            other,
            Layout::Field {
                magic,
                bank,
                offset,
            },
        ) => {
            let field = BinaryField {
                magic,
                bank,
                offset,
            };
            match other {
                Layout::Field { .. } => layout_fields_overlap(other, field),
                Layout::Literal(bytes) => field_overlaps_literal(field, bytes),
                Layout::DecimalMenu { .. } => field.magic[0] == b'W',
            }
        }
        (Layout::Literal(a), Layout::Literal(b)) => bytes_equal(a, b),
        (Layout::DecimalMenu { selector: a, .. }, Layout::DecimalMenu { selector: b, .. }) => {
            a == b
        }
        // A standalone W would enter the same submenu transaction namespace.
        (Layout::Literal(bytes), Layout::DecimalMenu { .. })
        | (Layout::DecimalMenu { .. }, Layout::Literal(bytes)) => bytes_equal(bytes, b"W"),
    }
}
