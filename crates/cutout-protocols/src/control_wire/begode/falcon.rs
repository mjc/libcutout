//! Falcon currently uses the inherited Begode command layouts without overrides.
//! Its model profile still restricts which settings and values are supported.

control_wire_dialect!(crate::FalconDialect: super::BegodeWire);
