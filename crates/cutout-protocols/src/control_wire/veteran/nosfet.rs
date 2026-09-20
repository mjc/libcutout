//! NOSFET additions to the Veteran command schema. No Aero-specific override is
//! declared without evidence of a model-specific protocol difference.

use super::{Layout, VeteranWire};

control_wire_dialect! {
    crate::NosfetDialect => NosfetWire : VeteranWire {
        BrakeOverpressureAlarm => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 25 },
    }
}
