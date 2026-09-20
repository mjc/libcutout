//! Begode protocol command destinations. Model support is selected separately.

use super::Layout;

mod falcon;

control_wire_schema! {
    crate::BegodeProtocol => BegodeWire {
        LightOn => Layout::Literal(b"Q"),
        LightOff => Layout::Literal(b"E"),
        LightStrobe => Layout::Literal(b"T"),
        PedalHard => Layout::Literal(b"h"),
        PedalMedium => Layout::Literal(b"f"),
        PedalSoft => Layout::Literal(b"s"),
        RollLow => Layout::Literal(b">"),
        RollMedium => Layout::Literal(b"="),
        RollHigh => Layout::Literal(b"<"),
        AlarmBoth => Layout::Literal(b"o"),
        AlarmStageOne => Layout::Literal(b"u"),
        MaxSpeed => Layout::DecimalMenu { selector: b'Y', digits: 2 },
        BeeperVolume => Layout::DecimalMenu { selector: b'B', digits: 1 },
        LedMode => Layout::DecimalMenu { selector: b'M', digits: 1 },
    }
}
