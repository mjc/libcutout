#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]

//! Protocol-family scaffolding for Cutout.

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-protocols"
}

#[cfg(test)]
mod tests {
    use super::crate_name;

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-protocols");
    }
}
