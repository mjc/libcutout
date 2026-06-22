use cutout_core::GattChannel;

/// Capture-backed FFE0 service UUID for NOSFET/Veteran-family sessions.
pub const VETERAN_SERVICE_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xff, 0xe0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

/// Capture-backed FFE1 data characteristic UUID for NOSFET/Veteran-family sessions.
pub const VETERAN_DATA_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xff, 0xe1, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

/// Placeholder write channel for Begode-family sessions.
pub const FALCON_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([0xB1; 16]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_backed_channels_preserve_expected_bytes() {
        assert_eq!(
            VETERAN_SERVICE_CHANNEL.as_bytes(),
            [
                0x00, 0x00, 0xff, 0xe0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]
        );
        assert_eq!(
            VETERAN_DATA_CHANNEL.as_bytes(),
            [
                0x00, 0x00, 0xff, 0xe1, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]
        );
        assert_eq!(FALCON_WRITE_CHANNEL.as_bytes(), [0xB1; 16]);
    }
}
