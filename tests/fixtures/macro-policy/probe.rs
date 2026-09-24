#![cfg_attr(test, allow(clippy::disallowed_macros))]

pub fn unqualified(value: Option<u8>) -> bool {
    matches!(value, Some(1))
}

pub fn core_qualified(value: Option<u8>) -> bool {
    core::matches!(value, Some(1))
}

pub fn std_qualified(value: Option<u8>) -> bool {
    std::matches!(value, Some(1))
}

#[test]
fn test_build_permits_all_spellings() {
    assert!(unqualified(Some(1)));
    assert!(core_qualified(Some(1)));
    assert!(std_qualified(Some(1)));
}
