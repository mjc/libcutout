//! Veteran protocol command destinations. Model support is selected separately.

use super::Layout;

pub(super) mod nosfet;

control_wire_schema! {
    crate::VeteranProtocol => VeteranWire {
        PedalAngle => Layout::Field { magic: *b"LkAp", bank: &[1], offset: 11 },
        SpeedAlarm => Layout::Field { magic: *b"LkAp", bank: &[1], offset: 12 },
        LateralTilt => Layout::Field { magic: *b"LkAp", bank: &[1], offset: 17 },
        PedalHardness => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 10 },
        TiltbackSpeed => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 12 },
        PwmTiltback => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 13 },
        DisplayBrightness => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 15 },
        GyroCalibration => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 16 },
        TransportMode => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 17 },
        DisplayUnits => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 18 },
        VoltageCorrection => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 19 },
        LowBatteryMode => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 20 },
        HighSpeedMode => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 21 },
        BeeperVolume => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 23 },
        DynamicAssist => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 26 },
        PedalDipCompensation => Layout::Field { magic: *b"LdAp", bank: &[1, 2], offset: 28 },
        Horn => Layout::Field { magic: *b"LkAp", bank: &[0], offset: 9 },
        ResetTrip => Layout::Field { magic: *b"LkAp", bank: &[0], offset: 6 },
        Headlight => Layout::Field { magic: *b"LkAp", bank: &[1], offset: 8 },
        RidingPreset => Layout::Field { magic: *b"LkAp", bank: &[1], offset: 7 },
        AsciiHorn => Layout::Literal(b"OLDCMDb"),
        AsciiResetTrip => Layout::Literal(b"CLEARMETER"),
        AsciiHeadlightOn => Layout::Literal(b"SetLightON"),
        AsciiHeadlightOff => Layout::Literal(b"SetLightOFF"),
        AsciiRidingSoft => Layout::Literal(b"SETs"),
        AsciiRidingMedium => Layout::Literal(b"SETm"),
        AsciiRidingHard => Layout::Literal(b"SETh"),
    }
}
