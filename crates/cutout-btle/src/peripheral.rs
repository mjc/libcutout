use std::pin::Pin;

use async_trait::async_trait;
use btleplug::api::{Characteristic, ValueNotification};
use cutout_core::WriteMode;
use futures_util::stream::Stream;

use crate::BtleError;

/// Minimal BTLE operations required by the protocol bridge.
#[async_trait]
pub trait SessionPeripheral: Send + Sync {
    /// Returns the negotiated MTU for the connected peripheral.
    fn mtu(&self) -> u16;

    /// Subscribes to notifications on the selected endpoint.
    async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), BtleError>;

    /// Writes a payload to the selected endpoint.
    async fn write(
        &self,
        characteristic: &Characteristic,
        bytes: &[u8],
        mode: WriteMode,
    ) -> Result<(), BtleError>;

    /// Returns the notification stream for the connected peripheral.
    async fn notifications(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>, BtleError>;

    /// Disconnects the peripheral.
    async fn disconnect(&self) -> Result<(), BtleError>;
}

#[async_trait]
impl SessionPeripheral for btleplug::platform::Peripheral {
    fn mtu(&self) -> u16 {
        btleplug::api::Peripheral::mtu(self)
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), BtleError> {
        btleplug::api::Peripheral::subscribe(self, characteristic)
            .await
            .map_err(BtleError::from)
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        bytes: &[u8],
        mode: WriteMode,
    ) -> Result<(), BtleError> {
        btleplug::api::Peripheral::write(
            self,
            characteristic,
            bytes,
            match mode {
                WriteMode::WithResponse => btleplug::api::WriteType::WithResponse,
                WriteMode::WithoutResponse => btleplug::api::WriteType::WithoutResponse,
            },
        )
        .await
        .map_err(BtleError::from)
    }

    async fn notifications(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>, BtleError> {
        btleplug::api::Peripheral::notifications(self)
            .await
            .map_err(BtleError::from)
    }

    async fn disconnect(&self) -> Result<(), BtleError> {
        btleplug::api::Peripheral::disconnect(self)
            .await
            .map_err(BtleError::from)
    }
}
