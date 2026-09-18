#![allow(
    clippy::indexing_slicing,
    reason = "const scans are length-bounded; runtime indices come from generated command discriminants"
)]

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
                Layout::Field { bank, offset, .. } => {
                    assert!(!bank.is_empty());
                    assert!(offset as usize >= 5 + bank.len() && offset <= 28);
                }
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

const fn overlaps(a: Layout, b: Layout) -> bool {
    match (a, b) {
        (
            Layout::Field {
                magic: am,
                bank: ab,
                offset: ao,
            },
            Layout::Field {
                magic: bm,
                bank: bb,
                offset: bo,
            },
        ) => {
            if ao != bo || !bytes_equal(&am, &bm) {
                return false;
            }
            // Unspecified bank bytes are 0x80 on the wire, not a new namespace.
            let mut i = 0;
            while i < ab.len() || i < bb.len() {
                let av = if i < ab.len() { ab[i] } else { 0x80 };
                let bv = if i < bb.len() { bb[i] } else { 0x80 };
                if av != bv {
                    return false;
                }
                i += 1;
            }
            true
        }
        (Layout::Literal(a), Layout::Literal(b)) => bytes_equal(a, b),
        (Layout::DecimalMenu { selector: a, .. }, Layout::DecimalMenu { selector: b, .. }) => {
            a == b
        }
        // A standalone W would enter the same submenu transaction namespace.
        (Layout::Literal(bytes), Layout::DecimalMenu { .. })
        | (Layout::DecimalMenu { .. }, Layout::Literal(bytes)) => bytes_equal(bytes, b"W"),
        (
            Layout::Field {
                magic,
                bank,
                offset,
            },
            Layout::Literal(bytes),
        )
        | (
            Layout::Literal(bytes),
            Layout::Field {
                magic,
                bank,
                offset,
            },
        ) => {
            if bytes.len() != offset as usize + 5 {
                return false;
            }
            let mut i = 0;
            while i < 4 {
                if bytes[i] != magic[i] {
                    return false;
                }
                i += 1;
            }
            if bytes[4] != offset + 5 {
                return false;
            }
            i = 5;
            while i < offset as usize {
                let expected = if i - 5 < bank.len() {
                    bank[i - 5]
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
        (Layout::Field { magic, .. }, Layout::DecimalMenu { .. })
        | (Layout::DecimalMenu { .. }, Layout::Field { magic, .. }) => magic[0] == b'W',
    }
}
