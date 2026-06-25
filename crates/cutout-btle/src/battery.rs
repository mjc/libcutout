use btleplug::api::Peripheral as _;

use crate::{
    BtleBatteryLevel, BtleError, ConnectionSummary, error::backend_call,
    types::characteristic_from_summary,
};

/// Reads the standard BLE Battery Level characteristic from a connected peripheral.
///
/// Returns `Ok(None)` when the device does not expose a readable Battery Level
/// characteristic, when the characteristic returns an empty payload, or when
/// the backend byte is outside the standard BLE 0..=100 percentage range.
///
/// # Errors
///
/// Returns [`BtleError::Backend`] if the underlying BTLE stack fails the read.
pub async fn read_battery_level(
    peripheral: &btleplug::platform::Peripheral,
    summary: &ConnectionSummary,
) -> Result<Option<BtleBatteryLevel>, BtleError> {
    let Some(characteristic) = summary.battery_level_characteristic() else {
        return Ok(None);
    };

    let value = backend_call(
        "read battery level",
        peripheral.read(&characteristic_from_summary(characteristic)),
    )
    .await?;
    Ok(value
        .first()
        .copied()
        .and_then(BtleBatteryLevel::from_backend_byte))
}
