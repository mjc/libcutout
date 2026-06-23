use super::GattChannel;
use uuid::Uuid;

#[test]
fn gatt_channel_round_trips_uuid_and_wire_bytes() {
    let uuid = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
    let channel = GattChannel::from_uuid(uuid);

    assert_eq!(channel.as_uuid(), uuid);
    assert_eq!(channel.as_bytes(), *uuid.as_bytes());
    assert_eq!(GattChannel::from_bytes(*uuid.as_bytes()), channel);
}
