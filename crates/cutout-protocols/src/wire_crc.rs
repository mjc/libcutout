use crc::{CRC_16_XMODEM, Crc};

const XMODEM: Crc<u16> = Crc::<u16>::new(&CRC_16_XMODEM);

pub(super) fn crc16_xmodem(bytes: &[u8]) -> u16 {
    XMODEM.checksum(bytes)
}
